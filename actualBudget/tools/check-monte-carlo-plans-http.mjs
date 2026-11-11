// Saved-plan migration and recovery, through the same public API as clients.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { startServer } from "../web/tests/server.mjs";

// Restart coverage must own its isolated process and data directory.
assert.ok(!process.env.ACTUALBUDGET_TEST_URL, "This check owns its server");
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/monte-carlo-plans-source.json", import.meta.url),
  ),
);
const archive = await readFile(
  new URL("../tests/fixtures/monte-carlo-plans-26.10.zip", import.meta.url),
);
assert.equal(
  createHash("sha256").update(archive).digest("hex"),
  fixture.archive_sha256,
);
const directory = await mkdtemp(tmpdir() + "/monte-carlo-recovery-");
let server;
async function call(path, body, { method, status = 200, headers = {} } = {}) {
  const response = await fetch(
    server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1" + path,
    {
      method: method ?? (body === undefined ? "GET" : "POST"),
      headers: {
        "Content-Type": Buffer.isBuffer(body)
          ? "application/zip"
          : "application/json",
        "Idempotency-Key": randomUUID(),
        ...headers,
      },
      body: Buffer.isBuffer(body)
        ? body
        : body === undefined
          ? undefined
          : JSON.stringify(body),
      signal: AbortSignal.timeout(30000),
    },
  );
  const output = response.headers
    .get("content-type")
    ?.startsWith("application/json")
    ? await response.json()
    : Buffer.from(await response.arrayBuffer());
  assert.equal(response.status, status, path + " " + JSON.stringify(output));
  return output;
}
async function simulate(root, config) {
  return (
    await call(root + "/reports/monte-carlo", {
      version: 1,
      request_id: randomUUID(),
      config,
      seed: 1234,
      captureRunDetail: 0,
    })
  ).data;
}
async function financialState(root) {
  const { snapshot, ...ledger } = await call(root + "/transactions");
  assert.equal(
    ledger.next_offset,
    null,
    "Fixture must fit in one register page",
  );
  assert.equal(ledger.data.length, ledger.filtered_count);
  return ledger;
}
try {
  server = await startServer({ dataDirectory: directory });
  const staged = (await call("/imports/actual", archive, { status: 201 })).data;
  assert.equal(staged.status, "ready", JSON.stringify(staged));
  const budget = (
    await call(`/imports/${staged.id}/activate`, {}, { status: 201 })
  ).data;
  const root = "/budgets/" + budget.id;
  const initial = (await call(root + "/reporting")).data;
  const ledger = await financialState(root);
  const epochHeaders = { "X-Actual-Budget-Epoch": initial.epoch };
  const plans = initial.documents.filter(
    (d) => d.config.type === "monte-carlo-card",
  );
  assert.equal(plans.length, fixture.widgets.length);
  for (const source of fixture.widgets) {
    const { id, tombstone, ...config } = source;
    const plan = plans.find((d) => d.id === id);
    assert.ok(plan, id);
    assert.equal(plan.kind, "widget");
    assert.equal(plan.tombstone, Boolean(tombstone));
    assert.deepEqual(
      plan.config,
      config,
      "Every source plan field must survive import",
    );
  }
  assert.equal(
    (await call(root + "/preferences")).data.experimental.monteCarloReport,
    false,
  );
  assert.equal(
    (
      await call(
        root + "/reports/monte-carlo",
        {
          version: 1,
          request_id: randomUUID(),
          config: plans[0].config.meta,
        },
        { status: 409 },
      )
    ).error.code,
    "feature_disabled",
  );
  await call(
    root + "/preferences",
    { experimental: { monteCarloReport: true } },
    {
      method: "PATCH",
      headers: epochHeaders,
    },
  );
  const outputs = new Map();
  const calculationCheckpoint = await call(root + "/checkpoint");
  for (const plan of plans)
    outputs.set(plan.id, await simulate(root, plan.config.meta));
  assert.deepEqual(await call(root + "/checkpoint"), calculationCheckpoint);
  assert.deepEqual(
    (await call(root + "/reporting")).data.documents,
    initial.documents,
  );
  assert.deepEqual(await financialState(root), ledger);

  // Change rich inputs together; retry and restart must preserve exactly one write.
  const rich = plans.find((d) =>
    d.config.meta?.name?.startsWith("progressive"),
  );
  assert.ok(rich);
  const edited = structuredClone(rich);
  edited.config.meta.name = "Saved household plan";
  edited.config.meta.incomeStreams[0].annualAmount += 10000;
  edited.config.meta.contributions[0].annualAmount += 5000;
  edited.config.meta.spendingPhases[0].annualWithdrawal += 2000;
  edited.config.meta.taxBands[1].rate = 0.21;
  edited.config.meta.pots[0].annualFeeFixed = 1500;
  const command = { writes: [edited] };
  const headers = { ...epochHeaders, "Idempotency-Key": randomUUID() };
  const saved = await call(root + "/reporting/commands", command, { headers });
  assert.deepEqual(
    await call(root + "/reporting/commands", command, { headers }),
    saved,
  );
  const savedPlan = saved.data.documents.find((d) => d.id === rich.id);
  assert.deepEqual(savedPlan.config, edited.config);
  assert.equal(savedPlan.revision, rich.revision + 1);
  assert.equal(
    (
      await call(
        root + "/reporting/commands",
        { writes: [rich] },
        {
          headers: epochHeaders,
          status: 409,
        },
      )
    ).error.code,
    "reporting_changed",
  );
  const savedResult = await simulate(root, savedPlan.config.meta);
  assert.notDeepEqual(savedResult.result, outputs.get(rich.id).result);
  const savedState = await call(root + "/reporting");
  await server.stop();
  server = await startServer({ dataDirectory: directory });
  assert.deepEqual(await call(root + "/reporting"), savedState);
  assert.deepEqual(
    await call(root + "/reporting/commands", command, { headers }),
    saved,
  );
  const afterRestart = await simulate(root, savedPlan.config.meta);
  assert.deepEqual(afterRestart.config, savedResult.config);
  assert.deepEqual(afterRestart.result, savedResult.result);
  assert.equal(afterRestart.cache.hit, false);
  assert.deepEqual(await financialState(root), ledger);

  // Export/re-upload the native archive, then restore after a deletion.
  const backup = (
    await call(
      root + "/backups",
      { label: "Saved plans" },
      {
        status: 201,
        headers: epochHeaders,
      },
    )
  ).data;
  const bytes = await call(`${root}/backups/${backup.id}/download`);
  const restore = (await call("/restores", bytes, { status: 201 })).data;
  assert.equal(restore.status, "ready", JSON.stringify(restore));
  await call(
    root + "/reporting/commands",
    {
      writes: [{ ...savedPlan, tombstone: true }],
    },
    { headers: epochHeaders },
  );
  assert.equal(
    (await call(root + "/reporting")).data.documents.find(
      (d) => d.id === rich.id,
    ).tombstone,
    true,
  );
  await call(
    `${root}/restores/${restore.id}/activate`,
    { confirm_name: budget.name },
    {
      headers: epochHeaders,
    },
  );
  const restored = (await call(root + "/reporting")).data;
  assert.notEqual(restored.epoch, initial.epoch);
  assert.deepEqual(restored.documents, savedState.data.documents);
  assert.equal(
    (await call(root + "/preferences")).data.experimental.monteCarloReport,
    true,
  );
  assert.deepEqual(await financialState(root), ledger);
  const restoredResult = await simulate(root, savedPlan.config.meta);
  assert.deepEqual(restoredResult.config, savedResult.config);
  assert.deepEqual(restoredResult.result, savedResult.result);
  assert.equal(
    restoredResult.cache.hit,
    false,
    "Restored epoch must invalidate computed cache",
  );
  assert.equal(
    (
      await call(
        root + "/reporting/commands",
        {
          writes: [{ ...savedPlan, tombstone: true }],
        },
        { headers: epochHeaders, status: 409 },
      )
    ).error.code,
    "budget_restored",
  );
  assert.deepEqual(
    (await call(root + "/reporting")).data.documents,
    restored.documents,
  );
  console.log(
    "PASS four genuine plan imports, opt-in, read-only calculation, rich edits, immutable retry, stale revision refusal, restart, native export/upload/restore, deletion recovery and epoch isolation",
  );
} finally {
  await server?.stop();
  await rm(directory, { recursive: true, force: true });
}

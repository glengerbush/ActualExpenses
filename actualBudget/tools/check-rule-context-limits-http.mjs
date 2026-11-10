// Public command refusal, atomicity, handler cleanup and recovery on real history.
import assert from "node:assert/strict";
import { randomUUID, createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/rule-context-work-source.json", import.meta.url),
  ),
);
const bytes = await readFile(
  new URL("../tests/fixtures/" + fixture.archive.filename, import.meta.url),
);
assert.equal(
  createHash("sha256").update(bytes).digest("hex"),
  fixture.archive.sha256,
);
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
const owner = randomUUID();
async function call(path, body, status = 200, headers = {}) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
      "X-Actual-History-Owner": owner,
      ...headers,
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
    signal: AbortSignal.timeout(120000),
  });
  const value = await response.json();
  assert.equal(response.status, status, `${path}: ${JSON.stringify(value)}`);
  return value;
}
try {
  const preparation = performance.now();
  const uploaded = await call("/imports/actual", bytes, 201);
  assert.equal(uploaded.data.status, "ready");
  await call(`/imports/${uploaded.data.id}/activate`, {}, 201);
  console.log(
    `Imported source history (${Math.round(performance.now() - preparation)}ms)`,
  );
  const root = `/budgets/${uploaded.data.budget_id}`;
  const checkpoint = async () => call(root + "/checkpoint");
  let cp = await checkpoint();
  const epoch = { "X-Actual-Budget-Epoch": cp.data.epoch };
  const ids = Array.from({ length: 100 }, () => randomUUID());
  await call(
    root + "/transactions/batch",
    {
      snapshot: cp.data.snapshot,
      operations: ids.map((id) => ({
        action: "create",
        transaction: {
          id,
          account_id: fixture.account,
          amount: 0,
          date: "2026-02-01",
          notes: "Pending",
          cleared: true,
        },
      })),
    },
    200,
    epoch,
  );
  const rule = randomUUID();
  cp = await checkpoint();
  await call(
    root + "/rules",
    {
      id: rule,
      snapshot: cp.data.snapshot,
      stage: null,
      conditionsOp: "and",
      conditions: [{ field: "account", op: "is", value: fixture.account }],
      actions: [
        {
          op: "set",
          field: "notes",
          value: "",
          options: {
            formula: `=CONCATENATE("Balance ", balance, IF(BALANCE_OF("History")=BALANCE_OF("${fixture.account}"), "", " mismatch"))`,
          },
        },
      ],
    },
    201,
    epoch,
  );
  const before = await checkpoint();
  const page = `/transactions?account_id=${fixture.account}&limit=100`;
  const ledger = await call(root + page);
  const account = await call(root + "/accounts/" + fixture.account);
  assert.equal(account.data.balance, fixture.total);
  const history = await call(root + "/history", undefined, 200, epoch);
  const command = { snapshot: before.data.snapshot, ids, rule_ids: [rule] };
  const started = performance.now();
  const refused = await call(root + "/rules/apply", command, 400, epoch);
  const elapsed = performance.now() - started;
  console.log(
    `Refused oversized command (${Math.round(elapsed)}ms): ${refused.error.message}`,
  );
  assert.match(
    refused.error.message,
    /Calculated rule (database )?work limit reached/,
  );
  assert.ok(elapsed < 15000, `Refusal took ${elapsed}ms`);
  assert.deepEqual(await checkpoint(), before);
  assert.deepEqual(await call(root + page), ledger);
  assert.deepEqual(await call(root + "/accounts/" + fixture.account), account);
  assert.deepEqual(
    await call(root + "/history", undefined, 200, epoch),
    history,
  );
  const key = randomUUID();
  const small = { ...command, ids: [ids[0]] };
  const saved = await call(root + "/rules/apply", small, 200, {
    ...epoch,
    "Idempotency-Key": key,
  });
  assert.equal(
    (await call(root + "/transactions/" + ids[0])).data.notes,
    `Balance ${fixture.total}`,
  );
  assert.deepEqual(
    await call(root + "/rules/apply", small, 200, {
      ...epoch,
      "Idempotency-Key": key,
    }),
    saved,
  );
  assert.equal(
    (await call(root + "/accounts/" + fixture.account)).data.balance,
    fixture.total,
  );
  console.log(
    `PASS: cumulative database refusal, unchanged ledger/history and exact retry (${Math.round(elapsed)}ms)`,
  );
} finally {
  await server.stop();
}

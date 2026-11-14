// Authentic23 database and portable archive; no version relabel or private SQL edits.
import assert from "node:assert/strict";
import { readFile, mkdtemp, copyFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "Upgrade requires its isolated genuine database",
);
const fixture = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/public-notes-schema23.json", import.meta.url),
    ),
  ),
  source = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/public-notes-source.json", import.meta.url),
    ),
  ),
  dir = await mkdtemp(tmpdir() + "/notes-upgrade-");
await copyFile(
  new URL("../tests/fixtures/public-notes-schema23.sqlite3", import.meta.url),
  dir + "/actualbudget.sqlite3",
);
assert.equal(
  createHash("sha256")
    .update(await readFile(dir + "/actualbudget.sqlite3"))
    .digest("hex"),
  fixture.database_sha256,
);
const server = await startServer({ dataDirectory: dir }),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  root = "/budgets/" + fixture.created.data.id;
async function call(path, body, status = 200, headers = {}, method) {
  const response = await fetch(api + path, {
    method: method ?? (body === undefined ? "GET" : "POST"),
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(
    response.status,
    status,
    `${path}: ${(await response.clone().text()).slice(0, 700)}`,
  );
  return response.json();
}
function financial(value, historical = false) {
  const result = structuredClone(value);
  delete result.snapshot;
  for (const row of result.data) {
    for (const key of [
      "category_color",
      "category_group_color",
      "color_override",
      "group_sort_order",
    ])
      delete row[key];
    if (historical)
      Object.assign(row, { notes: "", cleared: true, reconciled: false });
  }
  if (historical)
    for (const key of ["balances", "filtered_balances"]) {
      result[key].cleared = result[key].total;
      result[key].uncleared = 0;
    }
  return result;
}
try {
  const register = await call(root + "/transactions");
  assert.deepEqual(financial(register), financial(fixture.register, true));
  const checkpoint = await call(root + "/checkpoint");
  assert.equal(checkpoint.data.epoch, fixture.checkpoint.data.epoch);
  assert.equal(checkpoint.data.generation, fixture.checkpoint.data.generation);
  assert.equal(checkpoint.data.snapshot, register.snapshot);
  const history = await call(
    root + "/history",
    undefined,
    200,
    fixture.owner_headers,
  );
  assert.deepEqual(
    { ...history.data, epoch: fixture.history.data.epoch },
    fixture.history.data,
    "Database migration retains owned actions while rotating the view epoch",
  );
  for (const c of fixture.commands)
    assert.deepEqual(
      await call(
        root + c.path,
        c.body,
        c.method === "POST" ? 201 : 200,
        { ...fixture.owner_headers, "Idempotency-Key": c.key },
        c.method,
      ),
      c.result,
      "Original command receipt unchanged",
    );
  const retained = fixture.stages[0],
    stageId = retained.result.data.id,
    original = await readFile(
      new URL("../tests/fixtures/" + retained.archive, import.meta.url),
    );
  const stage = (await call("/imports/" + stageId)).data;
  assert.equal(
    stage.status,
    "ready",
    "Genuine old blocked note stage receives its converter",
  );
  assert.deepEqual(
    Buffer.from(
      await (
        await fetch(api + "/imports/" + stageId + "/original.zip")
      ).arrayBuffer(),
    ),
    original,
  );
  const receipt = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": retained.key,
    },
    body: original,
  });
  assert.equal(receipt.status, 201);
  assert.deepEqual(
    await receipt.json(),
    retained.result,
    "Original blocked upload receipt remains immutable",
  );
  // Full archive note conversion is tested independently on an empty singleton.
  // The upgraded installation retains the original staged bytes and receipt above.
  const fresh = await startServer();
  try {
    const freshApi =
      fresh.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    const uploaded = await fetch(freshApi + "/imports/actual", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: original,
    });
    assert.equal(uploaded.status, 201);
    const freshStage = (await uploaded.json()).data;
    const activation = await fetch(
      freshApi + `/imports/${freshStage.id}/activate`,
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": randomUUID(),
        },
        body: "{}",
      },
    );
    assert.equal(activation.status, 201);
    const active = (await activation.json()).data;
    const note = await fetch(freshApi + `/budgets/${active.id}/notes/read`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: source.id }),
    });
    assert.equal(note.status, 200);
    assert.deepEqual((await note.json()).data, source.first);
  } finally {
    await fresh.stop();
  }
  const archive = await readFile(
    new URL("../tests/fixtures/public-notes-schema23.zip", import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(archive).digest("hex"),
    fixture.archive_sha256,
  );
  const upload = await fetch(api + "/restores", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: archive,
  });
  assert.equal(upload.status, 201, await upload.clone().text());
  const restore = (await upload.json()).data,
    budget = (await call(root)).data;
  await call(
    root + "/restores/" + restore.id + "/activate",
    { confirm_name: budget.name },
    200,
    { "X-Actual-Budget-Epoch": budget.epoch },
  );
  const restored = (await call(root)).data;
  assert.notEqual(restored.epoch, budget.epoch);
  assert.deepEqual(
    financial(await call(root + "/transactions")),
    financial(fixture.register, true),
  );
  assert.equal(
    (
      await call(root + "/history", undefined, 200, {
        ...fixture.owner_headers,
        "X-Actual-Budget-Epoch": restored.epoch,
      })
    ).data.undo,
    null,
    "Portable restore rotates epoch and excludes source history",
  );
  for (const c of fixture.commands)
    assert.deepEqual(
      await call(
        root + c.path,
        c.body,
        c.method === "POST" ? 201 : 200,
        { ...fixture.owner_headers, "Idempotency-Key": c.key },
        c.method,
      ),
      c.result,
    );
  console.log(
    "PASS genuine23→current database/blocked stage/history/exact receipts and native23 financial/epoch restore",
  );
} finally {
  await server.stop();
  await rm(dir, { recursive: true, force: true });
}

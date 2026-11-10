// Real external HTTP caller consumes genuine SDK financial outcome. Extra guards
// prove the replacement's explicit atomicity guarantee, not SDK rollback parity.
import assert from "node:assert/strict";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
import { cloneImportedTemplate } from "../web/tests/import-template.mjs";
const source = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/public-batch-source.json", import.meta.url),
  ),
);
const directory = process.env.ACTUALBUDGET_TEST_URL
  ? null
  : await mkdtemp(tmpdir() + "/public-batch-http-");
let server = await startServer(directory ? { dataDirectory: directory } : {}),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200, headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(
    r.status,
    status,
    path + " " + (await r.clone().text()).slice(0, 1000),
  );
  return r.json();
}
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + source.archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    source.archive.sha256,
  );
  const upload = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  assert.equal(upload.status, 201);
  const stage = (await upload.json()).data,
    template = (await call("/imports/" + stage.id + "/activate", {}, 201)).data,
    id = await cloneImportedTemplate(api, template.id),
    root = "/budgets/" + id,
    epoch = (await call(root)).data.epoch,
    owner = randomUUID(),
    clock =
      new Date(Date.now() + 1000).toISOString() + "-0000-4747474747474747",
    headers = {
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": epoch,
      "X-Actual-History-Owner": owner,
      "X-Actual-Edit-Clock": clock,
    },
    checkpoint = async () => (await call(root + "/checkpoint")).data,
    query = async () => {
      const cp = await checkpoint();
      return (
        await call(root + "/query", {
          version: 1,
          table: "transactions",
          splits: "all",
          select: source.fields,
          order_by: ["id"],
          snapshot: cp.snapshot,
        })
      ).data;
    },
    before = await query();
  assert.deepEqual(before, source.before);
  const boundBefore = await checkpoint(),
    boundHistory = (await call(root + "/history", undefined, 200, headers))
      .data;
  const boundRefusal = await call(
    root + "/transactions/batch",
    {
      snapshot: boundBefore.snapshot,
      max_graph_members: 2,
      operations: [
        source.operations[0],
        {
          action: "update",
          id: source.before.find((r) => r.is_parent).id,
          fields: { notes: "Must not expand unbounded children" },
        },
      ],
    },
    400,
    { ...headers, "Idempotency-Key": randomUUID() },
  );
  assert.equal(
    boundRefusal.error.code,
    "graph_limit",
    "Resource refusal comes from bounded traversal, not unsupported input syntax",
  );
  assert.deepEqual(await checkpoint(), boundBefore);
  assert.deepEqual(await query(), before);
  assert.deepEqual(
    (await call(root + "/history", undefined, 200, headers)).data,
    boundHistory,
  );
  const body = {
      snapshot: (await checkpoint()).snapshot,
      operations: source.operations,
      max_graph_members: 2,
    },
    saved = await call(root + "/transactions/batch", body, 200, headers);
  assert.equal(saved.edit_clock, clock);
  assert.equal(saved.data.operation_count, 3);
  for (const operation of source.operations.filter(
    (op) => op.action !== "delete",
  )) {
    const row = (
      await call(
        root + "/transactions/" + (operation.id ?? operation.transaction.id),
      )
    ).data;
    assert.equal(
      row.field_versions.amount,
      clock,
      "Original caller amount HLC, not a per-operation server clock",
    );
    assert.equal(row.field_versions.notes, clock);
  }
  assert.deepEqual(
    await query(),
    source.after,
    "Complete source graph after mixed SDK-equivalent command",
  );
  const sums = (
    await call(root + "/query", {
      version: 1,
      table: "transactions",
      group_by: ["account"],
      select: ["account", { total: { $sum: "$amount" } }],
      order_by: ["account"],
      snapshot: (await checkpoint()).snapshot,
    })
  ).data;
  assert.deepEqual(sums, source.sums);
  const committedCp = await checkpoint(),
    history = (await call(root + "/history", undefined, 200, headers)).data;
  assert.ok(history.undo?.id);
  assert.deepEqual(
    await call(root + "/transactions/batch", body, 200, headers),
    saved,
  );
  assert.deepEqual(
    await call(root + "/transactions/batch", body, 200, {
      ...headers,
      "X-Actual-Receipt-Only": "true",
    }),
    saved,
  );
  assert.deepEqual(await checkpoint(), committedCp);
  await call(root + "/transactions/batch", body, 409, {
    ...headers,
    "X-Actual-Budget-Epoch": randomUUID().replaceAll("-", ""),
  });
  assert.deepEqual(
    (await call(root + "/history", undefined, 200, headers)).data,
    history,
  );
  await call(root + "/history/undo", { action_id: history.undo.id }, 200, {
    "X-Actual-Budget-Epoch": epoch,
    "X-Actual-History-Owner": owner,
  });
  assert.deepEqual(
    await query(),
    source.before,
    "One undo reverses entire mixed batch",
  );
  const redo = (await call(root + "/history", undefined, 200, headers)).data
    .redo;
  await call(root + "/history/redo", { action_id: redo.id }, 200, {
    "X-Actual-Budget-Epoch": epoch,
    "X-Actual-History-Owner": owner,
  });
  assert.deepEqual(await query(), source.after);
  const now = await checkpoint(),
    rows = await call(root + "/transactions?limit=1000"),
    hist = (await call(root + "/history", undefined, 200, headers)).data,
    newId = randomUUID(),
    create = { ...source.operations[0].transaction, id: newId },
    locked = source.before.find(
      (r) => r.reconciled && !r.transfer_id && !r.is_parent && !r.is_child,
    ).id;
  const failing = {
      snapshot: now.snapshot,
      operations: [
        { action: "create", transaction: create },
        {
          action: "update",
          id: locked,
          fields: { notes: "Must not cross lock" },
        },
      ],
    },
    failureHeaders = {
      ...headers,
      "Idempotency-Key": randomUUID(),
      "X-Actual-Edit-Clock":
        new Date(Date.now() + 2000).toISOString() + "-0000-4747474747474747",
    };
  await call(root + "/transactions/batch", failing, 409, failureHeaders);
  assert.deepEqual(await call(root + "/transactions?limit=1000"), rows);
  assert.deepEqual(await checkpoint(), now);
  assert.deepEqual(
    (await call(root + "/history", undefined, 200, headers)).data,
    hist,
  );
  await call(root + "/transactions/batch", failing, 404, {
    ...failureHeaders,
    "X-Actual-Receipt-Only": "true",
  });
  const cancelBody = {
      snapshot: now.snapshot,
      operations: [{ action: "create", transaction: create }],
    },
    cancelHeaders = { ...failureHeaders, "Idempotency-Key": randomUUID() };
  await call(root + "/transactions/batch", cancelBody, 409, {
    ...cancelHeaders,
    "X-Actual-Cancel-If-Uncommitted": "true",
  });
  await call(root + "/transactions/batch", cancelBody, 409, cancelHeaders);
  assert.deepEqual(await checkpoint(), now);
  assert.deepEqual(await query(), source.after); // A fresh cursor does not authorize an older per-field intent.
  const staleBody = {
    snapshot: now.snapshot,
    operations: [
      { action: "create", transaction: create },
      {
        action: "update",
        id: source.operations[1].id,
        fields: { notes: "Old intent cannot win" },
      },
    ],
  };
  await call(root + "/transactions/batch", staleBody, 409, {
    ...headers,
    "Idempotency-Key": randomUUID(),
  });
  assert.deepEqual(await checkpoint(), now);
  assert.deepEqual(await call(root + "/transactions?limit=1000"), rows);
  assert.deepEqual(
    (await call(root + "/history", undefined, 200, headers)).data,
    hist,
  );
  // Distinct target IDs can still overlap the same reciprocal financial component.
  const pair = source.after.find((r) => r.transfer_id),
    overlap = {
      snapshot: now.snapshot,
      operations: [
        { action: "update", id: pair.id, fields: { notes: "First" } },
        { action: "update", id: pair.transfer_id, fields: { notes: "Second" } },
      ],
    };
  await call(root + "/transactions/batch", overlap, 409, {
    ...failureHeaders,
    "Idempotency-Key": randomUUID(),
  });
  assert.deepEqual(await checkpoint(), now);
  assert.deepEqual(await query(), source.after);
  assert.deepEqual(
    (await call(root + "/history", undefined, 200, headers)).data,
    hist,
  );
  await call(
    root + "/transactions/batch",
    {
      snapshot: now.snapshot,
      operations: Array.from({ length: 101 }, () => ({
        action: "delete",
        id: randomUUID(),
      })),
    },
    400,
    { ...failureHeaders, "Idempotency-Key": randomUUID() },
  );
  assert.deepEqual(await checkpoint(), now);
  // Portable restore rotates generation/history, retaining exact original receipts.
  const backup = (
      await call(
        root + "/backups",
        { label: "Public batch original receipt" },
        201,
      )
    ).data,
    archive = await (
      await fetch(api + root + "/backups/" + backup.id + "/download")
    ).arrayBuffer(),
    restoreResponse = await fetch(api + "/restores", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: archive,
    });
  assert.equal(restoreResponse.status, 201);
  const restore = (await restoreResponse.json()).data,
    name = (await call(root)).data.name;
  await call(
    root + "/restores/" + restore.id + "/activate",
    { confirm_name: name },
    200,
    { "X-Actual-Budget-Epoch": epoch },
  );
  assert.notEqual((await call(root)).data.epoch, epoch);
  const restoredCp = await checkpoint();
  assert.deepEqual(
    await call(root + "/transactions/batch", body, 200, {
      ...headers,
      "X-Actual-Receipt-Only": "true",
    }),
    saved,
  );
  assert.deepEqual(
    await call(root + "/transactions/batch", body, 200, headers),
    saved,
  );
  await call(root + "/transactions/batch", body, 409, {
    ...headers,
    "Idempotency-Key": randomUUID(),
  });
  assert.deepEqual(await checkpoint(), restoredCp);
  assert.deepEqual(await query(), source.after);
  if (directory) {
    await server.stop();
    server = await startServer({ dataDirectory: directory });
    api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    assert.deepEqual(
      await call(root + "/transactions/batch", body, 200, {
        ...headers,
        "X-Actual-Receipt-Only": "true",
      }),
      saved,
      "Exact original receipt survives process restart after restore",
    );
    assert.deepEqual(await checkpoint(), restoredCp);
    assert.deepEqual(await query(), source.after);
    console.log("PASS restart retains original receipt/graph/checkpoint");
  }
  console.log(
    "PASS source mixed graph/aggregates, atomic midfailure/stale-field/overlap/bounds, one undo/redo, original clock/epoch/receipt, portable restore and durable cancellation",
  );
} finally {
  await server.stop();
  if (directory) await rm(directory, { recursive: true, force: true });
}

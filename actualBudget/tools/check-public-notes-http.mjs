// Real SDK arbitrary-note archive consumed through public API and native restore.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
import { cloneImportedTemplate } from "../web/tests/import-template.mjs";
const source = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/public-notes-source.json", import.meta.url),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200, headers = {}) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
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
async function upload(path, bytes) {
  const r = await fetch(api + path, {
    method: "POST",
    headers: {
      "Idempotency-Key": randomUUID(),
      "Content-Type": "application/zip",
    },
    body: bytes,
  });
  assert.equal(r.status, 201, await r.clone().text());
  return (await r.json()).data;
}
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + source.archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    source.archive.sha256,
  );
  const stage = await upload("/imports/actual", bytes);
  assert.equal(
    stage.status,
    "ready",
    "Standalone source notes must not block or disappear",
  );
  assert.deepEqual(
    Buffer.from(
      await (
        await fetch(api + "/imports/" + stage.id + "/original")
      ).arrayBuffer(),
    ),
    bytes,
    "Original source bytes retained exactly",
  );
  const activated = (await call("/imports/" + stage.id + "/activate", {}, 201))
      .data,
    id = await cloneImportedTemplate(api, activated.id),
    root = "/budgets/" + id,
    ledger = await call(root + "/transactions?limit=1000"),
    epoch = (await call(root)).data.epoch;
  const fields = [
    "id",
    "account",
    "payee",
    "category",
    "amount",
    "date",
    "notes",
    "parent_id",
    "transfer_id",
    "cleared",
    "reconciled",
  ];
  const nativeRows = ledger.data
    .flatMap((r) => [r, ...(r.children ?? [])])
    .map((r) =>
      Object.fromEntries(
        fields.map((k) => [
          k,
          r[
            {
              account: "account_id",
              payee: "payee_id",
              category: "category_id",
            }[k] ?? k
          ] ?? null,
        ]),
      ),
    )
    .sort((a, b) => a.id.localeCompare(b.id));
  assert.deepEqual(
    nativeRows,
    source.financial.map((r) =>
      Object.fromEntries(fields.map((k) => [k, r[k] ?? null])),
    ),
    "Exact genuine source financial rows",
  );
  const read = () => call(root + "/notes/read", { id: source.id });
  assert.equal((await read()).data.note, source.first.note);
  assert.equal(
    (await call(root + "/notes/read", { id: "absent:note" })).data,
    source.missing,
  );
  const body = {
      id: source.id,
      note: source.updated.note,
      snapshot: (await call(root + "/checkpoint")).data.snapshot,
    },
    key = randomUUID(),
    clock =
      new Date(Date.now() + 1000).toISOString() + "-0000-4747474747474747",
    headers = {
      "X-Actual-Edit-Clock": clock,
      "Idempotency-Key": key,
      "X-Actual-Budget-Epoch": epoch,
      "X-Actual-History-Owner": randomUUID(),
    },
    saved = await call(root + "/notes/commands", body, 200, headers);
  assert.equal(saved.data.note, source.updated.note);
  assert.equal(saved.edit_clock, clock, "Original caller HLC retained exactly");
  const changes = await call(
    root + "/changes?cursor=" + encodeURIComponent(body.snapshot),
  );
  assert.ok(
    changes.data.some(
      (r) =>
        r.entity === "notes" &&
        r.id === source.id &&
        r.data.note === source.updated.note &&
        r.data.field_versions.note === clock,
    ),
    "Incremental feed exposes the committed note",
  );
  assert.deepEqual(
    await call(root + "/notes/commands", body, 200, {
      ...headers,
      "X-Actual-Receipt-Only": "true",
    }),
    saved,
  );
  assert.equal((await read()).data.note, source.updated.note);
  const owned = {
      "X-Actual-History-Owner": headers["X-Actual-History-Owner"],
      "X-Actual-Budget-Epoch": epoch,
    },
    history = (await call(root + "/history", undefined, 200, owned)).data;
  assert.ok(history.undo?.id);
  await call(
    root + "/history/undo",
    { action_id: history.undo.id },
    200,
    owned,
  );
  assert.equal((await read()).data.note, source.first.note);
  const redo = (await call(root + "/history", undefined, 200, owned)).data.redo;
  await call(root + "/history/redo", { action_id: redo.id }, 200, owned);
  assert.equal((await read()).data.note, source.updated.note);
  const guarded = (await call(root + "/checkpoint")).data;
  await call(
    root + "/notes/commands",
    { ...body, snapshot: guarded.snapshot, note: "Must not overwrite" },
    409,
    { ...owned, "X-Actual-Edit-Clock": saved.edit_clock },
  );
  assert.deepEqual(
    (await call(root + "/checkpoint")).data,
    guarded,
    "Equal/older clock refusal changes nothing",
  );
  const cancelBody = {
      id: source.id,
      note: "Canceled note",
      snapshot: guarded.snapshot,
    },
    cancelKey = randomUUID(),
    cancelHeaders = { ...owned, "Idempotency-Key": cancelKey };
  await call(root + "/notes/commands", cancelBody, 409, {
    ...cancelHeaders,
    "X-Actual-Cancel-If-Uncommitted": "true",
  });
  await call(root + "/notes/commands", cancelBody, 409, cancelHeaders);
  assert.equal((await read()).data.note, source.updated.note);
  const createdId = "new:automation-note",
    createdBody = {
      id: createdId,
      note: "Fresh caller note",
      snapshot: (await call(root + "/checkpoint")).data.snapshot,
    };
  await call(root + "/notes/commands", createdBody, 200, owned);
  const createdAction = (await call(root + "/history", undefined, 200, owned))
    .data.undo;
  await call(
    root + "/history/undo",
    { action_id: createdAction.id },
    200,
    owned,
  );
  assert.equal(
    (await call(root + "/notes/read", { id: createdId })).data,
    null,
    "Undo creation restores absence, not hidden retained text",
  );
  const createRedo = (await call(root + "/history", undefined, 200, owned)).data
    .redo;
  await call(root + "/history/redo", { action_id: createRedo.id }, 200, owned);
  assert.equal(
    (await call(root + "/notes/read", { id: createdId })).data.note,
    createdBody.note,
  );
  const categoryId = source.financial.find((r) => r.category)?.category,
    categories = (await call(root + "/categories")).data,
    category = categories.find((c) => c.id === categoryId);
  assert.ok(category);
  await call(
    root + "/notes/commands",
    {
      id: categoryId,
      note: "Caller category annotation",
      snapshot: (await call(root + "/checkpoint")).data.snapshot,
    },
    200,
    owned,
  );
  assert.equal(
    (await call(root + "/categories")).data.find((c) => c.id === categoryId)
      .notes,
    "Caller category annotation",
    "Known SDK note identity updates its authoritative category field",
  );
  const knownUndo = (await call(root + "/history", undefined, 200, owned)).data
    .undo;
  await call(root + "/history/undo", { action_id: knownUndo.id }, 200, owned);
  assert.equal(
    (await call(root + "/categories")).data.find((c) => c.id === categoryId)
      .notes,
    category.notes,
  );
  const fresh = {
    id: source.id,
    note: source.cleared.note,
    snapshot: (await call(root + "/checkpoint")).data.snapshot,
  };
  await call(root + "/notes/commands", fresh, 200, {
    "X-Actual-Budget-Epoch": epoch,
  });
  assert.equal((await read()).data.note, source.cleared.note);
  const backup = (await call(root + "/backups", { label: "Caller notes" }, 201))
      .data,
    archive = Buffer.from(
      await (
        await fetch(api + root + "/backups/" + backup.id + "/download")
      ).arrayBuffer(),
    ),
    restore = await upload("/restores", archive),
    budget = (await call(root)).data;
  await call(
    root + "/restores/" + restore.id + "/activate",
    { confirm_name: budget.name },
    200,
    { "X-Actual-Budget-Epoch": epoch },
  );
  assert.equal((await read()).data.note, source.cleared.note);
  assert.deepEqual(
    await call(root + "/notes/commands", body, 200, headers),
    saved,
    "Original old-epoch receipt replays after restore",
  );
  const financial = ({ snapshot, ...rest }) => rest;
  assert.deepEqual(
    financial(await call(root + "/transactions?limit=1000")),
    financial(ledger),
  );
  console.log(
    "PASS genuine arbitrary notes import/read/update/clear, exact receipt/native roundtrip and unchanged financial ledger",
  );
} finally {
  await server.stop();
}

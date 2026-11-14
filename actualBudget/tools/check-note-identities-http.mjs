// Genuine source missing/cleared identity semantics through running API and native archive.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
import { cloneImportedTemplate } from "../web/tests/import-template.mjs";
const source = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/note-identities" +
          (process.argv.includes("--prefix") ? "-prefix" : "") +
          "-source.json",
        import.meta.url,
      ),
    ),
  ),
  server = await startServer(),
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
    `${path}: ${(await r.clone().text()).slice(0, 700)}`,
  );
  return r.json();
}
async function upload(path, bytes) {
  const r = await fetch(api + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  assert.equal(r.status, 201, await r.clone().text());
  return (await r.json()).data;
}
async function imported(archive) {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    archive.sha256,
  );
  const stage = await upload("/imports/actual", bytes),
    active = (await call("/imports/" + stage.id + "/activate", {}, 201)).data;
  return "/budgets/" + (await cloneImportedTemplate(api, active.id));
}
const retired = new Set(
  source.identities
    .filter(([kind]) => ["category", "group", "month"].includes(kind))
    .map(([, id]) => id),
);
async function state(root, expected, label) {
  const cp = (await call(root + "/checkpoint")).data;
  for (const entry of expected.reads)
    assert.deepEqual(
      (await call(root + "/notes/read", { id: entry.id })).data,
      retired.has(entry.id) ? null : entry.value,
      label + "/" + entry.kind,
    );
  const rows = await call(root + "/query", {
    version: 1,
    table: "notes",
    select: ["id", "note"],
    filter: [{ id: { $oneof: source.identities.map((x) => x[1]) } }],
    order_by: ["id"],
    snapshot: cp.snapshot,
  });
  assert.deepEqual(
    rows.data,
    expected.rows.filter((row) => !retired.has(row.id)),
    label + " exact source q(notes) identities",
  );
  assert.deepEqual(
    (await call(root + "/checkpoint")).data,
    cp,
    "Reads never fabricate empty note rows",
  );
}
try {
  const root = await imported(source.input),
    epoch = (await call(root)).data.epoch,
    ledger = await call(root + "/transactions?limit=1000");
  for (const c of source.cases) {
    if (c.change)
      await call(
        root + "/notes/commands",
        {
          ...c.change,
          snapshot: (await call(root + "/checkpoint")).data.snapshot,
        },
        retired.has(c.change.id) ? 400 : 200,
        { "X-Actual-Budget-Epoch": epoch },
      );
    await state(root, c.state, c.id);
  }
  if (!process.argv.includes("--prefix")) {
    const owner = {
      "X-Actual-History-Owner": randomUUID(),
      "X-Actual-Budget-Epoch": epoch,
    };
    for (const note of ["", "Owned category prose"]) {
      const id = randomUUID();
      await call(
        root + "/accounts",
        {
          id,
          name: "Owned note " + id,
          opening_balance: 0,
          opening_date: "2026-11-09",
        },
        201,
        owner,
      );
      const creation = (await call(root + "/history", undefined, 200, owner))
        .data.undo;
      const rows = async () => {
        const cp = (await call(root + "/checkpoint")).data;
        const result = await call(root + "/query", {
          version: 1,
          table: "notes",
          select: ["id", "note"],
          filter: [{ id }],
          snapshot: cp.snapshot,
        });
        assert.deepEqual((await call(root + "/checkpoint")).data, cp);
        return result.data;
      };
      assert.deepEqual(await rows(), []);
      await call(
        root + "/notes/commands",
        {
          id,
          note,
          snapshot: (await call(root + "/checkpoint")).data.snapshot,
        },
        200,
        owner,
      );
      assert.deepEqual(await rows(), [{ id, note }]);
      const action = (await call(root + "/history", undefined, 200, owner)).data
        .undo;
      await call(root + "/history/undo", { action_id: action.id }, 200, owner);
      assert.deepEqual(await rows(), []);
      assert.equal((await call(root + "/notes/read", { id })).data, null);
      const redo = (await call(root + "/history", undefined, 200, owner)).data
        .redo;
      await call(root + "/history/redo", { action_id: redo.id }, 200, owner);
      assert.deepEqual(await rows(), [{ id, note }]);
      await call(root + "/history/undo", { action_id: action.id }, 200, owner);
      assert.deepEqual(await rows(), []);
      assert.equal(
        (await call(root + "/history", undefined, 200, owner)).data.undo.id,
        creation.id,
      );
      await call(
        root + "/history/undo",
        { action_id: creation.id },
        200,
        owner,
      );
      assert.deepEqual(await rows(), []);
    }
  }
  const final = source.cases.at(-1).state;
  const sourceCopy = await imported(source.archive);
  await state(sourceCopy, final, "genuine cleared source import");
  const point = (
      await call(root + "/backups", { label: "Cleared note identities" }, 201)
    ).data,
    bytes = Buffer.from(
      await (
        await fetch(api + root + "/backups/" + point.id + "/download")
      ).arrayBuffer(),
    ),
    restore = await upload("/restores", bytes),
    budget = (await call(root)).data;
  await call(
    root + "/restores/" + restore.id + "/activate",
    { confirm_name: budget.name },
    200,
    { "X-Actual-Budget-Epoch": epoch },
  );
  await state(root, final, "native cleared identities");
  const financial = ({ snapshot, ...rest }) => rest;
  assert.deepEqual(
    financial(await call(root + "/transactions?limit=1000")),
    financial(ledger),
  );
  console.log(
    "PASS" +
      source.cases.length +
      " genuine known/standalone missing→written→cleared states, imported empty identities and native roundtrip",
  );
} finally {
  await server.stop();
}

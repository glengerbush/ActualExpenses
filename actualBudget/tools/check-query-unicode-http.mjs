import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const source = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/query-unicode-source.json", import.meta.url),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = "POST", status = 200) {
  const r = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
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
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + source.input.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    source.input.sha256,
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
    active = (await call("/imports/" + stage.id + "/activate", {}, "POST", 201))
      .data,
    b = active.id,
    root = "/budgets/" + b;
  for (const c of source.cases) {
    await call(
      root + "/notes/commands",
      {
        id: source.id,
        note: c.notes,
        snapshot: (await call(root + "/checkpoint", undefined, "GET")).data
          .snapshot,
      },
      "POST",
    );
    const cp = (await call(root + "/checkpoint", undefined, "GET")).data,
      ledger = await call(root + "/transactions?limit=1000", undefined, "GET");
    const result = await call(
      root + "/query",
      {
        version: 1,
        table: "notes",
        select: ["id", "note"],
        filter: [{ id: source.id, note: { [c.op]: c.pattern } }],
        order_by: ["id"],
        snapshot: cp.snapshot,
      },
      "POST",
      c.error ? 400 : 200,
    );
    if (!c.error)
      assert.deepEqual(
        result.data,
        c.data.map(({ notes, ...row }) => ({ ...row, note: notes })),
        c.id,
      );
    assert.deepEqual(
      (await call(root + "/checkpoint", undefined, "GET")).data,
      cp,
    );
    assert.deepEqual(
      await call(root + "/transactions?limit=1000", undefined, "GET"),
      ledger,
    );
    console.log("PASS " + c.id);
  }
} finally {
  await server.stop();
}

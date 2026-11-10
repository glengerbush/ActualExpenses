// Consume independently executed ActualQL observations through running HTTP.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/public-query-source.json", import.meta.url),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function request(path, body, status = 200, headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(r.status, status, `${path} ${await r.clone().text()}`);
  return r.json();
}
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + fixture.archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    fixture.archive.sha256,
  );
  const response = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Idempotency-Key": randomUUID(),
      "Content-Type": "application/zip",
    },
    body: bytes,
  });
  assert.equal(response.status, 201, await response.clone().text());
  const stage = (await response.json()).data,
    activated = (await request("/imports/" + stage.id + "/activate", {}, 201))
      .data,
    root = "/budgets/" + activated.id,
    before = await request(root + "/transactions?limit=1000"),
    checkpoint = (await request(root + "/checkpoint")).data;
  for (const item of fixture.cases) {
    const selected = process.argv
      .find((a) => a.startsWith("--case="))
      ?.slice(7);
    if (selected && item.id !== selected) continue;
    if (["scalar-functions", "literal-unicode"].includes(item.id)) continue; // Baseline first-key-only and raw literal bugs are retained, not accepted as a native SQL escape.
    const q = item.query,
      body = {
        version: 1,
        table: q.table,
        select: q.selectExpressions,
        filter: q.filterExpressions,
        group_by: q.groupExpressions,
        order_by: q.orderExpressions,
        calculation: q.calculation,
        splits: q.tableOptions.splits ?? "inline",
        include_deleted: q.withDead,
        validate_refs: q.validateRefs,
        raw: q.rawMode,
        limit: q.limit ?? 1000,
        offset: q.offset ?? 0,
        snapshot: checkpoint.snapshot,
      };
    const result = await request(root + "/query", body, item.error ? 400 : 200);
    if (!item.error) {
      assert.deepEqual(
        result.data,
        item.data,
        item.id + " exact source query output",
      );
      assert.equal(result.snapshot, checkpoint.snapshot);
      assert.equal(typeof result.epoch, "string");
    }
    console.log("PASS " + item.id);
  }
  assert.deepEqual(await request(root + "/transactions?limit=1000"), before);
  assert.deepEqual(
    (await request(root + "/checkpoint")).data,
    checkpoint,
    "Read-only query never advances mutation state",
  );
} finally {
  await server.stop();
}

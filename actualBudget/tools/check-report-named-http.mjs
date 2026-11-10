// Shared report condition semantics against unchanged pinned spreadsheet output.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const source = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/custom-report-named-source-26.10.json",
        import.meta.url,
      ),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(r.status, status, await r.clone().text());
  return r.json();
}
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + source.source_archive, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    source.source_sha256,
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
    budget = (await call("/imports/" + stage.id + "/activate", {}, 201)).data,
    root = "/budgets/" + budget.id,
    before = await call(root + "/transactions?limit=1000"),
    cp = (await call(root + "/checkpoint")).data,
    failures = [];
  for (const item of source.cases) {
    const label = JSON.stringify(item.options.conditions[0].customName);
    try {
      const result = (
        await call(root + "/reports/query", {
          version: 1,
          kind: "custom",
          as_of: "2026-11-09",
          options: item.options,
        })
      ).data.result;
      delete result.legend;
      assert.deepEqual(
        result,
        item.expected,
        "Exact source full custom spreadsheet marker " + label,
      );
      console.log("PASS customName " + label);
    } catch (e) {
      failures.push(label);
      console.error("FAIL " + label + ": " + String(e).slice(0, 1000));
    }
  }
  assert.deepEqual(await call(root + "/transactions?limit=1000"), before);
  assert.deepEqual((await call(root + "/checkpoint")).data, cp);
  assert.deepEqual(failures, []);
  console.log(
    "PASS8 source JS truthiness report markers, financial/checkpoint unchanged",
  );
} finally {
  await server.stop();
}

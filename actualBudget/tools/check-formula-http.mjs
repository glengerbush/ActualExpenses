// Whole production-hook observations through the running replacement API.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
import { cloneImportedTemplate } from "../web/tests/import-template.mjs";
const edges = process.argv.includes("--edges"),
  formatting = process.argv.includes("--formatting"),
  budget = process.argv.includes("--budget"),
  fixture = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/" +
          (formatting
            ? "formula-formatting-source.json"
            : process.argv.includes("--clock")
              ? "formula-clock-source.json"
              : edges
                ? "formula-edges-source.json"
                : budget
                  ? "formula-budget-source.json"
                  : "formula-source.json"),
        import.meta.url,
      ),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  filter = process.argv.find((a) => a.startsWith("--case="))?.slice(7);
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
  assert.equal(r.status, status, path + " " + (await r.clone().text()));
  return r.json();
}
async function check(archive, observations) {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    archive.sha256,
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
    template = (await call("/imports/" + stage.id + "/activate", {}, 201)).data
      .id,
    b = await cloneImportedTemplate(api, template),
    root = "/budgets/" + b,
    before = await call(root + "/checkpoint"),
    ledger = await call(root + "/transactions");
  let count = 0;
  for (const item of observations.filter((item) =>
    process.argv.includes("--limits")
      ? item.name === "spreadsheet-sum"
      : !filter || new RegExp(filter).test(item.name),
  )) {
    const input = {
        formula: item.formula,
        queries: item.queries ?? {},
        named_expressions: item.namedExpressions ?? {},
        snapshot: before.data.snapshot,
        as_of: "2026-11-09",
        evaluated_at:
          item.evaluated_at ?? fixture.clock ?? "2026-11-09T12:00:00Z",
        ...(item.locale ? { locale: item.locale } : {}),
      },
      output = await call(root + "/reports/formula", input);
    assert.equal(output.data.snapshot, before.data.snapshot);
    if (item.tolerance) {
      assert.equal(typeof output.data.output.result, "number");
      assert.equal(output.data.output.error, item.output.error);
      assert.ok(
        Math.abs(output.data.output.result - item.output.result) <=
          item.tolerance * Math.max(1, Math.abs(item.output.result)),
        item.name,
      );
    } else assert.deepEqual(output.data.output, item.output, item.name);
    count++;
  }
  assert.ok(count > 0, "No matching source observations");
  const valid = {
    formula: "=SUM(1,2,3)",
    queries: {},
    named_expressions: {},
    snapshot: before.data.snapshot,
    as_of: "2026-11-09",
  };
  const stale =
    valid.snapshot.slice(0, -1) + (valid.snapshot.endsWith("0") ? "1" : "0");
  await call(root + "/reports/formula", { ...valid, snapshot: stale }, 409);
  for (const invalid of [
    { ...valid, formula: "=" + "1".repeat(8192) },
    { ...valid, named_expressions: { Bad: { nested: "value" } } },
    {
      ...valid,
      queries: Object.fromEntries(
        Array.from({ length: 33 }, (_, i) => ["Q" + i, {}]),
      ),
    },
    { ...valid, locale: "a".repeat(101) },
  ])
    await call(root + "/reports/formula", invalid, 400);
  await call(root + "/reports/formula", valid, 400, {
    "X-Actual-Receipt-Only": "true",
  });
  if (edges) {
    const bounded = await call(
      root + "/reports/formula",
      {
        ...valid,
        formula: '=QUERY_EXTRACT_CATEGORIES("Bounded")',
        queries: {
          Bounded: {
            conditions: [
              { field: "category", op: "matches", value: "a".repeat(2049) },
            ],
          },
        },
      },
      400,
    );
    assert.equal(bounded.error.code, "regex_complexity_limit");
    assert.deepEqual(
      (await call(root + "/reports/formula", valid)).data.output,
      { result: 6, error: null },
    );
  }

  if (process.argv.includes("--limits")) {
    const response = await fetch(api + root + "/reports/formula", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ...valid,
          formula: "=MMULT(SEQUENCE(1000,1000),SEQUENCE(1000,1000))",
        }),
      }),
      result = await response.json(),
      message =
        response.status === 200
          ? result.data.output.error
          : result.error?.message;
    assert.ok([200, 400].includes(response.status));
    assert.match(
      message ?? "",
      /Formula engine failed|memory|interrupt|limit|allocation/i,
      "Large supported spreadsheet must fail safely with an actionable diagnostic",
    );
    assert.deepEqual(
      (await call(root + "/reports/formula", valid)).data.output,
      { result: 6, error: null },
      "Failure cannot poison the next fresh context",
    );
    assert.deepEqual(
      (
        await call(root + "/reports/formula", {
          ...valid,
          formula: "=NOT_A_FUNCTION(1)",
        })
      ).data.output,
      fixture.observations.find((o) => o.name === "unknown-function").output,
    );
  }
  assert.deepEqual(await call(root + "/checkpoint"), before);
  assert.deepEqual(await call(root + "/transactions"), ledger);
  console.log(
    "PASS production formula source " + count + "; read-only ledger/checkpoint",
  );
}
try {
  if (budget || edges || formatting) {
    const scenario = process.argv
      .find((a) => a.startsWith("--scenario="))
      ?.slice(11);
    const cases = fixture.cases.filter(
      (c) => !scenario || new RegExp(scenario).test(c.id ?? c.method),
    );
    assert.ok(cases.length > 0, "No matching scenarios");
    for (const c of cases) await check(c.archive, c.observations);
  } else
    await check(
      {
        filename: fixture.source.archive,
        sha256: fixture.source.archive_sha256,
      },
      fixture.observations,
    );
} finally {
  await server.stop();
}

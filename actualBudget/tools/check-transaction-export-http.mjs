// The first normal export is captured verbatim from Actual's real CSV writer.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const { parse: parseCSV } = require("csv-parse/sync");
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
// Preserve every source CSV cell except the three explicitly retired entry fields.
function completedCSV(csv) {
  if (!csv) return csv;
  const rows = parseCSV(csv, { record_delimiter: "\n" }),
    header = rows[0],
    detailed = header.includes("Split_Amount");
  for (const row of rows.slice(1))
    for (const [field, value] of [
      ["Notes", ""],
      ["Cleared", detailed ? "Cleared" : "1"],
      ["Reconciled", ""],
    ]) {
      const index = header.indexOf(field);
      if (index >= 0)
        row[index] =
          field === "Notes" && detailed
            ? (row[index].match(/^\(SPLIT (?:INTO \d+|\d+ OF \d+)\) /)?.[0] ??
              "")
            : value;
    }
  return (
    rows
      .map((row) =>
        row
          .map((cell) =>
            /[",\r\n]/.test(cell)
              ? '"' + cell.replaceAll('"', '""') + '"'
              : cell,
          )
          .join(","),
      )
      .join("\n") + "\n"
  );
}
let f = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-import-manual.json",
        import.meta.url,
      ),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Idempotency-Key": randomUUID(),
      "Content-Type": "application/json",
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.ok(r.ok, `${path}: ${r.status} ${await r.clone().text()}`);
  return r.json();
}
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + f.archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    f.archive.sha256,
  );
  const r = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  assert.equal(r.status, 201);
  const stage = (await r.json()).data;
  stage.budget_id = (await call(`/imports/${stage.id}/activate`, {})).data.id;
  const root = `/budgets/${stage.budget_id}`;
  const upload = await fetch(
    `${api}${root}/accounts/${f.ids.account}/transaction-imports?filename=bank.csv`,
    {
      method: "POST",
      headers: { "Idempotency-Key": randomUUID() },
      body: f.input.text,
    },
  );
  assert.equal(upload.status, 201);
  const path = root + "/transaction-imports/" + (await upload.json()).data.id;
  const cp = (await call(root + "/checkpoint")).data;
  const mapping = Object.fromEntries(
    Object.entries(f.input.mapping).map(([key, value]) => [
      key === "inOut" ? "in_out" : key,
      value,
    ]),
  );
  const body = {
    snapshot: cp.snapshot,
    mapping,
    options: { has_header_row: true },
    rows: [{ row: 0, action: "merge", match_id: f.ids.manual }],
  };
  const committed = await fetch(api + path + "/commit", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": cp.epoch,
    },
    body: JSON.stringify(body),
  });
  assert.equal(committed.status, 200, await committed.clone().text());
  const before = await call(root + "/transactions"),
    checkpoint = (await call(root + "/checkpoint")).data;
  const exported = await fetch(api + root + "/transactions/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      snapshot: checkpoint.snapshot,
      account_id: f.ids.account,
      layout: "standard",
    }),
  });
  assert.equal(exported.status, 200, await exported.clone().text());
  assert.match(exported.headers.get("Content-Type"), /^text\/csv/);
  assert.equal(
    await exported.text(),
    completedCSV(f.exported),
    "Source CSV financial cells and escaping with completed status and retired notes",
  );
  assert.deepEqual(await call(root + "/transactions"), before);
  assert.deepEqual((await call(root + "/checkpoint")).data, checkpoint);
  console.log(
    "PASS genuine normal CSV export with no financial or checkpoint mutation",
  );
  const source = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-export-source.json",
        import.meta.url,
      ),
    ),
  );
  const original = await readFile(
    new URL("../tests/fixtures/" + source.archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(original).digest("hex"),
    source.archive.sha256,
  );

  await server.stop();
  server = await startServer();
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  const uploaded = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: original,
  });
  assert.equal(uploaded.status, 201);
  const stage2 = (await uploaded.json()).data;
  assert.ok(["ready", "activated"].includes(stage2.status));
  stage2.budget_id = (await call(`/imports/${stage2.id}/activate`, {})).data.id;
  const root2 = `/budgets/${stage2.budget_id}`;
  const financial = await call(root2 + "/transactions"),
    saved = (await call(root2 + "/checkpoint")).data;
  const failures = [];
  for (const originalCase of source.cases) {
    const item = structuredClone(originalCase);
    if (item.id === "detailed-cleared")
      item.csv = source.cases.find((c) => c.id === "detailed-account").csv;
    if (item.id.endsWith("-empty"))
      item.request.query = {
        filter: {
          conditions: [{ field: "amount", op: "eq", value: 999999999 }],
        },
      };
    const response = await fetch(api + root2 + "/transactions/export", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ snapshot: saved.snapshot, ...item.request }),
    });
    try {
      assert.equal(response.status, 200, await response.clone().text());
      const csv = await response.text();
      const expected = completedCSV(item.csv);
      assert.equal(
        csv,
        expected,
        "Exact source CSV cells except retired notes/status",
      );
      const originalRows = parseCSV(expected, { columns: true }),
        actualRows = parseCSV(csv, { columns: true });
      assert.deepEqual(
        actualRows,
        originalRows,
        "Independent parser preserves every retained row/cell",
      );
      assert.deepEqual(await call(root2 + "/transactions"), financial);
      assert.deepEqual((await call(root2 + "/checkpoint")).data, saved);
      console.log("PASS " + item.id);
    } catch (error) {
      failures.push(item.id);
      console.error("FAIL " + item.id + ": " + error.message);
    }
  }
  assert.equal(failures.length, 0, failures.join(","));
  for (const invalid of [
    { layout: "unknown" },
    { ids: [source.ids.first, source.ids.first] },
    { ids: Array.from({ length: 10001 }, () => randomUUID()) },
    {
      query: {
        filter: {
          conditions: [{ field: "unsupported", op: "eq", value: "x" }],
        },
      },
    },
  ]) {
    const rejected = await fetch(api + root2 + "/transactions/export", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ snapshot: saved.snapshot, ...invalid }),
    });
    assert.equal(rejected.status, 400);
    assert.deepEqual(await call(root2 + "/transactions"), financial);
    assert.deepEqual((await call(root2 + "/checkpoint")).data, saved);
  }
  const longPayee = randomUUID();
  await call(root2 + "/payees", { id: longPayee, name: "a".repeat(119) + "!" });
  const changed = await fetch(
    api + root2 + "/transactions/" + source.ids.first,
    {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body: JSON.stringify({ payee_id: longPayee }),
    },
  );
  assert.equal(changed.status, 200);
  const currentFinancial = await call(root2 + "/transactions"),
    currentCheckpoint = (await call(root2 + "/checkpoint")).data;
  const stale = await fetch(api + root2 + "/transactions/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ snapshot: saved.snapshot }),
  });
  assert.equal(stale.status, 409);
  const bounded = await fetch(api + root2 + "/transactions/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      snapshot: currentCheckpoint.snapshot,
      query: {
        filter: {
          conditions: [
            { field: "payee_name", op: "matches", value: "^(a+)+$" },
          ],
        },
      },
    }),
  });
  assert.equal(bounded.status, 400);
  assert.equal(
    (await bounded.json()).error.code,
    "regex_work_limit",
    "Exhausted shared work refuses instead of emitting a partial/empty CSV",
  );
  assert.deepEqual(await call(root2 + "/transactions"), currentFinancial);
  assert.deepEqual((await call(root2 + "/checkpoint")).data, currentCheckpoint);
  console.log(
    "PASS export selection/bounds/stale/shared-regex refusals preserve state",
  );
} finally {
  await server.stop();
}

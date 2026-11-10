// Execute the unchanged production three-pass hook with real pinned engine DTOs.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { formulaSourceRuntime } from "./formula-source-runtime.mjs";
process.env.TZ = "UTC";
const NativeDate = Date,
  clock = "2026-11-09T12:00:00.000Z";
globalThis.Date = class extends NativeDate {
  constructor(...args) {
    super(...(args.length ? args : [clock]));
  }
  static now() {
    return NativeDate.parse(clock);
  }
};
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/formula-source-"),
  file = new URL("../tests/fixtures/formula-source.json", import.meta.url),
  archive = await readFile(
    new URL("../tests/fixtures/reports-source-26.10.zip", import.meta.url),
  ),
  original = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/reports-source-26.10.json", import.meta.url),
    ),
  ),
  sha = (v) => createHash("sha256").update(v).digest("hex");
assert.equal(sha(archive), original.archive_sha256);
try {
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(archive, { filename: "formula-source.zip" });
  const runtime = formulaSourceRuntime(api, engine),
    accounts = await api.getAccounts(),
    all = { All: { conditions: [] } },
    food = {
      Food: {
        conditions: [{ field: "category", op: "is", value: original.ids.food }],
        timeFrame: { mode: "sliding-window", start: "2026-10", end: "2026-11" },
      },
    },
    inputs = [
      { name: "spreadsheet-sum", formula: "=SUM(1,2,3)" },
      { name: "query-sum", formula: '=QUERY("All")', queries: all },
      { name: "query-count", formula: '=QUERY_COUNT("All")', queries: all },
      { name: "query-filtered", formula: '=QUERY("Food")', queries: food },
      {
        name: "query-filtered-count",
        formula: '=QUERY_COUNT("Food")',
        queries: food,
      },
      { name: "query-missing", formula: '=QUERY("Absent")' },
      { name: "query-case-sensitive", formula: '=QUERY("all")', queries: all },
      {
        name: "query-composition",
        formula: '=ROUND(QUERY("All")/QUERY_COUNT("All"),2)',
        queries: all,
      },
      {
        name: "balance-name",
        formula:
          "=BALANCE_OF(" +
          JSON.stringify(
            accounts.find((a) => a.id === original.ids.checking).name,
          ) +
          ")",
      },
      {
        name: "balance-id",
        formula: `=BALANCE_OF("${original.ids.checking}")`,
      },
      { name: "balance-missing", formula: '=BALANCE_OF("Missing")' },
      {
        name: "categories-count",
        formula: '=COUNTA(QUERY_EXTRACT_CATEGORIES("Food"))',
        queries: food,
      },
      {
        name: "timeframe-start",
        formula: '=QUERY_EXTRACT_TIMEFRAME_START("Food")',
        queries: food,
      },
      {
        name: "timeframe-end",
        formula: '=QUERY_EXTRACT_TIMEFRAME_END("Food")',
        queries: food,
      },
      {
        name: "missing-timeframe",
        formula: '=QUERY_EXTRACT_TIMEFRAME_START("Absent")',
      },
      { name: "format-number", formula: "=FORMATNUMBER(12345.678,2)" },
      {
        name: "format-separators",
        formula: '=FORMATNUMBER(-12345.678,3," ",",")',
      },
      { name: "format-currency", formula: '=FORMATCURRENCY(-1234.56,"€",2)' },
      {
        name: "fixed-extra-argument-error",
        formula: "=FIXED(1234.567,2,TRUE)",
      },
      { name: "fixed", formula: "=FIXED(1234.567,2)" },
      { name: "minor-to-display", formula: "=INTEGER_TO_AMOUNT(7299)" },
      {
        name: "named-values",
        formula: "=ROUND(Rate*Base,2)",
        namedExpressions: { Rate: 0.075, Base: 1234.56 },
      },
      {
        name: "conditional-color",
        formula: "=IF(RESULT>0,theme_positive,theme_negative)",
        namedExpressions: {
          RESULT: 1204,
          theme_positive: "#123456",
          theme_negative: "#654321",
        },
      },
      {
        name: "unicode-comparison",
        formula: '=IF("é"="e","same","different")',
      },
      { name: "unicode-ligature", formula: '=IF("ß"="ss","same","different")' },
      {
        name: "lookup-boolean-name-error",
        formula: "=VLOOKUP(2,{1,10;2,20;3,30},2,FALSE)",
      },
      { name: "lookup", formula: "=VLOOKUP(2,{1,10;2,20;3,30},2,FALSE())" },
      { name: "statistics", formula: "=STDEV(1,2,3,4,5)", tolerance: 1e-12 },
      {
        name: "net-present-value",
        formula: "=NPV(0.1,100,200,300)",
        tolerance: 1e-12,
      },
      {
        name: "date-leap-year",
        formula: '=TEXT(DATE(2024,2,29),"yyyy-mm-dd")',
      },
      { name: "if-error", formula: '=IFERROR(1/0,"unavailable")' },
      { name: "divide-zero", formula: "=1/0" },
      { name: "unknown-function", formula: "=NOT_A_FUNCTION(1)" },
      { name: "invalid-syntax", formula: "=SUM(" },
      { name: "missing-equals", formula: "SUM(1,2)" },
    ],
    observations = [];
  for (const dimension of [
    "budgeted",
    "spent",
    "goal",
    "balance_start",
    "balance_end",
  ])
    inputs.push({
      name: "budget-" + dimension,
      formula: `=BUDGET_QUERY("${dimension}",QUERY_EXTRACT_CATEGORIES("Food"),QUERY_EXTRACT_TIMEFRAME_START("Food"),QUERY_EXTRACT_TIMEFRAME_END("Food"))`,
      queries: food,
    });
  for (const input of inputs)
    observations.push({
      ...input,
      output: await runtime.evaluate(
        input.formula,
        input.queries,
        input.namedExpressions,
        accounts,
      ),
    });
  assert.deepEqual(
    observations[0].output,
    { result: 6, error: null },
    "Production hook must actually evaluate, not settle with a harness error",
  );
  const output = {
    source: {
      commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
      api: "26.10.0",
      hyperformula: require("hyperformula").HyperFormula.version,
      clock,
      archive: "reports-source-26.10.zip",
      archive_sha256: sha(archive),
      module_hashes: runtime.hashes,
    },
    observations,
  };
  if (process.argv.includes("--check"))
    assert.deepEqual(output, JSON.parse(await readFile(file)));
  else {
    let previous;
    try {
      previous = JSON.parse(await readFile(file));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    if (previous) {
      assert.ok(
        process.argv.includes("--extend"),
        "Existing source observations are immutable; use --check or append-only --extend",
      );
      for (const row of previous.observations)
        assert.deepEqual(
          observations.find((next) => next.name === row.name),
          row,
          row.name,
        );
      for (const [key, value] of Object.entries(previous.source.module_hashes))
        assert.equal(output.source.module_hashes[key], value, key);
      assert.equal(
        output.source.archive_sha256,
        previous.source.archive_sha256,
      );
    }
    await writeFile(file, JSON.stringify(output, null, 2) + "\n");
  }
  console.log("PASS genuine production formula hook " + observations.length);
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

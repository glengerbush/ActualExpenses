// Saved source widget's actual range/settings, unchanged projection and CSV.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { sourceRuntime } from "./reports-source-runtime.mjs";
process.env.TZ = "UTC";
const NativeDate = Date,
  clock = "2026-11-09T12:00:00Z";
globalThis.Date = class extends NativeDate {
  constructor(...args) {
    super(...(args.length ? args : [clock]));
  }
  static now() {
    return NativeDate.parse(clock);
  }
};
const api = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  )("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/analysis-widget-source-"),
  fixtures = new URL("../tests/fixtures/", import.meta.url),
  path = new URL("budget-analysis-widget-source-26.10.json", fixtures),
  cases = [];
try {
  const engine = await api.init({ dataDir: directory });
  for (const method of ["envelope", "tracking"]) {
    const stem = "budget-analysis-" + method + "-source-26.10",
      base = JSON.parse(
        await readFile(new URL(stem + ".json", fixtures), "utf8"),
      ),
      bytes = await readFile(new URL(stem + ".zip", fixtures));
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      base.archive_sha256,
    );
    await api.importBudget(bytes, { filename: "saved-analysis.zip" });
    const runtime = sourceRuntime(api, engine),
      months = runtime.core("shared/months"),
      ranges = runtime.load("reportRanges"),
      projection = runtime.load("spreadsheets/budget-analysis-spreadsheet"),
      exporter = runtime.load("spreadsheets/budget-analysis-export"),
      meta = base.widgets[0].meta,
      [start, end] = ranges.calculateTimeRange(meta.timeFrame);
    const options = {
      startDate: months.monthFromDate(start) + "-01",
      endDate: months.getMonthEnd(months.monthFromDate(end) + "-01"),
      conditions: meta.conditions,
      conditionsOp: meta.conditionsOp,
      showHiddenCategories: meta.showHiddenCategories,
    };
    let result;
    await projection.createBudgetAnalysisSpreadsheet(options)(
      {},
      (value) => (result = JSON.parse(JSON.stringify(value))),
    );
    cases.push({
      method,
      archive_sha256: base.archive_sha256,
      widget: base.widgets[0],
      options,
      result,
      csv: exporter.buildBudgetAnalysisCsv(result.intervalData),
      source_files: runtime.hashes,
    });
  }
  const output = JSON.parse(
    JSON.stringify({
      oracle:
        "Saved26.10 widget settings/range through unchanged frontend projection/exporter and real engine",
      clock,
      cases,
    }),
  );
  if (process.argv.includes("--check"))
    assert.deepEqual(output, JSON.parse(await readFile(path, "utf8")));
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS two saved-widget source ranges, conditions, visibility, financial outputs and CSV",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

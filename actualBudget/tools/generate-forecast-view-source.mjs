// Pinned forecast + unchanged chart projection: saved day-shaped view over full-month inputs.
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
  constructor(...a) {
    super(...(a.length ? a : [clock]));
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
  fixtures = new URL("../tests/fixtures/", import.meta.url),
  directory = await mkdtemp(tmpdir() + "/forecast-view-source-");
try {
  const engine = await api.init({ dataDir: directory }),
    base = JSON.parse(
      await readFile(
        new URL("forecast-scenarios-source-26.10.json", fixtures),
        "utf8",
      ),
    ),
    original = JSON.parse(
      await readFile(new URL("reports-source-26.10.json", fixtures), "utf8"),
    ),
    bytes = await readFile(
      new URL("forecast-scenarios-source-26.10.zip", fixtures),
    );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    base.archive_sha256,
  );
  await api.importBudget(bytes, { filename: "forecast-view-source.zip" });
  const runtime = sourceRuntime(api, engine),
    charts = runtime.load("reports/balanceForecastChartData");
  const viewOptions = {
    startDate: "2026-11-01",
    endDate: "2026-11-30",
    accountIds: [original.ids.checking, original.ids.savings],
    source: "schedules",
    includeAccountlessSchedules: false,
    viewStart: "2026-11-15",
    viewEnd: "2026-11-16",
  };
  const result = await engine.send("forecast/generate", viewOptions),
    views = {};
  for (const granularity of ["Daily", "Monthly"]) {
    const args = {
        forecastData: result,
        start: viewOptions.viewStart,
        end: viewOptions.viewEnd,
        granularity,
      },
      chart = charts.buildBalanceForecastChartData(args);
    views[granularity] = {
      chart,
      scheduled_occurrence_count:
        charts.countForecastScheduledOccurrences(args),
      lowest_displayed: charts.getLowestChartDataPoint(chart) ?? null,
    };
  }
  // A supported source SDK category can have an income flag in an expense group.
  // Forecast totals follow source group membership, not the category flag alone.
  const mutation = {
    month: "2027-01",
    category: original.ids.salary,
    amount: 10000,
  };
  await api.setBudgetAmount(mutation.month, mutation.category, mutation.amount);
  await api.getBudgetMonth(mutation.month);
  const groupOptions = {
      startDate: "2027-01-01",
      endDate: "2027-01-31",
      source: "tracking-budget",
    },
    groupExpected = await engine.send("forecast/generate", groupOptions);
  const output = JSON.parse(
    JSON.stringify({
      oracle:
        "Whole26.10 forecast handler and unchanged balanceForecastChartData.ts",
      clock,
      base_sha256: base.archive_sha256,
      source_files: runtime.hashes,
      view: { options: viewOptions, result, views },
      group_plan: { mutation, options: groupOptions, expected: groupExpected },
    }),
  );
  const path = new URL("forecast-view-source-26.10.json", fixtures);
  if (process.argv.includes("--check"))
    assert.deepEqual(output, JSON.parse(await readFile(path, "utf8")));
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS whole-month forecast/day viewport carry-forward, monthly display/counts and source group-based tracking allocation",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

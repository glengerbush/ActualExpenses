// Genuine source witness: valid final-month category data after ten funded months.
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
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  fixtures = new URL("../tests/fixtures/", import.meta.url),
  check = process.argv.includes("--check"),
  directory = await mkdtemp(tmpdir() + "/analysis-large-source-"),
  jsonPath = new URL("budget-analysis-large-source-26.10.json", fixtures),
  zipPath = new URL("budget-analysis-large-source-26.10.zip", fixtures),
  hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
try {
  const engine = await api.init({ dataDir: directory }),
    base = JSON.parse(
      await readFile(new URL("reports-source-26.10.json", fixtures), "utf8"),
    ),
    prior = check ? JSON.parse(await readFile(jsonPath, "utf8")) : null;
  const bytes = await readFile(
    check ? zipPath : new URL("reports-source-26.10.zip", fixtures),
  );
  assert.equal(hash(bytes), check ? prior.archive_sha256 : base.archive_sha256);
  await api.importBudget(bytes, { filename: "analysis-large-source.zip" });
  let ids = prior?.ids;
  if (!check) {
    await engine.send("preferences/save", {
      id: "flags.budgetAnalysisReport",
      value: "true",
    });
    const group = await api.createCategoryGroup({
        name: "Large analysis expenses",
      }),
      incomeGroup = (await api.getCategoryGroups()).find(
        (group) => group.is_income,
      );
    ids = {
      account: await api.createAccount({ name: "Large funded months" }, 0),
      expense: await api.createCategory({
        name: "Large ordinary expense",
        group_id: group,
        is_income: false,
      }),
      income: await api.createCategory({
        name: "Large ordinary income",
        group_id: incomeGroup.id,
        is_income: true,
      }),
    };
    for (let i = 1; i <= 10; i++) {
      const month = "2026-" + String(i).padStart(2, "0");
      await api.setBudgetAmount(month, ids.expense, 1e15);
      await api.addTransactions(ids.account, [
        {
          id: `36360000-0000-4000-9000-${String(i * 2 - 1).padStart(12, "0")}`,
          date: month + "-02",
          amount: 1e15,
          category: ids.income,
        },
        {
          id: `36360000-0000-4000-9000-${String(i * 2).padStart(12, "0")}`,
          date: month + "-03",
          amount: -1e15,
          category: ids.expense,
        },
      ]);
    }
    await writeFile(zipPath, await api.exportBudget(), { flag: "wx" });
  }
  const runtime = sourceRuntime(api, engine),
    source = runtime.load("spreadsheets/budget-analysis-spreadsheet"),
    options = {
      startDate: "2026-10-01",
      endDate: "2026-10-31",
      conditions: [{ field: "category", op: "is", value: ids.expense }],
      conditionsOp: "and",
    };
  let expected;
  await source.createBudgetAnalysisSpreadsheet(options)(
    {},
    (value) => (expected = JSON.parse(JSON.stringify(value))),
  );
  assert.equal(expected.totalBudgeted, 1e15);
  assert.equal(expected.totalSpent, -1e15);
  assert.equal(expected.intervalData[0].balance, 0);
  const output = JSON.parse(
    JSON.stringify({
      oracle:
        "Unchanged26.10 createBudgetAnalysisSpreadsheet with ten real funded months",
      clock,
      archive_sha256: hash(await readFile(zipPath)),
      base_sha256: base.archive_sha256,
      source_files: runtime.hashes,
      ids,
      options,
      expected,
    }),
  );
  if (check) assert.deepEqual(output, prior);
  else
    await writeFile(jsonPath, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS genuine ten-month funded source: final analysis +1e15/-1e15, zero carry/balance",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

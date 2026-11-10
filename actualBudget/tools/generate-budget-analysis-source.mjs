// Unchanged pinned frontend projection/export over real source budget DTOs.
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
  directory = await mkdtemp(tmpdir() + "/analysis-source-");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex"),
  plain = (x) => JSON.parse(JSON.stringify(x));
try {
  const engine = await api.init({ dataDir: directory }),
    base = JSON.parse(
      await readFile(new URL("reports-source-26.10.json", fixtures), "utf8"),
    );
  for (const method of ["envelope", "tracking"]) {
    const stem = "budget-analysis-" + method + "-source-26.10",
      jsonPath = new URL(stem + ".json", fixtures),
      archivePath = new URL(stem + ".zip", fixtures),
      prior = check ? JSON.parse(await readFile(jsonPath, "utf8")) : null;
    const ids = check ? prior.ids : { ...base.ids };
    const bytes = await readFile(
      check ? archivePath : new URL("reports-source-26.10.zip", fixtures),
    );
    assert.equal(
      hash(bytes),
      check ? prior.archive_sha256 : base.archive_sha256,
    );
    await api.importBudget(bytes, { filename: stem + ".zip" });
    if (!check) {
      await engine.send("preferences/save", {
        id: "budgetType",
        value: method,
      });
      await api.importBudget(await api.exportBudget(), {
        filename: "analysis-method-reload.zip",
      });
      await engine.send("preferences/save", {
        id: "flags.budgetAnalysisReport",
        value: "true",
      });
      await api.updateCategory(ids.food, { name: "Food café ? %" });
      const incomeGroup = (await api.getCategoryGroups()).find(
        (g) => g.is_income,
      );
      ids.incomeGroupExpense = await api.createCategory({
        name: "Analysis expense in income group",
        group_id: incomeGroup.id,
        is_income: false,
      });
      const mixed = (await api.getCategories()).find(
        (c) => c.id === ids.incomeGroupExpense,
      );
      assert.equal(mixed.is_income, false);
      assert.equal(mixed.group_id, incomeGroup.id);
      await api.setBudgetAmount("2026-11", ids.incomeGroupExpense, 743);
      for (const [month, category, amount] of [
        ["2026-09", "food", 3000],
        ["2026-10", "food", 200],
        ["2026-11", "food", 1000],
        ["2026-12", "food", 1200],
        ["2026-10", "empty", 1000],
        ["2026-11", "empty", 200],
        ["2026-11", "hidden", 500],
      ])
        await api.setBudgetAmount(month, ids[category], amount);
      for (const [month, category, flag] of [
        ["2026-10", "food", true],
        ["2026-11", "food", false],
        ["2026-10", "hidden", true],
        ["2026-11", "hidden", true],
      ])
        await api.setBudgetCarryover(month, ids[category], flag);
      await api.addTransactions(ids.checking, [
        {
          id: "36360000-0000-4000-8000-000000000001",
          date: "2026-12-05",
          amount: -2500,
          category: ids.empty,
          payee: ids.market,
          notes: "Analysis overspending without carryover",
        },
        {
          id: "36360000-0000-4000-8000-000000000002",
          date: "2027-01-05",
          amount: 125,
          category: ids.food,
          payee: ids.market,
          notes: "Analysis category refund",
        },
        {
          id: "36360000-0000-4000-8000-000000000003",
          date: "2026-11-23",
          amount: -213,
          category: ids.incomeGroupExpense,
          payee: ids.market,
          notes: "Supported source expense-category/income-group membership",
        },
      ]);
      await engine.send("dashboard-add-widget", {
        dashboard_page_id: ids.dashboard,
        type: "budget-analysis-card",
        x: 0,
        y: 12,
        width: 12,
        height: 4,
        meta: {
          name: "Saved budget analysis",
          timeFrame: { start: "2026-10", end: "2027-03", mode: "static" },
          interval: "Monthly",
          graphType: "Line",
          showBalance: false,
          balanceOnly: true,
          showHiddenCategories: true,
          conditions: [
            { field: "category_group", op: "contains", value: "Expenses" },
          ],
          conditionsOp: "and",
        },
      });
      await writeFile(archivePath, await api.exportBudget(), { flag: "wx" });
    }
    const runtime = sourceRuntime(api, engine),
      projection = runtime.load("spreadsheets/budget-analysis-spreadsheet"),
      exporter = runtime.load("spreadsheets/budget-analysis-export");
    const baseCategories = (await engine.send("get-categories")).list.filter(
      (category) => projection.isBaseCategory(category, false),
    );
    const detailQuery = api
      .q("transactions")
      .filter({
        category: { $oneof: baseCategories.map((category) => category.id) },
        "account.offbudget": false,
        is_parent: false,
        tombstone: false,
      })
      .filter({ date: { $gte: "2026-11-01" } })
      .filter({ date: { $lte: "2026-11-30" } })
      .select(["id", "date", "amount", "account", "category", "payee", "notes"])
      .options({ splits: "inline" });
    const details = plain({
      month: "2026-11",
      query: detailQuery.serialize(),
      rows: (await api.aqlQuery(detailQuery)).data.sort((a, b) =>
        a.id.localeCompare(b.id),
      ),
    });

    const general = { startDate: "2026-10-01", endDate: "2027-03-31" },
      food = { field: "category", op: "is", value: ids.food },
      empty = { field: "category", op: "is", value: ids.empty };
    const options =
      method === "envelope"
        ? [
            general,
            {
              ...general,
              startDate: "2026-11-01",
              endDate: "2026-11-30",
              conditions: [food],
            },
            { ...general, startDate: "2026-12-01", conditions: [empty] },
            { ...general, showHiddenCategories: true },
            {
              ...general,
              conditions: [
                { field: "category_group", op: "contains", value: "EXPENSES" },
                { field: "category", op: "doesNotContain", value: "hidden" },
              ],
            },
            {
              ...general,
              conditions: [
                food,
                { field: "category", op: "is", value: ids.hidden },
              ],
              conditionsOp: "or",
              showHiddenCategories: true,
            },
            {
              ...general,
              conditions: [
                { field: "category", op: "matches", value: "^FOOD" },
                { field: "amount", op: "gt", value: 99999999 },
                {
                  field: "category",
                  op: "is",
                  value: ids.empty,
                  customName: "Named filter ignored",
                },
              ],
            },
            {
              ...general,
              conditions: [{ field: "category", op: "contains", value: "? %" }],
            },
            {
              ...general,
              conditions: [{ field: "category", op: "matches", value: "[" }],
            },
            {
              ...general,
              conditions: [
                { field: "category", op: "matches", value: "x".repeat(257) },
              ],
            },
          ]
        : [
            general,
            {
              ...general,
              startDate: "2026-11-01",
              endDate: "2026-11-30",
              conditions: [food],
            },
            { ...general, startDate: "2026-12-01", conditions: [empty] },
            { ...general, showHiddenCategories: true },
          ];
    const cases = [];
    for (const option of options) {
      let result;
      await projection.createBudgetAnalysisSpreadsheet(option)(
        {},
        (value) => (result = plain(value)),
      );
      assert.ok(result);
      cases.push({
        options: option,
        expected: result,
        csv: exporter.buildBudgetAnalysisCsv(result.intervalData),
      });
    }
    assert.equal(
      details.rows.reduce((sum, row) => sum + row.amount, 0),
      cases[0].expected.intervalData.find((row) => row.date === details.month)
        .spent,
      "Independent source ledger detail total agrees with source month cells",
    );
    const dto = { categories: await engine.send("get-categories"), months: {} };
    for (const month of [
      "2026-09",
      "2026-10",
      "2026-11",
      "2026-12",
      "2027-01",
      "2027-02",
      "2027-03",
    ])
      dto.months[month] = await engine.send("envelope-budget-month", { month });
    const widgets = (
        await api.aqlQuery(
          api
            .q("dashboard")
            .filter({ type: "budget-analysis-card" })
            .select("*"),
        )
      ).data,
      output = plain({
        oracle:
          "Unchanged26.10 createBudgetAnalysisSpreadsheet +buildBudgetAnalysisCsv; real get-categories/envelope-budget-month DTOs",
        clock,
        method,
        ids,
        source_files: runtime.hashes,
        archive_sha256: hash(await readFile(archivePath)),
        base_sha256: base.archive_sha256,
        widgets,
        dto,
        cases,
        details,
      });
    if (check) assert.deepEqual(output, prior);
    else
      await writeFile(jsonPath, JSON.stringify(output, null, 2) + "\n", {
        flag: "wx",
      });
    console.log(
      "PASS independent " +
        method +
        " analysis: " +
        cases.length +
        " projections/CSV, saved opted-in config, genuine source monthly DTOs",
    );
  }
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

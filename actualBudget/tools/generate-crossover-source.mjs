// Complete unchanged pinned Crossover handler, genuine exported engine/AQL data.
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
  directory = await mkdtemp(tmpdir() + "/crossover-source-"),
  fixtures = new URL("../tests/fixtures/", import.meta.url),
  archivePath = new URL("crossover-source-26.10.zip", fixtures),
  jsonPath = new URL("crossover-source-26.10.json", fixtures),
  check = process.argv.includes("--check"),
  hash = (bytes) => createHash("sha256").update(bytes).digest("hex"),
  plain = (value) => JSON.parse(JSON.stringify(value));
try {
  const engine = await api.init({ dataDir: directory });
  const prior = check ? JSON.parse(await readFile(jsonPath, "utf8")) : null;
  const base = JSON.parse(
    await readFile(
      new URL("budget-analysis-envelope-source-26.10.json", fixtures),
      "utf8",
    ),
  );
  const bytes = await readFile(
    check
      ? archivePath
      : new URL("budget-analysis-envelope-source-26.10.zip", fixtures),
  );
  assert.equal(hash(bytes), check ? prior.archive_sha256 : base.archive_sha256);
  await api.importBudget(bytes, { filename: "crossover-source.zip" });
  const ids = check ? prior.ids : { ...base.ids };
  if (!check) {
    ids.investment = await api.createAccount({
      name: "Crossover investment",
      offbudget: true,
    });
    ids.zero = await api.createAccount({
      name: "Crossover zero start",
      offbudget: false,
    });
    ids.crossoverExpense = await api.createCategory({
      name: "Crossover expenses café",
      group_id: ids.expenses,
      is_income: false,
    });
    const months = Array.from(
      { length: 12 },
      (_, i) =>
        `${i < 2 ? 2025 : 2026}-${String(((i + 10) % 12) + 1).padStart(2, "0")}`,
    );
    const expenseAmounts = [
      -10000, -12000, -11000, -10000, -9000, -300000, -10000, -11000, -12000,
      -9000, -10000, -11000,
    ];
    await api.addTransactions(
      ids.checking,
      months.map((month, i) => ({
        id: `38380000-0000-4000-8000-${String(i + 1).padStart(12, "0")}`,
        date: month + "-12",
        amount: expenseAmounts[i],
        category: ids.crossoverExpense,
        payee: ids.market,
        notes: "Crossover historical expense " + month,
      })),
    );
    await api.addTransactions(
      ids.investment,
      months.map((month, i) => ({
        id: `38380000-0000-4000-8000-${String(i + 101).padStart(12, "0")}`,
        date: month + "-15",
        amount: i === 0 ? 1000000 : i === 6 ? -50000 : 25000,
        payee: ids.employer,
        notes: "Investment balance change includes contributions",
      })),
    );
    await api.addTransactions(ids.zero, [
      {
        id: "38380000-0000-4000-8000-000000000201",
        date: "2026-02-01",
        amount: 100000,
        payee: ids.employer,
      },
      {
        id: "38380000-0000-4000-8000-000000000202",
        date: "2026-10-01",
        amount: 100000,
        payee: ids.employer,
      },
    ]);
    await engine.send("dashboard-add-widget", {
      dashboard_page_id: ids.dashboard,
      type: "crossover-card",
      x: 0,
      y: 24,
      width: 12,
      height: 6,
      meta: {
        name: "Saved crossover café",
        expenseCategoryIds: [ids.crossoverExpense, ids.hidden],
        incomeAccountIds: [ids.investment, ids.zero],
        timeFrame: { start: "2025-11", end: "2026-10", mode: "static" },
        safeWithdrawalRate: 0.04,
        estimatedReturn: 0.05,
        expectedContribution: 10000,
        projectionType: "hampel",
        expenseAdjustmentFactor: 1.1,
        showHiddenCategories: false,
      },
    });
    await writeFile(archivePath, await api.exportBudget(), { flag: "wx" });
    await api.importBudget(await readFile(archivePath), {
      filename: "crossover-reload.zip",
    });
  }
  const traces = [],
    seen = new Set();
  const runtime = sourceRuntime(
    {
      ...api,
      aqlQuery: async (query) => {
        const args = query.serialize(),
          result = await api.aqlQuery(query),
          key = JSON.stringify(args);
        if (!seen.has(key)) {
          seen.add(key);
          traces.push(plain({ query: args, result }));
        }
        return result;
      },
    },
    engine,
  );
  const handler = runtime.load("spreadsheets/crossover-spreadsheet");
  const options = {
    start: "2025-11",
    end: "2026-10",
    expenseCategoryIds: [ids.crossoverExpense],
    incomeAccountIds: [ids.investment, ids.zero],
    safeWithdrawalRate: 0.04,
    estimatedReturn: 0.05,
    expectedContribution: 10000,
    projectionType: "hampel",
    expenseAdjustmentFactor: 1.1,
  };
  const definitions = [
    ["saved-hampel", {}],
    ["mean-contribution", { projectionType: "mean" }],
    [
      "median-no-adjustment",
      { projectionType: "median", expenseAdjustmentFactor: 1 },
    ],
    [
      "historical-growth",
      { estimatedReturn: null, expectedContribution: null },
    ],
    [
      "first-nonzero-cagr",
      {
        incomeAccountIds: [ids.zero],
        estimatedReturn: null,
        expectedContribution: null,
      },
    ],
    [
      "negative-growth-no-crossover",
      { estimatedReturn: -0.25, expectedContribution: -10000 },
    ],
    ["empty-accounts", { incomeAccountIds: [] }],
    ["empty-expenses", { expenseCategoryIds: [] }],
    [
      "zero-withdrawal-nonfinite",
      { expenseCategoryIds: [], safeWithdrawalRate: 0 },
    ],
    [
      "selected-closed-offbudget",
      {
        incomeAccountIds: [ids.closed, ids.brokerage],
        expenseCategoryIds: [ids.food, ids.hidden],
        start: "2026-09",
      },
    ],
  ];
  const cases = [];
  for (const [name, patch] of definitions) {
    const params = { ...options, ...patch };
    let raw;
    const before = traces.length;
    await handler.createCrossoverSpreadsheet(params)(
      {},
      (value) => (raw = value),
    );
    const nonfinite = [];
    function visit(value, path = "") {
      if (typeof value === "number" && !Number.isFinite(value))
        nonfinite.push(path);
      else if (value && typeof value === "object")
        for (const [key, item] of Object.entries(value))
          visit(item, path + "/" + key);
    }
    visit(raw);
    cases.push({
      name,
      options: params,
      result: plain(raw),
      nonfinite_paths: nonfinite,
      new_queries: traces.length - before,
    });
  }
  assert.equal(cases.find((c) => c.name === "empty-accounts").new_queries, 0);
  assert(
    cases
      .find((c) => c.name === "zero-withdrawal-nonfinite")
      .nonfinite_paths.includes("/targetNestEgg"),
  );
  assert.notDeepEqual(cases[0].result, cases[1].result);
  const output = plain({
    oracle:
      "Whole unchanged pinned26.10 Crossover handler over genuine engine/AQL and original exported records",
    clock,
    archive_sha256: hash(await readFile(archivePath)),
    ids,
    widgets: (
      await api.aqlQuery(
        api.q("dashboard").filter({ type: "crossover-card" }).select("*"),
      )
    ).data,
    categories: await api.getCategories(),
    accounts: await api.getAccounts(),
    source_files: runtime.hashes,
    cases,
    traces,
  });
  if (check) assert.deepEqual(output, prior);
  else
    await writeFile(jsonPath, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    `PASS ${cases.length} whole-source Crossover results, ${traces.length} genuine AQL queries, nonfinite and empty-selection observations`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

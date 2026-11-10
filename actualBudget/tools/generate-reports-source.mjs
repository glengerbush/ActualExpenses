// Independent Actual26.10 financial/date expectations. No replacement endpoint
// or replacement arithmetic is loaded to generate these results.
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
  { enUS } = require("date-fns/locale");
const plain = (value) => JSON.parse(JSON.stringify(value));
const directory = await mkdtemp(tmpdir() + "/reports-oracle-"),
  check = process.argv.includes("--check");
const outputPath = new URL(
    "../tests/fixtures/reports-source-26.10.json",
    import.meta.url,
  ),
  archivePath = new URL(
    "../tests/fixtures/reports-source-26.10.zip",
    import.meta.url,
  );
try {
  const engine = await api.init({ dataDir: directory });
  const ids = {};
  await api.runImport("Historical reports baseline", async () => {
    for (const [name, amount, offbudget] of [
      ["checking", 10000, false],
      ["savings", 5000, false],
      ["brokerage", 100000, true],
      ["closed", 2000, false],
    ])
      ids[name] = await api.createAccount({ name, offbudget }, amount);
    ids.expenses = await api.createCategoryGroup({
      name: "Expenses",
      is_income: false,
    });
    ids.income = await api.createCategoryGroup({
      name: "Report income",
      is_income: true,
    });
    ids.hiddenGroup = await api.createCategoryGroup({
      name: "Hidden group",
      is_income: false,
    });
    for (const [name, group, income] of [
      ["food", "expenses", false],
      ["hidden", "expenses", false],
      ["empty", "expenses", false],
      ["hiddenGroupCategory", "hiddenGroup", false],
      ["salary", "income", true],
    ])
      ids[name] = await api.createCategory({
        name,
        group_id: ids[group],
        is_income: income,
      });
    ids.market = await api.createPayee({ name: "Market" });
    ids.employer = await api.createPayee({ name: "Employer" });
  });
  for (const name of ["checking", "savings", "brokerage", "closed"]) {
    const [opening] = await api.getTransactions(
      ids[name],
      "1900-01-01",
      "2100-01-01",
    );
    ids["opening_" + name] = opening.id;
    await api.updateTransaction(opening.id, { date: "2026-10-01" });
  }
  await api.updateCategory(ids.hidden, { name: "hidden", hidden: true });
  await api.updateCategoryGroup(ids.hiddenGroup, {
    name: "Hidden group",
    hidden: true,
  });
  await api.updateAccount(ids.closed, { name: "closed", closed: true });
  const fixed = (n) => `33340000-0000-4000-8000-${String(n).padStart(12, "0")}`;
  const payees = await api.getPayees(),
    transferTo = (name) => payees.find((p) => p.transfer_acct === ids[name]).id;
  await api.addTransactions(
    ids.checking,
    [
      {
        id: fixed(1),
        date: "2026-10-02",
        amount: -1000,
        payee: ids.market,
        category: ids.food,
        notes: "Previous period food",
      },
      {
        id: fixed(2),
        date: "2026-11-01",
        amount: 10000,
        payee: ids.employer,
        category: ids.salary,
      },
      {
        id: fixed(3),
        date: "2026-11-02",
        amount: -1200,
        payee: ids.market,
        category: ids.food,
      },
      {
        id: fixed(4),
        date: "2026-11-03",
        amount: 200,
        payee: ids.market,
        category: ids.food,
        notes: "Refund",
      },
      {
        id: fixed(5),
        date: "2026-11-04",
        amount: -300,
        notes: "Uncategorized",
      },
      {
        id: fixed(6),
        date: "2026-11-05",
        amount: -700,
        payee: ids.market,
        category: ids.hidden,
      },
      {
        id: fixed(7),
        date: "2026-11-06",
        amount: -400,
        payee: ids.market,
        category: ids.hiddenGroupCategory,
      },
      {
        id: fixed(8),
        date: "2026-11-07",
        amount: -1500,
        payee: ids.market,
        subtransactions: [
          { id: fixed(9), amount: -500, category: ids.food },
          { id: fixed(10), amount: -1000, category: ids.hidden },
        ],
      },
      {
        id: fixed(11),
        date: "2026-11-08",
        amount: -2000,
        payee: transferTo("savings"),
        notes: "Internal transfer",
      },
      {
        id: fixed(12),
        date: "2026-11-08",
        amount: -2500,
        payee: transferTo("brokerage"),
        category: ids.food,
        notes: "Boundary transfer",
      },
      {
        id: fixed(13),
        date: "2026-11-20",
        amount: -800,
        payee: ids.market,
        category: ids.food,
        notes: "Future dated recorded entry",
      },
    ],
    { runTransfers: true },
  );
  await api.addTransactions(ids.brokerage, [
    {
      id: fixed(14),
      date: "2026-11-03",
      amount: -900,
      payee: ids.market,
      notes: "Off budget purchase",
    },
  ]);
  const rows = (
    await api.aqlQuery(
      api.q("transactions").options({ splits: "all" }).select("*"),
    )
  ).data;
  const internalPeer = rows.find((t) => t.transfer_id === fixed(11));
  assert.ok(internalPeer);
  await api.updateTransaction(internalPeer.id, { date: "2026-12-02" });
  await api.setBudgetAmount("2026-10", ids.food, 2000);
  await api.setBudgetAmount("2026-11", ids.food, 3000);
  const runtime = sourceRuntime(api, engine),
    options = runtime.load("ReportOptions"),
    { q } = runtime.core("shared/query");
  const accounts = await api.getAccounts(),
    categories = {
      list: await api.getCategories(),
      grouped: await api.getCategoryGroups(),
    },
    allPayees = await api.getPayees();
  const capture = async (fn) => {
    let result;
    await fn({}, (data) => {
      result = plain(data);
    });
    assert.notEqual(result, undefined);
    return result;
  };
  const format = (value) => String(value);
  const cases = [];
  for (const concise of [false, true])
    cases.push({
      kind: "cash-flow",
      options: { start: "2026-10", end: "2026-11", isConcise: concise },
      expected: await capture(
        runtime
          .load("spreadsheets/cash-flow-spreadsheet")
          .cashFlowByDate(
            "2026-10",
            "2026-11",
            concise,
            [],
            "and",
            enUS,
            format,
          ),
      ),
    });
  for (const interval of ["Daily", "Weekly", "Monthly", "Yearly"])
    for (const includeOff of [false, true]) {
      const selected = accounts.filter((a) => includeOff || !a.offbudget);
      cases.push({
        kind: "net-worth",
        options: {
          start: "2026-11",
          end: "2026-12",
          interval,
          accounts: selected.map((a) => a.id),
        },
        expected: await capture(
          runtime
            .load("spreadsheets/net-worth-spreadsheet")
            .createSpreadsheet(
              "2026-11",
              "2026-12",
              selected,
              [],
              "and",
              enUS,
              interval,
              "1",
              format,
            ),
        ),
      });
    }
  for (const mode of [
    "sum",
    "avgPerTransact",
    "avgPerMonth",
    "avgPerYear",
    "percentage",
  ])
    cases.push({
      kind: "summary",
      options: {
        start: "2026-10",
        end: "2026-11",
        summary: {
          type: mode,
          ...(mode === "percentage"
            ? {
                divisorConditions: [
                  { field: "amount", op: "gt", type: "number", value: 0 },
                ],
                divisorConditionsOp: "and",
                divisorAllTimeDateRange: false,
              }
            : {}),
        },
      },
      expected: await capture(
        runtime
          .load("spreadsheets/summary-spreadsheet")
          .summarySpreadsheet(
            "2026-10",
            "2026-11",
            [],
            "and",
            {
              type: mode,
              ...(mode === "percentage"
                ? {
                    divisorConditions: [
                      { field: "amount", op: "gt", type: "number", value: 0 },
                    ],
                    divisorConditionsOp: "and",
                    divisorAllTimeDateRange: false,
                  }
                : {}),
            },
            enUS,
          ),
      ),
    });
  for (const firstDay of ["0", "1"])
    cases.push({
      kind: "calendar",
      options: {
        start: "2026-11",
        end: "2026-11",
        first_day_of_week: firstDay,
      },
      expected: await capture(
        runtime
          .load("spreadsheets/calendar-spreadsheet")
          .calendarSpreadsheet("2026-11", "2026-11", [], "and", firstDay),
      ),
    });
  for (const averageRange of [
    { mode: "last-n-months", months: 3 },
    { mode: "year-to-date" },
    { mode: "all-time" },
  ])
    cases.push({
      kind: "spending",
      options: { compare: "2026-11", compareTo: "2026-10", averageRange },
      expected: await capture(
        runtime
          .load("spreadsheets/spending-spreadsheet")
          .createSpendingSpreadsheet({
            compare: "2026-11",
            compareTo: "2026-10",
            conditionsOp: "and",
            averageRange,
          }),
      ),
    });
  const customBase = {
    startDate: "2026-10",
    endDate: "2026-11",
    interval: "Monthly",
    categories,
    payees: allPayees,
    accounts,
    conditions: [],
    conditionsOp: "and",
    showEmpty: false,
    showOffBudget: false,
    showHiddenCategories: true,
    showUncategorized: true,
    trimIntervals: false,
    groupBy: "Category",
    balanceTypeOp: "totalDebts",
    sortByOp: "desc",
    graphType: "BarGraph",
    firstDayOfWeekIdx: "1",
  };
  const customCases = [
    ...[
      "totalAssets",
      "totalDebts",
      "totalTotals",
      "netAssets",
      "netDebts",
      "totalBudgeted",
    ].map((balanceTypeOp) => ({ balanceTypeOp })),
    ...["Group", "CategoryGroup", "Payee", "Account", "Interval"].map(
      (groupBy) => ({ groupBy }),
    ),
    { showOffBudget: true },
    { showHiddenCategories: false },
    { showUncategorized: false },
    { showEmpty: true },
    { trimIntervals: true },
    ...["Daily", "Weekly", "Yearly"].map((interval) => ({
      interval,
      startDate: interval === "Yearly" ? "2026" : "2026-10-01",
      endDate: interval === "Yearly" ? "2026" : "2026-11-30",
    })),
    {
      conditions: [
        { field: "category", op: "is", type: "id", value: ids.food },
      ],
    },
  ];
  for (const changes of customCases) {
    const input = { ...customBase, ...changes };
    const { categories: c, payees: p, accounts: a, ...configuration } = input;
    cases.push({
      kind: "custom",
      options: configuration,
      expected: await capture(
        runtime
          .load("spreadsheets/custom-spreadsheet")
          .createCustomSpreadsheet(input),
      ),
    });
  }
  const live = [];
  for (const { key } of options.ReportOptions.dateRange)
    for (const includeCurrent of [false, true])
      live.push({
        range: key,
        includeCurrent,
        expected: plain(
          runtime
            .load("getLiveRange")
            .getLiveRange(key, "2026-10-01", "2026-12-02", includeCurrent, "1"),
        ),
      });
  ids.dashboard = await engine.send("dashboard-create", {
    name: "Household history",
  });
  ids.report = await engine.send("report/create", {
    ...options.defaultReport,
    name: "Saved category history",
    startDate: "2026-10",
    endDate: "2026-11",
    isDateStatic: true,
    conditions: [{ field: "category", op: "is", type: "id", value: ids.food }],
  });
  await engine.send("dashboard-add-widget", {
    dashboard_page_id: ids.dashboard,
    type: "cash-flow-card",
    x: 0,
    y: 0,
    width: 6,
    height: 3,
    meta: {
      name: "Cash notes",
      timeFrame: { start: "2026-10", end: "2026-11", mode: "static" },
      showBalance: true,
    },
  });
  await engine.send("dashboard-add-widget", {
    dashboard_page_id: ids.dashboard,
    type: "markdown-card",
    x: 6,
    y: 0,
    width: 6,
    height: 3,
    meta: { content: "Household **recorded** history", text_align: "left" },
  });
  await engine.send("dashboard-add-widget", {
    dashboard_page_id: ids.dashboard,
    type: "custom-report",
    x: 0,
    y: 3,
    width: 12,
    height: 4,
    meta: { id: ids.report },
  });
  const source = {
    dashboard_pages: (
      await api.aqlQuery(q("dashboard_pages").withDead().select("*"))
    ).data,
    dashboard: (await api.aqlQuery(q("dashboard").withDead().select("*"))).data,
    custom_reports: (
      await api.aqlQuery(q("custom_reports").withDead().select("*"))
    ).data,
  };
  const transactions = (
    await api.aqlQuery(q("transactions").options({ splits: "all" }).select("*"))
  ).data;
  const archive = check
    ? await readFile(archivePath)
    : await api.exportBudget();
  const output = {
    oracle: "@actual-app/api@26.10.0 + unchanged pinned frontend spreadsheets",
    clock,
    source_files: runtime.hashes,
    ids,
    accounts,
    categories,
    payees: allPayees,
    transactions,
    source,
    cases,
    live,
    archive_sha256: createHash("sha256").update(archive).digest("hex"),
  };
  const normalize = (record) => {
    const map = new Map(
      Object.entries(record.ids).map(([name, id]) => [id, name]),
    );
    for (const [kind, rows] of Object.entries({
      payee: record.payees,
      category: record.categories.list,
      group: record.categories.grouped,
      widget: record.source.dashboard,
      page: record.source.dashboard_pages,
    }))
      rows.forEach((row, index) => {
        if (!map.has(row.id))
          map.set(row.id, `${kind}:${row.name ?? row.type ?? index}:${index}`);
      });
    for (const row of record.transactions)
      if (!map.has(row.id))
        map.set(
          row.id,
          `transaction:${map.get(row.account)}:${row.date}:${row.amount}:${row.notes ?? ""}:${row.is_child}`,
        );
    function walk(v) {
      if (typeof v === "string") return map.get(v) ?? v;
      if (Array.isArray(v)) return v.map(walk);
      if (v && typeof v === "object")
        return Object.fromEntries(
          Object.entries(v).map(([k, x]) => [map.get(k) ?? k, walk(x)]),
        );
      return v;
    }
    const result = walk(record);
    result.transactions.sort((a, b) => a.id.localeCompare(b.id));
    return result;
  };
  if (check) {
    const expected = JSON.parse(await readFile(outputPath, "utf8"));
    assert.deepEqual(normalize(output), normalize(expected));
  } else {
    await writeFile(archivePath, archive, { flag: "wx" });
    await writeFile(outputPath, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  }
  console.log(
    `PASS pinned real-AQL report oracle: ${cases.length} financial cases, ${live.length} live ranges and preserved source layouts/reports`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

// Whole unchanged pinned Sankey projection over real source engine calls.
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
  directory = await mkdtemp(tmpdir() + "/sankey-source-"),
  fixtures = new URL("../tests/fixtures/", import.meta.url),
  check = process.argv.includes("--check"),
  hash = (value) => createHash("sha256").update(value).digest("hex"),
  plain = (value) => JSON.parse(JSON.stringify(value));
try {
  const engine = await api.init({ dataDir: directory });
  for (const method of ["envelope", "tracking"]) {
    const stem = "sankey-" + method + "-source-26.10",
      archivePath = new URL(stem + ".zip", fixtures),
      jsonPath = new URL(stem + ".json", fixtures),
      prior = check ? JSON.parse(await readFile(jsonPath, "utf8")) : null,
      base = JSON.parse(
        await readFile(
          new URL("budget-analysis-" + method + "-source-26.10.json", fixtures),
          "utf8",
        ),
      ),
      bytes = await readFile(
        check
          ? archivePath
          : new URL(
              "budget-analysis-" + method + "-source-26.10.zip",
              fixtures,
            ),
      );
    assert.equal(
      hash(bytes),
      check ? prior.archive_sha256 : base.archive_sha256,
    );
    await api.importBudget(bytes, { filename: stem + ".zip" });
    let ids = check ? prior.ids : { ...base.ids };
    if (!check) {
      await engine.send("preferences/save", {
        id: "flags.sankeyReport",
        value: "true",
      });
      ids.third = await api.createAccount({
        name: "Sankey card account",
        offbudget: false,
      });
      const payees = await api.getPayees(),
        transfer = (id) => payees.find((p) => p.transfer_acct === id).id,
        fixed = (n) => "37370000-0000-4000-8000-" + String(n).padStart(12, "0");
      await api.addTransactions(
        ids.savings,
        [
          {
            id: fixed(1),
            date: "2026-11-12",
            amount: 1200,
            category: ids.food,
            payee: ids.market,
            notes: "Positive net category refund ? literal%",
          },
          {
            id: fixed(2),
            date: "2026-11-13",
            amount: -500,
            category: ids.salary,
            payee: ids.employer,
            notes: "Income reversal",
          },
          {
            id: fixed(3),
            date: "2026-11-14",
            amount: -700,
            payee: transfer(ids.third),
            notes: "Cycle leg A",
          },
          {
            id: fixed(4),
            date: "2026-11-15",
            amount: -300,
            payee: transfer(ids.checking),
            notes: "Reverse transfer",
          },
        ],
        { runTransfers: true },
      );
      await api.addTransactions(
        ids.third,
        [
          {
            id: fixed(5),
            date: "2026-11-14",
            amount: -400,
            payee: transfer(ids.checking),
            notes: "Cycle leg B",
          },
          {
            id: fixed(6),
            date: "2026-11-15",
            amount: -250,
            category: ids.hidden,
            payee: ids.market,
            notes: "Card expense",
          },
        ],
        { runTransfers: true },
      );
      await api.addTransactions(
        ids.checking,
        [
          {
            id: fixed(7),
            date: "2026-11-14",
            amount: -600,
            payee: transfer(ids.savings),
            notes: "Cycle leg C",
          },
        ],
        { runTransfers: true },
      );
      const incomeGroup = (await api.getCategoryGroups()).find(
        (g) => g.is_income,
      );
      ids.sankeyIncome = await api.createCategory({
        name: "Sankey income",
        group_id: incomeGroup.id,
        is_income: true,
      });
      await api.addTransactions(ids.checking, [
        {
          id: fixed(8),
          date: "2026-11-16",
          amount: 2000,
          category: ids.sankeyIncome,
          payee: ids.employer,
        },
        {
          id: fixed(9),
          date: "2026-11-17",
          amount: 1000,
          category: ids.sankeyIncome,
          payee: ids.market,
        },
      ]);
      await api.setBudgetAmount("2026-11", ids.empty, -200);
      if (method === "envelope")
        await engine.send("budget/hold-for-next-month", {
          month: "2026-11",
          amount: 400,
        });
      await engine.send("dashboard-add-widget", {
        dashboard_page_id: ids.dashboard,
        type: "sankey-card",
        x: 0,
        y: 16,
        width: 12,
        height: 6,
        meta: {
          name: "Saved money flow",
          timeFrame: { start: "2026-10", end: "2026-12", mode: "static" },
          mode: "spent",
          topNcategories: 15,
          categorySort: "per-group",
          showPercentages: true,
          groupAccounts: false,
          showTransfers: true,
          layerFrom: "payee",
          layerTo: "category",
          conditions: [],
          conditionsOp: "and",
        },
      });
      await writeFile(archivePath, await api.exportBudget(), { flag: "wx" });
      // Reopen the exact exported archive, as a real importer sees it.
      await api.importBudget(await readFile(archivePath), {
        filename: "sankey-reload.zip",
      });
    }
    const traces = [],
      traceIndex = new Map(),
      capture = (command, args, data) => {
        const key = JSON.stringify([command, args]);
        if (!traceIndex.has(key)) {
          traceIndex.set(key, traces.length);
          traces.push(plain({ command, args, data }));
        }
        return data;
      },
      runtime = sourceRuntime(
        {
          ...api,
          aqlQuery: async (query) =>
            capture("aql", query.serialize(), await api.aqlQuery(query)),
        },
        {
          send: async (command, args) =>
            capture(command, args, await engine.send(command, args)),
        },
        {
          translate: (key, params = {}) =>
            key.replace(
              /{{(\w+)}}/g,
              (_, name) => params[name] ?? "{{" + name + "}}",
            ),
          theme: {
            toBudgetPositive: "positive",
            toBudgetNegative: "negative",
            reportsGray: "neutral",
            reportsBlue: "flow",
          },
          colors: ["palette0", "palette1", "palette2"],
        },
      ),
      projection = runtime.load("spreadsheets/sankey-spreadsheet"),
      categories = plain((await engine.send("get-categories")).grouped),
      baseOptions = {
        startDate: "2026-11-01",
        endDate: "2026-11-30",
        mode: "spent",
        conditions: [],
        conditionsOp: "and",
        groupAccounts: false,
        showTransfers: false,
        topNcategories: 15,
        categorySort: "per-group",
        layerFrom: "payee",
        layerTo: "category",
      },
      named = {
        field: "category",
        op: "is",
        value: ids.food,
        customName: "Source named category filter",
      },
      food = { field: "category", op: "is", value: ids.food };
    const spent = [
      ["spent", {}],
      ["transfers", { showTransfers: true }],
      ["grouped-transfers", { groupAccounts: true, showTransfers: true }],
      [
        "small-global",
        { topNcategories: 1, categorySort: "global", showTransfers: true },
      ],
      ["small-per-group", { topNcategories: 2, showTransfers: true }],
      [
        "budget-order-layers",
        {
          categorySort: "budget-order",
          layerFrom: "account",
          layerTo: "category",
          showTransfers: true,
        },
      ],
      [
        "account-filter-peer-missing",
        {
          conditions: [{ field: "account", op: "is", value: ids.checking }],
          showTransfers: true,
        },
      ],
      [
        "cross-month-peer",
        { startDate: "2026-10-01", endDate: "2026-12-31", showTransfers: true },
      ],
      ["category-filter", { conditions: [food] }],
      ["named-spent-ignored", { conditions: [named] }],
      [
        "uncategorized",
        { conditions: [{ field: "category", op: "is", value: null }] },
      ],
    ];
    const planned = [
      ["budgeted", {}],
      ["budgeted-multi", { startDate: "2026-10-01", endDate: "2026-12-31" }],
      ["budgeted-category", { conditions: [food] }],
      ["budgeted-named-applies", { conditions: [named] }],
      [
        "budgeted-small-layer",
        {
          topNcategories: 1,
          categorySort: "global",
          layerFrom: "account",
          layerTo: "category",
        },
      ],
      [
        "budgeted-invalid-regex",
        { conditions: [{ field: "category", op: "matches", value: "[" }] },
      ],
    ];
    const cases = [];
    for (const [name, extra] of [
      ...(method === "envelope" ? spent : []),
      ...planned,
    ]) {
      const options = {
        ...baseOptions,
        ...(name.startsWith("budgeted")
          ? { mode: "budgeted", layerFrom: "income_category" }
          : {}),
        ...extra,
      };
      let graph;
      await projection.createBaseGraphSpreadsheet(
        options.startDate.slice(0, 7),
        options.endDate.slice(0, 7),
        categories,
        options.conditions,
        options.conditionsOp,
        options.mode,
        options.groupAccounts,
        options.showTransfers,
      )({}, (value) => (graph = value));
      const baseGraph = plain(
          [...graph].map(([key, node]) => ({ key, ...node, to: [...node.to] })),
        ),
        result = plain(
          projection.buildSankeyData(
            graph,
            options.topNcategories,
            categories,
            options.categorySort,
            options.layerFrom,
            options.layerTo,
          ),
        );
      cases.push({ name, options, baseGraph, result });
    }
    if (method === "envelope") {
      assert.notDeepEqual(
        cases.find((c) => c.name === "transfers").baseGraph,
        cases.find((c) => c.name === "spent").baseGraph,
        "Genuine paired transfers exercise additional base edges",
      );
      assert.ok(
        cases
          .find((c) => c.name === "spent")
          .baseGraph.some((n) => n.type === "payee"),
        "Positive income exercises payee layer",
      );
    }
    const output = plain({
      oracle:
        "Whole unchanged pinned26.10 Sankey base and final graph over real engine/AQL/budget DTOs",
      clock,
      archive_sha256: hash(await readFile(archivePath)),
      ids,
      categories,
      widgets: (
        await api.aqlQuery(
          api.q("dashboard").filter({ type: "sankey-card" }).select("*"),
        )
      ).data,
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
      "PASS source Sankey " +
        method +
        " " +
        cases.length +
        " whole base/final cases, " +
        traces.length +
        " genuine query/budget DTOs",
    );
  }
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

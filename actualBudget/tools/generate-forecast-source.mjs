// Whole pinned26.10 forecast handler; genuine source mutations/export only.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
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
  api = require("@actual-app/api");
assert.equal(
  JSON.parse(
    await readFile(
      require
        .resolve("@actual-app/api")
        .replace(/dist\/index\.js$/, "package.json"),
      "utf8",
    ),
  ).version,
  "26.10.0",
);
const extended = process.argv.includes("--extended"),
  stem = extended ? "forecast-scenarios-source-26.10" : "forecast-source-26.10";
const fixtures = new URL("../tests/fixtures/", import.meta.url),
  jsonPath = new URL(stem + ".json", fixtures),
  zipPath = new URL(stem + ".zip", fixtures),
  check = process.argv.includes("--check"),
  directory = await mkdtemp(tmpdir() + "/forecast-source-");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
try {
  const engine = await api.init({ dataDir: directory });
  const original = JSON.parse(
    await readFile(new URL("reports-source-26.10.json", fixtures), "utf8"),
  );
  const prior = check ? JSON.parse(await readFile(jsonPath, "utf8")) : null;
  const base = extended
    ? JSON.parse(
        await readFile(new URL("forecast-source-26.10.json", fixtures), "utf8"),
      )
    : original;
  const archive = await readFile(
    check
      ? zipPath
      : new URL(
          extended ? "forecast-source-26.10.zip" : "reports-source-26.10.zip",
          fixtures,
        ),
  );
  assert.equal(
    hash(archive),
    check ? prior.archive_sha256 : base.archive_sha256,
  );
  await api.importBudget(archive, { filename: "forecast-source.zip" });
  const schedule = "35350000-0000-4000-8000-000000000001";
  if (!check && !extended) {
    await engine.send("preferences/save", {
      id: "flags.balanceForecastReport",
      value: "true",
    });
    assert.equal(
      await engine.send("schedule/create", {
        schedule: {
          id: schedule,
          name: "Daily forecast bill",
          posts_transaction: false,
        },
        conditions: [
          { field: "account", op: "is", value: original.ids.checking },
          { field: "payee", op: "is", value: original.ids.market },
          { field: "amount", op: "is", value: -100 },
          {
            field: "date",
            op: "is",
            value: {
              start: "2026-11-09",
              frequency: "daily",
              endMode: "never",
            },
          },
        ],
      }),
      schedule,
    );
    await engine.send("dashboard-add-widget", {
      dashboard_page_id: original.ids.dashboard,
      type: "balance-forecast-card",
      x: 0,
      y: 8,
      width: 12,
      height: 4,
      meta: {
        name: "Saved balance forecast",
        timeFrame: { start: "2026-11", end: "2026-11", mode: "static" },
        granularity: "Daily",
        accounts: [original.ids.checking],
        source: "schedules",
        conditions: [],
        conditionsOp: "and",
      },
    });
  }
  const scenarioIds = {
    transfer: "35350000-0000-4000-8000-000000000002",
    unassigned: "35350000-0000-4000-8000-000000000003",
    posted: "35350000-0000-4000-8000-000000000004",
    rules: "35350000-0000-4000-8000-000000000005",
    range: "35350000-0000-4000-8000-000000000006",
    closed: "35350000-0000-4000-8000-000000000007",
    completed: "35350000-0000-4000-8000-000000000008",
  };
  if (!check && extended) {
    const transfer = (await api.getPayees()).find(
        (p) => p.transfer_acct === original.ids.savings,
      ).id,
      controlled = await api.createPayee({ name: "Forecast controlled" });
    async function scheduleCase(
      id,
      name,
      account,
      payee,
      amount,
      date,
      extra = {},
    ) {
      const conditions = [
        ...(account ? [{ field: "account", op: "is", value: account }] : []),
        { field: "payee", op: "is", value: payee },
        {
          field: "amount",
          op: typeof amount === "object" ? "isbetween" : "is",
          value: amount,
        },
        {
          field: "date",
          op: id === scenarioIds.posted ? "isapprox" : "is",
          value: date,
        },
      ];
      assert.equal(
        await engine.send("schedule/create", {
          schedule: { id, name, posts_transaction: false, ...extra },
          conditions,
        }),
        id,
      );
      return conditions;
    }
    await scheduleCase(
      scenarioIds.transfer,
      "Forecast move",
      original.ids.checking,
      transfer,
      -300,
      "2026-11-12",
    );
    await scheduleCase(
      scenarioIds.unassigned,
      "Unassigned forecast",
      null,
      original.ids.market,
      -50,
      "2026-11-13",
    );
    const postedConditions = await scheduleCase(
      scenarioIds.posted,
      "Already paid range",
      original.ids.checking,
      original.ids.market,
      { num1: -100, num2: -101 },
      { start: "2026-11-14", frequency: "weekly", endMode: "never" },
    );
    await api.addTransactions(original.ids.checking, [
      {
        id: "35350000-0000-4000-8001-000000000001",
        date: "2026-11-13",
        amount: -100,
        payee: original.ids.market,
        schedule: scenarioIds.posted,
      },
    ]);
    await engine.send("schedule/update", {
      schedule: { id: scenarioIds.posted },
      conditions: postedConditions,
      resetNextDate: true,
    });
    await scheduleCase(
      scenarioIds.rules,
      "Rule-adjusted forecast",
      original.ids.checking,
      controlled,
      -200,
      "2026-11-15",
    );
    const rule = await engine.send("rule-add", {
      stage: "post",
      conditionsOp: "and",
      conditions: [{ field: "payee", op: "is", value: controlled }],
      actions: [
        { op: "set", field: "amount", value: -250 },
        { op: "set", field: "category", value: original.ids.food },
      ],
    });
    assert.ok(!rule.error);
    const unassignedRule = await engine.send("rule-add", {
      stage: "post",
      conditionsOp: "and",
      conditions: [
        { field: "account", op: "onBudget", value: null },
        { field: "amount", op: "is", value: -50 },
      ],
      actions: [{ op: "set", field: "amount", value: -60 }],
    });
    assert.ok(!unassignedRule.error);
    await scheduleCase(
      scenarioIds.range,
      "Positive range",
      original.ids.savings,
      original.ids.employer,
      { num1: 100, num2: 101 },
      "2026-11-15",
    );
    await scheduleCase(
      scenarioIds.closed,
      "Closed account estimate",
      original.ids.closed,
      original.ids.market,
      -70,
      "2026-11-16",
    );
    await scheduleCase(
      scenarioIds.completed,
      "Completed schedule",
      original.ids.checking,
      original.ids.market,
      -9999,
      "2026-11-10",
      { completed: true },
    );
    await api.addTransactions(original.ids.checking, [
      {
        id: "35350000-0000-4000-8001-000000000002",
        date: "2026-11-16",
        amount: -40,
        payee: original.ids.market,
      },
    ]);
    await engine.send("preferences/save", {
      id: "budgetType",
      value: "tracking",
    });
    // A real reload applies the changed budget method to source spreadsheet metadata.
    await api.importBudget(await api.exportBudget(), {
      filename: "forecast-tracking-reload.zip",
    });
    const incomeGroup = (await api.getCategoryGroups()).find(
      (g) => g.is_income,
    );
    assert.ok(incomeGroup);
    const forecastIncome = await api.createCategory({
      name: "Forecast salary",
      group_id: incomeGroup.id,
      is_income: true,
    });
    for (const [month, income, expense] of [
      ["2026-11", 10000, 3000],
      ["2026-12", 8000, 3500],
    ]) {
      await api.setBudgetAmount(month, forecastIncome, income);
      await api.setBudgetAmount(month, original.ids.food, expense);
      await api.getBudgetMonth(month);
    }
  }
  const inputs = [
    {
      startDate: "2026-11-09",
      endDate: "2026-11-11",
      accountIds: [original.ids.checking],
      source: "schedules",
      conditions: [],
      conditionsOp: "and",
      includeAccountlessSchedules: false,
    },
  ];
  if (extended) {
    const range = {
      startDate: "2026-11-09",
      endDate: "2026-11-16",
      source: "schedules",
    };
    inputs.push(
      { ...range },
      { ...range, includeAccountlessSchedules: true },
      {
        ...range,
        accountIds: [original.ids.checking],
        conditions: [{ field: "category", op: "is", value: original.ids.food }],
        conditionsOp: "and",
      },
      {
        ...range,
        accountIds: [original.ids.checking],
        conditions: [{ field: "amount", op: "lt", value: 0 }],
        conditionsOp: "and",
      },
      {
        ...range,
        conditions: [{ field: "account", op: "onBudget", value: null }],
        conditionsOp: "and",
      },
      {
        ...range,
        conditions: [
          { field: "account", op: "is", value: original.ids.checking },
          { field: "amount", op: "gt", value: 0 },
        ],
        conditionsOp: "or",
      },
      { ...range, accountIds: [original.ids.savings] },
      { ...range, accountIds: [original.ids.closed] },
      { startDate: "2026-10-20", endDate: "2026-10-21", source: "schedules" },
      { ...range, accountIds: [] },
      {
        startDate: "2026-11-09",
        endDate: "2026-12-31",
        source: "tracking-budget",
        accountIds: [],
        conditions: [{ field: "amount", op: "gt", value: 99999999 }],
        conditionsOp: "and",
      },
    );
  }
  const cases = [];
  for (const options of inputs) {
    const expected = await engine.send("forecast/generate", options);
    assert.ok(Array.isArray(expected.dataPoints));
    cases.push({ options, expected });
  }
  const widgets = (
    await api.aqlQuery(
      api.q("dashboard").filter({ type: "balance-forecast-card" }).select("*"),
    )
  ).data;
  assert.equal(widgets.length, 1);
  const prefs = await engine.send("preferences/get");
  assert.equal(prefs["flags.balanceForecastReport"], "true");
  const source_files = {};
  for (const file of [
    "server/forecast/app.ts",
    "server/forecast/forecast-accounts.ts",
    "server/forecast/forecast-filters.ts",
    "server/forecast/forecast-schedules.ts",
    "server/forecast/forecast-projection.ts",
    "server/forecast/forecast-tracking-budget.ts",
  ])
    source_files[file] = hash(
      await readFile(
        new URL(
          "../reference/actual-26.10.0/packages/loot-core/src/" + file,
          import.meta.url,
        ),
      ),
    );
  const bytes = check ? archive : await api.exportBudget();
  const output = {
    oracle: "Whole @actual-app/api@26.10.0 forecast/generate",
    clock,
    source_files,
    archive_sha256: hash(bytes),
    base_sha256: base.archive_sha256,
    widgets,
    forecast_enabled: true,
    schedule,
    cases,
  };
  if (check) assert.deepEqual(output, prior);
  else {
    await writeFile(zipPath, bytes, { flag: "wx" });
    await writeFile(jsonPath, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  }
  console.log(
    `PASS ${cases.length} whole-source forecast cases, saved opted-in widget, ${extended ? "filters/transfers/rules/ranges/paid/tracking" : "recurring schedule"} projection`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

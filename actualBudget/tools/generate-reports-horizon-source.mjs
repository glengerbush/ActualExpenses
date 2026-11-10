// Sparse >15-year histories, unchanged pinned spreadsheets +real source AQL.
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
const directory = await mkdtemp(tmpdir() + "/reports-horizon-"),
  check = process.argv.includes("--check"),
  path = new URL(
    "../tests/fixtures/reports-horizon-source-26.10.json",
    import.meta.url,
  ),
  zip = new URL(
    "../tests/fixtures/reports-horizon-source-26.10.zip",
    import.meta.url,
  );
const plain = (v) => JSON.parse(JSON.stringify(v));
try {
  const engine = await api.init({ dataDir: directory });
  let account, category;
  await api.runImport("Sparse historical reports", async () => {
    account = await api.createAccount({ name: "Long history" }, 10000);
    const group = await api.createCategoryGroup({
      name: "Historical purchases",
      is_income: false,
    });
    category = await api.createCategory({
      name: "Food",
      group_id: group,
      is_income: false,
    });
  });
  const [opening] = await api.getTransactions(
    account,
    "1900-01-01",
    "2100-01-01",
  );
  await api.updateTransaction(opening.id, { date: "2011-01-01" });
  await api.addTransactions(account, [
    {
      date: "2018-06-15",
      amount: -1000,
      category,
      notes: "Historical purchase",
    },
    { date: "2026-11-02", amount: -200, category, notes: "Current purchase" },
  ]);
  const runtime = sourceRuntime(api, engine),
    accounts = await api.getAccounts(),
    cases = [];
  const capture = async (fn) => {
    let value;
    await fn({}, (data) => (value = plain(data)));
    assert.notEqual(value, undefined);
    return value;
  };
  for (const interval of ["Monthly", "Yearly"]) {
    const options = {
      start: "2011-01",
      end: "2026-11",
      interval,
      accounts: [account],
    };
    cases.push({
      kind: "net-worth",
      options,
      expected: await capture(
        runtime
          .load("spreadsheets/net-worth-spreadsheet")
          .createSpreadsheet(
            options.start,
            options.end,
            accounts,
            [],
            "and",
            enUS,
            interval,
            "1",
            String,
          ),
      ),
    });
  }
  const cash = { start: "2011-01", end: "2026-11", isConcise: true };
  cases.push({
    kind: "cash-flow",
    options: cash,
    expected: await capture(
      runtime
        .load("spreadsheets/cash-flow-spreadsheet")
        .cashFlowByDate(cash.start, cash.end, true, [], "and", enUS, String),
    ),
  });
  const summary = {
    start: "2011-01",
    end: "2026-11",
    summary: { type: "avgPerYear" },
  };
  cases.push({
    kind: "summary",
    options: summary,
    expected: await capture(
      runtime
        .load("spreadsheets/summary-spreadsheet")
        .summarySpreadsheet(
          summary.start,
          summary.end,
          [],
          "and",
          summary.summary,
          enUS,
        ),
    ),
  });
  const spending = {
    compare: "2026-11",
    compareTo: "2026-10",
    averageRange: { mode: "all-time" },
  };
  let expected = await capture(
    runtime
      .load("spreadsheets/spending-spreadsheet")
      .createSpendingSpreadsheet(spending),
  );
  // Keep every displayed average/cumulative/total. Per-month dictionaries repeat
  // mostly zeros; their complete month counts are recorded for each day instead.
  expected.intervalData = expected.intervalData.map(({ months, ...day }) => ({
    ...day,
    month_count: Object.keys(months).length,
  }));
  cases.push({ kind: "spending", options: spending, expected });
  const bytes = check ? await readFile(zip) : await api.exportBudget(),
    output = {
      oracle: "Unchanged Actual26.10 spreadsheets +real AQL",
      clock,
      source_files: runtime.hashes,
      account,
      transaction_count: 3,
      cases,
      archive_sha256: createHash("sha256").update(bytes).digest("hex"),
    };
  const normalize = (record) =>
    JSON.parse(JSON.stringify(record).replaceAll(record.account, "ACCOUNT"));
  if (check)
    assert.deepEqual(
      normalize(output),
      normalize(JSON.parse(await readFile(path, "utf8"))),
    );
  else {
    await writeFile(zip, bytes, { flag: "wx" });
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  }
  console.log(
    "PASS sparse2011→2026 source oracle:monthly/yearly net worth, monthly cash flow, yearly summary average, all-time spending average",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

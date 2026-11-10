// Correct internal frontend DTOs from the pinned engine; not API external models.
// Financial results come only from unchanged26.10 spreadsheet reducers +real AQL.
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
  api = require("@actual-app/api");
const directory = await mkdtemp(tmpdir() + "/custom-reports-oracle-"),
  check = process.argv.includes("--check"),
  path = new URL(
    process.argv.includes("--named")
      ? "../tests/fixtures/custom-report-named-source-26.10.json"
      : process.argv.includes("--grouped")
        ? "../tests/fixtures/custom-report-groups-source-26.10.json"
        : process.argv.includes("--dates")
          ? "../tests/fixtures/custom-report-dates-source-26.10.json"
          : "../tests/fixtures/custom-reports-source-26.10.json",
    import.meta.url,
  );
try {
  const original = JSON.parse(
      await readFile(
        new URL("../tests/fixtures/reports-source-26.10.json", import.meta.url),
        "utf8",
      ),
    ),
    bytes = await readFile(
      new URL("../tests/fixtures/reports-source-26.10.zip", import.meta.url),
    );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    original.archive_sha256,
  );
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(bytes, { filename: "historical-source.zip" });
  const categories = await engine.send("get-categories"),
    accounts = await api.getAccounts(),
    payees = await api.getPayees(),
    runtime = sourceRuntime(api, engine);
  assert.ok(categories.list.every((c) => typeof c.group === "string"));
  assert.ok(categories.grouped.every((g) => Array.isArray(g.categories)));
  const base = original.cases.find((c) => c.kind === "custom").options,
    inputs = original.cases
      .filter((c) => c.kind === "custom")
      .map((c) => c.options);
  for (const groupBy of [
    "Category",
    "Group",
    "CategoryGroup",
    "Payee",
    "Account",
    "Interval",
  ])
    for (const balanceTypeOp of [
      "totalAssets",
      "totalDebts",
      "totalTotals",
      "netAssets",
      "netDebts",
    ])
      inputs.push({
        ...base,
        groupBy,
        balanceTypeOp,
        showOffBudget: true,
        showHiddenCategories: false,
        showUncategorized: false,
      });
  for (const sortByOp of ["asc", "desc", "name", "budget"])
    inputs.push({ ...base, sortByOp, showEmpty: true });
  for (const groupBy of ["Category", "Group", "CategoryGroup", "Interval"])
    for (const interval of ["Monthly", "Yearly"])
      inputs.push({
        ...base,
        groupBy,
        interval,
        balanceTypeOp: "totalBudgeted",
        startDate: interval === "Yearly" ? "2026" : "2026-10",
        endDate: interval === "Yearly" ? "2026" : "2026-11",
        showHiddenCategories: false,
      });
  inputs.push({
    ...base,
    conditions: [
      {
        field: "category_group",
        op: "is",
        type: "id",
        value: original.ids.expenses,
      },
    ],
  });
  if (process.argv.includes("--dates")) {
    inputs.length = 0;
    for (const interval of ["Monthly", "Yearly", "Daily", "Weekly"])
      for (const balanceTypeOp of interval === "Monthly" ||
      interval === "Yearly"
        ? ["totalDebts", "totalBudgeted"]
        : ["totalDebts"])
        inputs.push({
          ...base,
          interval,
          balanceTypeOp,
          startDate: "2026-10-15",
          endDate: "2026-11-09",
        });
  }
  if (process.argv.includes("--grouped")) {
    const selected = inputs.filter((o) => o.groupBy === "CategoryGroup");
    inputs.length = 0;
    inputs.push(...selected.map((o) => ({ ...o, nested: true })));
  }
  if (process.argv.includes("--named")) {
    inputs.length = 0;
    for (const customName of [
      "",
      null,
      false,
      0,
      "Saved category",
      1,
      [],
      {},
    ]) {
      inputs.push({
        ...base,
        conditions: [
          {
            field: "category",
            op: "is",
            type: "id",
            value: original.ids.food,
            customName,
          },
        ],
      });
    }
  }
  const cases = [];
  for (const options of inputs) {
    let expected;
    await runtime
      .load("spreadsheets/custom-spreadsheet")
      .createCustomSpreadsheet({ ...options, categories, accounts, payees })(
      {},
      (data) => (expected = JSON.parse(JSON.stringify(data))),
    );
    assert.ok(expected);
    delete expected.legend;
    if (options.nested) {
      await runtime
        .load("spreadsheets/grouped-spreadsheet")
        .createGroupedSpreadsheet({ ...options, categories, accounts, payees })(
        {},
        (data) => (expected.groupedData = JSON.parse(JSON.stringify(data))),
      );
    }
    cases.push({ options, expected });
  }
  const output = {
    oracle:
      "Unchanged pinned26.10 custom spreadsheet +real AQL +get-categories internal DTOs",
    clock,
    source_archive: "reports-source-26.10.zip",
    source_sha256: original.archive_sha256,
    source_files: runtime.hashes,
    categories,
    accounts,
    payees,
    cases,
  };
  if (check) assert.deepEqual(output, JSON.parse(await readFile(path, "utf8")));
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    `PASS ${cases.length} pinned custom report financial cases; frontend category group/order/hidden metadata intact`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}

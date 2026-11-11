// Independent pinned-engine verification after measured browser saves. Oracle
// setup/download/calculation are outside every performance observation window.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { resolve } from "node:path";
import { sourceRuntime } from "./reports-source-runtime.mjs";
// Native report wire dates are UTC, as in the existing pinned report oracle.
process.env.TZ = "UTC";
const NativeDate = Date;
globalThis.Date = class extends NativeDate {
  constructor(...args) {
    super(...(args.length ? args : ["2026-11-10T12:00:00Z"]));
  }
}; // Date.now stays real for synchronization clocks; only calendar today is fixed.
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api"),
  { enUS } = require("date-fns/locale");
const [directoryArgument, countArgument = "20", onlyCase] =
  process.argv.slice(2);
assert.ok(
  directoryArgument,
  "Usage: node check-resource-workloads.mjs LAB_DIRECTORY [COUNT=20] [empty|typical|large]",
);
const directory = resolve(directoryArgument),
  count = Number(countArgument);
assert.ok(Number.isInteger(count) && count >= 1 && count <= 100);
assert.ok(
  onlyCase === undefined || ["empty", "typical", "large"].includes(onlyCase),
);
const source = JSON.parse(await readFile(directory + "/baseline.json")),
  native = JSON.parse(await readFile(directory + "/native.json"));
for (const config of [source, native])
  assert.equal(new URL(config.server).hostname, "127.0.0.1");
const cache = await mkdtemp(directory + "/workload-oracle-"),
  result = { source_version: "26.10.0", count, cases: [] };
const output =
  directory +
  "/" +
  (onlyCase ? onlyCase + "-" : "") +
  "workload-financial-proof.json";
await writeFile(output, "{}\n", { flag: "wx" });
async function call(path, body) {
  const response = await fetch(native.server + "/api/v1" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  assert.ok(response.ok, path + ": " + (await response.clone().text()));
  return (await response.json()).data;
}
try {
  const engine = await api.init({
    dataDir: cache,
    serverURL: source.server,
    password: source.password,
  });
  const runtime = sourceRuntime(api, engine);
  for (const test of source.cases.filter(
    (test) => !onlyCase || test.name === onlyCase,
  )) {
    assert.ok(test.label.startsWith("Workload "));
    const remote = (await api.getBudgets()).find(
      (budget) => budget.cloudFileId === test.cloud_id,
    );
    assert.ok(remote?.groupId, "Source budget sync identity missing");
    await api.downloadBudget(remote.groupId);
    const target = native.cases.find((item) => item.name === test.name),
      prefix = "Resource acknowledgement " + test.name + " ";
    const expected = Array.from({ length: count }, (_, index) => ({
      account: test.account,
      date: "2026-11-10",
      amount: -1234,
      notes: prefix + String(index + 1).padStart(3, "0"),
      cleared: false,
    }));
    const sourceRows = (
      await api.getTransactions(test.account, "1900-01-01", "2100-01-01")
    ).filter((row) => row.notes?.startsWith(prefix));
    const financial = (rows) =>
      rows
        .map(({ account, date, amount, notes, cleared }) => ({
          account,
          date,
          amount,
          notes,
          cleared,
        }))
        .sort((a, b) => a.notes.localeCompare(b.notes));
    assert.deepEqual(
      financial(sourceRows),
      expected,
      "Server received every acknowledged source expense exactly once",
    );
    const actual = await call("/budgets/" + target.id + "/query", {
      version: 1,
      table: "transactions",
      select: ["id", "account", "date", "amount", "notes", "cleared"],
      filter: [{ account: test.account }, { notes: { $like: prefix + "%" } }],
      order_by: ["notes"],
      limit: 100,
    });
    assert.deepEqual(
      financial(actual),
      expected,
      "Replacement persisted exactly the same expenses",
    );
    const original = target.accounts.find(
      (account) => account.id === test.account,
    ).balance;
    // SDK getAccountBalance defaults to a today cutoff. The register and the
    // source manifest include every recorded entry, including future dates.
    const balance =
      (
        await api.aqlQuery(
          api
            .q("transactions")
            .filter({ account: test.account })
            .calculate({ $sum: "$amount" }),
        )
      ).data ?? 0;
    assert.equal(balance, original - count * 1234);
    const nativeAccounts = await call("/budgets/" + target.id + "/accounts");
    assert.equal(
      nativeAccounts.find((account) => account.id === test.account).balance,
      balance,
    );
    let cashFlow;
    await runtime
      .load("spreadsheets/cash-flow-spreadsheet")
      .cashFlowByDate(
        "2026-10",
        "2026-11",
        false,
        [],
        "and",
        enUS,
        String,
      )({}, (data) => {
      cashFlow = JSON.parse(JSON.stringify(data));
    });
    assert.ok(cashFlow);
    const report = await call("/budgets/" + target.id + "/reports/query", {
      version: 1,
      kind: "cash-flow",
      as_of: "2026-11-10",
      options: { start: "2026-10", end: "2026-11", isConcise: false },
    });
    assert.deepEqual(
      report.result,
      cashFlow,
      "Native cash-flow result matches pinned calculation after browser edits",
    );
    result.cases.push({
      name: test.name,
      archive_sha256: test.archive_sha256,
      expenses: expected,
      balance,
      cash_flow: cashFlow,
    });
    await engine.send("close-budget");
    console.log(
      "PASS",
      test.name,
      "acknowledged expenses, balance and independent cash-flow result",
    );
  }
  result.source_files = runtime.hashes;
} catch (error) {
  result.error = String(error);
  throw error;
} finally {
  await api.shutdown();
  await rm(cache, { recursive: true, force: true });
  await writeFile(output, JSON.stringify(result, null, 2) + "\n");
}

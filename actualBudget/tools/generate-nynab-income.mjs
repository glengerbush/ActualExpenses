// Genuine pinned import of absent/null/explicit Starting Balance references.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  check = process.argv.includes("--check"),
  directory = await mkdtemp(tmpdir() + "/nynab-income-"),
  path = new URL("../tests/fixtures/nynab-income.json", import.meta.url),
  hash = (b) => createHash("sha256").update(b).digest("hex");
assert.equal(
  JSON.parse(
    await readFile(
      require
        .resolve("@actual-app/api")
        .replace(/dist\/index\.js$/, "package.json"),
    ),
  ).version,
  "26.10.0",
);
const demo = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/ynab/nynab-demo.json", import.meta.url),
    ),
  ).budget,
  expected = check ? JSON.parse(await readFile(path)) : null,
  records = [];
try {
  await api.init({ dataDir: directory });
  for (const mode of [
    "absent-starting-payee",
    "present-null-payee",
    "explicit-starting-payee",
  ]) {
    const data = structuredClone(demo),
      account = data.accounts[0],
      income = data.categories.find(
        (c) =>
          c.name === "Inflow: Ready to Assign" || c.name === "To be Budgeted",
      ),
      starting = data.payees.find((p) => p.name === "Starting Balance");
    assert.ok(income && starting);
    data.accounts = [account];
    data.payees = mode === "absent-starting-payee" ? [] : [starting];
    data.scheduled_transactions = [];
    data.scheduled_subtransactions = [];
    data.subtransactions = [];
    data.transactions = [
      {
        ...data.transactions[0],
        id: "27000000-0000-4000-8000-000000000027",
        date: "2023-08-10",
        amount: 10000,
        memo: "Unassigned income",
        category_id: income.id,
        payee_id: mode === "explicit-starting-payee" ? starting.id : null,
        transfer_account_id: null,
        transfer_transaction_id: null,
        flag_name: null,
        flag_color: null,
        import_id: null,
      },
    ];
    data.months = [];
    const input = Buffer.from(JSON.stringify({ plan: data })),
      inputFile = `nynab-income-${mode}.json`,
      actualFile = `nynab-income-${mode}-actual-26.10.zip`,
      base = new URL("../tests/fixtures/ynab/", import.meta.url);
    if (!check)
      await writeFile(new URL(inputFile, base), input, { flag: "wx" });
    assert.deepEqual(await readFile(new URL(inputFile, base)), input);
    await api.importBudget(input, { type: "ynab5", filename: inputFile });
    const accounts = await api.getAccounts(),
      categories = await api.getCategories(),
      rows = await api.getTransactions(
        accounts[0].id,
        "1900-01-01",
        "2099-12-31",
      );
    assert.equal(rows.length, 1);
    const row = rows[0],
      category = categories.find((c) => c.id === row.category)?.name ?? null,
      balance = await api.getAccountBalance(accounts[0].id);
    const actual = check
      ? await readFile(new URL(actualFile, base))
      : await api.exportBudget();
    if (!check)
      await writeFile(new URL(actualFile, base), actual, { flag: "wx" });
    records.push({
      id: mode,
      input: inputFile,
      input_sha256: hash(input),
      actual: actualFile,
      actual_sha256: hash(actual),
      category,
      amount: row.amount,
      balance,
      payee: row.payee,
      starting_balance_flag: row.starting_balance_flag,
    });
    assert.equal(row.payee, null);
    assert.equal(
      category,
      mode === "explicit-starting-payee" ? "Starting Balances" : "Income",
    );
  }
  const output = {
    oracle: "@actual-app/api@26.10.0",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    normalization:
      "Only generated IDs omitted; all three original source JSONs and genuine Actual exports retained. Income category is asserted by public name.",
    cases: records,
  };
  if (check) assert.deepEqual(output, expected);
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS genuine nYNAB absent, null and explicit starting-payee income cases",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

// Execute original importer and scheduler name normalization through the public SDK.
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
  directory = await mkdtemp(tmpdir() + "/nynab-names-"),
  path = new URL("../tests/fixtures/nynab-names.json", import.meta.url),
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
  records = [];
try {
  await api.init({ dataDir: directory });
  for (const [mode, names] of [
    ["unnamed-22", Array(22).fill(null)],
    ["trim-case", [null, "", "   ", " Payee ", "Payee", "payee", "Payee "]],
  ]) {
    const data = structuredClone(demo),
      template = data.scheduled_transactions[0];
    data.transactions = [];
    data.subtransactions = [];
    data.scheduled_subtransactions = [];
    data.months = [];
    data.scheduled_transactions = names.map((memo, i) => ({
      ...template,
      id: "27000000-0000-4000-8000-" + String(i + 1).padStart(12, "0"),
      amount: -10000 - i * 10,
      memo,
      date_next: "2026-11-12",
      frequency: "never",
      flag_name: null,
      flag_color: null,
    }));
    const input = Buffer.from(JSON.stringify({ budget: data })),
      inputFile = `nynab-names-${mode}.json`,
      actualFile = `nynab-names-${mode}-actual-26.10.zip`,
      base = new URL("../tests/fixtures/ynab/", import.meta.url);
    if (!check)
      await writeFile(new URL(inputFile, base), input, { flag: "wx" });
    assert.deepEqual(await readFile(new URL(inputFile, base)), input);
    await api.importBudget(input, { type: "ynab5", filename: inputFile });
    const schedules = await api.getSchedules(),
      rules = await api.getRules(),
      accounts = await api.getAccounts(),
      payees = await api.getPayees(),
      categories = await api.getCategories(),
      groups = await api.getCategoryGroups(),
      ids = new Map([
        ...accounts.map((a) => [a.id, "account:" + a.name]),
        ...payees.map((p) => [p.id, "payee:" + p.name]),
        ...categories.map((c) => [
          c.id,
          "category:" +
            groups.find((g) => g.id === c.group_id).name +
            "/" +
            c.name,
        ]),
        ...schedules.map((s) => [s.id, "schedule:" + s.amount]),
      ]);
    const normalize = (v) =>
      Array.isArray(v)
        ? v.map(normalize)
        : v && typeof v === "object"
          ? Object.fromEntries(
              Object.entries(v).map(([k, v]) => [k, normalize(v)]),
            )
          : (ids.get(v) ?? v);
    const definitions = schedules
      .map((s) => {
        const r = rules.find((r) => r.id === s.rule);
        assert.ok(r);
        return normalize({
          name: s.name,
          amount: s.amount,
          date: s.date,
          next_date: s.next_date,
          posts_transaction: s.posts_transaction,
          completed: s.completed,
          conditions: r.conditions,
          actions: r.actions,
        });
      })
      .sort((a, b) => a.amount - b.amount);
    assert.equal(definitions.length, names.length);
    for (const a of accounts)
      assert.deepEqual(
        await api.getTransactions(a.id, "1900-01-01", "2099-12-31"),
        [],
      );
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
      definitions,
    });
  }
  const output = {
    oracle: "@actual-app/api@26.10.0",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    normalization:
      "Generated reference IDs labeled by public names; schedule links labeled by unique source amount. Exact name values/case/whitespace and financial/action multiplicity retained.",
    cases: records,
  };
  if (check) assert.deepEqual(output, JSON.parse(await readFile(path)));
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS genuine nYNAB 22 unnamed and trim/case schedule names; no money posted",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

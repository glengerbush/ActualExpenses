// Persisted Actual operations: distinguish preview conversions from valid writes.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
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
const root = new URL("../tests/fixtures/", import.meta.url),
  check = process.argv.includes("--check"),
  splitCases = process.argv.includes("--splits"),
  childCases = process.argv.includes("--children"),
  nonfiniteCases = process.argv.includes("--nonfinite"),
  sha = (b) => createHash("sha256").update(b).digest("hex");
const source = JSON.parse(
    await readFile(
      new URL(
        nonfiniteCases
          ? "rule-expression-nonfinite-source.json"
          : splitCases
            ? "rule-expression-execution-source.json"
            : "rule-expressions-source.json",
        root,
      ),
    ),
  ),
  directory = await mkdtemp(tmpdir() + "/rule-expression-commits-");
const subject = childCases
    ? "33340000-0000-4000-8000-000000000008"
    : "42420000-0000-4000-8000-000000000099",
  filename = childCases
    ? "rule-expression-children-saved-26.10.zip"
    : "rule-expressions-saved-26.10.zip";
const childNotes = (splitIndex, options) => ({
  op: "set",
  field: "notes",
  value: "",
  options: { splitIndex, ...options },
});
const definitions = childCases
  ? [
      {
        name: "existing child explicit payee name remains visible",
        actions: [
          {
            op: "set",
            field: "payee_name",
            value: "Market",
            options: { splitIndex: 1 },
          },
          childNotes(1, { formula: "=payee_name" }),
        ],
      },
      {
        name: "existing child stored fields",
        actions: [
          childNotes(1, {
            template:
              "{{sort_order}}|{{is_parent}}|{{is_child}}|{{imported_id}}|{{raw_synced_data}}|{{tombstone}}|{{starting_balance_flag}}|{{notes}}|{{payee_name}}",
          }),
        ],
      },
      {
        name: "existing child formula fields",
        actions: [
          childNotes(1, {
            formula:
              '=CONCATENATE(sort_order,"|",IFERROR(is_parent,"missing"),"|",is_child,"|",IFERROR(imported_id,"missing"),"|",IFERROR(raw_synced_data,"missing"),"|",IFERROR(tombstone,"missing"),"|",starting_balance_flag)',
          }),
        ],
      },
      {
        name: "new child after existing children",
        actions: [
          {
            op: "set-split-amount",
            value: 0,
            options: { splitIndex: 2, method: "fixed-amount" },
          },
          childNotes(2, {
            formula:
              '=CONCATENATE(sort_order,"|",IFERROR(is_parent,"missing"),"|",is_child,"|",IFERROR(imported_id,"missing"),"|",IFERROR(raw_synced_data,"missing"),"|",IFERROR(tombstone,"missing"),"|",starting_balance_flag)',
          }),
        ],
      },
    ].map((test) => ({ ...test, transaction: source.cases[0].transaction }))
  : nonfiniteCases
    ? source.cases.filter((c) => !c.following)
    : splitCases
      ? source.cases.filter(
          (c) =>
            c.name.startsWith("fractional") ||
            c.name === "generated child variables",
        )
      : [
          "formula amount units",
          "template numeric prefix",
          "formula string numeric prefix",
          "formula failure keeps later actions",
          "split parent context",
        ].map((name) => source.cases.find((c) => c.name === name));
try {
  const engine = await api.init({ dataDir: directory });
  let bytes;
  if (childCases && !check) {
    const seed = await readFile(new URL(source.seed.filename, root));
    assert.equal(sha(seed), source.seed.sha256);
    await api.importBudget(seed, { filename: source.seed.filename });
    const metadata = {
      id: "33340000-0000-4000-8000-000000000010",
      imported_id: "child-source-reference",
      raw_synced_data: '{"memo":"historical child"}',
    };
    await engine.send("transactions-batch-update", {
      updated: [metadata],
      learnCategories: false,
    });
    const saved = (
      await api.aqlQuery(
        api
          .q("transactions")
          .options({ splits: "all" })
          .filter({ id: metadata.id })
          .select("*"),
      )
    ).data[0];
    for (const key of ["imported_id", "raw_synced_data"])
      assert.equal(saved[key], metadata[key]);
    bytes = await api.exportBudget();
    await writeFile(new URL(filename, root), bytes, { flag: "wx" });
  } else if (check || splitCases || nonfiniteCases)
    bytes = await readFile(new URL(filename, root));
  else {
    bytes = await readFile(new URL(source.seed.filename, root));
    assert.equal(sha(bytes), source.seed.sha256);
    await api.importBudget(bytes, { filename: source.seed.filename });
    for (const r of await api.getRules()) await api.deleteRule(r.id);
    const transaction = {
      ...source.cases[0].transaction,
      id: subject,
      notes: "expression commit",
    };
    await api.addTransactions(transaction.account, [transaction]);
    for (const flag of ["formulaMode", "actionTemplating"])
      await engine.send("preferences/save", {
        id: "flags." + flag,
        value: "true",
      });
    for (const test of [
      source.cases.find((c) => c.name === "template unescaped text"),
      source.cases.find((c) => c.name === "formula amount units"),
    ]) {
      const saved = await engine.send("rule-add", test.rule);
      assert.ok(!saved.error);
    }
    bytes = await api.exportBudget();
    await writeFile(new URL(filename, root), bytes, { flag: "wx" });
  }
  const cases = [];
  let savedRules;
  function view(rows) {
    return rows
      .filter((r) => r.id === subject || r.parent_id === subject)
      .map((r) => ({
        id: r.id === subject ? subject : "generated-child",
        parent: r.parent_id === subject,
        amount: r.amount,
        notes: r.notes ?? "",
        date: r.date,
        account: r.account,
        category: r.category,
        payee: r.payee,
        cleared: !!r.cleared,
        is_parent: !!r.is_parent,
        is_child: !!r.is_child,
      }))
      .sort(
        (a, b) =>
          Number(a.is_child) - Number(b.is_child) ||
          a.amount - b.amount ||
          a.notes.localeCompare(b.notes),
      );
  }
  const ledger = async () =>
    (
      await api.aqlQuery(
        api
          .q("transactions")
          .options({ splits: "all" })
          .select("*")
          .orderBy("id"),
      )
    ).data;
  for (const test of definitions) {
    await api.importBudget(bytes, { filename });
    savedRules = await api.getRules();
    const before = await ledger(),
      transaction = (await api.getTransactions(test.transaction.account)).find(
        (r) => r.id === subject,
      );
    assert.ok(transaction);
    let error = null;
    try {
      await engine.send("rule-apply-actions", {
        actions: test.actions,
        transactions: [transaction],
      });
    } catch (e) {
      error = e.message;
    }
    const after = await ledger();
    const others = (rows) =>
      rows.filter((r) => r.id !== subject && r.parent_id !== subject);
    assert.deepEqual(others(after), others(before));
    if (error) assert.deepEqual(after, before);
    cases.push({
      name: test.name,
      actions: test.actions,
      before: view(before),
      after: view(after),
      error,
    });
  }
  const result = {
    source_commit: source.source_commit,
    clock,
    archive: { filename, sha256: sha(bytes) },
    subject,
    rules: savedRules,
    ...(childCases
      ? {
          conditions: [
            {
              field: "account",
              op: "is",
              value: source.cases[0].transaction.account,
            },
          ],
        }
      : {}),
    cases,
  };
  const file = new URL(
    nonfiniteCases
      ? "rule-expression-nonfinite-commits-source.json"
      : childCases
        ? "rule-expression-child-commits-source.json"
        : splitCases
          ? "rule-expression-split-commits-source.json"
          : "rule-expression-commits-source.json",
    root,
  );
  if (check) assert.deepEqual(result, JSON.parse(await readFile(file)));
  else
    await writeFile(file, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    `PASS ${cases.length} genuine persisted expression operations, fractional-unit refusal without writes, saved-rule export/reload and unchanged unrelated financial rows`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

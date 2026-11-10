// Genuine SDK financial outcome. Its spreadsheet batching is NOT a rollback guarantee.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
process.env.TZ = "UTC";
const NativeDate = Date,
  clock = "2026-11-09T12:00:00.000Z";
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
  directory = await mkdtemp(tmpdir() + "/public-batch-source-"),
  path = new URL("../tests/fixtures/public-batch-source.json", import.meta.url),
  hash = (v) => createHash("sha256").update(v).digest("hex"),
  seed = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-export-source.json",
        import.meta.url,
      ),
    ),
  ),
  archive = await readFile(
    new URL(
      "../tests/fixtures/transaction-export-base-26.10.zip",
      import.meta.url,
    ),
  );
assert.equal(hash(archive), seed.archive.sha256);
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
const fields = [
  "id",
  "account",
  "payee",
  "category",
  "amount",
  "date",
  "notes",
  "is_parent",
  "is_child",
  "parent_id",
  "transfer_id",
  "cleared",
  "reconciled",
];
const created = "47470000-0000-4000-8000-000000000001",
  operations = [
    {
      action: "create",
      transaction: {
        id: created,
        account_id: seed.ids.account,
        payee_id: seed.ids.payee,
        category_id: seed.ids.food,
        amount: -333,
        date: "2026-11-09",
        notes: "External batch purchase",
        cleared: true,
      },
    },
    {
      action: "update",
      id: seed.ids.first,
      fields: { amount: -1300, notes: "External batch correction" },
    },
    { action: "delete", id: seed.ids.zero },
  ];
try {
  await api.init({ dataDir: directory });
  await api.importBudget(archive, { filename: "public-batch-source.zip" });
  const ledger = async () =>
      JSON.parse(
        JSON.stringify(
          (
            await api.aqlQuery(
              api
                .q("transactions")
                .options({ splits: "all" })
                .select(fields)
                .orderBy("id"),
            )
          ).data,
        ),
      ),
    before = await ledger();
  await api.batchBudgetUpdates(async () => {
    const t = operations[0].transaction;
    await api.addTransactions(t.account_id, [
      {
        id: t.id,
        payee: t.payee_id,
        category: t.category_id,
        amount: t.amount,
        date: t.date,
        notes: t.notes,
        cleared: t.cleared,
      },
    ]);
    await api.updateTransaction(operations[1].id, operations[1].fields);
    await api.deleteTransaction(operations[2].id);
  });
  let after;
  for (let i = 0; i < 100; i++) {
    after = await ledger();
    if (
      after.some((r) => r.id === created) &&
      after.find((r) => r.id === seed.ids.first)?.notes ===
        "External batch correction" &&
      !after.some((r) => r.id === seed.ids.zero)
    )
      break;
    await new Promise((r) => setTimeout(r, 10));
  }
  assert.equal(after.find((r) => r.id === created)?.amount, -333);
  assert.equal(after.find((r) => r.id === seed.ids.first)?.amount, -1300);
  assert.ok(!after.some((r) => r.id === seed.ids.zero));
  const sums = (
      await api.aqlQuery(
        api
          .q("transactions")
          .groupBy("account")
          .select(["account", { total: { $sum: "$amount" } }])
          .orderBy("account"),
      )
    ).data,
    output = {
      oracle:
        "Actual26.10 real SDK add/update/delete under batchBudgetUpdates; settled public ledger",
      source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
      engine_entry_sha256: hash(
        await readFile(require.resolve("@actual-app/api")),
      ),
      clock,
      archive: {
        filename: "transaction-export-base-26.10.zip",
        sha256: hash(archive),
      },
      fields,
      operations,
      before,
      after,
      sums,
    };
  if (process.argv.includes("--check"))
    assert.deepEqual(output, JSON.parse(await readFile(path)));
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log("PASS genuine mixed SDK batch financial graph/aggregates");
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

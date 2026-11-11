// Composite fixture authored only through the pinned Actual engine.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { sourceArchive } from "./migration-archive.mjs";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
const root = new URL("../tests/fixtures/complete-migration/", import.meta.url);
const check = process.argv.includes("--check");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const directory = await mkdtemp(tmpdir() + "/complete-migration-source-");
const seed = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/monte-carlo-strategy-plans-source.json",
      import.meta.url,
    ),
  ),
);
const ids = { ...seed.ids };
const manifest = {
  source_version: "26.10.0",
  source_commit: seed.source_commit,
  cases: [],
};
const prior = check
  ? JSON.parse(await readFile(new URL("manifest.json", root)))
  : null;
try {
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
  const engine = await api.init({ dataDir: directory });
  const send = (command, body) => engine.send(command, body);
  await mkdir(root, { recursive: true });
  for (const name of ["empty", "complete", "large", "deleted-history"]) {
    const path = new URL(name + ".zip", root);
    if (check) {
      const bytes = await readFile(path);
      assert.equal(
        hash(bytes),
        prior.cases.find((c) => c.name === name).archive_sha256,
      );
      await api.importBudget(bytes, { filename: name + ".zip" });
    } else {
      if (name === "empty")
        await api.runImport("Empty qualification", async () => {});
      if (name === "complete") {
        const base = await readFile(
          new URL(
            "../tests/fixtures/monte-carlo-strategy-plans-26.10.zip",
            import.meta.url,
          ),
        );
        assert.equal(hash(base), seed.archive_sha256);
        await api.importBudget(base, { filename: "complete-seed.zip" });
        ids.accountGroup = await send("account-group-create", {
          name: "Everyday accounts",
        });
        await send("account-update", {
          id: ids.checking,
          account_group_id: ids.accountGroup,
        });
        await api.createTag({
          tag: "manual",
          color: "#336699",
          description: "Manual purchase",
        });
        await api.updatePayee(ids.market, {
          favorite: true,
          learn_categories: false,
        });
        await send("payee-location-create", {
          payeeId: ids.market,
          latitude: 40,
          longitude: -74,
        });
        ids.alias = await api.createPayee({ name: "Old Market" });
        await api.mergePayees(ids.market, [ids.alias]);
        await send("notes-save", {
          id: ids.food,
          note: "Food reserve\n#template 72.99\n#goal 1000\n#cleanup Pool source",
        });
        await send("notes-save", {
          id: ids.empty,
          note: "#cleanup Pool sink 2",
        });
        await send("budget/store-note-templates");
        await send("budget/create-cleanup-group", { name: "Pool" });
        await send("budget/store-note-cleanups");
        await send("budget/apply-goal-template", { month: "2026-11" });
        await send("rule-add", {
          stage: "pre",
          conditionsOp: "and",
          conditions: [{ field: "payee", op: "is", value: ids.market }],
          actions: [{ op: "set", field: "category", value: ids.food }],
        });
        await send("schedule/create", {
          schedule: {
            id: randomUUID(),
            name: "Manual rent",
            posts_transaction: false,
          },
          conditions: [
            { field: "account", op: "is", value: ids.checking },
            { field: "payee", op: "is", value: ids.market },
            { field: "amount", op: "is", value: -15000 },
            {
              field: "date",
              op: "is",
              value: {
                start: "2030-01-15",
                frequency: "monthly",
                endMode: "never",
              },
            },
          ],
        });
        await send("filter-create", {
          state: {
            name: "Food expenses",
            conditionsOp: "and",
            conditions: [{ field: "category", op: "is", value: ids.food }],
          },
          filters: [],
        });
        await send("preferences/save", {
          id: "dateFormat",
          value: "dd/MM/yyyy",
        });
        await send("preferences/save", { id: "hideFraction", value: "false" });
      }
      if (name === "large") {
        await api.importBudget(await readFile(new URL("complete.zip", root)), {
          filename: "large-seed.zip",
        });
        await api.addTransactions(
          ids.checking,
          Array.from({ length: 10000 }, (_, i) => ({
            id: `50500000-0000-4000-8000-${String(i).padStart(12, "0")}`,
            date: `2026-${String(1 + (i % 10)).padStart(2, "0")}-${String(1 + (i % 28)).padStart(2, "0")}`,
            amount: -(1 + (i % 9999)),
            payee: ids.market,
            category: ids.food,
            notes: `Purchase ${i} #manual`,
            cleared: i % 3 !== 0,
          })),
        );
      }
      if (name === "deleted-history") {
        await api.importBudget(await readFile(new URL("large.zip", root)), {
          filename: "deleted-seed.zip",
        });
        await send("transactions-batch-update", {
          deleted: Array.from({ length: 10000 }, (_, i) => ({
            id: `50500000-0000-4000-8000-${String(i).padStart(12, "0")}`,
          })),
        });
      }
      await writeFile(path, await api.exportBudget(), { flag: "wx" });
      await api.importBudget(await readFile(path), {
        filename: name + "-reload.zip",
      });
    }
    const bytes = await readFile(path),
      source = sourceArchive(bytes);
    const accounts = await api.getAccounts();
    const ledger = (
      await api.aqlQuery(
        api.q("transactions").select("*").orderBy({ id: "asc" }),
      )
    ).data;
    const balances = [];
    for (const account of accounts) {
      const { data } = await api.aqlQuery(
        api
          .q("transactions")
          .filter({ account: account.id })
          .calculate({ $sum: "$amount" }),
      );
      balances.push({ id: account.id, balance: data ?? 0 });
    }
    manifest.cases.push({
      name,
      archive_sha256: hash(bytes),
      source_counts: Object.fromEntries(
        Object.entries(source.tables).map(([table, rows]) => [
          table,
          rows.length,
        ]),
      ),
      ledger_sha256: hash(JSON.stringify(ledger)),
      balances,
    });
  }
  if (check) assert.deepEqual(manifest, prior);
  else
    await writeFile(
      new URL("manifest.json", root),
      JSON.stringify(manifest, null, 2) + "\n",
      { flag: "wx" },
    );
  console.log(
    "PASS four genuine Actual archives: combined features, 10000 purchases, deleted history, empty; reloaded source observations",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

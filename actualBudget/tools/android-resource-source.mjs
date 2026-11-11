// Disposable source copies and independent post-measurement Actual26.10 oracle.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash, randomUUID } from "node:crypto";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
const packagePath = resolve(
  dirname(require.resolve("@actual-app/api")),
  "../package.json",
);
assert.equal(JSON.parse(await readFile(packagePath)).version, "26.10.0");
const [mode, labArg, outputArg, selected, samplesArg = "20"] =
  process.argv.slice(2);
assert.ok(["prepare", "verify"].includes(mode) && labArg && outputArg);
const lab = resolve(labArg),
  output = resolve(outputArg),
  samples = Number(samplesArg);
const config = JSON.parse(await readFile(lab + "/baseline.json"));
assert.equal(new URL(config.server).hostname, "127.0.0.1");
assert.ok(Number.isInteger(samples) && samples >= 1 && samples <= 20);
const cache = await mkdtemp(output + "/source-oracle-");
const hash = (rows) =>
  createHash("sha256").update(JSON.stringify(rows)).digest("hex");
async function balances(account) {
  const sum = async (filter) =>
    (
      await api.aqlQuery(
        api.q("transactions").filter(filter).calculate({ $sum: "$amount" }),
      )
    ).data ?? 0;
  const total = await sum({ account }),
    cleared = await sum({ account, cleared: true });
  return { total, cleared, uncleared: total - cleared };
}
try {
  const engine = await api.init({
    dataDir: cache,
    serverURL: config.server,
    password: config.password,
  });
  if (mode === "prepare") {
    const cases = [];
    for (const original of config.cases.filter(
      (c) => !selected || c.name === selected,
    )) {
      const remote = (await api.getBudgets()).find(
        (b) => b.cloudFileId === original.cloud_id,
      );
      assert.ok(remote?.groupId);
      await api.downloadBudget(remote.groupId);
      const rows = (
        await api.aqlQuery(
          api.q("transactions").select("*").orderBy({ id: "asc" }),
        )
      ).data;
      assert.equal(rows.length, original.transactions);
      assert.equal(
        hash(rows),
        original.ledger_sha256,
        "Original source financial fixture changed",
      );
      const local = (await api.getBudgets()).find(
        (b) => b.cloudFileId === original.cloud_id,
      );
      const label =
        "AAA Android baseline " +
        original.name +
        " " +
        randomUUID().slice(0, 6);
      await engine.send("close-budget");
      const id = await engine.send("duplicate-budget", {
        id: local.id,
        newName: label,
        cloudSync: true,
        open: "copy",
      });
      await api.sync();
      const copy = (await api.getBudgets()).find((b) => b.id === id);
      assert.ok(copy?.groupId && copy?.cloudFileId);
      const copied = (
        await api.aqlQuery(
          api.q("transactions").select("*").orderBy({ id: "asc" }),
        )
      ).data;
      assert.equal(hash(copied), hash(rows));
      const account = (await api.getAccounts()).find(
        (a) => a.id === original.account,
      );
      assert.ok(account);
      cases.push({
        name: original.name,
        label,
        account: original.account,
        account_name: account.name,
        cloud_id: copy.cloudFileId,
        group_id: copy.groupId,
        local_id: id,
        transactions: original.transactions,
        ledger_sha256: hash(copied),
        source_archive_sha256: original.source_archive_sha256,
        before: await balances(original.account),
      });
      await engine.send("close-budget");
    }
    await writeFile(
      output + "/source-cases.json",
      JSON.stringify({ cases }, null, 2) + "\n",
      { flag: "wx" },
    );
    console.log("Prepared", cases.length, "unchanged source copies");
  } else {
    const fixture = JSON.parse(await readFile(output + "/source-cases.json"));
    const c = fixture.cases.find((c) => c.name === selected);
    assert.ok(c);
    await api.downloadBudget(c.group_id);
    const payees = await api.getPayees();
    const prefix = "Resource expense " + c.label + " ";
    const all = (await api.aqlQuery(api.q("transactions").select("*"))).data;
    const purchases = [];
    for (let i = 0; i < samples; i++) {
      const matches = payees.filter((p) => p.name === prefix + i);
      assert.equal(matches.length, 1);
      const rows = all.filter(
        (r) => r.payee === matches[0].id && r.account === c.account,
      );
      assert.equal(rows.length, 1);
      assert.equal(rows[0].amount, -427);
      assert.equal(Boolean(rows[0].cleared), true);
      purchases.push({
        id: rows[0].id,
        amount: rows[0].amount,
        date: rows[0].date,
        cleared: rows[0].cleared,
      });
    }
    assert.equal(new Set(purchases.map((r) => r.id)).size, samples);
    assert.equal(all.length, c.transactions + samples);
    const after = await balances(c.account);
    assert.equal(after.total, c.before.total - 427 * samples);
    assert.equal(after.cleared, c.before.cleared - 427 * samples);
    assert.equal(after.uncleared, c.before.uncleared);
    await writeFile(
      output + "/" + selected + "-financial-proof.json",
      JSON.stringify(
        {
          added_rows: samples,
          before: c.before,
          after,
          purchases,
          source_version: "26.10.0",
        },
        null,
        2,
      ) + "\n",
      { flag: "wx" },
    );
    console.log("PASS source financial proof", selected, samples);
  }
} finally {
  await api.shutdown();
  await rm(cache, { recursive: true, force: true });
}

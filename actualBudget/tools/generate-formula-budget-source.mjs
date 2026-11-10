// Small carry/goal/method supplement, unchanged production three-pass hook.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { formulaSourceRuntime } from "./formula-source-runtime.mjs";
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
  directory = await mkdtemp(tmpdir() + "/formula-budget-source-"),
  seed = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/budget-template-financial.json",
        import.meta.url,
      ),
    ),
  ),
  root = new URL("../tests/fixtures/", import.meta.url),
  file = new URL("formula-budget-source.json", root),
  sha = (v) => createHash("sha256").update(v).digest("hex"),
  check = process.argv.includes("--check");
try {
  const engine = await api.init({ dataDir: directory }),
    cases = [];
  for (const method of ["envelope", "tracking"]) {
    const original = seed.cases.find(
        (c) => c.name === method + "-multiline-long-goal",
      ),
      bytes = await readFile(
        new URL("budget-template-financial/" + original.archive.filename, root),
      );
    assert.equal(sha(bytes), original.archive.sha256);
    await api.importBudget(bytes, {
      filename: method + "-existing-budget.zip",
    });
    const ledger = () =>
        api.aqlQuery(
          api
            .q("transactions")
            .options({ splits: "all" })
            .select(["id", "amount", "account", "category", "date"])
            .orderBy("id"),
        ),
      before = await ledger();
    await engine.send("budget/overwrite-goal-template", { month: "2026-11" });
    assert.deepEqual(
      await ledger(),
      before,
      "Goal preparation must not alter transactions",
    );
    const runtime = formulaSourceRuntime(api, engine),
      accounts = await api.getAccounts(),
      queries = {
        A: {
          conditions: [{ field: "category", op: "is", value: original.ids.A }],
          timeFrame: { mode: "static", start: "2026-11", end: "2026-11" },
        },
      },
      observations = [];
    for (const dimension of [
      "budgeted",
      "spent",
      "goal",
      "balance_start",
      "balance_end",
    ]) {
      const formula = `=BUDGET_QUERY("${dimension}",QUERY_EXTRACT_CATEGORIES("A"),QUERY_EXTRACT_TIMEFRAME_START("A"),QUERY_EXTRACT_TIMEFRAME_END("A"))`;
      observations.push({
        name: dimension,
        formula,
        queries,
        output: await runtime.evaluate(formula, queries, undefined, accounts),
      });
    }
    const filename = "formula-budget-" + method + "-26.10.zip",
      archive = check
        ? await readFile(new URL(filename, root))
        : await api.exportBudget();
    if (!check)
      await writeFile(new URL(filename, root), archive, { flag: "wx" });
    cases.push({
      method,
      seed: original.archive,
      preparation: {
        message: "budget/overwrite-goal-template",
        args: { month: "2026-11" },
      },
      archive: { filename, sha256: sha(archive) },
      source_hashes: runtime.hashes,
      ledger: before.data,
      observations,
    });
  }
  assert.ok(
    cases.some(
      (c) => c.observations.find((o) => o.name === "goal").output.result > 0,
    ),
    "Supplement must prove nonzero goal",
  );
  assert.ok(
    cases.some(
      (c) =>
        c.observations.find((o) => o.name === "balance_start").output.result >
        0,
    ),
    "Supplement must prove nonzero carry",
  );
  const output = { source_commit: seed.source_commit, clock, cases };
  if (check) assert.deepEqual(output, JSON.parse(await readFile(file)));
  else
    await writeFile(file, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS genuine formula method/carry/goal supplement10 observations",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

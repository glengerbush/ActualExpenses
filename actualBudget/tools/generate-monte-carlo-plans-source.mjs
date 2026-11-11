// Genuine saved-widget migration input, created/exported/reloaded by Actual26.10.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
const root = new URL("../tests/fixtures/", import.meta.url);
const strategies = process.argv.includes("--strategies");
const prefix = strategies ? "monte-carlo-strategy-plans" : "monte-carlo-plans";
const file = new URL(`${prefix}-source.json`, root);
const archive = new URL(`${prefix}-26.10.zip`, root);
const check = process.argv.includes("--check");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const seed = JSON.parse(
  await readFile(new URL("crossover-source-26.10.json", root)),
);
const scenarios = JSON.parse(
  await readFile(
    new URL(
      `monte-carlo-${strategies ? "strategies" : "baseline"}-source.json`,
      root,
    ),
  ),
);
const prior = check ? JSON.parse(await readFile(file)) : null;
const directory = await mkdtemp(tmpdir() + "/monte-carlo-plans-source-");
try {
  const engine = await api.init({ dataDir: directory });
  const bytes = await readFile(
    check ? archive : new URL("crossover-source-26.10.zip", root),
  );
  assert.equal(hash(bytes), check ? prior.archive_sha256 : seed.archive_sha256);
  await api.importBudget(bytes, { filename: "monte-plans-source.zip" });
  const ledger = async () =>
    (
      await api.aqlQuery(
        api.q("transactions").select("*").orderBy({ id: "asc" }),
      )
    ).data;
  const before = await ledger();
  if (!check) {
    const names = strategies
      ? [
          "guardrails-rising",
          "ratcheting-rising",
          "floor-ceiling-rising",
          "boundaries-rising",
        ]
      : [
          "new-plan-defaults",
          "legacy-single-pot-no-surplus",
          "progressive-tax-before-tax-contributions",
          "historical-bootstrap-mixes",
        ];
    for (const [i, name] of names.entries()) {
      const selected = scenarios.cases.find((test) => test.name === name);
      const meta =
        selected.meta === null ? null : structuredClone(selected.meta);
      if (meta) {
        meta.name = name;
        if (strategies)
          meta.withdrawalStrategy = [
            "proportional",
            "sequential",
            "best-performer",
            "target-mix",
          ][i];
        if (name.startsWith("progressive"))
          meta.pots[0].accountId = seed.ids.investment;
      }
      await engine.send("dashboard-add-widget", {
        dashboard_page_id: seed.ids.dashboard,
        type: "monte-carlo-card",
        x: 0,
        y: 36 + i * 6,
        width: 12,
        height: 6,
        meta,
      });
    }
    await engine.send("preferences/save", {
      id: "flags.monteCarloReport",
      value: "false",
    });
    assert.deepEqual(await ledger(), before);
    await writeFile(archive, await api.exportBudget(), { flag: "wx" });
    await api.importBudget(await readFile(archive), {
      filename: "monte-plans-reload.zip",
    });
  }
  assert.deepEqual(await ledger(), before);
  const widgets = (
    await api.aqlQuery(
      api
        .q("dashboard")
        .filter({ type: "monte-carlo-card" })
        .select("*")
        .orderBy({ id: "asc" }),
    )
  ).data;
  assert.equal(widgets.length, 4);
  const preferences = await engine.send("preferences/get");
  assert.equal(preferences["flags.monteCarloReport"], "false");
  const output = JSON.parse(
    JSON.stringify({
      source_version: "26.10.0",
      source_commit: scenarios.source_commit,
      seed_sha256: seed.archive_sha256,
      archive_sha256: hash(await readFile(archive)),
      ledger_sha256: hash(JSON.stringify(before)),
      ids: seed.ids,
      experimental_enabled: false,
      widgets,
      accounts: await api.getAccounts(),
    }),
  );
  if (check) assert.deepEqual(output, prior);
  else
    await writeFile(file, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS four genuine saved plans, exact metadata, disabled opt-in, archive reload and unchanged ledger",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

// Unchanged FormulaResult display callback/useFormat and actual engine preferences.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { formulaSourceRuntime } from "./formula-source-runtime.mjs";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api"),
  root = new URL("../tests/fixtures/", import.meta.url);
const directory = await mkdtemp(tmpdir() + "/formula-display-");
const file = new URL("formula-display-source.json", root),
  check = process.argv.includes("--check");
const seeds = JSON.parse(
  await readFile(new URL("formula-formatting-source.json", root)),
).cases;
const definitions = [
  { id: "absent-en", seed: "absent", locale: "en" },
  { id: "absent-fr", seed: "absent", locale: "fr" },
  ...["EUR", "CHF", "JPY"].map((seed) => ({ id: seed, seed, locale: "en" })),
  {
    id: "hidden-fraction",
    seed: "EUR",
    locale: "fr",
    source: { hideFraction: "true" },
    changes: { hide_fraction: true },
  },
  {
    id: "explicit-after",
    seed: "EUR",
    locale: "fr",
    source: {
      numberFormat: "space-comma",
      currencySymbolPosition: "after",
      currencySpaceBetweenAmountAndSymbol: "true",
    },
    changes: {
      number_format: "space-comma",
      currency_symbol_position: "after",
      currency_space: true,
    },
  },
  {
    id: "explicit-apostrophe",
    seed: "CHF",
    locale: "en",
    source: { numberFormat: "apostrophe-dot" },
    changes: { number_format: "apostrophe-dot" },
  },
];
const values = [
  1234.567, -1234.567, -0.005, -0.5, 22517998136852.47, 22517998136852.48, 1e20,
];
try {
  const engine = await api.init({ dataDir: directory }),
    cases = [];
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
  for (const definition of definitions) {
    const seed = seeds.find((s) => s.id === definition.seed),
      bytes = await readFile(new URL(seed.archive.filename, root));
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      seed.archive.sha256,
    );
    await api.importBudget(bytes, { filename: seed.archive.filename });
    const before = await ledger(),
      runtime = formulaSourceRuntime(api, engine),
      observations = [];
    for (const [id, value] of Object.entries(definition.source ?? {}))
      await engine.send("preferences/save", { id, value });
    for (const value of values) {
      const formula = "=" + String(value),
        output = await runtime.evaluate(
          formula,
          {},
          {},
          await api.getAccounts(),
          definition.locale,
        );
      let display = null,
        error = null;
      try {
        display = await runtime.display(output.result, definition.locale);
      } catch (e) {
        assert.match(e.message, /^safeNumber:/);
        error = e.message;
      }
      observations.push({ formula, output, display, error });
    }
    assert.deepEqual(await ledger(), before);
    cases.push({
      ...definition,
      archive: seed.archive,
      observations,
      source_hashes: runtime.hashes,
    });
  }
  const result = {
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    cases,
  };
  if (check) assert.deepEqual(result, JSON.parse(await readFile(file)));
  else
    await writeFile(file, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS56 unchanged source numeric-card displays/errors, preference/locale variants and original ledger preserved",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

// Real source preference changes; only preferences change, never ledger rows.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { formulaSourceRuntime } from "./formula-source-runtime.mjs";
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
);
const api = require("@actual-app/api"),
  root = new URL("../tests/fixtures/", import.meta.url);
const directory = await mkdtemp(tmpdir() + "/formula-formatting-edits-");
const file = new URL("formula-formatting-edits-source.json", root),
  check = process.argv.includes("--check");
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const seed = JSON.parse(
  await readFile(new URL("formula-formatting-source.json", root)),
).cases.find((c) => c.id === "EUR");
const bytes = await readFile(new URL(seed.archive.filename, root));
assert.equal(sha(bytes), seed.archive.sha256);
try {
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(bytes, { filename: seed.archive.filename });
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
  const before = await ledger(),
    steps = [],
    runtime = formulaSourceRuntime(api, engine);
  async function observe() {
    const observations = [];
    for (const input of seed.observations)
      observations.push({
        ...input,
        output: await runtime.evaluate(
          input.formula,
          {},
          {},
          await api.getAccounts(),
          input.locale,
        ),
      });
    assert.deepEqual(await ledger(), before);
    return observations;
  }
  for (const step of [
    {
      name: "unrelated",
      source: { dateFormat: "yyyy-MM-dd", "flags.formulaMode": "true" },
      changes: {
        date_format: "yyyy-MM-dd",
        experimental: { formulaMode: true },
      },
    },
    {
      name: "explicit",
      source: {
        numberFormat: "apostrophe-dot",
        currencySymbolPosition: "before",
      },
      changes: {
        number_format: "apostrophe-dot",
        currency_symbol_position: "before",
      },
    },
    {
      name: "explicit-default",
      source: { numberFormat: "comma-dot", currencySymbolPosition: "after" },
      changes: {
        number_format: "comma-dot",
        currency_symbol_position: "after",
      },
    },
  ]) {
    for (const [id, value] of Object.entries(step.source))
      await engine.send("preferences/save", { id, value });
    steps.push({ ...step, observations: await observe() });
  }
  for (const id of ["numberFormat", "currencySymbolPosition"])
    await engine.send("preferences/save", { id, value: null });
  const filename = "formula-formatting-null-26.10.zip";
  const nullBytes = check
    ? await readFile(new URL(filename, root))
    : await api.exportBudget();
  if (!check)
    await writeFile(new URL(filename, root), nullBytes, { flag: "wx" });
  await api.importBudget(nullBytes, { filename });
  const nullCase = {
    archive: { filename, sha256: sha(nullBytes) },
    observations: await observe(),
  };
  const result = {
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    seed: seed.archive,
    initial: seed.observations,
    steps,
    null_case: nullCase,
    source_hashes: runtime.hashes,
  };
  if (check) assert.deepEqual(result, JSON.parse(await readFile(file)));
  else
    await writeFile(file, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS unchanged source preference edits, explicit defaults and null-preference export/reload; original ledger preserved",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

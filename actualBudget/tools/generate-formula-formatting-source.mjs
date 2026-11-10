// Observe missing optional preferences through the unchanged production hook.
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
const directory = await mkdtemp(tmpdir() + "/formula-formatting-");
const filename = new URL("formula-formatting-source.json", root),
  check = process.argv.includes("--check");
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const seed = await readFile(new URL("reports-source-26.10.zip", root));
assert.equal(
  sha(seed),
  JSON.parse(await readFile(new URL("reports-source-26.10.json", root)))
    .archive_sha256,
);
const base = { filename: "reports-source-26.10.zip", sha256: sha(seed) };
const locales = [
  "ca",
  "de",
  "en-GB",
  "en",
  "es",
  "fr",
  "it",
  "nb-NO",
  "nl",
  "pl",
  "pt-BR",
  "ru",
  "uk",
  "zh-Hans",
  "zh-Hant",
  "fr-FR",
  "de-CH",
  "ar-EG",
  "hi-IN",
];
const inputs = [
  ["number", "=FORMATNUMBER(1234.567)"],
  ["currency", "=FORMATCURRENCY(-1234.567)"],
  ["explicit-decimals", "=FORMATNUMBER(1234.567,3)"],
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
  for (const code of [null, "EUR", "CHF", "JPY"]) {
    await api.importBudget(seed, { filename: "formatting-seed.zip" });
    const before = await ledger();
    let archive = base;
    if (code) {
      await engine.send("preferences/save", {
        id: "flags.currency",
        value: "true",
      });
      await engine.send("preferences/save", {
        id: "defaultCurrencyCode",
        value: code,
      });
      const name = `formula-formatting-${code.toLowerCase()}-26.10.zip`;
      const bytes = check
        ? await readFile(new URL(name, root))
        : await api.exportBudget();
      if (!check) await writeFile(new URL(name, root), bytes, { flag: "wx" });
      archive = { filename: name, sha256: sha(bytes) };
      await api.importBudget(bytes, { filename: name });
    }
    assert.deepEqual(await ledger(), before);
    const preferences = (
      await api.aqlQuery(api.q("preferences").select("*").orderBy("id"))
    ).data;
    assert.ok(
      !preferences.some((p) =>
        ["numberFormat", "currencySymbolPosition"].includes(p.id),
      ),
    );
    const runtime = formulaSourceRuntime(api, engine),
      observations = [];
    for (const locale of code ? ["en-US"] : locales) {
      for (const [name, formula] of inputs)
        observations.push({
          name: `${locale}-${name}`,
          formula,
          locale,
          output: await runtime.evaluate(
            formula,
            {},
            {},
            await api.getAccounts(),
            locale,
          ),
        });
    }
    assert.deepEqual(await ledger(), before);
    cases.push({
      id: code ?? "absent",
      archive,
      preferences,
      source_hashes: runtime.hashes,
      observations,
    });
  }
  const result = {
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    cases,
  };
  if (check) assert.deepEqual(result, JSON.parse(await readFile(filename)));
  else
    await writeFile(filename, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    `PASS ${cases.reduce((n, c) => n + c.observations.length, 0)} unchanged-source missing-format observations; ${locales.length} locales, EUR/CHF/JPY defaults, original ledger preserved`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

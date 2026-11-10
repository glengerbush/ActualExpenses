// Reproduce trusted production assets from pinned source/dependencies; no user JS.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
const dependencies = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  web = createRequire(new URL("../web/package.json", import.meta.url)),
  { rolldown } = await import(web.resolve("rolldown")),
  base = fileURLToPath(
    new URL(
      "../reference/actual-26.10.0/packages/loot-core/src/",
      import.meta.url,
    ),
  ),
  output = new URL("../server/src/formulas/vendor/", import.meta.url),
  hashes = {},
  sha = (v) => createHash("sha256").update(v).digest("hex");
assert.equal(dependencies("hyperformula").HyperFormula.version, "3.5.0");
const bundle = await rolldown({
  input: base + "shared/formulas/customFunctions.ts",
  external: ["hyperformula"],
  plugins: [
    {
      name: "pinned-source-aliases",
      resolveId(name) {
        if (name.startsWith("#shared/"))
          return base + "shared/" + name.slice(8) + ".ts";
        if (name === "date-fns") return dependencies.resolve(name);
      },
      async load(id) {
        if (id.startsWith(base)) {
          const text = await readFile(id, "utf8");
          hashes[id.slice(base.length)] = sha(text);
          return text;
        }
      },
    },
  ],
});
const generated = await bundle.generate({
  format: "iife",
  name: "ActualFormulaFunctions",
  globals: { hyperformula: "HyperFormula" },
  minify: true,
});
await bundle.close();
assert.equal(generated.output.length, 1);
const assets = {
  "custom-functions.js": generated.output[0].code,
  "enUS-3.5.0.json": JSON.stringify(
    dependencies("hyperformula/i18n/languages/enUS").default,
  ),
  "hyperformula-3.5.0.min.js": await readFile(
    new URL(
      "../dist/hyperformula.full.min.js",
      new URL("file://" + dependencies.resolve("hyperformula")),
    ),
    "utf8",
  ),
};
for (const [name, text] of Object.entries(assets)) {
  if (process.argv.includes("--check"))
    assert.equal(await readFile(new URL(name, output), "utf8"), text, name);
  else await writeFile(new URL(name, output), text);
}
const manifest = {
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  hyperformula: "3.5.0",
  source_hashes: hashes,
  assets: Object.fromEntries(
    Object.entries(assets).map(([k, v]) => [k, sha(v)]),
  ),
};
if (process.argv.includes("--check"))
  assert.deepEqual(
    JSON.parse(await readFile(new URL("manifest.json", output))),
    manifest,
  );
else
  await writeFile(
    new URL("manifest.json", output),
    JSON.stringify(manifest, null, 2) + "\n",
  );
console.log("PASS reproducible trusted formula runtime assets");

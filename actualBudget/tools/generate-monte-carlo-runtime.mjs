import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
const web = createRequire(new URL("../web/package.json", import.meta.url));
const { rolldown } = await import(web.resolve("rolldown"));
const base = fileURLToPath(
  new URL("../reference/actual-26.10.0/", import.meta.url),
);
const directory = new URL("../server/src/monte_carlo/vendor/", import.meta.url);
const hash = (value) => createHash("sha256").update(value).digest("hex");
const license = await readFile(base + "LICENSE.txt", "utf8");
assert.equal(
  await readFile(
    new URL(
      "../server/src/formulas/vendor/ACTUAL-LICENSE.txt",
      import.meta.url,
    ),
    "utf8",
  ),
  license,
);
const utility = await readFile(
  base + "packages/loot-core/src/shared/util.ts",
  "utf8",
);
const constant = utility.match(/^export const MAX_SAFE_NUMBER = .+;$/m)?.[0];
assert.ok(constant);
const sources = { "packages/loot-core/src/shared/util.ts": hash(utility) };
const bundle = await rolldown({
  input:
    base +
    "packages/desktop-client/src/components/reports/reports/monte-carlo/monteCarloSimulation.ts",
  plugins: [
    {
      name: "pinned-monte-carlo",
      resolveId(id) {
        if (id === "@actual-app/core/shared/util") return "\0display-constant";
      },
      async load(id) {
        if (id === "\0display-constant") return constant;
        if (id.startsWith(base)) {
          const text = await readFile(id, "utf8");
          sources[id.slice(base.length)] = hash(text);
          return text;
        }
      },
    },
  ],
});
const result = await bundle.generate({
  format: "iife",
  name: "ActualMonteCarlo",
  minify: true,
});
await bundle.close();
assert.equal(result.output.length, 1);
const asset = result.output[0].code;
const manifest = {
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  source_hashes: sources,
  asset_sha256: hash(asset),
  actual_license_sha256: hash(license),
};
await mkdir(directory, { recursive: true });
if (process.argv.includes("--check")) {
  assert.equal(
    await readFile(new URL("simulation.js", directory), "utf8"),
    asset,
  );
  assert.deepEqual(
    JSON.parse(await readFile(new URL("manifest.json", directory))),
    manifest,
  );
} else {
  await writeFile(new URL("simulation.js", directory), asset);
  await writeFile(
    new URL("manifest.json", directory),
    JSON.stringify(manifest, null, 2) + "\n",
  );
}
console.log(
  "PASS reproducible pinned Monte Carlo asset; no application transport or host dependencies",
);

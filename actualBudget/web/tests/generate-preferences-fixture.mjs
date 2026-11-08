// Run pinned Actual formatting code, not the rewrite's implementation.
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import vm from "node:vm";
import ts from "typescript";
const source = await readFile(
  new URL(
    "../../reference/actual-26.10.0/packages/loot-core/src/shared/util.ts",
    import.meta.url,
  ),
  "utf8",
);
const section = source.slice(
  source.indexOf("const NUMBER_FORMATS ="),
  source.indexOf("export function integerToCurrencyWithDecimal("),
);
const baseline = { exports: {}, Intl };
vm.runInNewContext(
  ts.transpileModule(section, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText,
  baseline,
);
const cases = [];
for (const format of [
  "comma-dot",
  "dot-comma",
  "space-comma",
  "apostrophe-dot",
  "comma-dot-in",
]) {
  for (const hide_fraction of [false, true]) {
    for (const decimal_places of [0, 2]) {
      for (const amount of [
        -123456789, -250, -150, -50, -1, 0, 1, 50, 150, 250, 123456789,
      ]) {
        const formatter = baseline.exports.getNumberFormat({
          format,
          hideFraction: hide_fraction,
          decimalPlaces: hide_fraction ? 0 : decimal_places,
        }).formatter;
        cases.push({
          number_format: format,
          hide_fraction,
          decimal_places,
          amount,
          expected: baseline.exports.integerToCurrency(
            amount,
            formatter,
            decimal_places,
          ),
        });
      }
    }
  }
}
const output =
  JSON.stringify(
    {
      baseline: "Actual 26.10.0",
      source: "packages/loot-core/src/shared/util.ts",
      source_sha256: createHash("sha256").update(source).digest("hex"),
      cases,
    },
    null,
    2,
  ) + "\n";
const path = new URL(
  "./fixtures/preferences-format-26.10.0.json",
  import.meta.url,
);
if (process.argv.includes("--check")) {
  if ((await readFile(path, "utf8")) !== output)
    throw new Error("Preference oracle fixture changed");
} else {
  await mkdir(new URL("./fixtures/", import.meta.url), { recursive: true });
  await writeFile(path, output);
}
console.log(`Verified ${cases.length} baseline formatting results`);

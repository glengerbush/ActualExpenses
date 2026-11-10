import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
const dependencies = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const web = createRequire(new URL("../web/package.json", import.meta.url));
const { rolldown } = await import(web.resolve("rolldown")),
  ts = web("typescript");
const base = fileURLToPath(
  new URL(
    "../reference/actual-26.10.0/packages/loot-core/src/",
    import.meta.url,
  ),
);
const output = new URL("../server/src/rules/vendor/", import.meta.url),
  hashes = {},
  sha = (x) => createHash("sha256").update(x).digest("hex");
const actualEngine = await readFile(
  dependencies.resolve("@actual-app/api"),
  "utf8",
);
const handlebarsVersion = actualEngine
  .split(
    "//#region ../../node_modules/handlebars/dist/cjs/handlebars/base.js",
  )[1]
  ?.match(/exports.VERSION = "([^"]+)"/)[1];
assert.equal(handlebarsVersion, "4.7.9");
assert.equal(dependencies("handlebars").VERSION, handlebarsVersion);
assert.equal(dependencies("hyperformula").HyperFormula.version, "3.5.0");
const assertions = await readFile(base + "server/rules/rule-utils.ts", "utf8");
const syntax = ts.createSourceFile(
  "rule-utils.ts",
  assertions,
  ts.ScriptTarget.Latest,
  true,
);
const assertion = syntax.statements.find(
  (n) => ts.isFunctionDeclaration(n) && n.name?.text === "assert",
);
assert.ok(assertion);
hashes["server/rules/rule-utils.ts"] = sha(assertions);
const bundle = await rolldown({
  input: fileURLToPath(
    new URL("./rule-actions-runtime-entry.ts", import.meta.url),
  ),
  external: ["hyperformula", "hyperformula/i18n/languages/enUS"],
  plugins: [
    {
      name: "pinned-rule-actions",
      resolveId(name, importer) {
        if (name === "#platform/server/log") return "\0rule-log";
        // Action only imports assert. Extract that unchanged declaration so unrelated
        // scheduling module initialization cannot enter the expression sandbox.
        if (
          name === "./rule-utils" &&
          importer === base + "server/rules/action.ts"
        )
          return "\0rule-assert";
        if (name === "#shared/platform") return "\0rule-platform";
        if (name.startsWith("#")) {
          const path = base + name.slice(1);
          for (const ext of [".ts", "/index.ts"]) {
            if (existsSync(path + ext)) return path + ext;
          }
        }
        if (name === "handlebars")
          return dependencies.resolve("handlebars/dist/handlebars.js");
        if (name === "date-fns" || name.startsWith("date-fns/"))
          return dependencies.resolve(name);
        if (
          !name.startsWith(".") &&
          !name.startsWith("/") &&
          !name.startsWith("\0")
        )
          try {
            return dependencies.resolve(name);
          } catch {}
      },
      async load(path) {
        if (path === "\0rule-log")
          return "export const logger={debug(){},error(){},warn(){},info(){}}";
        if (path === "\0rule-platform")
          return "export const OS='linux',env='server',isPlaywright=false";
        if (path === "\0rule-assert")
          return {
            code:
              "import {RuleError} from " +
              JSON.stringify(base + "server/errors.ts") +
              ";" +
              assertion.getText(syntax),
            moduleType: "ts",
          };
        if (path.startsWith(base)) {
          const source = await readFile(path, "utf8");
          hashes[path.slice(base.length)] = sha(source);
          return source;
        }
      },
    },
  ],
});
const generated = await bundle.generate({
  format: "iife",
  name: "ActualRuleActions",
  globals: {
    hyperformula: "HyperFormula",
    "hyperformula/i18n/languages/enUS": "ActualRuleLanguage",
  },
  minify: true,
});
await bundle.close();
assert.equal(generated.output.length, 1);
const text = generated.output[0].code;
const license = await readFile(
  new URL(
    "LICENSE",
    new URL("file://" + dependencies.resolve("handlebars/package.json")),
  ),
  "utf8",
);
const manifest = {
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  actual_engine_sha256: sha(actualEngine),
  hyperformula: dependencies("hyperformula").HyperFormula.version,
  handlebars: dependencies("handlebars").VERSION,
  source_hashes: hashes,
  entry_sha256: sha(
    await readFile(new URL("./rule-actions-runtime-entry.ts", import.meta.url)),
  ),
  asset_sha256: sha(text),
  handlebars_asset_sha256: sha(
    await readFile(dependencies.resolve("handlebars/dist/handlebars.js")),
  ),
  license_sha256: sha(license),
};
await mkdir(output, { recursive: true });
if (process.argv.includes("--check")) {
  assert.equal(
    await readFile(new URL("rule-actions.js", output), "utf8"),
    text,
  );
  assert.deepEqual(
    JSON.parse(await readFile(new URL("manifest.json", output))),
    manifest,
  );
  assert.equal(
    await readFile(new URL("HANDLEBARS-LICENSE", output), "utf8"),
    license,
  );
} else {
  await writeFile(new URL("rule-actions.js", output), text);
  await writeFile(
    new URL("manifest.json", output),
    JSON.stringify(manifest, null, 2) + "\n",
  );
  await writeFile(new URL("HANDLEBARS-LICENSE", output), license);
}
console.log(
  "PASS reproducible trusted Action/Handlebars bundle",
  text.length,
  "bytes",
);

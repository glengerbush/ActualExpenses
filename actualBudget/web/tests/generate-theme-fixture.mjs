// Read-only execution of pinned Actual theme helpers. No upstream network calls.
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import vm from "node:vm";
import ts from "typescript";
const reference = new URL("../../reference/actual-26.10.0/", import.meta.url);
const helperPath = "packages/desktop-client/src/style/customThemes.ts";
const source = await readFile(new URL(helperPath, reference), "utf8");
const context = { exports: {} };
vm.runInNewContext(
  ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText,
  context,
);
const baseline = context.exports;
const cssInputs = [
  [
    "literal",
    ":root { --color-pageBackground: #123456; --color-pageText: #fff; }",
  ],
  [
    "comments",
    "/* retained source comment */ :root { --color-pageText: #fff; }",
  ],
  [
    "reference",
    ":root { --my-accent: #abcdef; --color-pageTextLink: var(--my-accent); }",
  ],
  ["font-family", ':root { --font-body: "Local Font", sans-serif; }'],
  [
    "embedded-font",
    "@font-face { font-family: Fixture; src: url(data:font/woff2;base64,AAAA); } :root { --font-body: Fixture, sans-serif; }",
  ],
  ["unknown-token", ":root { --color-futureWidget: #123456; }"],
  ["empty-value", ":root { --color-pageText: ; }"],
  ["empty-root", ":root {}"],
  ["selector", "body { color: #fff; }"],
  ["named-color", ":root { --color-pageText: red; }"],
  ["var-fallback", ":root { --color-pageText: var(--missing, #fff); }"],
  [
    "multiple-roots",
    ":root { --color-pageText: #fff; } :root { --color-pageText: #000; }",
  ],
  [
    "import",
    '@import "https://invalid.test/a.css"; :root { --color-pageText: #fff; }',
  ],
  [
    "remote-font",
    "@font-face { font-family: Fixture; src: url(https://invalid.test/font.woff2); } :root { --font-body: Fixture; }",
  ],
  ["nested", ":root { body { color: #fff; } }"],
];
const validation = cssInputs.map(([name, css]) => {
  try {
    return { name, css, valid: true, cleaned: baseline.validateThemeCss(css) };
  } catch (error) {
    return { name, css, valid: false, error: error.message };
  }
});
const theme = {
  id: "fixture",
  name: "Fixture",
  repo: "https://github.com/example/theme",
  cssContent: cssInputs[0][1],
  baseTheme: "dark",
};
const installedInputs = [
  undefined,
  "null",
  "{bad",
  JSON.stringify(theme),
  JSON.stringify({ ...theme, baseTheme: "unknown" }),
  JSON.stringify({ ...theme, cssContent: undefined }),
];
const installed = installedInputs.map((input) => ({
  input: input ?? null,
  parsed: baseline.parseInstalledTheme(input),
}));
const legacyLight = JSON.stringify({
  ...theme,
  overrideCss: ":root { --color-pageText: #111; }",
});
const legacyDark = JSON.stringify({
  ...theme,
  id: "dark",
  overrideCss: ":root { --color-pageText: #222; }",
});
const migrations = [
  { existingOverride: undefined, lightJson: legacyLight, darkJson: legacyDark },
  {
    existingOverride: ":root { --color-pageText: #333; }",
    lightJson: legacyLight,
    darkJson: legacyDark,
  },
  { existingOverride: " ", lightJson: "{bad", darkJson: legacyDark },
  {
    existingOverride: undefined,
    lightJson: JSON.stringify(theme),
    darkJson: undefined,
  },
].map((input) => ({ input, result: baseline.migrateLegacyOverride(input) }));
const catalogPath = "packages/desktop-client/src/data/customThemeCatalog.json";
const catalogSource = await readFile(new URL(catalogPath, reference), "utf8");
const tokens = {};
for (const name of [
  "palette",
  "light",
  "dark",
  "midnight",
  "sidebar-redesign-light",
]) {
  const css = await readFile(
    new URL(`packages/component-library/src/themes/${name}.css`, reference),
    "utf8",
  );
  tokens[name] = {
    sha256: createHash("sha256").update(css).digest("hex"),
    names: [
      ...new Set(
        [...css.matchAll(/(--[A-Za-z0-9_-]+)\s*:/g)].map((match) => match[1]),
      ),
    ].sort(),
  };
}
const fixture = {
  baseline: "Actual 26.10.0",
  source: helperPath,
  source_sha256: createHash("sha256").update(source).digest("hex"),
  catalog: {
    source: catalogPath,
    sha256: createHash("sha256").update(catalogSource).digest("hex"),
    entries: JSON.parse(catalogSource),
  },
  tokens,
  validation,
  installed,
  migrations,
};
const path = new URL("./fixtures/custom-themes-26.10.0.json", import.meta.url);
const output = JSON.stringify(fixture, null, 2) + "\n";
if (process.argv.includes("--check")) {
  if ((await readFile(path, "utf8")) !== output)
    throw Error("Theme oracle fixture changed");
} else {
  await mkdir(new URL("./fixtures/", import.meta.url), { recursive: true });
  await writeFile(path, output);
}
console.log(
  `Verified ${validation.length} CSS cases, ${installed.length} installed records, ${migrations.length} migrations, ${fixture.catalog.entries.length} catalog entries`,
);

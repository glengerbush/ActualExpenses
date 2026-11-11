// Feasibility measurement, not a production endpoint or whole-system benchmark.
// Requires the server's existing debug rquickjs build; all generated files are temporary.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import {
  mkdtemp,
  readFile,
  writeFile,
  readdir,
  rm,
  mkdir,
} from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const execute = promisify(execFile);
const web = createRequire(new URL("../web/package.json", import.meta.url));
const { rolldown } = await import(web.resolve("rolldown"));
const base = fileURLToPath(
  new URL("../reference/actual-26.10.0/", import.meta.url),
);
const entry =
  base +
  "packages/desktop-client/src/components/reports/reports/monte-carlo/monteCarloSimulation.ts";
const utility = await readFile(
  base + "packages/loot-core/src/shared/util.ts",
  "utf8",
);
const constant = utility.match(/^export const MAX_SAFE_NUMBER = .+;$/m)?.[0];
assert.ok(constant);
const bundle = await rolldown({
  input: entry,
  plugins: [
    {
      name: "source-display-constant",
      resolveId(id) {
        if (id === "@actual-app/core/shared/util") return "\0source-constant";
      },
      load(id) {
        if (id === "\0source-constant") return constant;
      },
    },
  ],
});
const asset = await bundle.generate({
  format: "iife",
  name: "MonteCarlo",
  minify: true,
});
await bundle.close();
assert.equal(asset.output.length, 1);
const fixtures = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/monte-carlo-baseline-source.json",
      import.meta.url,
    ),
  ),
);
const directory = await mkdtemp(tmpdir() + "/actualbudget-monte-measure-");
try {
  const probe = await readFile(
    new URL("./monte-carlo-runtime-probe.rs", import.meta.url),
  );
  const release = process.argv.includes("--release");
  let binary = directory + "/probe";
  if (release) {
    await mkdir(directory + "/src");
    await writeFile(directory + "/src/main.rs", probe);
    await writeFile(
      directory + "/Cargo.toml",
      '[package]\nname="monte-probe"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nrquickjs={version="=0.13.0",default-features=false,features=["std"]}\n',
    );
    await execute(
      "cargo",
      [
        "build",
        "--offline",
        "--release",
        "--manifest-path",
        directory + "/Cargo.toml",
      ],
      { timeout: 180000 },
    );
    binary = directory + "/target/release/monte-probe";
  } else {
    await writeFile(directory + "/probe.rs", probe);
    const deps = fileURLToPath(
      new URL("../server/target/debug/deps/", import.meta.url),
    );
    const libraries = (await readdir(deps)).filter((name) =>
      /^librquickjs-[a-z0-9]+\.rlib$/.test(name),
    );
    assert.equal(
      libraries.length,
      1,
      "Build the server first; expected one current rquickjs library",
    );
    await execute("rustc", [
      "--edition=2024",
      directory + "/probe.rs",
      "-o",
      binary,
      "-L",
      "dependency=" + deps,
      "--extern",
      "rquickjs=" + deps + libraries[0],
    ]);
  }
  for (const name of [
    "baseline-proportional",
    "new-plan-defaults",
    "historical-sequence",
    "progressive-tax-before-tax-contributions",
    "maximum-horizon-clamp",
  ]) {
    const test = fixtures.cases.find((test) => test.name === name);
    const input = JSON.stringify({
      config: test.config,
      options: test.options,
    });
    const code =
      asset.output[0].code +
      `\nconst input=${input}; JSON.stringify(MonteCarlo.runMonteCarloSimulation({...input.config,horizonYears:MonteCarlo.getMonteCarloHorizonYears(input.config),...input.options}),(_key,value)=>ArrayBuffer.isView(value)?Array.from(value):value);`;
    await writeFile(directory + "/simulation.js", code);
    const result = await execute(binary, [directory + "/simulation.js"], {
      maxBuffer: 8 * 1024 * 1024,
      timeout: 30000,
    });
    const measured = JSON.parse(result.stdout);
    const actual = JSON.parse(measured.result);
    // Compare all financial/distribution/cash-flow output, allowing float dust only.
    function compare(a, b, path = name) {
      if (typeof b === "number") {
        assert.equal(typeof a, "number", path);
        const tolerance = Number.isInteger(b)
          ? 0
          : Math.min(0.01, Math.max(1e-6, Math.abs(b) * 1e-12));
        assert.ok(
          Number.isFinite(a) && Math.abs(a - b) <= tolerance,
          `${path}: ${a} != ${b}`,
        );
      } else if (b !== null && typeof b === "object") {
        assert.deepEqual(Object.keys(a), Object.keys(b), path);
        for (const key of Object.keys(b))
          compare(a[key], b[key], path + "." + key);
      } else assert.equal(a, b, path);
    }
    compare(actual, test.result);
    console.log(
      JSON.stringify({
        name,
        profile: release ? "release" : "debug",
        elapsed_ms: measured.elapsed_ms,
        runs: actual.simulationCount,
        years: actual.horizonYears,
        heap_limit_mib: 64,
        output_bytes: result.stdout.length,
        source_match: true,
      }),
    );
  }
} finally {
  await rm(directory, { recursive: true, force: true });
}

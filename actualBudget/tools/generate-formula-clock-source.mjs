// Complete unchanged source hook: the same instant in three local timezones.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { formulaSourceRuntime } from "./formula-source-runtime.mjs";
const NativeDate = Date,
  clock = "2026-11-09T23:45:30Z";
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
  ),
  api = require("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/formula-clock-"),
  root = new URL("../tests/fixtures/", import.meta.url),
  filename = new URL("formula-clock-source.json", root),
  base = JSON.parse(await readFile(new URL("reports-source-26.10.json", root))),
  bytes = await readFile(new URL("reports-source-26.10.zip", root));
assert.equal(
  createHash("sha256").update(bytes).digest("hex"),
  base.archive_sha256,
);
try {
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(bytes, { filename: "formula-clock.zip" });
  const observations = [],
    hashes = {};
  for (const [zone, evaluated_at] of [
    ["UTC", "2026-11-09T23:45:30Z"],
    ["Pacific/Kiritimati", "2026-11-10T13:45:30+14:00"],
    ["America/New_York", "2026-11-09T18:45:30-05:00"],
  ]) {
    process.env.TZ = zone;
    const runtime = formulaSourceRuntime(api, engine);
    for (const fn of ["TODAY", "NOW"]) {
      const formula = `=${fn}()`;
      observations.push({
        name: zone + "-" + fn,
        formula,
        evaluated_at,
        output: await runtime.evaluate(formula),
      });
    }
    Object.assign(hashes, runtime.hashes);
  }
  const result = {
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    source: {
      archive: "reports-source-26.10.zip",
      archive_sha256: base.archive_sha256,
    },
    source_hashes: hashes,
    observations,
  };
  if (process.argv.includes("--check"))
    assert.deepEqual(result, JSON.parse(await readFile(filename)));
  else
    await writeFile(filename, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS six complete source TODAY/NOW observations across UTC/+14/-05",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

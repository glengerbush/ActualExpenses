// Publish credential-free summaries and compressed raw observations. Inputs
// must be successful, matching full-size runs, never exploratory pilots.
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import { resolve } from "node:path";
import { percentile } from "./resource-metrics.mjs";
const [
  directoryArgument,
  outputArgument,
  baselineFile = "baseline-openings.json",
] = process.argv.slice(2);
assert.ok(
  directoryArgument && outputArgument,
  "Usage: node summarize-resource-openings.mjs LAB_DIRECTORY NEW_OUTPUT_DIRECTORY [BASELINE_FILENAME]",
);
const directory = resolve(directoryArgument),
  output = resolve(outputArgument);
const variants = {};
for (const variant of ["baseline", "native"]) {
  const bytes = await readFile(
    directory +
      "/" +
      (variant === "baseline" ? baselineFile : "native-openings.json"),
  );
  const data = JSON.parse(bytes);
  assert.equal(data.variant, variant);
  assert.ok(data.count >= 20);
  assert.deepEqual(
    data.cases.map((item) => item.name),
    ["empty", "typical", "large"],
  );
  for (const test of data.cases) {
    assert.equal(test.error, undefined);
    assert.ok(test.idle);
    for (const phase of ["cold", "warm", "unchanged"]) {
      assert.equal(test[phase].length, data.count);
      assert.ok(test.summary[phase]);
    }
  }
  variants[variant] = { bytes, data };
}
assert.deepEqual(
  variants.baseline.data.hardware,
  variants.native.data.hardware,
);
assert.equal(variants.baseline.data.browser, variants.native.data.browser);
assert.equal(variants.baseline.data.count, variants.native.data.count);
assert.deepEqual(
  variants.baseline.data.cases.map((item) => item.archive_sha256),
  variants.native.data.cases.map((item) => item.archive_sha256),
);
await mkdir(output); // Refuse replacing prior evidence.
const quantiles = (values) => ({
  p50: percentile(values, 0.5),
  p95: percentile(values, 0.95),
});
const report = {
  scope: "Opening/unchanged/35s idle only; ticket51 remains incomplete",
  variants: {},
};
for (const [variant, { bytes, data }] of Object.entries(variants)) {
  const compressed = gzipSync(bytes, { level: 9 });
  await writeFile(output + "/" + variant + "-openings.json.gz", compressed, {
    flag: "wx",
  });
  report.variants[variant] = {
    count: data.count,
    hardware: data.hardware,
    browser: data.browser,
    image: data.image,
    profile: data.profile,
    retained_from: data.retained_from ?? null,
    prior_run_errors: data.prior_run_errors ?? [],
    raw_sha256: createHash("sha256").update(bytes).digest("hex"),
    compressed_sha256: createHash("sha256").update(compressed).digest("hex"),
    cases: data.cases.map((test) => ({
      name: test.name,
      archive_sha256: test.archive_sha256,
      prior_errors: test.prior_errors ?? [],
      phases: Object.fromEntries(
        ["cold", "warm", "unchanged"].map((phase) => {
          const rows = test[phase];
          const devices = [
            ...new Set(rows.flatMap((row) => Object.keys(row.server.io))),
          ].sort();
          return [
            phase,
            {
              ...test.summary[phase],
              server_write_bytes_by_device: Object.fromEntries(
                devices.map((device) => [
                  device,
                  quantiles(
                    rows.map((row) => row.server.io[device]?.wbytes ?? 0),
                  ),
                ]),
              ),
              incomplete_browser_io_samples: rows.filter(
                (row) => row.client.unavailable_io_processes,
              ).length,
              // Browser inventories include cache hits and omit some worker events;
              // kernel network counters remain the source for transfer totals.
              request_paths: [
                ...new Set(
                  rows.flatMap((row) =>
                    row.network.map((request) => request.path),
                  ),
                ),
              ].sort(),
            },
          ];
        }),
      ),
      idle: {
        duration_ms: test.idle.elapsed_ms,
        client: test.idle.client,
        server: test.idle.server,
        requests: test.idle.network,
        inflight: test.idle.inflight,
        failures: test.idle.failures,
      },
    })),
  };
}
await writeFile(
  output + "/summary.json",
  JSON.stringify(report, null, 2) + "\n",
  { flag: "wx" },
);
console.log(
  "Validated matching samples and published raw/summary evidence:",
  output,
);

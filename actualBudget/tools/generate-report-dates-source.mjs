// Widget time frames from unchanged pinned26.10 date functions.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { sourceRuntime } from "./reports-source-runtime.mjs";
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
try {
  const runtime = sourceRuntime({}, {}),
    { calculateTimeRange } = runtime.load("reportRanges");
  const frames = [
    undefined,
    ...[
      "full",
      "sliding-window",
      "static",
      "lastMonth",
      "lastYear",
      "yearToDate",
      "priorYearToDate",
      "currentQuarter",
      "previousQuarter",
    ].map((mode) => ({ mode, start: "2026-01", end: "2026-03" })),
    { mode: "sliding-window", start: "2026-01-10", end: "2026-01-20" },
    { mode: "sliding-window", start: "2026-04", end: "2026-01" },
  ];
  const cases = frames.map((frame) => ({
    options: frame ? { timeFrame: frame } : {},
    latest: "2026-12-02",
    expected: calculateTimeRange(frame, undefined, "2026-12-02"),
  }));
  const output = { clock, source_files: runtime.hashes, cases },
    path = new URL(
      "../tests/fixtures/report-dates-source-26.10.json",
      import.meta.url,
    );
  if (process.argv.includes("--check"))
    assert.deepEqual(
      JSON.parse(JSON.stringify(output)),
      JSON.parse(await readFile(path, "utf8")),
    );
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(`PASS pinned widget date oracle:${cases.length} frames`);
} finally {
  globalThis.Date = NativeDate;
}

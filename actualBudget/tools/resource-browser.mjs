// Shared browser/cgroup measurement protocol; UI workflows remain at call sites.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { measure, percentile, processChildren } from "./resource-metrics.mjs";
export async function browserProcess(profile, pid = process.pid) {
  const children = await processChildren(pid);
  for (const child of children) {
    try {
      // Chromium may rewrite argv into one space-separated process title.
      const args = (await readFile(`/proc/${child}/cmdline`, "utf8")).split(
        /[\0\s]+/,
      );
      if (
        args.includes("--user-data-dir=" + profile) &&
        !args.some((arg) => arg.startsWith("--type="))
      )
        return Number(child);
      const nested = await browserProcess(profile, child);
      if (nested) return nested;
    } catch (error) {
      if (!["ENOENT", "ESRCH"].includes(error.code)) throw error;
    }
  }
}
export function summarize(rows) {
  const values = (field) => rows.map(field);
  const quantiles = (rows) => ({
    p50: percentile(rows, 0.5),
    p95: percentile(rows, 0.95),
  });
  const pss = values((r) => r.client.peak_pss_bytes).filter(
    (value) => value !== null,
  );
  return {
    samples: rows.length,
    ui_ready_ms: quantiles(values((r) => r.value.ui_ready_ms)),
    settled_ms: quantiles(values((r) => r.elapsed_ms)),
    browser_cpu_ms: quantiles(values((r) => r.client.cpu_ms)),
    browser_peak_rss_bytes: quantiles(values((r) => r.client.peak_rss_bytes)),
    browser_peak_pss_bytes: { samples: pss.length, ...quantiles(pss) },
    server_cpu_ms: quantiles(values((r) => r.server.cpu_ms)),
    server_peak_memory_bytes: quantiles(
      values((r) => r.server.peak_memory_bytes),
    ),
    server_sent_bytes: quantiles(values((r) => r.server.network.eth0.tx_bytes)),
    server_received_bytes: quantiles(
      values((r) => r.server.network.eth0.rx_bytes),
    ),
    observed_response_body_bytes: quantiles(
      values((r) =>
        r.network.reduce(
          (total, request) => total + request.responseBodySize,
          0,
        ),
      ),
    ),
    observed_requests: quantiles(values((r) => r.network.length)),
    browser_write_bytes: quantiles(values((r) => r.client.write_bytes)),
  };
}
export async function measureBrowser(
  context,
  action,
  { variant, server, browserPid, serverCgroup },
) {
  const pending = new Set();
  let lastActivity = performance.now();
  // Chromium/Playwright emits no requestfinished for this SharedWorker entry.
  // Register readiness proves execution; kernel interface counters include its
  // complete transfer. Preserve the missing event explicitly in the result.
  const sharedWorker = (path) =>
    variant === "baseline" &&
    /^\/static\/shared-browser-server-[^/]+\.js$/.test(path);
  const started = (request) => {
    if (!sharedWorker(new URL(request.url()).pathname)) pending.add(request);
    lastActivity = performance.now();
  };
  const finished = (request) => {
    pending.delete(request);
    lastActivity = performance.now();
  };
  context.on("request", started);
  context.on("requestfinished", finished);
  context.on("requestfailed", finished);
  let row;
  let deadline;
  const quiet = () => !pending.size && performance.now() - lastActivity >= 150;
  async function waitForQuiet() {
    while (!quiet()) {
      assert.ok(
        performance.now() < deadline,
        "Requests did not settle: " +
          JSON.stringify([...pending].map((request) => request.url())),
      );
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  try {
    row = await measure({
      browserPid,
      serverCgroup: serverCgroup,
      context,
      settle: async () => {
        if (quiet()) return false;
        await waitForQuiet();
        return true;
      },
      action: async () => {
        const start = performance.now();
        await action();
        const ui_ready_ms = performance.now() - start;
        // Do not charge the quiet window to UI latency. Capture trailing transfers
        // before starting another phase, rather than attributing them to refresh.
        deadline = performance.now() + 15000;
        await waitForQuiet();
        return { ui_ready_ms };
      },
    });
  } finally {
    context.off("request", started);
    context.off("requestfinished", finished);
    context.off("requestfailed", finished);
  }
  assert.ok(
    !row.network.some(
      (request) => request.origin === server && request.status >= 400,
    ),
    "Failed application request: " +
      JSON.stringify(row.network.filter((r) => r.status >= 400)),
  );
  assert.deepEqual(
    row.failures.filter(
      (failure) =>
        failure.origin === server &&
        !(sharedWorker(failure.path) && failure.error === "net::ERR_ABORTED"),
    ),
    [],
    "Failed application transfer",
  );
  row.unobserved_worker_completion = row.inflight.filter(sharedWorker);
  assert.deepEqual(
    row.inflight.filter((path) => !sharedWorker(path)),
    [],
    "Unattributed request crossed a measurement boundary",
  );
  return row;
}

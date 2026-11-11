// Observe application-origin persistence independently from Chromium's profile
// housekeeping. No intercepted application request or fake visibility event.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { watch } from "node:fs";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { resolve } from "node:path";
const require = createRequire(new URL("../web/package.json", import.meta.url));
const { chromium, expect } = require("playwright/test");
const [directoryArgument, variant, outputArgument] = process.argv.slice(2);
assert.ok(
  directoryArgument &&
    outputArgument &&
    ["native", "baseline"].includes(variant),
  "Usage: node check-resource-idle.mjs LAB_DIRECTORY native|baseline NEW_OUTPUT_JSON",
);
const directory = resolve(directoryArgument),
  output = resolve(outputArgument);
const config = JSON.parse(await readFile(directory + "/" + variant + ".json"));
assert.equal(new URL(config.server).hostname, "127.0.0.1");
const test = config.cases.find((item) => item.name === "large");
assert.ok(test);
await writeFile(output, "{}\n", { flag: "wx" });
const profile = await mkdtemp(directory + "/idle-profile-");
const report = {
  variant,
  budget: test.name,
  archive_sha256: test.archive_sha256,
  settle_seconds: 60,
  observation_seconds: 60,
  periods: [],
};
async function memory() {
  const fields = Object.fromEntries(
    (await readFile(config.cgroup + "/memory.stat", "utf8"))
      .trim()
      .split("\n")
      .map((line) => {
        const [key, value] = line.split(" ");
        return [key, Number(value)];
      }),
  );
  return {
    time: new Date().toISOString(),
    anonymous_bytes: fields.anon,
    file_bytes: fields.file,
    cgroup_current_bytes: Number(
      await readFile(config.cgroup + "/memory.current", "utf8"),
    ),
  };
}
report.before_browser = await memory();
let context, observer;
try {
  context = await chromium.launchPersistentContext(profile, {
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1, EXCLUDE localhost",
    ],
    viewport: { width: 1440, height: 1000 },
    extraHTTPHeaders: { "X-Forwarded-For": "198.19.0.1" },
  });
  if (variant === "baseline")
    await context.setStorageState(directory + "/auth.json");
  const page = context.pages()[0];
  await page.goto(config.server);
  if (variant === "baseline") {
    await page
      .getByRole("row", { name: test.label, exact: true })
      .press("Enter");
    await page.locator(`a[href="/accounts/${test.account}"]`).click();
    await expect(
      page.getByRole("button", { name: "Add New", exact: true }),
    ).toBeVisible();
  } else {
    await page
      .getByRole("button", { name: new RegExp(test.label + ".*Open") })
      .click();
    await page
      .getByRole("button", { name: /^checking / })
      .first()
      .click();
    await expect(page.getByTestId("register-total")).toBeVisible();
  }
  // Browser profile and initial origin stores may flush long after UI readiness.
  // Preserve these observations separately from steady-state idle.
  const events = [],
    requests = [];
  observer = watch(profile, { recursive: true }, (event, path) => {
    events.push({ elapsed_ms: performance.now(), event, path: String(path) });
  });
  observer.on("error", (error) => {
    report.observer_error = String(error);
  });
  context.on("request", (request) => {
    requests.push({
      elapsed_ms: performance.now(),
      origin: new URL(request.url()).origin,
      path: new URL(request.url()).pathname,
    });
  });
  async function observe(name, seconds) {
    const before = await context.storageState({ indexedDB: true });
    const memoryBefore = await memory();
    const start = performance.now(),
      eventOffset = events.length,
      requestOffset = requests.length;
    await page.waitForTimeout(seconds * 1000);
    const end = performance.now();
    const files = events
      .slice(eventOffset)
      .map((event) => ({ ...event, elapsed_ms: event.elapsed_ms - start }));
    const application = files.filter((event) =>
      /^Default\/(IndexedDB|Local Storage|File System)\//.test(event.path),
    );
    const observedRequests = requests.slice(requestOffset);
    const memoryAfter = await memory();
    // Reading IndexedDB must not count as an application write observation.
    const after = await context.storageState({ indexedDB: true });
    const ownState = (state) =>
      state.origins.filter((origin) => origin.origin === config.server);
    const stateUnchanged =
      JSON.stringify(ownState(before)) === JSON.stringify(ownState(after));
    const period = {
      name,
      elapsed_ms: end - start,
      server_memory_before: memoryBefore,
      server_memory_after: memoryAfter,
      origin_state_unchanged: stateUnchanged,
      origin_file_events: application,
      browser_profile_event_count: files.length,
      browser_profile_changed_files: [
        ...new Set(files.map((event) => event.path)),
      ].sort(),
      requests: observedRequests.map((request) => ({
        ...request,
        elapsed_ms: request.elapsed_ms - start,
      })),
    };
    report.periods.push(period);
    return period;
  }
  await observe("settling", report.settle_seconds);
  const idle = await observe("steady-idle", report.observation_seconds);
  assert.equal(report.observer_error, undefined);
  assert.equal(
    idle.origin_state_unchanged,
    true,
    "Idle changed application-origin storage",
  );
  assert.deepEqual(
    idle.requests.filter((request) => request.origin === config.server),
    [],
    "Idle application polling",
  );
  assert.deepEqual(
    idle.origin_file_events,
    [],
    "Idle rewrote application-origin persistence",
  );
  console.log(
    "PASS",
    variant,
    "large budget: 60s steady idle, no app requests/origin file events/state changes",
  );
} catch (error) {
  report.error = String(error);
  throw error;
} finally {
  observer?.close();
  await context?.close();
  await rm(profile, { recursive: true, force: true });
  await writeFile(output, JSON.stringify(report, null, 2) + "\n");
}

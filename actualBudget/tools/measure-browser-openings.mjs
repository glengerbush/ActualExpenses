// Matched production-container/browser measurements; setup/auth excluded.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { resolve } from "node:path";
import os from "node:os";
import { createHash } from "node:crypto";
import {
  browserProcess,
  measureBrowser,
  summarize,
} from "./resource-browser.mjs";
const require = createRequire(new URL("../web/package.json", import.meta.url));
const { chromium, expect } = require("playwright/test");
const [
  directoryArgument,
  variant,
  outputArgument,
  countArgument = "20",
  completedArgument,
] = process.argv.slice(2);
assert.ok(
  directoryArgument &&
    outputArgument &&
    ["baseline", "native"].includes(variant),
  "Usage: node measure-browser-openings.mjs LAB_DIRECTORY baseline|native NEW_OUTPUT_JSON [SAMPLES=20]",
);
const directory = resolve(directoryArgument),
  output = resolve(outputArgument),
  count = Number(countArgument);
assert.ok(Number.isInteger(count) && count >= 3 && count <= 100);
const configuration = JSON.parse(
  await readFile(directory + "/" + variant + ".json"),
);
const native = JSON.parse(await readFile(directory + "/native.json"));
assert.equal(new URL(configuration.server).hostname, "127.0.0.1");
assert.ok(
  configuration.cgroup?.startsWith("/sys/fs/cgroup/"),
  "Record the owned container cgroup in the lab configuration",
);
await writeFile(output, "{}\n", { flag: "wx" });
const document = {
  variant,
  count,
  profile:
    "production containers; fresh persistent browser profile for cold, same profile reload for warm; browser launch/auth setup excluded; OS page cache not cleared; distinct benchmark client addresses via trusted-proxy X-Forwarded-For, limiter remains enabled; DNS restricted to loopback (optional external news/update requests fail); per-client LAN cost, not single-IP load capacity or internet-connected cost",
  hardware: {
    hostname: os.hostname(),
    cpu: os.cpus()[0].model,
    cpus: os.cpus().length,
    memory: os.totalmem(),
    kernel: os.release(),
  },
  browser: null,
  image: configuration.image,
  cases: [],
};
let browserPid;
if (completedArgument) {
  assert.ok(completedArgument.startsWith("--completed-from="));
  const bytes = await readFile(resolve(completedArgument.slice(17)));
  const previous = JSON.parse(bytes);
  for (const field of ["variant", "count", "image", "profile"])
    assert.deepEqual(
      previous[field],
      document[field],
      "Prior run differs: " + field,
    );
  assert.deepEqual(previous.hardware, document.hardware);
  document.browser = previous.browser;
  document.retained_from = {
    sha256: createHash("sha256").update(bytes).digest("hex"),
    cases: [],
  };
  // Preserve failed first profiles too, even when there is no complete trio
  // from their case to retain. Never turn resumption into a success-only log.
  document.prior_run_errors = [
    ...(previous.prior_run_errors ?? []),
    ...previous.cases
      .filter((test) => test.error)
      .map((test) => ({
        case: test.name,
        error: test.error,
        source_sha256: document.retained_from.sha256,
        completed_phases: Object.fromEntries(
          ["cold", "warm", "unchanged"].map((phase) => [
            phase,
            test[phase].length,
          ]),
        ),
      })),
  ];
  for (const test of previous.cases) {
    assert.equal(
      test.archive_sha256,
      configuration.cases.find((item) => item.name === test.name)
        ?.archive_sha256,
    );
    const completed = Math.min(
      ...["cold", "warm", "unchanged"].map((phase) => test[phase].length),
      test.idle ? count : count - 1,
    );
    if (!completed) continue;
    if (test.error) {
      test.prior_errors ??= [];
      test.prior_errors.push({
        error: test.error,
        completed_profiles: completed,
      });
      delete test.error;
    }
    for (const phase of ["cold", "warm", "unchanged"])
      test[phase] = test[phase].slice(0, completed);
    document.cases.push(test);
    document.retained_from.cases.push({
      name: test.name,
      completed_profiles: completed,
    });
  }
}
const regex = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
async function pace(identity, needed) {
  if (variant !== "baseline") return;
  const deadline = performance.now() + 125000;
  for (;;) {
    const response = await fetch(configuration.server + "/health", {
      headers: { "X-Forwarded-For": identity },
    });
    const remaining = Number(response.headers.get("ratelimit-remaining"));
    const reset = Number(response.headers.get("ratelimit-reset"));
    assert.ok(
      response.headers.has("ratelimit-remaining") &&
        response.headers.has("ratelimit-reset") &&
        Number.isFinite(remaining) &&
        Number.isFinite(reset),
      "Baseline limiter headers missing",
    );
    if (response.ok && remaining >= needed) return;
    assert.ok(
      performance.now() < deadline,
      "Baseline limiter never provided a fresh request window",
    );
    console.log(
      `Baseline pacing: ${remaining} requests remain; window resets in ${reset}s`,
    );
    await new Promise((resolve) =>
      setTimeout(resolve, Math.min(60000, Math.max(1000, reset * 1000 + 100))),
    );
  }
}
async function ready(page, test) {
  if (variant === "baseline") {
    await expect(
      page.getByRole("button", { name: "Add New", exact: true }),
    ).toBeVisible({ timeout: 45000 });
    await expect(
      page.locator(`a[href="/accounts/${test.account}"]`),
    ).toBeVisible();
  } else {
    await expect(page.getByTestId("register-total")).toBeVisible({
      timeout: 45000,
    });
    await expect(
      page.getByRole("button", { name: "Refresh transactions", exact: true }),
    ).toBeEnabled();
  }
}
async function cold(page, test) {
  await page.goto(configuration.server);
  if (variant === "baseline") {
    await page
      .getByRole("row", { name: test.label, exact: true })
      .press("Enter");
    await page.locator(`a[href="/accounts/${test.account}"]`).click();
  } else {
    await page
      .getByRole("button", { name: new RegExp(regex(test.label) + ".*Open") })
      .click();
    const account = native.cases
      .find((c) => c.name === test.name)
      .accounts.find((a) => a.id === test.account);
    await page
      .getByRole("button", {
        name: new RegExp("^" + regex(account.name) + " "),
      })
      .first()
      .click();
  }
  await ready(page, test);
}
const record = (context, action) =>
  measureBrowser(context, action, {
    variant,
    server: configuration.server,
    browserPid,
    serverCgroup: configuration.cgroup,
  });
for (const [caseIndex, test] of configuration.cases.entries()) {
  const retained = document.cases.find(
    (previous) => previous.name === test.name,
  );
  if (retained?.cold.length === count && retained.idle) continue;
  const result = retained ?? {
    name: test.name,
    archive_sha256: test.archive_sha256,
    cold: [],
    warm: [],
    unchanged: [],
    idle: null,
  };
  if (!retained) document.cases.push(result);
  for (let index = result.cold.length; index < count; index++) {
    const identity = `198.18.${caseIndex}.${index + 1}`;
    await pace(identity, 400);
    // Keep durable profiles beside the results, not /tmp (often tmpfs).
    const profile = await mkdtemp(directory + "/browser-profile-");
    let context, page;
    try {
      context = await chromium.launchPersistentContext(profile, {
        executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
        args: [
          "--no-sandbox",
          "--no-proxy-server",
          "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1, EXCLUDE localhost",
        ],
        viewport: { width: 1440, height: 1000 },
        extraHTTPHeaders: { "X-Forwarded-For": identity },
      });
      const browserVersion = context.browser().version();
      if (document.browser !== null)
        assert.equal(browserVersion, document.browser);
      document.browser = browserVersion;
      browserPid = await browserProcess(profile);
      assert.ok(browserPid, "Owned Chromium process not found");
      if (variant === "baseline")
        await context.setStorageState(directory + "/auth.json");
      page = await context.newPage();
      page.setDefaultTimeout(45000);
      const browserErrors = [];
      page.on("pageerror", (error) => browserErrors.push(String(error)));
      page.on("console", (message) => {
        if (message.type() === "error") browserErrors.push(message.text());
      });
      result.latest_browser_errors = browserErrors;
      result.cold.push(await record(context, () => cold(page, test)));
      await pace(identity, 80);
      result.warm.push(
        await record(context, async () => {
          await page.reload();
          await ready(page, test);
        }),
      );
      await pace(identity, 10);
      result.unchanged.push(
        await record(context, async () => {
          const path = variant === "baseline" ? "/sync/sync" : "/changes";
          const response = page.waitForResponse(
            (response) => new URL(response.url()).pathname.endsWith(path),
            { timeout: 45000 },
          );
          await page
            .getByRole("button", {
              name:
                variant === "baseline" ? "Server Sync" : "Refresh transactions",
              exact: true,
            })
            .click();
          assert.ok((await response).ok());
          await ready(page, test);
        }),
      );
      if (index === count - 1)
        result.idle = await record(context, () => page.waitForTimeout(35000));
    } catch (error) {
      result.error = String(error);
      if (
        page &&
        (await page
          .getByRole("heading", { name: "Fatal Error", exact: true })
          .isVisible())
      )
        await page
          .getByText("Show Error", { exact: true })
          .click()
          .catch(() => {});
      if (page)
        await writeFile(
          output.replace(/\.json$/, "-failure.txt"),
          await page.locator("body").ariaSnapshot(),
        );
      throw error;
    } finally {
      await context?.close();
      await rm(profile, { recursive: true, force: true });
      await writeFile(output, JSON.stringify(document, null, 2) + "\n");
    }
    console.log(`${variant} ${test.name}: ${index + 1}/${count}`);
  }
  result.summary = Object.fromEntries(
    ["cold", "warm", "unchanged"].map((key) => [key, summarize(result[key])]),
  );
  await writeFile(output, JSON.stringify(document, null, 2) + "\n");
}

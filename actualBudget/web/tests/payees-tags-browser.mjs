import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const evidence = fileURLToPath(
  new URL("../../../.scratch/evidence/", import.meta.url),
);
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, method = "POST") {
  const r = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    first = randomUUID(),
    second = randomUUID(),
    tx = randomUUID();
  const root = `/budgets/${budget}`;
  await write("/budgets", { id: budget, name: "Payees fixture" });
  await write(root + "/accounts", {
    id: account,
    name: "Daily",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const [id, name] of [
    [first, "Market"],
    [second, "MARKET 001"],
  ])
    await write(root + "/payees", { id, name });
  await write(root + "/transactions", {
    id: tx,
    account_id: account,
    payee_id: second,
    amount: -1250,
    date: "2026-01-02",
    notes: "#Food ##Food #Foodie",
  });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  const page = await context.newPage();
  await page.goto(server.url);
  await page.getByRole("button", { name: "Payees", exact: true }).click();
  await page
    .getByRole("button", { name: "Edit MARKET 001", exact: true })
    .click();
  await page.getByLabel("Payee name", { exact: true }).fill("Market branch");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Changes saved." }),
  ).toBeVisible();
  await page
    .getByRole("checkbox", { name: "Market branch 1 transactions" })
    .check();
  await page.getByLabel("Merge into").selectOption(first);
  await page.getByRole("button", { name: "Merge selected payees" }).click();
  await expect(
    page.getByRole("button", { name: "Edit Market branch", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Tags", exact: true }),
  ).toHaveCount(0);
  expect((await fetch(api + "/api/v1" + root + "/tags")).status).toBe(404);
  await page.getByLabel("Show", { exact: true }).selectOption("unused");
  await expect(
    page.getByRole("button", { name: "Edit Market", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Show", { exact: true }).selectOption("all");
  await page.getByLabel("Payee name", { exact: true }).fill("Travel");
  let sent;
  await page.route(`**/api/v1${root}/payees`, async (route) => {
    if (route.request().method() === "POST") {
      sent = {
        body: route.request().postData(),
        key: route.request().headers()["idempotency-key"],
        clock: route.request().headers()["x-actual-edit-clock"],
      };
      await route.fetch({
        url: route.request().url().replace("actualbudget.test", "127.0.0.1"),
        headers: {
          ...route.request().headers(),
          origin: api,
          host: new URL(api).host,
        },
      });
      await route.abort();
    } else await route.continue();
  });
  const stale = await context.newPage();
  await stale.goto(server.url);
  await expect(
    stale.getByRole("heading", { name: "Payees", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Add payee", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Pending change" }),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  await stale.getByRole("button", { name: "Add payee", exact: true }).click();
  await expect(stale.getByRole("alert")).toContainText("Another tab");
  await page.unroute(`**/api/v1${root}/payees`);
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Pending change" }),
  ).toBeVisible();
  let retried;
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith(root + "/payees"))
      retried = {
        body: r.postData(),
        key: r.headers()["idempotency-key"],
        clock: r.headers()["x-actual-edit-clock"],
      };
  });
  await page.getByRole("button", { name: "Retry pending change" }).click();
  await expect(
    page.getByRole("button", { name: "Edit Travel", exact: true }),
  ).toBeVisible();
  expect(retried).toEqual(sent);
  // A corrected device clock cannot rewrite an attempted request. Cancel the
  // original safely, then explicitly reset only the local generator.
  await page.evaluate(() => {
    const future = Date.now() + 600_000;
    Date.now = () => future;
  });
  await page.getByLabel("Payee name", { exact: true }).fill("ClockRecovery");
  await page.getByRole("button", { name: "Add payee", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("clock");
  const originalClockRequest = retried;
  await page.reload();
  let cancelRequest;
  await page.route(`**/api/v1${root}/payees`, async (route) => {
    if (
      route.request().headers()["x-actual-cancel-if-uncommitted"] === "true"
    ) {
      cancelRequest = {
        body: route.request().postData(),
        key: route.request().headers()["idempotency-key"],
        clock: route.request().headers()["x-actual-edit-clock"],
      };
      await route.fetch({
        url: route.request().url().replace("actualbudget.test", "127.0.0.1"),
        headers: {
          ...route.request().headers(),
          origin: api,
          host: new URL(api).host,
        },
      });
      await route.abort();
    } else await route.continue();
  });
  let releaseInfo;
  const infoHeld = new Promise((resolve) => {
    releaseInfo = resolve;
  });
  let markInfo;
  const infoRequested = new Promise((resolve) => {
    markInfo = resolve;
  });
  await page.route("**/api/v1/info", async (route) => {
    markInfo();
    await infoHeld;
    await route.continue();
  });
  await page
    .getByRole("button", { name: "Cancel pending change", exact: true })
    .click();
  await infoRequested;
  await expect(
    page.getByRole("button", { name: "Retry pending change", exact: true }),
  ).toBeDisabled();
  releaseInfo();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  await page.unroute("**/api/v1/info");
  expect(cancelRequest).toEqual(originalClockRequest);
  await page.unroute(`**/api/v1${root}/payees`);
  await page.reload();
  await page
    .getByRole("button", { name: "Retry pending change", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Pending change" }),
  ).toHaveCount(0);
  await expect(page.getByLabel("Payee name", { exact: true })).toHaveValue(
    "ClockRecovery",
  );
  await page.getByRole("button", { name: "Add payee", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Use corrected device time",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Use corrected device time", exact: true })
    .focus();
  await mkdir(evidence, { recursive: true });
  await page.screenshot({
    path: evidence + "payees-clock-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.locator("body")).toHaveJSProperty("scrollWidth", 390);
  await page.screenshot({
    path: evidence + "payees-clock-mobile.png",
    fullPage: true,
  });
  await page.keyboard.press("Enter");
  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.getByLabel("Payee name", { exact: true })).toHaveValue(
    "ClockRecovery",
  );
  await page.getByRole("button", { name: "Add payee", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Edit ClockRecovery", exact: true }),
  ).toBeVisible();
  expect(retried.key).not.toBe(originalClockRequest.key);
  const clockTags = (
    await (await fetch(api + "/api/v1" + root + "/payees")).json()
  ).data;
  expect(clockTags.filter((tag) => tag.name === "ClockRecovery")).toHaveLength(
    1,
  );
  await page.getByLabel("Search payees").focus();
  await page.keyboard.type("Travel");
  await expect(
    page.getByRole("button", { name: "Edit Market", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Search payees").fill("");
  await mkdir(evidence, { recursive: true });
  await page.screenshot({
    path: evidence + "payees-tags-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.locator("body")).toHaveJSProperty("scrollWidth", 390);
  await mkdir(evidence, { recursive: true });
  await page.screenshot({
    path: evidence + "payees-tags-mobile.png",
    fullPage: true,
  });
  const textZoom = await page.addStyleTag({
    content: ":root {font-size:200%;}",
  });
  await expect(page.locator("body")).toHaveJSProperty("scrollWidth", 390);
  await textZoom.evaluate((element) => element.remove());
  await context.setOffline(true);
  await expect(
    page.getByRole("button", { name: "Add payee", exact: true }),
  ).toBeDisabled();
  await context.setOffline(false);
  await page.reload();
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await page.locator('summary[aria-label="Filters and columns"]').click();
  await page
    .getByRole("button", { name: "Add condition", exact: true })
    .click();
  await expect(
    page
      .getByLabel(/^Field 1/)
      .getByRole("option", { name: "Notes", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel(/^Field 1/).selectOption("amount");
  await page.getByLabel(/^Operator 1/).selectOption("is");
  await page.getByLabel("Amount value 1", { exact: true }).fill("-12.50");
  await page
    .getByRole("button", { name: "Apply filters and columns", exact: true })
    .click();
  await expect(page.locator("td.amount")).toHaveText(["-12.50"]);
  console.log(
    "PASS payees: rename merge tag retirement immutable retry cross-tab Done filters mobile offline",
  );
} finally {
  await browser?.close();
  await server.stop();
}

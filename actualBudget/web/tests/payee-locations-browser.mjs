import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  url = server.url.replace("actualbudget.test", "localhost"),
  api = url + "/api/v1";
const oracle = JSON.parse(
  await readFile(
    new URL("../../tests/fixtures/payee-locations.json", import.meta.url),
  ),
);
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../../tests/fixtures/payee-locations-import.json",
      import.meta.url,
    ),
  ),
);
const evidence = new URL(
  "../../../.scratch/evidence/locations46/",
  import.meta.url,
);
await mkdir(evidence, { recursive: true });
async function write(path, body, method = "POST") {
  const r = await fetch(api + path, {
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
async function read(path) {
  const r = await fetch(api + path);
  expect(r.ok).toBe(true);
  return (await r.json()).data;
}
const budget = randomUUID(),
  account = randomUUID(),
  root = `/budgets/${budget}`;
let browser;
try {
  await write("/budgets", { id: budget, name: "Location fixture" });
  await write(root + "/accounts", {
    id: account,
    name: "Daily",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const [i, id] of fixture.payees.entries())
    await write(root + "/payees", {
      id,
      name: `Shop ${String(i).padStart(2, "0")}`,
    });
  for (const row of fixture.locations)
    await write(root + "/payee-locations", row);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    permissions: ["geolocation"],
    geolocation: { latitude: 40, longitude: -74 },
  });
  await context.addInitScript(() => {
    window.geoCalls = 0;
    const original = navigator.geolocation.getCurrentPosition.bind(
      navigator.geolocation,
    );
    navigator.geolocation.getCurrentPosition = (...args) => {
      window.geoCalls++;
      return original(...args);
    };
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => console.log("PAGEERROR", e.message));
  const external = [];
  page.on("request", (r) => {
    if (!r.url().startsWith(url) && !r.url().startsWith("data:"))
      external.push(r.url());
  });
  await page.goto(url);

  await page.getByRole("button", { name: /Location fixture.*Open/ }).click();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  expect(await page.evaluate(() => window.geoCalls)).toBe(0);
  await page.getByLabel("Amount", { exact: true }).fill("1.23");
  const panel = page.getByLabel("Payee locations", { exact: true });
  async function checkSnapshot(index) {
    const expected = oracle.snapshots[index];
    await context.setGeolocation(expected.position);
    await panel
      .getByRole("button", { name: /Request location|Refresh nearby payees/ })
      .click();
    await expect(panel.getByText("Locating or saving…")).toHaveCount(0);
    const buttons = panel.locator(".choose-payee");
    await expect(buttons).toHaveCount(expected.nearby.length);
    for (let i = 0; i < expected.nearby.length; i++) {
      const e = expected.nearby[i];
      await expect(buttons.nth(i)).toContainText(
        `Shop ${e.payee.slice(4).padStart(2, "0")}`,
      );
      await expect(buttons.nth(i)).toContainText(
        `${Math.round(e.distance)} m away`,
      );
    }
  }
  await checkSnapshot(0);
  await panel.getByRole("button", { name: "Use nearest payee" }).click();
  await expect(page.getByLabel("Payee", { exact: true })).toHaveValue(
    fixture.payees[0],
  );
  await expect(
    panel.getByRole("button", { name: "Save location", exact: true }),
  ).toHaveCount(0);
  await panel.screenshot({
    path: new URL("mobile-panel.png", evidence).pathname,
  });
  await page.screenshot({
    path: new URL("mobile-nearby.png", evidence).pathname,
    fullPage: true,
  });
  await checkSnapshot(1);
  await context.setGeolocation({ latitude: 40, longitude: -74 });
  await panel.getByRole("button", { name: "Refresh nearby payees" }).click();
  await expect(panel.locator(".choose-payee")).toHaveCount(10);
  await panel
    .getByRole("button", { name: "Forget nearby location for Shop 00" })
    .click();
  await expect(
    panel.getByText("Location forgotten.", { exact: true }),
  ).toBeVisible();
  await checkSnapshot(2);
  await write(root + "/payees/batch", {
    action: "merge",
    ids: [fixture.payees[0]],
    target_id: fixture.payees[1],
  });
  await checkSnapshot(3);
  await write(root + "/payees/batch", {
    action: "remove",
    ids: [fixture.payees[2]],
  });
  await checkSnapshot(4);
  // Explicit capture of another branch; intercepted lost acknowledgement survives reload.
  await page
    .getByLabel("Payee", { exact: true })
    .selectOption(fixture.payees[3]);
  await context.setGeolocation({ latitude: 41, longitude: -75 });
  await panel.getByRole("button", { name: "Refresh nearby payees" }).click();
  await expect(panel.getByText("Locating or saving…")).toHaveCount(0);
  let submitted;
  await page.route("**/payee-locations", async (route) => {
    if (route.request().method() === "POST") {
      submitted = {
        body: route.request().postData(),
        headers: route.request().headers(),
      };
      await route.fetch();
      await route.abort("failed");
    } else await route.continue();
  });
  await panel
    .getByRole("button", { name: "Save location", exact: true })
    .click();
  await expect(
    panel.getByRole("button", { name: "Retry location change" }),
  ).toBeVisible();
  await expect(
    panel.getByRole("button", { name: "Retry location change" }),
  ).toBeEnabled();
  const afterSaved = (await read(root + "/payee-locations")).length;
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Add transaction", exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("1.23");
  expect(await page.evaluate(() => window.geoCalls)).toBe(0);
  await page.unroute("**/payee-locations");
  // A restored mobile request must remain recoverable after widening the window.
  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(
    panel.getByRole("button", { name: "Retry location change" }),
  ).toBeVisible();
  await panel.screenshot({
    path: new URL("desktop-recovery.png", evidence).pathname,
  });
  let retried;
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith("/payee-locations"))
      retried = { body: r.postData(), headers: r.headers() };
  });
  await panel.getByRole("button", { name: "Retry location change" }).click();
  await expect(panel).toHaveAttribute("aria-busy", "false");
  await expect(
    panel.getByText("Location saved in your budget.", { exact: true }),
  ).toBeVisible();
  expect((await read(root + "/payee-locations")).length).toBe(afterSaved);
  await panel.screenshot({
    path: new URL("desktop-recovered.png", evidence).pathname,
  });
  expect(retried.body).toBe(submitted.body);
  for (const h of [
    "idempotency-key",
    "x-actual-edit-clock",
    "x-actual-budget-epoch",
  ])
    expect(retried.headers[h]).toBe(submitted.headers[h]);
  // New payee is committed with location, then ordinary expense uses that identity.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByLabel("Payee", { exact: true }).selectOption("new");
  await page.getByLabel("New payee name").fill("New local merchant");
  await panel
    .getByRole("button", { name: "Save location", exact: true })
    .click();
  await expect(
    panel.getByText("Location saved in your budget.", { exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Payee", { exact: true })).not.toHaveValue(
    "new",
  );
  const firstCreated = await page
    .getByLabel("Payee", { exact: true })
    .inputValue();
  await page.getByLabel("Payee", { exact: true }).selectOption("new");
  await page.getByLabel("New payee name").fill("Another local merchant");
  await panel
    .getByRole("button", { name: "Save location", exact: true })
    .click();
  await expect(
    panel.getByText("Location saved in your budget.", { exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Payee", { exact: true })).not.toHaveValue(
    "new",
  );
  expect(await page.getByLabel("Payee", { exact: true }).inputValue()).not.toBe(
    firstCreated,
  );
  // A delayed location acknowledgement must not replace newer payee typing.
  await page.getByLabel("Payee", { exact: true }).selectOption("new");
  await page.getByLabel("New payee name").fill("Captured merchant");
  let releaseLocation, locationAccepted;
  const holdLocation = new Promise((resolve) => (releaseLocation = resolve));
  const acceptedLocation = new Promise(
    (resolve) => (locationAccepted = resolve),
  );
  await page.route("**/payee-locations", async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    const response = await route.fetch();
    locationAccepted();
    await holdLocation;
    await route.fulfill({ response });
  });
  await panel
    .getByRole("button", { name: "Save location", exact: true })
    .click();
  await acceptedLocation;
  await expect(
    page.getByRole("button", { name: "Save transaction", exact: true }),
  ).toBeDisabled();
  await page
    .locator(".transaction-form")
    .screenshot({ path: new URL("new-payee-waiting.png", evidence).pathname });
  await page.getByLabel("New payee name").fill("Keep typing merchant");
  releaseLocation();
  await expect(
    panel.getByText("Location saved in your budget.", { exact: true }),
  ).toBeVisible();
  await expect(panel.getByText("Locating or saving…")).toHaveCount(0);
  await expect(page.getByLabel("Payee", { exact: true })).toHaveValue("new");
  await expect(page.getByLabel("New payee name")).toHaveValue(
    "Keep typing merchant",
  );
  await page.unroute("**/payee-locations");
  await panel
    .getByRole("button", { name: "Save location", exact: true })
    .click();
  await expect(
    panel.getByText("Location saved in your budget.", { exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Payee", { exact: true })).not.toHaveValue(
    "new",
  );
  await page.getByLabel("Payee", { exact: true }).selectOption(firstCreated);

  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Add transaction", exact: true }),
  ).toHaveCount(0);
  const expenses = await read(root + "/transactions");
  expect(
    expenses.some(
      (r) => r.payee_name === "New local merchant" && r.amount === -123,
    ),
  ).toBe(true);
  // Offline capture still works; location work is gated independently.
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await context.setOffline(true);
  await expect(
    panel.getByText(/Reconnect to use payee locations/),
  ).toBeVisible();
  await expect(
    panel.getByRole("button", { name: "Request location" }),
  ).toBeDisabled();
  await page.getByLabel("Amount", { exact: true }).fill("2.34");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Add transaction", exact: true }),
  ).toHaveCount(0);
  await context.setOffline(false);
  expect(external).toEqual([]);
  // Real plain HTTP restriction; expense path remains enabled.
  const insecure = await browser.newContext({
      viewport: { width: 390, height: 844 },
    }),
    http = await insecure.newPage();
  await http.goto(server.url);
  await http.getByRole("button", { name: /Location fixture.*Open/ }).click();
  await http
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  expect(await http.evaluate(() => window.isSecureContext)).toBe(false);
  await expect(
    http.getByRole("button", { name: "Request location" }),
  ).toBeDisabled();
  await http.getByLabel("Amount", { exact: true }).fill("3.45");
  await http.screenshot({
    path: new URL("http-fallback.png", evidence).pathname,
    fullPage: true,
  });
  await http
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    http.getByRole("heading", { name: "Add transaction", exact: true }),
  ).toHaveCount(0);
  // Permission denial, keyboard actions, large text/dark, desktop no location controls.
  const denied = await browser.newContext({
    viewport: { width: 390, height: 844 },
    colorScheme: "dark",
  });
  await denied.addInitScript(() => {
    navigator.geolocation.getCurrentPosition = (_ok, fail) => fail({ code: 1 });
  });
  const d = await denied.newPage();
  await d.goto(url);
  await d.getByRole("button", { name: /Location fixture.*Open/ }).click();
  await d.getByRole("button", { name: "Add transaction", exact: true }).click();
  await d.getByRole("button", { name: "Request location" }).focus();
  await d.keyboard.press("Enter");
  await expect(
    d.getByRole("alert").filter({ hasText: "Location permission denied" }),
  ).toBeVisible();
  await d.addStyleTag({ content: "html {font-size:200%}" });
  await d
    .getByLabel("Payee locations", { exact: true })
    .screenshot({ path: new URL("denied-panel.png", evidence).pathname });
  await d.screenshot({
    path: new URL("denied-dark-large.png", evidence).pathname,
    fullPage: true,
  });
  expect(
    await d.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  await d.getByLabel("Amount", { exact: true }).fill("4.56");
  await d
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    d.getByRole("heading", { name: "Add transaction", exact: true }),
  ).toHaveCount(0);
  await page.setViewportSize({ width: 1280, height: 900 });
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(panel).toBeHidden();
  await page.screenshot({
    path: new URL("desktop-entry.png", evidence).pathname,
    fullPage: true,
  });
  // Unpublished metadata stays durable, can be canceled, and cannot cross restore epochs.
  const recovery = await browser.newContext({
    viewport: { width: 390, height: 844 },
    permissions: ["geolocation"],
    geolocation: { latitude: 42, longitude: -76 },
  });
  const r = await recovery.newPage();
  await r.goto(url);
  await r.getByRole("button", { name: /Location fixture.*Open/ }).click();
  await r.getByRole("button", { name: "Add transaction", exact: true }).click();
  const rp = r.getByLabel("Payee locations", { exact: true });
  await r.getByLabel("Amount", { exact: true }).fill("7.89");
  await r.getByLabel("Payee", { exact: true }).selectOption(fixture.payees[3]);
  await rp.getByRole("button", { name: "Request location" }).click();
  await expect(rp.getByText("Locating or saving…")).toHaveCount(0);
  const beforeCancel = (await read(root + "/payee-locations")).length;
  const abortWrite = (route) =>
    route.request().method() === "POST"
      ? route.abort("failed")
      : route.continue();
  await r.route("**/payee-locations", abortWrite);
  await rp.getByRole("button", { name: "Save location", exact: true }).click();
  await expect(
    rp.getByRole("button", { name: "Cancel location change" }),
  ).toBeEnabled();
  const sibling = await recovery.newPage();
  await sibling.goto(url);
  const sp = sibling.getByLabel("Payee locations", { exact: true });
  await expect(
    sp.getByRole("button", { name: "Retry location change" }),
  ).toBeVisible();
  await r.unroute("**/payee-locations");
  await rp.getByRole("button", { name: "Cancel location change" }).click();
  await expect(
    rp.getByText("Pending location change canceled.", { exact: true }),
  ).toBeVisible();
  expect((await read(root + "/payee-locations")).length).toBe(beforeCancel);
  await sp.getByRole("button", { name: "Retry location change" }).click();
  await expect(sp.getByRole("alert")).toContainText(
    "Another tab changed location work",
  );
  await sp.getByRole("button", { name: "Reload saved location work" }).click();
  await expect(
    sp.getByRole("button", { name: "Retry location change" }),
  ).toHaveCount(0);
  await sibling.close();
  const backup = await write(root + "/backups", { label: "Location epoch" });
  await r.route("**/payee-locations", abortWrite);
  await rp.getByRole("button", { name: "Save location", exact: true }).click();
  await expect(
    rp.getByRole("button", { name: "Retry location change" }),
  ).toBeEnabled();
  await r.unroute("**/payee-locations");
  await write(root + `/backups/${backup.id}/restore`, {
    confirm_name: "Location fixture",
  });
  await rp.getByRole("button", { name: "Retry location change" }).click();
  await expect(rp.getByRole("alert")).toContainText(/restor/i);
  await expect(r.getByLabel("Amount", { exact: true })).toHaveValue("7.89");
  await rp.screenshot({
    path: new URL("restore-panel.png", evidence).pathname,
  });
  await r.screenshot({
    path: new URL("restore-pending.png", evidence).pathname,
    fullPage: true,
  });
  await rp.getByRole("button", { name: "Cancel location change" }).click();
  await expect(
    rp.getByText("Pending location change canceled.", { exact: true }),
  ).toBeVisible();
  expect((await read(root + "/payee-locations")).length).toBe(beforeCancel);
  console.log(
    "PASS pinned proximity, explicit capture, branch forget, merge/remove, lost ack/reload, new payee, offline expense, HTTP, denial, keyboard, large text, desktop",
  );
} finally {
  await browser?.close();
  await server.stop();
}

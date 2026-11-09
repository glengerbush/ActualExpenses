import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
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
    transaction = randomUUID();
  const root = `/budgets/${budget}`;
  await write("/budgets", { id: budget, name: "Reconciliation acceptance" });
  await write(root + "/preferences", { date_format: "dd/MM/yyyy" }, "PATCH");
  await write(root + "/accounts", {
    id: account,
    name: "Credit card",
    opening_balance: -10000,
    opening_date: "2026-01-01",
  });
  await write(root + "/transactions", {
    id: transaction,
    account_id: account,
    amount: -300,
    date: "2026-02-28",
    cleared: false,
    notes: "Uncleared purchase",
  });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1280, height: 1000 },
  });
  await page.goto(server.url);
  await page
    .getByRole("button", { name: "Reconciliation acceptance Tracking Open" })
    .click();
  await page.getByRole("button", { name: "Credit card", exact: true }).click();
  await page
    .getByRole("button", { name: "Reconcile account", exact: true })
    .click();
  const date = page.getByLabel("Adjustment date", { exact: true });
  await expect(date).toHaveAttribute("placeholder", "DD/MM/YYYY");
  await date.fill("03/04/2026");
  // Reload without blur: retained text must keep its captured day/month meaning.
  await write(root + "/preferences", { date_format: "MM/dd/yyyy" }, "PATCH");
  await page.reload();
  await expect(date).toHaveAttribute("placeholder", "DD/MM/YYYY");
  await expect(date).toHaveValue("03/04/2026");
  await page.getByLabel("Statement balance", { exact: true }).fill("-125.00");
  await page
    .getByRole("button", { name: "Compare statement", exact: true })
    .click();
  await expect(page.getByTestId("reconciliation-difference")).toHaveText(
    "-25.00",
  );
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/reconciliation-desktop.png",
    fullPage: true,
  });
  // Lose the acknowledgement after committing the adjustment, then reload/retry unchanged.
  let dropped = false,
    original;
  await page.route(
    "**/api/v1/budgets/*/accounts/*/reconciliation",
    async (route) => {
      if (route.request().method() === "POST" && !dropped) {
        dropped = true;
        original = {
          body: route.request().postData(),
          key: route.request().headers()["idempotency-key"],
          clock: route.request().headers()["x-actual-edit-clock"],
        };
        const url = new URL(route.request().url());
        const headers = { ...route.request().headers(), host: url.host };
        url.hostname = "127.0.0.1";
        await route.fetch({ url: url.href, headers });
        await route.abort("failed");
      } else await route.continue();
    },
  );
  await page
    .getByRole("button", {
      name: "Create reconciliation transaction",
      exact: true,
    })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry reconciliation", exact: true }),
  ).toBeVisible();
  expect(JSON.parse(original.body).date).toBe("2026-04-03");
  await write(root + "/preferences", { date_format: "yyyy-MM-dd" }, "PATCH");
  await page.reload();
  await expect(date).toHaveAttribute("placeholder", "DD/MM/YYYY");
  await expect(date).toHaveValue("03/04/2026");
  await expect(
    page.getByRole("button", { name: "Retry reconciliation", exact: true }),
  ).toBeVisible();
  const retried = page.waitForRequest(
    (r) => r.method() === "POST" && r.url().endsWith("/reconciliation"),
  );
  await page
    .getByRole("button", { name: "Retry reconciliation", exact: true })
    .click();
  const retry = await retried;
  expect(retry.postData()).toBe(original.body);
  expect(retry.headers()["idempotency-key"]).toBe(original.key);
  expect(retry.headers()["x-actual-edit-clock"]).toBe(original.clock);
  await expect(page.getByTestId("reconciliation-difference")).toHaveText(
    "0.00",
  );
  const savedRows = (
    await (await fetch(api + "/api/v1" + root + "/transactions")).json()
  ).data;
  expect(
    savedRows.find((row) => row.notes === "Reconciliation balance adjustment")
      .date,
  ).toBe("2026-04-03");
  await page
    .getByRole("button", { name: "Lock transactions", exact: true })
    .click();
  await expect(
    page.locator("tbody tr").filter({ hasText: "Reconciled" }),
  ).toHaveCount(2);
  // Confirmed edit keeps protection. Explicit unlock keeps the cleared flag.
  const adjustment = page
    .locator("tbody tr")
    .filter({ hasText: "Reconciliation balance adjustment" });
  await adjustment.getByRole("button", { name: /^Edit / }).click();
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Confirmed statement correction");
  await page
    .getByLabel("Confirm editing this reconciled transaction", { exact: true })
    .check();
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page
      .locator("tbody tr")
      .filter({ hasText: "Confirmed statement correction" }),
  ).toContainText("Reconciled");
  await page
    .locator("tbody tr")
    .filter({ hasText: "Confirmed statement correction" })
    .getByRole("button", { name: /^Unlock / })
    .click();
  await page
    .getByRole("button", { name: "Confirm unlock", exact: true })
    .click();
  await expect(
    page
      .locator("tbody tr")
      .filter({ hasText: "Confirmed statement correction" }),
  ).toContainText("Cleared");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/reconciliation-mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByRole("button", { name: "Exit reconciliation", exact: true })
    .click();
  await expect(page.getByText(/Last reconciled/)).toBeVisible();
  await page.context().setOffline(true);
  await page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Reconcile account", exact: true }),
  ).toBeDisabled();
  await page.context().setOffline(false);
  await page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reconcile account", exact: true })
    .click();
  // Quota failure cannot send a financial mutation or claim success.
  await page.getByLabel("Statement balance", { exact: true }).fill("-126.00");
  await page
    .getByRole("button", { name: "Compare statement", exact: true })
    .click();
  await page.evaluate(() => {
    const original = Storage.prototype.setItem;
    globalThis.restoreStorage = () => (Storage.prototype.setItem = original);
    Storage.prototype.setItem = function (key, value) {
      if (
        key.startsWith("actualBudget.reconciliation.") &&
        JSON.parse(value).pending
      )
        throw new DOMException("Quota exceeded", "QuotaExceededError");
      return original.call(this, key, value);
    };
  });
  await page
    .getByRole("button", {
      name: "Create reconciliation transaction",
      exact: true,
    })
    .click();
  await expect(page.locator(".reconciliation [role=alert]")).toContainText(
    "Quota",
  );
  await page.evaluate(() => globalThis.restoreStorage());
  // A rejected future intent stays immutable; cancel, explicitly recover the local clock, retry new intent.
  await page.evaluate(() => {
    const original = Date.now;
    globalThis.restoreNow = () => (Date.now = original);
    Date.now = () => original() + 86400000;
  });
  await page
    .getByRole("button", {
      name: "Create reconciliation transaction",
      exact: true,
    })
    .click();
  await expect(page.locator(".reconciliation [role=alert]")).toContainText(
    "five minutes ahead",
  );
  await page.evaluate(() => globalThis.restoreNow());
  await page
    .getByRole("button", {
      name: "Cancel reconciliation change safely",
      exact: true,
    })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry reconciliation", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("button", {
      name: "Create reconciliation transaction",
      exact: true,
    })
    .click();
  await page
    .getByRole("button", {
      name: "Clock corrected — resume editing",
      exact: true,
    })
    .click();
  await page
    .getByRole("button", {
      name: "Create reconciliation transaction",
      exact: true,
    })
    .click();
  await expect(page.getByTestId("reconciliation-difference")).toHaveText(
    "0.00",
  );
  await page.setViewportSize({ width: 1280, height: 1000 });
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await page.screenshot({
    path: "test-results/reconciliation-dark.png",
    fullPage: true,
  });
  // Keyboard focus and enlarged text retain usable controls.
  await page.getByLabel("Statement balance", { exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(
    page.getByLabel("Adjustment date", { exact: true }),
  ).toBeFocused();
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({
    path: "test-results/reconciliation-large-text.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => (document.documentElement.style.fontSize = "100%"));
  await page.getByRole("button", { name: "All accounts", exact: true }).click();
  await page
    .locator("tbody tr")
    .filter({ hasText: "Starting Balance" })
    .getByRole("button", { name: /^Unlock / })
    .click();
  await page
    .getByRole("button", { name: "Confirm unlock", exact: true })
    .click();
  await expect(
    page.locator("tbody tr").filter({ hasText: "Starting Balance" }),
  ).toContainText("Cleared");
  // A hidden register must not hide settled split members from reconciliation.
  const splitBudget = randomUUID(),
    splitAccount = randomUUID(),
    splitParent = randomUUID();
  const splitRoot = `/budgets/${splitBudget}`;
  await write("/budgets", {
    id: splitBudget,
    name: "Settled split acceptance",
  });
  await write(splitRoot + "/accounts", {
    id: splitAccount,
    name: "Checking",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await write(splitRoot + "/transactions", {
    id: splitParent,
    account_id: splitAccount,
    amount: -300,
    date: "2026-02-28",
    cleared: true,
    notes: "Settled split",
  });
  const splitPath = splitRoot + `/transactions/${splitParent}/splits`;
  const splitCurrent = await (await fetch(api + "/api/v1" + splitPath)).json();
  await write(splitPath, {
    snapshot: splitCurrent.snapshot,
    children: [
      { id: randomUUID(), amount: -100, notes: "First allocation" },
      { id: randomUUID(), amount: -200, notes: "Second allocation" },
    ],
  });
  await page.goto(server.url);
  await page.getByRole("button", { name: "All budgets", exact: true }).click();
  await page
    .locator(".budget-list button")
    .filter({ hasText: "Settled split acceptance" })
    .click();
  await page.getByRole("button", { name: "Checking", exact: true }).click();
  await page.getByText("Filters and columns", { exact: true }).click();
  await page.getByLabel("Hide cleared transactions", { exact: true }).check();
  await page
    .getByRole("button", { name: "Find transactions", exact: true })
    .click();
  await expect(page.locator("tbody tr")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Reconcile account", exact: true })
    .click();
  await expect(page.getByTestId("reconciliation-difference")).toHaveText(
    "0.00",
  );
  await expect(page.locator(".reconciliation")).toContainText(
    "1 cleared entries ready to lock",
  );
  await page
    .getByRole("button", { name: "Lock transactions", exact: true })
    .click();
  await expect(page.locator(".reconciliation")).toContainText(
    "1 already locked",
  );
  await page.getByLabel("Hide cleared transactions", { exact: true }).uncheck();
  await page
    .getByLabel("Hide reconciled transactions", { exact: true })
    .check();
  await page
    .getByRole("button", { name: "Find transactions", exact: true })
    .click();
  await expect(page.locator("tbody tr")).toHaveCount(0);
  await page
    .getByLabel("Hide reconciled transactions", { exact: true })
    .uncheck();
  await page
    .getByRole("button", { name: "Find transactions", exact: true })
    .click();
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await page.getByRole("button", { name: "Edit splits", exact: true }).click();
  await page
    .getByLabel("Split 1 notes", { exact: true })
    .fill("Confirmed settled allocation");
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Confirm editing this reconciled split",
  );
  await page
    .getByLabel("Confirm editing this reconciled split", { exact: true })
    .check();
  await page.screenshot({
    path: "test-results/reconciliation-split-confirmation.png",
    fullPage: true,
  });
  let splitAttempt;
  await page.route("**/transactions/*/splits", async (route) => {
    if (route.request().method() === "POST" && !splitAttempt) {
      splitAttempt = {
        body: route.request().postData(),
        key: route.request().headers()["idempotency-key"],
        clock: route.request().headers()["x-actual-edit-clock"],
      };
      const url = new URL(route.request().url());
      url.hostname = "127.0.0.1";
      await route.fetch({ url: url.href });
      await route.abort("failed");
    } else await route.continue();
  });
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  expect(
    Object.keys(JSON.parse(splitAttempt.body).reconciled_versions),
  ).toHaveLength(3);
  await page.reload();
  const splitRetry = page.waitForRequest(
    (r) => r.method() === "POST" && r.url().endsWith("/splits"),
  );
  await page
    .getByRole("button", { name: "Retry saved split", exact: true })
    .click();
  const retriedSplit = await splitRetry;
  expect(retriedSplit.postData()).toBe(splitAttempt.body);
  expect(retriedSplit.headers()["idempotency-key"]).toBe(splitAttempt.key);
  expect(retriedSplit.headers()["x-actual-edit-clock"]).toBe(
    splitAttempt.clock,
  );
  await expect(page.locator("tbody tr")).toContainText("Reconciled");
  const protectedSplit = (
    await (await fetch(api + "/api/v1" + splitPath)).json()
  ).data;
  expect(protectedSplit.children.every((row) => row.reconciled)).toBe(true);
  await expect(page.getByRole("button", { name: /^Unlock / })).toBeEnabled();
  await page.screenshot({
    path: "test-results/reconciliation-splits.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: /^Unlock / }).click();
  await expect(page.locator(".reconciliation")).toContainText("all its splits");
  await page
    .getByRole("button", { name: "Confirm unlock", exact: true })
    .click();
  await expect(page.locator("tbody tr")).toContainText("Cleared");
  const unlockedSplit = (
    await (await fetch(api + "/api/v1" + splitPath)).json()
  ).data;
  expect(
    [unlockedSplit, ...unlockedSplit.children].every(
      (row) => row.cleared && !row.reconciled,
    ),
  ).toBe(true);
  console.log(
    "PASS reconciliation browser: plain HTTP, captured date preference/recovery, protected split/filter/retry integration, immutable replay/cancel, quota failure, corrected clock recovery, confirmed edit, unlock, keyboard, mobile/dark/large text, offline boundary",
  );
} finally {
  await browser?.close();
  await server.stop();
}

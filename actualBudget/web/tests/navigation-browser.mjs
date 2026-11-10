import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
let browser;
try {
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
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  const api = async (path, body, method = "POST") =>
    page.evaluate(
      async ({ path, body, method, key }) => {
        const r = await fetch("/api/v1" + path, {
          method,
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
          },
          body: JSON.stringify(body),
        });
        if (!r.ok) throw Error(await r.text());
        return r.json();
      },
      { path, body, method, key: randomUUID() },
    );
  const id = randomUUID(),
    account = randomUUID();
  await api("/budgets", {
    id,
    name: "Navigation fixture",
    budget_type: "tracking",
  });
  await api(`/budgets/${id}/accounts`, {
    id: account,
    name: "Everyday",
    opening_balance: 123456,
    opening_date: "2026-11-09",
    off_budget: false,
  });
  await page.reload();
  await page.getByRole("button", { name: /Navigation fixture.*Open/ }).click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  await expect(page.locator("td.amount").first()).toHaveText("1,234.56");
  await page.route(`**/api/v1/budgets/${id}/preferences`, async (route) => {
    if (route.request().method() === "PATCH")
      await route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          error: { code: "unavailable", message: "Try again" },
        }),
      });
    else await route.continue();
  });
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.getByRole("alert").filter({ hasText: "Retry the action" }),
  ).toBeVisible();
  await expect(page.locator("td.amount").first()).toHaveText("1,234.56");
  await page.unroute(`**/api/v1/budgets/${id}/preferences`);
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(page.locator("td.amount").first()).toHaveText("••••");
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  const amountInput = page.getByLabel("Opening balance", { exact: true });
  await amountInput.fill("345.67");
  await page.getByLabel("Account name").focus();
  await page.mouse.move(0, 0);
  expect(
    await page.evaluate(
      async () => (await document.fonts.load('16px "Redacted Script"')).length,
    ),
  ).toBeGreaterThan(0);
  await expect(amountInput).toHaveCSS(
    "font-family",
    '"Redacted Script", sans-serif',
  );
  await expect(amountInput).toHaveValue("345.67");
  await amountInput.focus();
  await expect(amountInput).not.toHaveCSS(
    "font-family",
    '"Redacted Script", sans-serif',
  );
  await page.getByLabel("Account name").focus();
  await amountInput.hover();
  await expect(amountInput).not.toHaveCSS(
    "font-family",
    '"Redacted Script", sans-serif',
  );
  // A new register reads its older offline snapshot before HTTP refresh.
  const held = [];
  await page.route(`**/api/v1/budgets/${id}/checkpoint`, (route) => {
    held.push(route);
  });
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect.poll(() => held.length).toBeGreaterThan(0);
  await expect(page.locator("td.amount").first()).toHaveText("••••");
  for (const route of held) await route.continue();
  await page.unroute(`**/api/v1/budgets/${id}/checkpoint`);
  await page.reload();
  await expect(page.locator("td.amount").first()).toHaveText("••••");

  await page.keyboard.press("Control+k");
  const search = page.getByRole("searchbox", { name: "Search commands" });
  await expect(search).toBeFocused();
  await search.fill("Everyday");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("heading", { name: "Everyday", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Control+k");
  await search.fill("All");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("?");
  await expect(
    page.getByRole("heading", { name: "actualBudget help", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+k");
  await search.fill("Settings");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("heading", { name: "Settings", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.locator(".account-navigation .nav-amount").first(),
  ).toHaveText("1,234.56");
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Settings", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.locator(".account-navigation .nav-amount").first(),
  ).toHaveText("••••");
  let preferenceReads = 0,
    refreshRoute;
  await page.route(`**/api/v1/budgets/${id}/preferences?*`, async (route) => {
    if (++preferenceReads === 2) refreshRoute = route;
    else await route.continue();
  });
  await page.locator("#week-start").selectOption("1");
  await page
    .getByRole("button", { name: "Save shared preferences", exact: true })
    .click();
  await expect.poll(() => !!refreshRoute).toBe(true);
  await expect(
    page.locator(".account-navigation .nav-amount").first(),
  ).toHaveText("••••");
  await refreshRoute.continue();
  await page.unroute(`**/api/v1/budgets/${id}/preferences?*`);
  await expect(
    page.getByRole("status").filter({ hasText: "Shared preferences saved." }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Settings", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Search commands", exact: true })
    .click();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("button", { name: "Search commands", exact: true }),
  ).toBeFocused();
  await page
    .getByRole("button", { name: "Help and releases", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "actualBudget help", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("No external update checks or analytics."),
  ).toBeVisible();
  await page.getByLabel("Show local release notices on this device").check();
  await page.getByRole("button", { name: "Close help", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "actualBudget navigation" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Help and releases", exact: true })
    .click();
  await page.getByLabel("Show local release notices on this device").uncheck();
  await page.getByRole("button", { name: "Close help", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Dismiss release notice" }),
  ).toHaveCount(0);
  await page.reload();
  await page
    .getByRole("button", { name: "Help and releases", exact: true })
    .click();
  await expect(
    page.getByLabel("Show local release notices on this device"),
  ).not.toBeChecked();
  await page.getByLabel("Show local release notices on this device").check();
  await page.getByRole("button", { name: "Close help", exact: true }).click();
  await page.getByRole("button", { name: "Dismiss release notice" }).click();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Dismiss release notice" }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Discard changes", exact: true })
    .click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page
    .getByRole("button", { name: "Search commands", exact: true })
    .click();
  await search.fill("Everyday");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("heading", { name: "Everyday", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".amount").first()).toContainText("••••");
  await context.setOffline(true);
  await page
    .getByRole("button", { name: "Help and releases", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "actualBudget help", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await mkdir("artifacts", { recursive: true });
  await page.screenshot({
    path: "artifacts/navigation-mobile.png",
    fullPage: false,
  });
  await page.getByRole("button", { name: "Close help", exact: true }).click();
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({
    path: "artifacts/navigation-desktop.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "navigation browser: privacy, keyboard search/focus, remembered views, notices, mobile/offline help passed",
  );
} finally {
  await browser?.close();
  await server.stop();
}

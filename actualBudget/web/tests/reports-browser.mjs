// Public browser workflow; external supplied servers are honored.
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
    }),
    page = await context.newPage();
  page.on("pageerror", (error) => console.error("PAGEERROR", error.message));
  await page.clock.setFixedTime(new Date("2026-11-09T12:00:00Z"));
  await page.goto(server.url);
  const budget = randomUUID(),
    account = randomUUID(),
    name = "Dashboard browser " + randomUUID();
  const api = (path, body) =>
    page.evaluate(
      async ({ path, body, key }) => {
        const r = await fetch("/api/v1" + path, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
          },
          body: JSON.stringify(body),
        });
        if (!r.ok) throw Error(await r.text());
        return r.json();
      },
      { path, body, key: randomUUID() },
    );
  await api("/budgets", { id: budget, name, budget_type: "tracking" });
  await api(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Report spending",
    off_budget: false,
    opening_balance: 10000,
    opening_date: "2026-11-01",
  });
  await page.reload();
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page
    .getByRole("button", { name: "New dashboard", exact: true })
    .click();
  await page.getByLabel("Dashboard name", { exact: true }).fill("Household");
  await page
    .getByRole("button", { name: "Save dashboard", exact: true })
    .click();
  await page.getByRole("button", { name: "Add widget", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Widget type", exact: true })
    .selectOption("markdown-card");
  await page
    .getByLabel("Text content", { exact: true })
    .fill("Household **recorded** history");
  await page.getByRole("button", { name: "Save widget", exact: true }).click();
  await expect(page.getByText("recorded", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Add widget", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Widget type", exact: true })
    .selectOption("summary-card");
  await page.getByLabel("Widget name", { exact: true }).fill("Recorded total");
  await page.getByRole("button", { name: "Save widget", exact: true }).click();
  await expect(
    page.getByTestId("report-widget").filter({ hasText: "Recorded total" }),
  ).toContainText("100.00");
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Dashboards and reports", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByTestId("report-widget").filter({ hasText: "Recorded total" }),
  ).toContainText("100.00");
  for (const type of [
    "cash-flow-card",
    "net-worth-card",
    "spending-card",
    "calendar-card",
  ]) {
    await page.getByRole("button", { name: "Add widget", exact: true }).click();
    await page
      .getByRole("combobox", { name: "Widget type", exact: true })
      .selectOption(type);
    await page
      .getByLabel("Widget name", { exact: true })
      .fill("Sample " + type);
    await page
      .getByRole("button", { name: "Save widget", exact: true })
      .click();
    await expect(
      page
        .getByTestId("report-widget")
        .filter({ hasText: "Sample " + type })
        .getByText("Loading report…", { exact: true }),
    ).toHaveCount(0);
  }
  await expect(page.getByTestId("report-widget")).toHaveCount(6);
  await expect(
    page.getByRole("region", { name: "Transaction calendar", exact: true }),
  ).toBeVisible();
  expect(await page.getByRole("img").count()).toBeGreaterThanOrEqual(3);
  await page
    .getByTestId("report-widget")
    .filter({ hasText: "Recorded total" })
    .getByRole("button", { name: "Edit Recorded total", exact: true })
    .click();
  await page
    .getByLabel("Widget name", { exact: true })
    .fill("Interrupted private summary");
  await context.route("**/reporting/commands", (route) => route.abort());
  await page.getByRole("button", { name: "Save widget", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Retry saved reporting change",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Cancel saved reporting change", exact: true })
    .click();
  await expect(page.getByText(/Cancellation is pending/)).toBeVisible();
  await page.reload();
  await expect(page.getByText(/Cancellation is pending/)).toBeVisible();
  // Public recovery flow clears downloaded caches; exact pending intent survives.
  await page
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("navigation", { name: "Settings sections" })
    .getByRole("button", { name: "Recovery", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reset downloaded data", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm cache reset", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(page.getByText(/Cancellation is pending/)).toBeVisible();
  await expect(page.getByLabel("Widget name", { exact: true })).toHaveValue(
    "Interrupted private summary",
  );
  await context.unroute("**/reporting/commands");
  const cancelRequest = page.waitForRequest((r) =>
    r.url().endsWith("/reporting/commands"),
  );
  await page
    .getByRole("button", { name: "Retry saved reporting change", exact: true })
    .click();
  expect(
    (await cancelRequest).headers()["x-actual-cancel-if-uncommitted"],
  ).toBe("true");
  await expect(
    page.getByText("Change canceled before saving. Your draft is retained.", {
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Discard local draft", exact: true })
    .click();
  await expect(
    page.getByTestId("report-widget").filter({ hasText: "Recorded total" }),
  ).toContainText("100.00");
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await page.screenshot({
    path: new URL("../artifacts/reports-desktop.png", import.meta.url).pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    page.getByRole("combobox", { name: "Dashboard", exact: true }),
  ).toHaveValue(/.+/);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: new URL("../artifacts/reports-mobile.png", import.meta.url).pathname,
    fullPage: true,
  });
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Theme", exact: true })
    .selectOption("dark");
  await page
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  const exact = page
    .getByTestId("report-widget")
    .filter({ hasText: "Recorded total" })
    .getByText("Exact values", { exact: true });
  await exact.focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  await expect(
    page
      .getByTestId("report-widget")
      .filter({ hasText: "Recorded total" })
      .getByRole("region", { name: "Historical report values", exact: true }),
  ).toBeFocused();
  await page.screenshot({
    path: new URL("../artifacts/reports-dark.png", import.meta.url).pathname,
    fullPage: true,
  });
  // Honest200% layout-zoom equivalent: half the CSS viewport and doubled DPR
  // retains the1280×900 physical viewport. This does not set browser chrome zoom.
  const zoomContext = await browser.newContext({
      viewport: { width: 640, height: 450 },
      deviceScaleFactor: 2,
      storageState: await context.storageState(),
    }),
    zoomPage = await zoomContext.newPage();
  await zoomPage.clock.setFixedTime(new Date("2026-11-09T12:00:00Z"));
  await zoomPage.goto(server.url);
  await expect(
    zoomPage.getByRole("heading", {
      name: "Dashboards and reports",
      exact: true,
    }),
  ).toBeVisible();
  await expect(zoomPage.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(
    await zoomPage.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await zoomPage
    .getByRole("button", { name: "Add widget", exact: true })
    .focus();
  await zoomPage.keyboard.press("Tab");
  await expect(
    zoomPage
      .getByTestId("report-widget")
      .first()
      .getByRole("button", { name: "Edit Text", exact: true }),
  ).toBeFocused();
  await zoomPage.screenshot({
    path: new URL(
      "../artifacts/reports-dark-200-equivalent.png",
      import.meta.url,
    ).pathname,
  });
  await zoomContext.close();
  console.log(
    "PASS dashboard browser: creation, text/summary, persisted layout, remembered destination, lost cancellation/reload/cache reset, mobile/dark/200% layout equivalent/keyboard",
  );
} finally {
  await browser?.close();
  await server.stop();
}

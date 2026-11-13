// Public browser workflow; external supplied servers are honored.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
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
    name = "Forecast browser " + budget;
  const api = (path, body, headers = {}) =>
    page.evaluate(
      async ({ path, body, headers, key }) => {
        const r = await fetch("/api/v1" + path, {
          method: body === undefined ? "GET" : "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
            ...headers,
          },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        if (!r.ok) throw Error(await r.text());
        return r.json();
      },
      { path, body, headers, key: randomUUID() },
    );
  const bytes = isolatedImportBytes(
    await readFile(
      new URL(
        "../../tests/fixtures/forecast-source-26.10.zip",
        import.meta.url,
      ),
    ),
  ).toString("base64");
  const imported = await page.evaluate(
    async ({ bytes, key }) => {
      const r = await fetch("/api/v1/imports/actual", {
        method: "POST",
        headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
        body: Uint8Array.from(atob(bytes), (c) => c.charCodeAt(0)),
      });
      if (!r.ok) throw Error(await r.text());
      return r.json();
    },
    { bytes, key: randomUUID() },
  );
  expect(["ready", "activated"]).toContain(imported.data.status);
  if (imported.data.status === "ready")
    await api(`/imports/${imported.data.id}/activate`, {});
  const root = "/budgets/" + imported.data.budget_id,
    state = await api(root + "/reporting");
  await api(
    root + "/duplicate",
    { id: budget, name },
    { "X-Actual-Budget-Epoch": state.data.epoch },
  );
  await page.reload();
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Dashboard", exact: true })
    .selectOption({ label: "Household history" });
  const widget = page.getByRole("article", {
    name: "Saved balance forecast",
    exact: true,
  });
  await expect(
    widget.getByRole("region", { name: "Balance forecast result" }),
  ).toBeVisible();
  await expect(widget).toContainText("Estimated balances");
  await expect(
    widget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("row").filter({ hasText: "104.00" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("row").filter({ hasText: "103.00" }),
  ).toBeVisible();
  await widget.getByText("Scheduled entry details", { exact: true }).click();
  await expect(
    widget.getByRole("region", { name: "Forecast schedule details" }),
  ).toContainText("-1.00");
  await widget.getByText("Assumptions and limits", { exact: true }).click();
  await expect(widget).toContainText("No entries are posted");
  await widget
    .getByRole("button", { name: "Edit Saved balance forecast", exact: true })
    .click();
  const editor = page.getByRole("form", { name: "Reporting editor" });
  await expect(
    editor.getByRole("combobox", { name: "Forecast source", exact: true }),
  ).toHaveValue("schedules");
  await expect(
    editor.getByRole("combobox", { name: "Interval", exact: true }),
  ).toHaveValue("Daily");
  await editor
    .getByLabel("Widget name", { exact: true })
    .fill("Forecast baseline");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  const savedWidget = page.getByRole("article", {
    name: "Forecast baseline",
    exact: true,
  });
  await expect(
    savedWidget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  const saved = (await api(`/budgets/${budget}/reporting`)).data.documents.find(
    (d) => d.config.meta?.name === "Forecast baseline",
  );
  await savedWidget
    .getByRole("button", { name: "Edit Forecast baseline", exact: true })
    .click();
  await editor
    .getByLabel("Widget name", { exact: true })
    .fill("Canceled forecast");
  await context.route("**/reporting/commands", (r) => r.abort());
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
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
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Recovery tools", exact: true })
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
  await expect(editor.getByLabel("Widget name", { exact: true })).toHaveValue(
    "Canceled forecast",
  );
  await expect(page.getByText(/Cancellation is pending/)).toBeVisible();
  await context.unroute("**/reporting/commands");
  const retry = page.waitForRequest((r) =>
    r.url().endsWith("/reporting/commands"),
  );
  await page
    .getByRole("button", { name: "Retry saved reporting change", exact: true })
    .click();
  expect((await retry).headers()["x-actual-cancel-if-uncommitted"]).toBe(
    "true",
  );
  await expect(
    page.getByText("Change canceled before saving. Your draft is retained.", {
      exact: true,
    }),
  ).toBeVisible();
  expect(
    (await api(`/budgets/${budget}/reporting`)).data.documents.find(
      (d) => d.id === saved.id,
    ),
  ).toEqual(saved);
  await editor
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  await editor
    .getByRole("button", { name: "Discard local draft", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Balance forecast", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Dashboard", exact: true })
    .selectOption({ label: "Household history" });
  await expect(savedWidget).toBeVisible();
  await savedWidget
    .getByRole("button", { name: "Edit Forecast baseline", exact: true })
    .click();
  await editor
    .getByRole("combobox", { name: "Forecast source", exact: true })
    .selectOption("tracking-budget");
  await editor
    .getByRole("combobox", { name: "Interval", exact: true })
    .selectOption("Monthly");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(savedWidget.getByRole("alert")).toContainText(
    "Tracking Budget file",
  );
  await savedWidget
    .getByRole("button", { name: "Edit Forecast baseline", exact: true })
    .click();
  await editor
    .getByRole("combobox", { name: "Forecast source", exact: true })
    .selectOption("schedules");
  await editor
    .getByRole("combobox", { name: "Interval", exact: true })
    .selectOption("Daily");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(
    savedWidget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await context.route("**/reports/forecast", (r) => r.abort());
  await savedWidget
    .getByRole("button", { name: "Recalculate forecast", exact: true })
    .click();
  await expect(savedWidget.getByRole("status")).toContainText(
    "Cached estimate",
  );
  await expect(
    savedWidget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await context.unroute("**/reports/forecast");
  await savedWidget
    .getByRole("button", { name: "Recalculate forecast", exact: true })
    .click();
  await expect(savedWidget.getByRole("status")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.keyboard.press("Control+k");
  await page
    .getByRole("searchbox", { name: "Search commands" })
    .fill("Balance forecast");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("button", { name: "Balance forecast", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "New forecast", exact: true }).click();
  await expect(
    editor.getByRole("combobox", { name: "Widget type", exact: true }),
  ).toHaveValue("balance-forecast-card");
  await editor
    .getByLabel("Widget name", { exact: true })
    .fill("New forecast witness");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  const createdWidget = page.getByRole("article", {
    name: "New forecast witness",
    exact: true,
  });
  await expect(
    createdWidget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await createdWidget
    .getByRole("button", { name: "Remove New forecast witness", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm remove", exact: true })
    .click();
  await expect(createdWidget).toHaveCount(0);
  const beforeOff = (await api(`/budgets/${budget}/reporting`)).data.documents;
  async function optIn(enabled) {
    await page
      .getByRole("button", { name: "Close reports", exact: true })
      .click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByText("Experimental features", { exact: true }).click();
    await page
      .getByRole("checkbox", { name: "Balance forecast", exact: true })
      .setChecked(enabled);
    await page
      .getByRole("button", { name: "Save shared preferences", exact: true })
      .click();
    await expect
      .poll(
        async () =>
          (await api(`/budgets/${budget}/preferences`)).data.experimental
            .balanceForecastReport,
      )
      .toBe(enabled);
    await page.getByRole("button", { name: "Done", exact: true }).click();
    await page
      .getByRole("button", { name: "Dashboards and reports", exact: true })
      .click();
  }
  await optIn(false);
  await expect(savedWidget).toContainText("Enable Balance forecast");
  await expect(
    savedWidget.getByRole("table", { name: "Forecast balances" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "New forecast", exact: true }),
  ).toBeDisabled();
  expect((await api(`/budgets/${budget}/reporting`)).data.documents).toEqual(
    beforeOff,
  );
  await optIn(true);
  await expect(
    savedWidget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await page.screenshot({
    path: new URL("../artifacts/forecast-desktop.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: new URL("../artifacts/forecast-mobile.png", import.meta.url).pathname,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Theme", exact: true })
    .selectOption("dark");
  await page.locator("#language").selectOption("de");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await savedWidget
    .getByRole("button", { name: "Bearbeiten Forecast baseline", exact: true })
    .click();
  await expect(
    editor
      .getByRole("combobox", { name: "Intervall", exact: true })
      .getByRole("option", { name: "Täglich", exact: true }),
  ).toHaveCount(1);
  await expect(
    editor
      .getByRole("combobox", { name: "Forecast source", exact: true })
      .getByRole("option", { name: "Terminpläne", exact: true }),
  ).toHaveCount(1);
  await expect(editor.getByLabel("Widget name", { exact: true })).toHaveValue(
    "Forecast baseline",
  );
  await editor.getByLabel("Widget name", { exact: true }).focus();
  await page.keyboard.press("Tab");
  expect(await page.evaluate(() => document.activeElement?.tagName)).toBe(
    "SELECT",
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({
    path: new URL("../artifacts/forecast-dark-german.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  // 640px CSS viewport plus 200% root font at DPR1; not browser-menu zoom.
  await page.setViewportSize({ width: 640, height: 450 });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: new URL("../artifacts/forecast-dark-large-text.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  await editor
    .getByRole("combobox", { name: "Forecast source", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL(
      "../artifacts/forecast-dark-large-text-viewport.png",
      import.meta.url,
    ).pathname,
  });
  expect(
    (await api(`/budgets/${budget}/reporting`)).data.documents.find(
      (d) => d.id === saved.id,
    ).config.meta.accounts,
  ).toEqual(saved.config.meta.accounts);
  console.log(
    "PASS source forecast dates/amounts, settings/save, canceled intent across reload/cache reset, method error recovery, German, mobile/dark/large-text keyboard",
  );
} finally {
  await browser?.close();
  await server.stop();
}

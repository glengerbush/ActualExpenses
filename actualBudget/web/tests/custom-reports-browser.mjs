// Public browser workflow; external supplied servers are honored.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
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
    name = "Custom browser " + budget;
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
  const bytes = (
    await readFile(
      new URL("../../tests/fixtures/reports-source-26.10.zip", import.meta.url),
    )
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
    .getByRole("button", { name: "Custom reports", exact: true })
    .click();
  await expect(
    page.getByRole("form", { name: "Custom report editor" }),
  ).toBeVisible();
  const editor = page.getByRole("form", { name: "Custom report editor" }),
    result = page.getByRole("region", { name: "Custom report result" });
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Saved category history",
  );
  await expect(result).toContainText("60.00");
  const savedBeforeLanguage = (
    await api(`/budgets/${budget}/reporting`)
  ).data.documents.find((d) => d.kind === "custom_report" && !d.tombstone);
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click(); // Fault injection: conflicting downloaded keys must fail visibly, not overwrite a message.
  await context.route("**/de-*.json", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        "collision.key": "first",
        collision: { key: "second" },
      }),
    }),
  );
  await page.locator("#language").selectOption("de");
  await expect(
    page.getByRole("alert").filter({ hasText: "Language preference is saved" }),
  ).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await context.unroute("**/de-*.json");
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  const germanMode = page.getByRole("combobox", {
      name: "Balance mode",
      exact: true,
    }),
    germanInterval = page.getByRole("combobox", {
      name: "Report interval",
      exact: true,
    }),
    germanGroup = page.getByRole("combobox", {
      name: "Group report by",
      exact: true,
    });
  await expect(
    germanMode.getByRole("option", { name: "Auszahlung", exact: true }),
  ).toHaveAttribute("value", "Payment");
  await expect(
    germanMode.getByRole("option", { name: "Nettoauszahlung", exact: true }),
  ).toHaveAttribute("value", "Net Payment");
  await expect(germanMode).toHaveValue("Payment");
  await expect(
    germanInterval.getByRole("option", { name: "Monatlich", exact: true }),
  ).toHaveAttribute("value", "Monthly");
  await expect(germanInterval).toHaveValue("Monthly");
  await expect(
    germanGroup.getByRole("option", { name: "Kategorie", exact: true }),
  ).toHaveAttribute("value", "Category");
  await expect(
    germanGroup.getByRole("option", { name: "Konto", exact: true }),
  ).toHaveAttribute("value", "Account");
  await germanGroup.selectOption({ label: "Konto" });
  await expect(germanGroup).toHaveValue("Account");
  await germanGroup.selectOption({ label: "Kategorie" });
  await page
    .getByRole("button", { name: "Preview report", exact: true })
    .click();
  await expect(result).toContainText("60.00");
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Saved category history",
  );
  await page.getByRole("button", { name: "Save report", exact: true }).click();
  await expect(page.getByLabel("Report name", { exact: true })).toBeEnabled();
  const savedAfterLanguage = (
    await api(`/budgets/${budget}/reporting`)
  ).data.documents.find((d) => d.id === savedBeforeLanguage.id);
  expect(savedAfterLanguage.config).toEqual(savedBeforeLanguage.config);
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await page.screenshot({
    path: new URL("../artifacts/custom-reports-german.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Einstellungen", exact: true })
    .click();
  await page.locator("#language").selectOption("en");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(result).toContainText("60.00");
  console.log(
    "PASS German shipped report labels preserve API option values, user report name, saved config and60.00 result",
  );
  await page.getByText("Exact values", { exact: true }).click();
  await expect(result.getByRole("row", { name: /food.*60.00/ })).toBeVisible();
  await page
    .getByRole("combobox", { name: "Balance mode", exact: true })
    .selectOption("Net Payment");
  await page
    .getByRole("button", { name: "Preview report", exact: true })
    .click();
  await expect(result).toContainText("58.00");
  await expect(result).toContainText(
    "Payments are shown as positive magnitudes; details retain recorded signs.",
  );
  await page
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  for (const [format, title] of [
    ["TableGraph", null],
    ["BarGraph", "Bar report"],
    ["LineGraph", "Line report"],
    ["AreaGraph", "Area report"],
    ["DonutGraph", "Donut report"],
  ]) {
    await page
      .getByRole("combobox", { name: "Report format", exact: true })
      .selectOption(format);
    await page
      .getByRole("button", { name: "Preview report", exact: true })
      .click();
    await expect(
      page.getByText("Loading report values…", { exact: true }),
    ).toHaveCount(0);
    if (title)
      await expect(
        result.getByRole("img", { name: new RegExp(title) }),
      ).toBeVisible();
    else
      await expect(
        result.getByRole("row", { name: /food.*60.00/ }),
      ).toBeVisible();
  }
  await page
    .getByRole("combobox", { name: "Report mode", exact: true })
    .selectOption("time");
  await page
    .getByRole("button", { name: "Preview report", exact: true })
    .click();
  await expect(result).toContainText("Nov '26");
  await page
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  await expect(
    page.getByRole("combobox", { name: "Report format", exact: true }),
  ).toHaveValue("BarGraph");
  await expect(
    page.getByRole("combobox", { name: "Report mode", exact: true }),
  ).toHaveValue("total");
  await page
    .getByText("Dates, visibility and filters", { exact: true })
    .click();
  await page.getByLabel("Use static dates", { exact: true }).uncheck();
  await page
    .getByRole("combobox", { name: "Live date range", exact: true })
    .selectOption("This month");
  await page
    .getByRole("button", { name: "Preview report", exact: true })
    .click();
  await expect(result).toContainText("50.00");
  await page
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  await page.getByText("Inspect recorded entries", { exact: true }).click();
  await page
    .getByRole("combobox", { name: "Report value", exact: true })
    .selectOption({ label: "food total" });
  await page.getByRole("button", { name: "Load entries", exact: true }).click();
  await expect(result).toContainText("5 recorded entries · Sum");
  await expect(result).toContainText("-60.00");
  await page
    .getByRole("button", { name: "Add report condition", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Report field 2", exact: true })
    .selectOption("amount");
  await page
    .getByRole("combobox", { name: "Report operator 2", exact: true })
    .selectOption("gt");
  await page.getByLabel("Amount value 2", { exact: true }).fill("0");
  await page
    .getByRole("button", { name: "Preview report", exact: true })
    .click();
  await expect(result).toContainText("0.00");
  await page
    .getByRole("button", { name: "Remove report condition 2", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Duplicate and save", exact: true })
    .click();
  await expect(page.getByText("Report saved.", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Saved category history copy",
  );
  await page
    .getByLabel("Report name", { exact: true })
    .fill("Household custom report");
  await page.getByRole("button", { name: "Save report", exact: true }).click();
  await expect(
    page.getByRole("option", { name: "Household custom report", exact: true }),
  ).toHaveCount(1);
  await expect(page.getByLabel("Report name", { exact: true })).toBeEnabled();
  await page.reload();
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Household custom report",
  );
  await page
    .getByLabel("Report name", { exact: true })
    .fill("Interrupted custom report");
  await context.route("**/reporting/commands", (route) => route.abort());
  await page.getByRole("button", { name: "Save report", exact: true }).click();
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
  await expect(page.getByText(/Cancellation is pending/)).toBeVisible();
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Interrupted custom report",
  );
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
    page.getByText(
      "Reporting change canceled before saving. Your draft is retained.",
      { exact: true },
    ),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Household custom report",
  );
  await page.getByRole("button", { name: "Dashboards", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Dashboard", exact: true })
    .selectOption({ label: "Household history" });
  const widget = page
    .getByTestId("report-widget")
    .filter({ hasText: "Saved category history" });
  await expect(widget).toContainText("60.00");
  await widget
    .getByRole("button", { name: "Open saved report", exact: true })
    .click();
  await expect(page.getByLabel("Report name", { exact: true })).toHaveValue(
    "Saved category history",
  );
  await page
    .getByRole("combobox", { name: "Report format", exact: true })
    .selectOption("TableGraph");
  await page
    .getByRole("button", { name: "Preview report", exact: true })
    .click();
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(result).not.toContainText("60.00");
  await expect(result).toContainText("••••");
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await page.screenshot({
    path: new URL("../artifacts/custom-reports-desktop.png", import.meta.url)
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
    path: new URL("../artifacts/custom-reports-mobile.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Reset to saved", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Theme", exact: true })
    .selectOption("dark");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByLabel("Report name", { exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(
    page.getByRole("combobox", { name: "Report format", exact: true }),
  ).toBeFocused();
  await page.screenshot({
    path: new URL("../artifacts/custom-reports-dark.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  const zoom = await browser.newContext({
      viewport: { width: 640, height: 450 },
      deviceScaleFactor: 2,
      storageState: await context.storageState(),
    }),
    zp = await zoom.newPage();
  await zp.goto(server.url);
  await zp.getByRole("button", { name: "Custom reports", exact: true }).click();
  await expect(
    zp.getByRole("form", { name: "Custom report editor" }),
  ).toBeVisible();
  expect(
    await zp.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  await zp.screenshot({
    path: new URL(
      "../artifacts/custom-reports-dark-200-equivalent.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await zoom.close();
  await page
    .getByRole("button", { name: "Delete saved report", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm delete report", exact: true })
    .click();
  await expect(
    page.getByText(
      "Saved report removed. The current view remains an unsaved draft.",
      { exact: true },
    ),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Discard local changes", exact: true })
    .click();
  await page.getByRole("button", { name: "Dashboards", exact: true }).click();
  await expect(
    page
      .getByTestId("report-widget")
      .filter({ hasText: "This saved report is unavailable." }),
  ).toHaveCount(1);
  const final = await api(`/budgets/${budget}/reporting`);
  expect(
    final.data.documents.some(
      (d) =>
        d.kind === "widget" &&
        !d.tombstone &&
        d.config.type === "custom-report",
    ),
  ).toBe(true);
  console.log(
    "PASS source saved report, five formats, live/static, time, reset, duplicate/save/reload, details, widget link, durable canceled intent/cache reset, privacy, mobile/dark/200%-equivalent keyboard",
  );
} finally {
  await browser?.close();
  await server.stop();
}

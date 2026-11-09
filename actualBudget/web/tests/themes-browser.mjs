import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
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
    acceptDownloads: true,
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(server.url);
  const id = randomUUID();
  await page.evaluate(async (id) => {
    const response = await fetch("/api/v1/budgets", {
      method: "POST",
      headers: { "Content-Type": "application/json", "Idempotency-Key": id },
      body: JSON.stringify({
        id,
        name: "Theme acceptance",
        budget_type: "tracking",
      }),
    });
    if (!response.ok) throw Error(await response.text());
    localStorage.setItem(
      "actualBudget.selection",
      JSON.stringify({ budgetId: id }),
    );
  }, id);
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Open theme editor" }).click();
  const source =
    "/* exact source retained */\n:root { --color-pageBackground: #e8f0ed; --color-pageText: #123456; --color-futureWidget: #abcdef; }";
  await page.getByLabel("Custom Theme CSS", { exact: true }).fill(source);
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Open theme editor" }).click();
  await expect(
    page.getByLabel("Custom Theme CSS", { exact: true }),
  ).toHaveValue(source);
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Custom theme saved" }),
  ).toBeVisible();
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(232, 240, 237)",
  );
  await expect(
    page.getByRole("list", { name: "Theme warnings" }),
  ).toContainText("--color-futureWidget");
  let startupDownloads = 0;
  page.on("request", (request) => {
    if (
      request.url().includes("raw.githubusercontent.com") ||
      request.url().includes("/themes/compile")
    )
      startupDownloads++;
  });
  await page.reload();
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(232, 240, 237)",
  );
  expect(startupDownloads).toBe(0);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Open theme editor" }).click();
  await page.getByLabel("Theme slot").selectOption("dark");
  await page.getByLabel("Base theme", { exact: true }).selectOption("midnight");
  await page
    .getByLabel("Custom Theme CSS", { exact: true })
    .fill(
      ":root { --color-pageBackground: #102030; --color-pageText: #eef5fa; }",
    );
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Custom theme saved" }),
  ).toBeVisible();
  await page.locator("#theme").selectOption("system");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(16, 32, 48)",
  );
  await expect(page.locator("html")).toHaveAttribute("data-theme", "midnight");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(232, 240, 237)",
  );
  await page.getByLabel("Theme slot").selectOption("override");
  await page
    .getByLabel("Custom Theme CSS", { exact: true })
    .fill(":root { --color-pageText: #000; --color-pageBackground: #000; }");
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(page.locator("html")).toHaveCSS("color", "rgb(0, 0, 0)");
  const recovery = page.getByRole("complementary", {
    name: "Custom theme recovery",
  });
  await expect(recovery).toHaveCSS("background-color", "rgb(255, 255, 255)");
  await expect(
    recovery.getByRole("button", { name: "Use built-in theme" }),
  ).toHaveCSS("color", "rgb(23, 35, 27)");
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/themes-recovery-desktop.png",
    fullPage: true,
  });
  const downloadPromise = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Export original theme data", exact: true })
    .click();
  const download = await downloadPromise;
  const exported = JSON.parse(await readFile(await download.path(), "utf8"));
  expect(exported.light.source).toBe(source);
  expect(exported.override.source).toContain("--color-pageText: #000");
  await recovery.getByRole("button", { name: "Use built-in theme" }).click();
  await expect(recovery).toHaveCount(0);
  await page.getByText("Import theme configuration", { exact: true }).click();
  await page
    .getByLabel("Saved theme data or Actual global preferences JSON")
    .fill(JSON.stringify(exported));
  await page.getByRole("button", { name: "Import on this device" }).click();
  await expect(recovery).toBeVisible();
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(0, 0, 0)",
  );
  await page.locator("#theme").selectOption("dark");
  const switchedDownload = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Export original theme data", exact: true })
    .click();
  const switched = JSON.parse(
    await readFile(await (await switchedDownload).path(), "utf8"),
  );
  expect(switched.light).toBeNull();
  expect(switched.dark).toBeNull();
  expect(switched.override.source).toContain("--color-pageText: #000");
  expect(switched.original.replaced_installed_slots.light.source).toBe(source);
  await recovery.getByRole("button", { name: "Use built-in theme" }).click();
  console.log(
    "PASS persistent theme draft/install, unsupported-token warning, no startup downloads, light/dark/system slots, readable recovery and original export/import",
  );
  // Catalog CSS and relative fonts are downloaded only after explicit choice.
  await page.route(
    "https://raw.githubusercontent.com/actualbudget/demo-theme/refs/heads/main/actual.css",
    (route) =>
      route.fulfill({
        status: 200,
        contentType: "text/css",
        body: "@font-face { font-family: Fixture; src: url(fonts/fixture.woff2); } :root { --color-pageBackground: #f0e8dd; --font-family: Fixture, system-ui; }",
      }),
  );
  await page.route(
    "https://raw.githubusercontent.com/actualbudget/demo-theme/refs/heads/main/fonts/fixture.woff2",
    async (route) =>
      route.fulfill({
        status: 200,
        body: await readFile(
          new URL("./fixtures/theme-fixture.woff2", import.meta.url),
        ),
        contentType: "font/woff2",
      }),
  );
  await page.getByLabel("Theme slot").selectOption("light");
  await page
    .getByLabel("Catalog theme", { exact: true })
    .selectOption("actualbudget/demo-theme");
  await expect(
    page.getByRole("status").filter({ hasText: "Custom theme saved" }),
  ).toBeVisible();
  const catalogDownload = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Export original theme data", exact: true })
    .click();
  const catalogExport = JSON.parse(
    await readFile(await (await catalogDownload).path(), "utf8"),
  );
  expect(
    catalogExport.original.catalog_sources[
      "https://github.com/actualbudget/demo-theme"
    ].source,
  ).toContain("url(fonts/fixture.woff2)");
  expect(catalogExport.light.source).toContain("data:font/woff2;base64,");
  expect(
    await page.getByLabel("Custom Theme CSS", { exact: true }).inputValue(),
  ).toContain("data:font/woff2;base64,");
  // Remove preserved standalone overlay to see the catalog theme.
  await page.getByLabel("Theme slot").selectOption("override");
  await page.getByLabel("Custom Theme CSS", { exact: true }).fill("");
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(240, 232, 221)",
  );
  expect(
    await page.evaluate(async () => {
      const loaded = await document.fonts.load("16px Fixture", "A");
      return (
        loaded.length > 0 && loaded.every((font) => font.status === "loaded")
      );
    }),
  ).toBe(true);
  startupDownloads = 0;
  await page.reload();
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(240, 232, 221)",
  );
  expect(startupDownloads).toBe(0);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Open theme editor" }).click();
  const isolatedContext = await browser.newContext();
  const isolatedPage = await isolatedContext.newPage();
  await isolatedPage.goto(server.url);
  await expect(isolatedPage.locator("html")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  await expect(
    isolatedPage.getByRole("complementary", { name: "Custom theme recovery" }),
  ).toHaveCount(0);
  await isolatedContext.close();
  await page.route(
    "https://raw.githubusercontent.com/Juulz/shades-of-coffee/**",
    (route) => route.fulfill({ status: 503, body: "unavailable" }),
  );
  await page
    .getByLabel("Catalog theme", { exact: true })
    .selectOption("Juulz/shades-of-coffee");
  await expect(
    page.getByRole("alert").filter({ hasText: "Theme download failed" }),
  ).toBeVisible();
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(240, 232, 221)",
  );
  await page
    .getByLabel("Custom Theme CSS", { exact: true })
    .fill("body { background: red; }");
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(
    page.getByRole("alert").filter({ hasText: "could not be applied" }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Custom Theme CSS", { exact: true }),
  ).toHaveValue("body { background: red; }");
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(240, 232, 221)",
  );
  await page.evaluate(() => {
    window.savedTransaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function () {
      throw new DOMException("Full", "QuotaExceededError");
    };
  });
  await page
    .getByLabel("Custom Theme CSS", { exact: true })
    .fill(":root { --color-pageBackground: #ffffff; }");
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Free space or enable storage",
  );
  await expect(page.locator("html")).toHaveCSS(
    "background-color",
    "rgb(240, 232, 221)",
  );
  await page.evaluate(() => {
    IDBDatabase.prototype.transaction = window.savedTransaction;
  });
  console.log(
    "PASS explicit catalog download/embedded font, fetch failure, malformed CSS and storage failure preserve active theme",
  );
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: "html{font-size:200%}" });
  await page.emulateMedia({ reducedMotion: "reduce" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/themes-mobile-large-text.png",
    fullPage: true,
  });
  await page.evaluate(() => scrollTo(0, 0));
  await page.screenshot({
    path: "test-results/themes-mobile-recovery-viewport.png",
  });
  // A failed startup read must not let a stale empty reset erase recovered sources.
  await page.addInitScript(() => {
    const transaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (...args) {
      if (
        this.name === "actualBudget.deviceThemes.v1" &&
        sessionStorage.getItem("failThemeRead") === "yes"
      )
        throw new DOMException("Temporarily unavailable", "UnknownError");
      return transaction.apply(this, args);
    };
  });
  await page.evaluate(() => sessionStorage.setItem("failThemeRead", "yes"));
  await page.reload();
  await expect(
    page
      .getByRole("complementary", { name: "Custom theme recovery" })
      .getByRole("alert"),
  ).toBeVisible();
  await page.evaluate(() => sessionStorage.removeItem("failThemeRead"));
  const startupRecovery = page.getByRole("complementary", {
    name: "Custom theme recovery",
  });
  await startupRecovery
    .getByRole("button", { name: "Use built-in theme" })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Open theme editor" }).click();
  const recoveredDownload = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Export original theme data", exact: true })
    .last()
    .click();
  const recovered = JSON.parse(
    await readFile(await (await recoveredDownload).path(), "utf8"),
  );
  expect(recovered.light.source).toContain("data:font/woff2;base64,");
  expect(
    recovered.original.catalog_sources[
      "https://github.com/actualbudget/demo-theme"
    ].source,
  ).toContain("url(fonts/fixture.woff2)");
  await startupRecovery
    .getByRole("button", { name: "Use built-in theme" })
    .click();
  await expect(startupRecovery).toHaveCount(0);
  await page.reload();
  await expect(startupRecovery).toHaveCount(0);
  console.log(
    "PASS failed startup theme read recovers source before explicit reset retry",
  );
  // Malformed saved configuration remains exportable; reset never destroys its source.
  const malformed = { version: 1, broken: "original bytes retained" };
  await page.evaluate(
    (value) =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("actualBudget.deviceThemes.v1", 1);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("records", "readwrite");
          tx.objectStore("records").put(value, "active");
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      }),
    malformed,
  );
  await page.reload();
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Saved custom themes are malformed" }),
  ).toBeVisible();
  const brokenDownload = page.waitForEvent("download");
  await page
    .getByRole("complementary", { name: "Custom theme recovery" })
    .getByRole("button", { name: "Export original theme data" })
    .click();
  expect(
    JSON.parse(await readFile(await (await brokenDownload).path(), "utf8")),
  ).toEqual(malformed);
  await page
    .getByRole("complementary", { name: "Custom theme recovery" })
    .getByRole("button", { name: "Use built-in theme" })
    .click();
  await expect(
    page.getByRole("complementary", { name: "Custom theme recovery" }),
  ).toHaveCount(0);
  await page.reload();
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Saved custom themes are malformed" }),
  ).toHaveCount(0);
  console.log(
    "PASS malformed IndexedDB source export and durable readable reset",
  );
  // Viewport-sized captures keep large text legible for review.
  await page.addStyleTag({ content: "html{font-size:200%}" });
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Open theme editor" }).click();
  await page
    .getByLabel("Custom Theme CSS", { exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/themes-mobile-editor-viewport.png",
  });
  await page
    .getByRole("button", { name: "Apply custom CSS" })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/themes-mobile-actions-viewport.png",
  });
  expect(errors).toEqual([]);
  await context.close();
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, expect } from "@playwright/test";
import { readFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
const fixtures = fileURLToPath(
  new URL("../../tests/fixtures/", import.meta.url),
);
const artifacts = fileURLToPath(new URL("../test-results/", import.meta.url));
const expected = JSON.parse(
  await readFile(`${fixtures}/import-26.10.0.json`, "utf8"),
);
const errors = [];
let browser;
async function read(path) {
  const response = await fetch(`${api}/api/v1${path}`);
  expect(response.ok).toBe(true);
  return (await response.json()).data;
}
function byId(rows) {
  return [...rows].sort((left, right) => left.id.localeCompare(right.id));
}
async function open(page) {
  await page
    .getByRole("button", { name: "Import Actual budget", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Import Actual budget", exact: true }),
  ).toBeVisible();
}
async function lose(page, pattern) {
  const attempts = [];
  await page.route(pattern, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    try {
      attempts.push({
        key: route.request().headers()["idempotency-key"],
        sha: createHash("sha256")
          .update(route.request().postDataBuffer() ?? "")
          .digest("hex"),
      });
      const url = new URL(route.request().url()),
        headers = { ...route.request().headers(), host: url.host };
      if (url.hostname === "actualbudget.test") url.hostname = "127.0.0.1";
      const response = await route.fetch({ url: url.href, headers });
      expect(response.ok()).toBe(true);
      if (attempts.length === 1) await route.abort("connectionreset");
      else await route.fulfill({ response });
    } catch (error) {
      errors.push(String(error));
      await route.abort("failed").catch(() => {});
    }
  });
  return attempts;
}
try {
  const originalBudgets = byId(await read("/budgets"));
  await mkdir(artifacts, { recursive: true });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
      viewport: { width: 1440, height: 1000 },
    }),
    page = await context.newPage();
  page.setDefaultTimeout(10000);
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(server.url);
  await open(page);
  const staleChooser = await context.newPage();
  staleChooser.on("pageerror", (error) => errors.push(error.message));
  let stalePosts = 0;
  staleChooser.on("request", (request) => {
    if (request.method() === "POST") stalePosts++;
  });
  await staleChooser.goto(server.url);
  await open(staleChooser);
  await expect(staleChooser.getByLabel("Actual ZIP export")).toBeEnabled();
  await page
    .getByLabel("Actual ZIP export")
    .setInputFiles(`${fixtures}/import-26.10.0.zip`);
  const uploads = await lose(page, "**/api/v1/imports/actual");
  await page.evaluate(() => {
    window.originalImportPut = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (...args) {
      if (this.name === "session")
        throw new DOMException("No space", "QuotaExceededError");
      return window.originalImportPut.apply(this, args);
    };
  });
  await page
    .getByRole("button", { name: "Review archive", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("could not be saved");
  expect(uploads).toHaveLength(0);
  await page.evaluate(() => {
    IDBObjectStore.prototype.put = window.originalImportPut;
  });
  await page
    .getByRole("button", { name: "Review archive", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved upload" }),
  ).toBeEnabled();
  expect(byId(await read("/budgets"))).toEqual(originalBudgets);
  const sourceStages = (await read("/imports")).filter(
    (stage) =>
      stage.report.archive_sha256 === expected.inspection.archive_sha256,
  );
  expect(sourceStages).toHaveLength(1);
  const staged = sourceStages[0];
  expect(staged.status).toBe("ready");
  expect(originalBudgets.some((budget) => budget.id === staged.budget_id)).toBe(
    false,
  );
  await staleChooser
    .getByLabel("Actual ZIP export")
    .setInputFiles(`${fixtures}/import-unsupported-rule-26.10.0.zip`);
  await staleChooser
    .getByRole("button", { name: "Review archive", exact: true })
    .click();
  await expect(staleChooser.getByRole("alert")).toContainText("Another tab");
  expect(stalePosts).toBe(0);
  await staleChooser.close();
  await page.reload();
  await open(page);
  await page.getByRole("button", { name: "Retry saved upload" }).click();
  await expect(
    page.getByRole("button", { name: "Activate imported budget" }),
  ).toBeVisible();
  expect(uploads).toHaveLength(2);
  expect(uploads[1]).toEqual(uploads[0]);
  expect(uploads[0].sha).toBe(expected.inspection.archive_sha256);
  await expect(
    page.locator(".balance-row").filter({ hasText: "Spending" }),
  ).toBeVisible();
  await expect(
    page.locator(".balance-row").filter({ hasText: "Reserve" }),
  ).toBeVisible();
  await page.screenshot({
    path: `${artifacts}/import-review-desktop.png`,
    fullPage: true,
  });
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: "html{font-size:200%}" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: `${artifacts}/import-review-mobile-zoom.png`,
    fullPage: true,
  });
  await page.addStyleTag({ content: "html{font-size:100%}" });
  await page.setViewportSize({ width: 1440, height: 1000 });
  const second = await context.newPage();
  second.on("pageerror", (error) => errors.push(error.message));
  await second.goto(server.url);
  await open(second);
  await expect(
    second.getByRole("button", { name: "Review another file" }),
  ).toBeVisible();
  const activations = await lose(page, "**/api/v1/imports/*/activate");
  await page.getByRole("button", { name: "Activate imported budget" }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved activation" }),
  ).toBeEnabled();
  await second.getByRole("button", { name: "Review another file" }).click();
  await expect(second.getByRole("alert")).toContainText("unconfirmed");
  await second.close();
  await page.reload();
  await open(page);
  await page.getByRole("button", { name: "Retry saved activation" }).click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  expect(activations).toHaveLength(2);
  expect(activations[1]).toEqual(activations[0]);
  expect(activations[0].sha).toBe(
    createHash("sha256").update("{}").digest("hex"),
  );
  const activatedBudgets = byId(await read("/budgets"));
  expect(
    activatedBudgets.filter((budget) => budget.id !== staged.budget_id),
  ).toEqual(originalBudgets);
  expect(
    activatedBudgets.filter((budget) => budget.id === staged.budget_id),
  ).toHaveLength(1);
  await page
    .getByRole("button", { name: /^Spending 314\.34/ })
    .first()
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("314.34");
  await expect(
    page.getByText("Corrected expense", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Published deletion", { exact: true }),
  ).toHaveCount(0);
  console.log(
    "PASS saved upload/activation survive lost acknowledgements, reload and stale tabs; quota failure prevents sending; financial register matches oracle",
  );
  await page.unroute("**/api/v1/imports/actual");
  await page.getByRole("button", { name: "All budgets", exact: true }).click();
  await open(page);
  await page
    .getByLabel("Actual ZIP export")
    .setInputFiles(`${fixtures}/import-unsupported-rule-26.10.0.zip`);
  await page
    .getByRole("button", { name: "Review archive", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Conversion required" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Activate imported budget" }),
  ).toBeDisabled();
  const original = await page
    .getByRole("link", { name: "Download original archive" })
    .getAttribute("href");
  expect(
    Buffer.from(await (await fetch(`${api}${original}`)).arrayBuffer()).equals(
      await readFile(`${fixtures}/import-unsupported-rule-26.10.0.zip`),
    ),
  ).toBe(true);
  expect(byId(await read("/budgets"))).toEqual(activatedBudgets);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: `${artifacts}/import-blocked-mobile.png`,
    fullPage: true,
  });
  await page.getByRole("button", { name: "Review another file" }).click();
  await page.getByLabel("Actual ZIP export").setInputFiles({
    name: "corrupt.zip",
    mimeType: "application/zip",
    buffer: Buffer.from("corrupt"),
  });
  await page
    .getByRole("button", { name: "Review archive", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("ZIP");
  await expect(page.getByLabel("Actual ZIP export")).toBeEnabled();
  expect(byId(await read("/budgets"))).toEqual(activatedBudgets);
  console.log(
    "PASS unsupported records retained/downloadable and blocked; corrupt source rejected with recoverable chooser",
  );
  expect(errors).toEqual([]);
  await context.close();
} finally {
  await browser?.close();
  await server.stop();
}

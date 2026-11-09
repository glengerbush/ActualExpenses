import { chromium, expect } from "@playwright/test";
import { randomUUID, createHash } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const response = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(response.ok).toBe(true);
  return (await response.json()).data;
}
try {
  const budget = await call("/budgets", {
    id: randomUUID(),
    name: "Lifecycle household",
  });
  await call(`/budgets/${budget.id}/accounts`, {
    id: randomUUID(),
    name: "Spending",
    opening_balance: 12345,
    opening_date: "2026-01-01",
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
      viewport: { width: 1280, height: 960 },
    }),
    page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.setDefaultTimeout(10000);
  await page.goto(server.url);
  await page
    .locator(".budget-list button")
    .filter({ hasText: "Lifecycle household" })
    .click();
  await page
    .getByRole("button", { name: "Manage budget", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Manage budget", exact: true }),
  ).toBeVisible();
  const renameAttempts = [];
  await page.route(`**/api/v1/budgets/${budget.id}`, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    renameAttempts.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
      epoch: route.request().headers()["x-actual-budget-epoch"],
    });
    const url = new URL(route.request().url()),
      headers = { ...route.request().headers(), host: url.host };
    url.hostname = "127.0.0.1";
    const response = await route.fetch({ url: url.href, headers });
    if (renameAttempts.length === 1) await route.abort("connectionreset");
    else await route.fulfill({ response });
  });
  await page
    .getByLabel("Budget name", { exact: true })
    .fill("Renamed household");
  await page
    .getByRole("button", { name: "Save budget name", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved operation", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await page
    .getByRole("button", { name: "Manage budget", exact: true })
    .click();
  await expect(page.getByLabel("Budget name", { exact: true })).toBeDisabled();
  await page
    .getByRole("button", { name: "Retry saved operation", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save budget name", exact: true }),
  ).toBeEnabled();
  expect(renameAttempts).toHaveLength(2);
  expect(renameAttempts[1]).toEqual(renameAttempts[0]);
  await page.unroute(`**/api/v1/budgets/${budget.id}`);
  await expect
    .poll(async () => (await call(`/budgets/${budget.id}`)).name)
    .toBe("Renamed household");
  let delayed;
  await page.route(`**/api/v1/budgets/${budget.id}`, async (route) => {
    if (
      route.request().method() !== "PATCH" ||
      route.request().headers()["x-actual-cancel-if-uncommitted"]
    )
      return route.continue();
    delayed = {
      method: "PATCH",
      headers: route.request().headers(),
      body: route.request().postData(),
    };
    await route.abort("connectionreset");
  });
  await page
    .getByLabel("Budget name", { exact: true })
    .fill("Must never be applied");
  await page
    .getByRole("button", { name: "Save budget name", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Cancel saved operation", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Cancel saved operation", exact: true })
    .click();
  await expect(
    page.getByText("Operation canceled. No delayed retry can apply it.", {
      exact: true,
    }),
  ).toBeVisible();
  await page.unroute(`**/api/v1/budgets/${budget.id}`);
  const late = await fetch(api + `/api/v1/budgets/${budget.id}`, {
    ...delayed,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": delayed.headers["idempotency-key"],
      "X-Actual-Budget-Epoch": delayed.headers["x-actual-budget-epoch"],
    },
  });
  expect(late.status).toBe(409);
  expect((await late.json()).error.code).toBe("operation_canceled");
  expect((await call(`/budgets/${budget.id}`)).name).toBe("Renamed household");
  await page.getByLabel("Backup label", { exact: true }).fill("Before changes");
  await page
    .getByRole("button", { name: "Create backup", exact: true })
    .click();
  const point = page
    .getByRole("listitem")
    .filter({ hasText: "Before changes" });
  await expect(point).toBeVisible();
  const download = page.waitForEvent("download");
  await point.getByRole("link", { name: "Download" }).click();
  expect((await download).suggestedFilename()).toContain(".zip");
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/budget-management-desktop.png",
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
    path: "test-results/budget-management-mobile.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Delete budget", exact: true })
    .click();
  const confirmationHeading = page.getByRole("heading", {
    name: "Delete this budget?",
    exact: true,
  });
  await expect(confirmationHeading).toBeFocused();
  expect(
    await confirmationHeading.evaluate((element) => {
      const box = element.getBoundingClientRect();
      return box.top >= 0 && box.bottom <= innerHeight;
    }),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/budget-confirmation-mobile.png",
  });
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Delete budget", exact: true }),
  ).toBeFocused();
  await page.addStyleTag({ content: "html{font-size:100%}" });
  await page.setViewportSize({ width: 1280, height: 960 });
  await page.getByLabel("Copy name", { exact: true }).fill("Independent copy");
  await page
    .getByRole("button", { name: "Duplicate budget", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call("/budgets")).filter((b) => b.name === "Independent copy")
          .length,
    )
    .toBe(1);
  const copy = (await call("/budgets")).find(
    (b) => b.name === "Independent copy",
  );
  expect((await call(`/budgets/${copy.id}/accounts`))[0].balance).toBe(12345);
  await expect(
    page.getByRole("heading", { name: "Manage budget", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Delete budget", exact: true })
    .click();
  await page
    .getByLabel("Confirm budget name", { exact: true })
    .fill("Renamed household");
  await page
    .getByRole("button", { name: "Confirm deletion", exact: true })
    .click();
  await expect(
    page.getByText("This budget is deleted.", { exact: true }),
  ).toBeVisible();
  await point.getByRole("button", { name: "Restore", exact: true }).click();
  await page
    .getByLabel("Confirm budget name", { exact: true })
    .fill("Renamed household");
  await page
    .getByRole("button", { name: "Confirm restore", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(`/budgets/${budget.id}?include_deleted=true`)).generation,
    )
    .toBe(1);
  expect((await call(`/budgets/${budget.id}/accounts`))[0].balance).toBe(12345);
  const backup = (await call(`/budgets/${budget.id}/backups`)).find(
    (p) => p.label === "Before changes",
  );
  const archive = Buffer.from(
    await (
      await fetch(
        api + `/api/v1/budgets/${budget.id}/backups/${backup.id}/download`,
      )
    ).arrayBuffer(),
  );
  await page
    .getByRole("button", { name: "Back to budgets", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Restore backup", exact: true })
    .click();
  const uploads = [];
  await page.route("**/api/v1/restores", async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    uploads.push({
      key: route.request().headers()["idempotency-key"],
      sha: createHash("sha256")
        .update(route.request().postDataBuffer())
        .digest("hex"),
    });
    const url = new URL(route.request().url()),
      headers = { ...route.request().headers(), host: url.host };
    url.hostname = "127.0.0.1";
    const response = await route.fetch({ url: url.href, headers });
    expect(response.ok()).toBe(true);
    if (uploads.length === 1) await route.abort("connectionreset");
    else await route.fulfill({ response });
  });
  await page
    .getByLabel("actualBudget backup ZIP", { exact: true })
    .setInputFiles({
      name: "actualBudget.zip",
      mimeType: "application/zip",
      buffer: archive,
    });
  await page.evaluate(() => {
    window.originalLifecyclePut = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (...args) {
      if (args[1] === "lifecycle-upload")
        throw new DOMException("Full", "QuotaExceededError");
      return window.originalLifecyclePut.apply(this, args);
    };
  });
  await page
    .getByRole("button", { name: "Review backup", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("Free device space");
  expect(uploads).toHaveLength(0);
  await page.evaluate(() => {
    IDBObjectStore.prototype.put = window.originalLifecyclePut;
  });
  await page
    .getByRole("button", { name: "Review backup", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved upload", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await page
    .getByRole("button", { name: "Restore backup", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Retry saved upload", exact: true })
    .click();
  await expect(
    page.getByText("Backup validated. The original archive is retained.", {
      exact: true,
    }),
  ).toBeVisible();
  expect(uploads).toHaveLength(2);
  expect(uploads[1]).toEqual(uploads[0]);
  expect(uploads[0].sha).toBe(
    createHash("sha256").update(archive).digest("hex"),
  );
  await expect(
    page.getByText("actualBudget.zip", { exact: false }),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/native-restore-desktop.png",
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
    path: "test-results/native-restore-mobile.png",
    fullPage: true,
  });
  await page.addStyleTag({ content: "html{font-size:100%}" });
  await page.setViewportSize({ width: 1280, height: 960 });
  const activations = [];
  await page.route("**/api/v1/budgets/*/restores/*/activate", async (route) => {
    activations.push({
      key: route.request().headers()["idempotency-key"],
      epoch: route.request().headers()["x-actual-budget-epoch"],
      body: route.request().postData(),
    });
    const url = new URL(route.request().url()),
      headers = { ...route.request().headers(), host: url.host };
    url.hostname = "127.0.0.1";
    const response = await route.fetch({ url: url.href, headers });
    expect(response.ok()).toBe(true);
    if (activations.length === 1) await route.abort("connectionreset");
    else await route.fulfill({ response });
  });
  await page
    .getByLabel("Confirm budget name", { exact: true })
    .fill("Renamed household");
  await page
    .getByRole("button", { name: "Restore reviewed backup", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved restore", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await page
    .getByRole("button", { name: "Restore backup", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Retry saved restore", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  expect((await call(`/budgets/${budget.id}`)).generation).toBe(2);
  expect(activations).toHaveLength(2);
  expect(activations[1]).toEqual(activations[0]);
  expect((await call(`/budgets/${budget.id}/accounts`))[0].balance).toBe(12345);
  expect(errors).toEqual([]);
  console.log(
    "PASS budget lost-ack rename, cancellation/delayed original, backup/download, duplication, deletion/recovery, native archive UI, plain-HTTP responsive management",
  );
} finally {
  await browser?.close();
  await server.stop();
}

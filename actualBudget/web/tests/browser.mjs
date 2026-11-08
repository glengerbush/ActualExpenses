import { chromium, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { startServer } from "./server.mjs";
const artifacts = resolve("test-results");
await mkdir(artifacts, { recursive: true });
const server = await startServer();
const baseURL = server.url;
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
  const page = await browser.newPage({
    viewport: { width: 1280, height: 900 },
  });
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto(baseURL);
  if (new URL(baseURL).hostname === "actualbudget.test") {
    expect(await page.evaluate(() => isSecureContext)).toBe(false);
    expect(await page.evaluate(() => typeof SharedArrayBuffer)).toBe(
      "undefined",
    );
  }
  await page.getByRole("button", { name: "New budget", exact: true }).click();
  const name = `Browser ${Date.now()}`;
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Budget name")).toBeFocused();
  await page.getByLabel("Budget name").fill(name);
  await page.getByLabel("Budget name").press("Enter");
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await page.getByLabel("Account name").fill("Spending");
  await page
    .getByLabel("Opening balance", { exact: true })
    .fill("90071992547409.92");
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("too large");
  await expect(page.getByLabel("Account name")).toHaveValue("Spending");
  await page.getByLabel("Opening balance", { exact: true }).fill("1234.56");
  await page.getByLabel("Opening date").fill("2026-11-08");
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Spending", exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("account-balance")).toHaveText("1,234.56");
  await page.reload();
  await expect(page.getByTestId("account-balance")).toHaveText("1,234.56");
  console.log(
    "PASS insecure HTTP, keyboard creation, exact amounts, validation retention, reload",
  );

  await page.getByRole("button", { name: "All accounts", exact: true }).click();
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await page.getByLabel("Account name").fill("Credit card");
  await page.getByLabel("Opening balance", { exact: true }).fill("-42.07");
  await page.getByLabel("Account type").selectOption({ label: "Off budget" });
  await page.getByLabel("Notes").fill("Statement opening balance");
  const attempts = [];
  const interceptionErrors = [];
  // Drop the first successful response AFTER the real server commits it.
  await page.route("**/api/v1/budgets/*/accounts", async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    try {
      attempts.push({
        key: route.request().headers()["idempotency-key"],
        body: route.request().postData(),
      });
      const target = new URL(route.request().url());
      const headers = { ...route.request().headers(), host: target.host };
      if (target.hostname === "actualbudget.test")
        target.hostname = "127.0.0.1";
      const response = await route.fetch({ url: target.href, headers });
      if (attempts.length === 1) {
        expect(response.ok()).toBe(true);
        await route.abort("connectionreset");
      } else await route.fulfill({ response });
    } catch (error) {
      interceptionErrors.push(String(error));
      await route.abort("failed").catch(() => {});
    }
  });
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("outcome is unconfirmed");
  await expect(page.getByLabel("Account name")).toHaveValue("Credit card");
  await expect(page.getByLabel("Account name")).toBeDisabled();
  await page.screenshot({
    path: `${artifacts}/uncertain-write.png`,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Retry creation", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Credit card", exact: true }),
  ).toBeVisible();
  expect(interceptionErrors).toEqual([]);
  expect(attempts).toHaveLength(2);
  expect(attempts[0]).toEqual(attempts[1]);
  await expect(page.getByTestId("account-balance")).toHaveText("-42.07");
  await expect(
    page.getByText("Statement opening balance", { exact: true }),
  ).toBeVisible();
  await page.unroute("**/api/v1/budgets/*/accounts");
  const rows = await page.evaluate(async () => {
    const { budgetId } = JSON.parse(
      localStorage.getItem("actualBudget.selection"),
    );
    return (await (await fetch(`/api/v1/budgets/${budgetId}/accounts`)).json())
      .data;
  });
  expect(rows.filter((account) => account.name === "Credit card")).toHaveLength(
    1,
  );
  console.log(
    "PASS lost committed response retries same payload/key exactly once",
  );

  await page.getByRole("button", { name: "All accounts", exact: true }).click();
  await page.screenshot({
    path: `${artifacts}/accounts-desktop.png`,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: `${artifacts}/accounts-mobile.png`,
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await page.screenshot({
    path: `${artifacts}/create-account-mobile.png`,
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.setViewportSize({ width: 320, height: 900 });
  await page.addStyleTag({ content: "html { font-size: 200%; }" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByLabel("Account name").focus();
  await page.keyboard.press("Tab");
  await expect(
    page.getByLabel("Opening balance", { exact: true }),
  ).toBeFocused();
  await page.screenshot({
    path: `${artifacts}/create-account-zoom.png`,
    fullPage: true,
  });
  await page.addStyleTag({ content: "html { font-size: 100%; }" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "All budgets", exact: true }).click();
  await page.getByRole("button", { name: new RegExp(`${name}.*Open`) }).click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  console.log(
    "PASS mobile layout, inline form cancellation, budget reselection",
  );

  await page.route(
    "**/api/v1/info",
    (route) => route.abort("connectionrefused"),
    { times: 1 },
  );
  await page.reload();
  await expect(page.getByRole("alert")).toContainText(
    "Cannot reach the server",
  );
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "All budgets", exact: true }).click();
  await page.route(
    "**/api/v1/budgets/*/accounts",
    (route) => route.abort("connectionrefused"),
    { times: 1 },
  );
  await page.getByRole("button", { name: new RegExp(`${name}.*Open`) }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Cannot reach the server",
  );
  await page.getByRole("button", { name: "All budgets", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Your budgets", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: new RegExp(`${name}.*Open`) }).click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  expect(pageErrors).toEqual([]);
  console.log(
    "PASS initial connection error recovery; no uncaught browser errors",
  );
} finally {
  await browser?.close();
  await server.stop();
}

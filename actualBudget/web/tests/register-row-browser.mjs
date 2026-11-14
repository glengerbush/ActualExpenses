import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";

const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
async function call(path, body, method = "POST", headers = {}) {
  const response = await fetch(`${api}/api/v1${path}`, {
    method: body ? method : "GET",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body: body && JSON.stringify(body),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return (await response.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID();
  const root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Row editing" });
  await call(`${root}/accounts`, {
    id: account,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const rows = [];
  for (const [amount, notes] of [
    [-1234, "Groceries"],
    [5000, "Refund"],
    [0, "Adjustment"],
  ]) {
    const id = randomUUID();
    await call(`${root}/transactions`, {
      id,
      account_id: account,
      date: "2026-01-02",
      amount,
      notes,
      cleared: true,
    });
    rows.push(id);
  }
  const settings = await call(`${root}/register-settings`);
  // Real persisted legacy columns must normalize on load, including duplicates.
  await call(
    `${root}/register-settings`,
    {
      revision: settings.revision,
      settings: {
        views: {
          all: {
            query: {
              search: "",
              sort: "date",
              direction: "desc",
              hide_cleared: false,
              hide_reconciled: false,
              filter: { op: "and", conditions: [] },
            },
            columns: [
              "date",
              "payee",
              "notes",
              "category",
              "payment",
              "deposit",
              "amount",
              "status",
            ],
            as_of: "",
          },
        },
        saved_filters: [],
      },
    },
    "PATCH",
    { "X-Actual-Budget-Epoch": settings.epoch },
  );
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(server.url);
  const table = page.locator(".register-table");
  const expense = table.locator(`[data-transaction-id="${rows[0]}"]`);
  const income = table.locator(`[data-transaction-id="${rows[1]}"]`);
  const zero = table.locator(`[data-transaction-id="${rows[2]}"]`);
  await expect(
    table.getByRole("columnheader", { name: "Amount", exact: true }),
  ).toHaveCount(1);
  await expect(
    table.getByRole("columnheader", { name: /Payment|Deposit/ }),
  ).toHaveCount(0);
  await expect(expense.locator(".amount")).toHaveText("-12.34");
  await expect(income.locator(".amount")).toHaveText("+50.00");
  await expect(zero.locator(".amount")).toHaveText("0.00");
  for (const row of [expense, income, zero]) {
    await expect(row.getByRole("button")).toHaveCount(1);
    await expect(row.getByRole("button", { name: /^Delete / })).toBeVisible();
  }
  await expect(page.getByLabel("Sort by", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Order", { exact: true })).toHaveCount(0);
  const amountHeader = table.getByRole("columnheader", {
    name: "Amount",
    exact: true,
  });
  await amountHeader.getByRole("button").click();
  await expect(amountHeader).toHaveAttribute("aria-sort", "ascending");
  await expect(
    table.locator("tbody tr[data-transaction-id]").first(),
  ).toHaveAttribute("data-transaction-id", rows[0]);
  await amountHeader.getByRole("button").click();
  await expect(amountHeader).toHaveAttribute("aria-sort", "descending");
  await expect(
    table.locator("tbody tr[data-transaction-id]").first(),
  ).toHaveAttribute("data-transaction-id", rows[1]);
  const changes = page.waitForResponse((r) =>
    r.url().includes(`${root}/changes`),
  );
  const history = page.waitForResponse((r) =>
    r.url().endsWith(`${root}/history`),
  );
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  expect((await changes).ok()).toBe(true);
  expect((await history).ok()).toBe(true);
  await expect(
    page.getByRole("button", { name: /Refresh transactions|Refresh history/ }),
  ).toHaveCount(0);
  await mkdir("test-results/row-ui", { recursive: true });
  await page.screenshot({ path: "test-results/row-ui/toolbar-desktop.png" });
  await expense.getByRole("checkbox").check();
  await expect(page.locator(".inline-editor")).toHaveCount(0);
  await expense.getByRole("checkbox").uncheck();
  await expense.getByRole("button", { name: /^Delete / }).click();
  await expect(page.locator(".inline-editor")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Keep transaction", exact: true })
    .click();
  await expense.focus();
  await page.keyboard.press("Enter");
  await expect(expense.getByLabel("Amount", { exact: true })).toHaveValue(
    "12.34",
  );
  await expect(expense.getByLabel("Date", { exact: true })).toBeFocused();
  await expense.getByLabel("Amount", { exact: true }).fill("15.00");
  await expect(expense.getByLabel("Notes", { exact: true })).toHaveCount(0);
  await expense
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(expense.locator(".amount")).toHaveText("-15.00");
  expect((await call(`${root}/transactions/${rows[0]}`)).amount).toBe(-1500);
  await expect(expense).toBeFocused();
  await page.locator('summary[aria-label="Filters and columns"]').click();
  await page.getByRole("button", { name: "Columns", exact: true }).click();
  await expect(
    page.getByLabel("Show Amount column", { exact: true }),
  ).toBeChecked();
  await expect(
    page.getByLabel("Show Amount column", { exact: true }),
  ).toBeDisabled();
  await expect(page.getByLabel(/Show (Payment|Deposit) column/)).toHaveCount(0);
  await page.locator('summary[aria-label="Filters and columns"]').click();
  await mkdir("test-results/row-ui", { recursive: true });
  for (const theme of ["light", "dark"]) {
    await page.evaluate(
      (theme) => (document.documentElement.dataset.theme = theme),
      theme,
    );
    const positive = await income
      .locator(".amount")
      .evaluate((el) => getComputedStyle(el).color);
    const neutral = await zero
      .locator(".amount")
      .evaluate((el) => getComputedStyle(el).color);
    expect(positive).not.toBe(neutral);
    await table.screenshot({ path: `test-results/row-ui/${theme}.png` });
  }
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await income.click();
  await expect(
    income.getByRole("radio", { name: "Income", exact: true }),
  ).toBeChecked();
  await income
    .locator(".inline-editor")
    .screenshot({ path: "test-results/row-ui/desktop-editor.png" });
  await income.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(amountHeader.getByRole("button")).toBeVisible();
  await page.screenshot({ path: "test-results/row-ui/toolbar-mobile.png" });
  await income.click();
  await expect(income.getByLabel("Amount", { exact: true })).toHaveValue(
    "50.00",
  );
  await income
    .locator(".inline-editor")
    .screenshot({ path: "test-results/row-ui/mobile-editor.png" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await income.getByRole("button", { name: "Cancel", exact: true }).click();
  await table.screenshot({ path: "test-results/row-ui/mobile.png" });
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(
    page.getByRole("radio", { name: "Expense", exact: true }),
  ).toBeChecked();
  await page.getByRole("radio", { name: "Expense", exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(
    page.getByRole("radio", { name: "Income", exact: true }),
  ).toBeChecked();
  await page.getByLabel("Amount", { exact: true }).fill("2.00");

  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    table.locator("tr").filter({ hasText: "+2.00" }).locator(".amount"),
  ).toHaveText("+2.00");
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(
    page.getByRole("radio", { name: "Expense", exact: true }),
  ).toBeChecked();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(errors).toEqual([]);
  console.log(
    "PASS signed amounts, legacy columns, row-only editing, selection/delete isolation, keyboard, persistence, light/dark and mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
}

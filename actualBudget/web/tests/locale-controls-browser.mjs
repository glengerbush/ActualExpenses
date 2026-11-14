// Shipped translations must reach existing controls without translating wire values.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const budget = randomUUID(),
  account = randomUUID();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function create(path, body) {
  const response = await fetch(api + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(response.status).toBe(201);
}
let browser;
try {
  await create("/budgets", {
    id: budget,
    name: "Locale controls",
    budget_type: "tracking",
  });
  await create(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Spending",
    opening_balance: 100000,
    opening_date: "2026-11-01",
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
    viewport: { width: 1280, height: 900 },
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(server.url);
  await page.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget, account },
  );
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.locator("#language").selectOption("de");
  await expect(
    page.getByRole("heading", { name: "Einstellungen", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  // Known German strings from the unchanged pinned catalog, not native implementation output.
  await expect(
    page.getByRole("button", { name: "Rückgängig", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Wiederholen", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Control+k");
  await expect(
    page
      .getByRole("dialog")
      .getByRole("button", { name: "Regeln", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog")
      .getByRole("button", { name: "Transactions", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page
      .locator(".register-table")
      .getByRole("button", { name: "Datum", exact: true }),
  ).toBeVisible();
  const amountHeader = page.getByRole("columnheader", {
    name: "Betrag",
    exact: true,
  });
  await amountHeader.getByRole("button").click();
  await expect(amountHeader).toHaveAttribute("aria-sort", "ascending");
  await expect(page.getByTestId("register-total")).toHaveText("1,000.00");
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await expect(page.getByRole("button", { name: /^Rückgängig/ })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Previous month", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Next month", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await page.getByRole("button", { name: "Regeln", exact: true }).click();
  await page
    .getByRole("button", { name: "Regel erstellen", exact: true })
    .click();
  const stage = page.getByRole("combobox", { name: "Stufe", exact: true });
  await expect(
    stage.getByRole("option", { name: "Vorher", exact: true }),
  ).toHaveJSProperty("value", "pre");
  await stage.selectOption({ label: "Vorher" });
  await expect(stage).toHaveValue("pre");
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click();
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  // Inline recurrence replaces the standalone schedule view (covered by recurrence-editor-browser).
  await page.setViewportSize({ width: 320, height: 800 });
  await page
    .getByRole("button", { name: "Einstellungen", exact: true })
    .click();
  await page.locator("#theme").selectOption("dark");
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Rückgängig", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await mkdir("test-results/locale-controls", { recursive: true });
  await page.screenshot({
    path: "test-results/locale-controls/german-mobile.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Einstellungen", exact: true })
    .click();
  await page.locator("#language").selectOption("en");
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Undo", exact: true }),
  ).toBeVisible();
  const stored = await (
    await fetch(`${api}/budgets/${budget}/transactions?account_id=${account}`)
  ).json();
  expect(stored.balances.total).toBe(100000);
  expect(stored.data).toHaveLength(1);
  expect(errors).toEqual([]);
  console.log(
    "PASS translated register/planning/rule controls, unchanged enums/names/finances, mobile and English switch",
  );
} finally {
  await browser?.close();
  await server.stop();
}

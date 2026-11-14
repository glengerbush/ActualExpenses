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
  const page = await browser.newPage({
    viewport: { width: 1280, height: 900 },
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(server.url);
  const api = (path, body) =>
    page.evaluate(
      async ({ path, body, key }) => {
        const response = await fetch(`/api/v1${path}`, {
          method: body ? "POST" : "GET",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
          },
          body: body && JSON.stringify(body),
        });
        if (!response.ok) throw Error(await response.text());
        return (await response.json()).data;
      },
      { path, body, key: randomUUID() },
    );
  await page.getByRole("button", { name: "New budget", exact: true }).click();
  await page.getByLabel("Budget name", { exact: true }).fill("Manual history");
  await page
    .getByRole("button", { name: "Create budget", exact: true })
    .click();
  const budget = (await api("/budgets")).find(
    (row) => row.name === "Manual history",
  ).id;
  await expect(
    page.getByRole("heading", { name: "Transaction history", exact: true }),
  ).toBeVisible();
  const nav = page.getByRole("complementary", { name: "Budget navigation" });
  await expect(
    page.getByRole("button", {
      name: /Add account|Organize accounts|All accounts/,
    }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  expect(await api(`/budgets/${budget}/accounts`)).toHaveLength(1);
  await nav.getByRole("button", { name: "Budget", exact: true }).click();
  await expect(
    nav.getByRole("button", { name: "Categories", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Add group", exact: true }).first(),
  ).toBeEnabled();
  await nav
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(page.getByLabel("Account", { exact: true })).toHaveCount(0);
  await page.getByLabel("Amount", { exact: true }).fill("12.34");
  await page.getByLabel("Date", { exact: true }).fill("2026-11-01");
  await expect(page.getByLabel("Notes", { exact: true })).toHaveCount(0);
  await nav.getByRole("button", { name: "Budget", exact: true }).click();
  await nav
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("12.34");
  await expect(page.getByLabel("Date", { exact: true })).toHaveValue(
    "11/01/2026",
  );
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".transaction-form")).toHaveCount(0);
  const transactions = await api(`/budgets/${budget}/transactions`);
  expect(transactions).toHaveLength(1);
  expect(transactions[0].amount).toBe(-1234);
  const legacy = randomUUID();
  const importedId = randomUUID();
  await api(`/budgets/${budget}/accounts`, {
    id: legacy,
    name: "Historical source",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await api(`/budgets/${budget}/transactions`, {
    id: importedId,
    account_id: legacy,
    amount: -500,
    date: "2026-01-01",
    notes: "Older imported expense",
  });
  await page.reload();
  await expect(
    page.locator(`[data-transaction-id="${importedId}"]`),
  ).toContainText("5.00");
  await expect(
    page.locator(`[data-transaction-id="${transactions[0].id}"]`),
  ).toContainText("12.34");
  // Retired routes cannot create fresh work, even when addressed directly.
  for (const view of ["account", "organize"]) {
    await page.goto(`${server.url}/?budget=${budget}&view=${view}`);
    await expect(
      page.getByRole("heading", { name: "Transaction history", exact: true }),
    ).toBeVisible();
    expect(
      await page.evaluate(
        (budget) => [
          localStorage.getItem(`actualBudget.creation.v1.account.${budget}`),
          localStorage.getItem(`actualBudget.accounts.v1.${budget}`),
        ],
        budget,
      ),
    ).toEqual([null, null]);
  }
  // Invalid saved work also remains reachable without replacing its bytes.
  await page.evaluate(
    (budget) =>
      localStorage.setItem(`actualBudget.creation.v1.account.${budget}`, "{}"),
    budget,
  );
  await page.goto(`${server.url}/?budget=${budget}&view=account`);
  await expect(
    page.getByText(/saved creation draft could not be validated/).first(),
  ).toBeVisible();
  expect(
    await page.evaluate(
      (budget) =>
        localStorage.getItem(`actualBudget.creation.v1.account.${budget}`),
      budget,
    ),
  ).toBe("{}");
  await page.evaluate(
    (budget) =>
      localStorage.removeItem(`actualBudget.creation.v1.account.${budget}`),
    budget,
  );
  // A saved account request still reopens and retries with its original identity.
  const pendingAccount = randomUUID();
  const epoch = (await api(`/budgets/${budget}/checkpoint`)).epoch;
  const pendingBody = {
    id: pendingAccount,
    name: "Recovered destination",
    opening_balance: 0,
    opening_date: "2026-11-01",
    off_budget: false,
    notes: "",
  };
  await page.evaluate(
    ({ budget, pendingAccount, pendingBody, epoch }) => {
      localStorage.setItem(
        `actualBudget.creation.v1.account.${budget}`,
        JSON.stringify({
          version: 1,
          origin: location.origin,
          draft_id: pendingAccount,
          kind: "account",
          budget_id: budget,
          budget_epoch: epoch,
          fields: {
            name: pendingBody.name,
            balance: "0.00",
            date: pendingBody.opening_date,
            off_budget: false,
            notes: "",
            number_format: "comma-dot",
            decimal_places: 2,
            date_format: "yyyy-MM-dd",
          },
          pending: { key: pendingAccount, body: pendingBody },
        }),
      );
    },
    { budget, pendingAccount, pendingBody, epoch },
  );
  await page.goto(`${server.url}/?budget=${budget}&view=account`);
  await expect(
    page.getByRole("heading", { name: "Add account", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Retry creation", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transaction history", exact: true }),
  ).toBeVisible();
  expect(
    (await api(`/budgets/${budget}/accounts`)).filter(
      (row) => row.id === pendingAccount,
    ),
  ).toHaveLength(1);
  expect(
    await page.evaluate(
      (budget) =>
        localStorage.getItem(`actualBudget.creation.v1.account.${budget}`),
      budget,
    ),
  ).toBeNull();
  await expect(
    page.getByRole("columnheader", { name: "Account", exact: true }),
  ).toHaveCount(0);
  await mkdir("test-results/unified-history", { recursive: true });
  await page.screenshot({
    path: "test-results/unified-history/desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  await page.screenshot({
    path: "test-results/unified-history/mobile.png",
    fullPage: true,
  });
  // Simulate a cache saved before automatic destinations existed.
  await page.evaluate(
    (budget) =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("actualBudget.offline.v1", 1);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("cache", "readwrite");
          const store = tx.objectStore("cache");
          const read = store.get(`budget:${budget}`);
          read.onsuccess = () => {
            const value = read.result;
            delete value.ledgerAccountId;
            store.put(value, `budget:${budget}`);
          };
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
        open.onerror = () => reject(open.error);
      }),
    budget,
  );
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  await page.route("**/api/v1/**", (route) => route.abort());
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(page.getByLabel("Amount", { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
  console.log(
    "PASS automatic ledger, no account UI, direct navigation, durable draft, combined history, mobile layout",
  );
} finally {
  await browser?.close();
  await server.stop();
}

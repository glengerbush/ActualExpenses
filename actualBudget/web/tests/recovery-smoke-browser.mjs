// External-compatible qualification: ALWAYS use startServer() without a copied
// data override. With ACTUALBUDGET_TEST_URL, every assertion runs on that server.
// Setup and mutations use only the versioned public API; no DB access/injection.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
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
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  const api = async (path, body) =>
    page.evaluate(
      async ({ path, body, key }) => {
        const response = await fetch("/api/v1" + path, {
          method: body === undefined ? "GET" : "POST",
          headers:
            body === undefined
              ? {}
              : { "Content-Type": "application/json", "Idempotency-Key": key },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        if (!response.ok) throw Error(await response.text());
        return response.json();
      },
      { path, body, key: randomUUID() },
    );
  const budget = randomUUID(),
    account = randomUUID(),
    name = `Recovery qualification ${randomUUID()}`;
  await api("/budgets", { id: budget, name, budget_type: "tracking" });
  await api(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Qualification spending",
    opening_balance: 1000,
    opening_date: "2026-11-09",
    off_budget: false,
  });
  await page.reload();
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Recovery", exact: true }).click();
  await page
    .getByRole("button", { name: "Inspect budget", exact: true })
    .click();
  const healthy = (await api(`/budgets/${budget}/recovery`)).data;
  expect(healthy.can_apply).toBe(false);
  expect(healthy.issues).toEqual([]);
  expect(healthy.repairs).toEqual([]);
  expect(healthy.accounts).toHaveLength(1);
  expect(healthy.accounts[0].before).toEqual({
    total: 1000,
    cleared: 1000,
    uncleared: 0,
  });
  expect(healthy.accounts[0].after).toEqual(healthy.accounts[0].before);
  await expect(page.getByTestId("recovery-account")).toContainText("10.00");
  await expect(
    page.getByRole("button", { name: "Apply reviewed repair", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Reconnect and refresh", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transaction history", exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("register-total")).toHaveText("10.00");
  await expect(
    page.getByRole("button", { name: "Refresh", exact: true }),
  ).toBeEnabled();
  // API outage only: static code stays reachable on plain HTTP across restart.
  await context.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1.23");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByRole("row", { name: /Edit -1.23/ })).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Recovery", exact: true }).click();
  await page
    .getByRole("button", { name: "Reset downloaded data", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm cache reset", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Recovery", exact: true }).click();
  await page
    .getByRole("button", { name: "Troubleshoot saved changes", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText("1.23");
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Recovery", exact: true }).click();
  await page
    .getByRole("button", { name: "Troubleshoot saved changes", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText("1.23");
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await context.unroute("**/api/v1/**");
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
  const transactions = (await api(`/budgets/${budget}/transactions`)).data;
  expect(transactions.filter((row) => row.amount === -123)).toHaveLength(1);
  await page.getByRole("button", { name: /^(All budgets|Budgets)$/ }).click();
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Recovery", exact: true }).click();
  await page
    .getByRole("button", { name: "Reset downloaded data", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm cache reset", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transaction history", exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("register-total")).toHaveText("8.77");
  expect(errors).toEqual([]);
  console.log(
    `PASS recovery qualification on ${server.url}: healthy preview, reconnect, cache reset and restarted outbox single delivery`,
  );
} finally {
  await browser?.close();
  await server.stop();
}

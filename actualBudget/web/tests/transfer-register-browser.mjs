import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
let browser;
async function call(path, body, method = "POST") {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const result = await response.json();
  expect(response.ok, JSON.stringify(result)).toBe(true);
  return result;
}
try {
  const budget = randomUUID(),
    cash = randomUUID(),
    card = randomUUID(),
    ordinary = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Transfer register" });
  for (const [id, name] of [
    [cash, "Checking"],
    [card, "Credit card"],
  ])
    await call(root + "/accounts", {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
  await call(root + "/payees", { id: ordinary, name: "Bakery" });
  await call(root + "/transactions", {
    id: randomUUID(),
    account_id: cash,
    amount: -400,
    date: "2026-01-01",
    payee_id: ordinary,
    notes: "Ordinary purchase",
  });
  const transfer = randomUUID();
  await call(root + "/transfers", {
    id: transfer,
    counterpart_id: randomUUID(),
    account_id: cash,
    to_account_id: card,
    amount: -600,
    date: "2026-01-01",
    notes: "Card payment",
  });
  const payees = (await call(root + "/payees?include_transfers=true")).data;
  const transferPayee = payees.find((p) => p.transfer_account_id === card);
  expect(transferPayee).toBeTruthy();
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
      viewport: { width: 1280, height: 960 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page
    .locator(".budget-list button")
    .filter({ hasText: "Transfer register" })
    .click();
  await page.getByRole("button", { name: "Checking", exact: true }).click();
  await page.getByText("Filters and columns", { exact: true }).click();
  await page
    .getByRole("button", { name: "Add condition", exact: true })
    .click();
  await page.getByLabel(/^Field 1/).selectOption("payee");
  const value = page.getByLabel(/^Value\s+1/);
  await expect(
    value.locator(`option[value="${transferPayee.id}"]`),
  ).toHaveCount(1);
  await value.selectOption(transferPayee.id);
  await page
    .getByRole("button", { name: "Apply filters and columns", exact: true })
    .click();
  await expect(page.getByText("Card payment", { exact: true })).toBeVisible();
  await expect(
    page.getByText("Ordinary purchase", { exact: true }),
  ).toHaveCount(0);
  await mkdir("test-results", { recursive: true });
  for (const [name, viewport] of [
    ["desktop", { width: 1280, height: 960 }],
    ["mobile", { width: 390, height: 844 }],
  ]) {
    await page.setViewportSize(viewport);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/transfer-register-${name}.png`,
      fullPage: true,
    });
  }
  await call(root + "/accounts/" + card, { name: "Renamed card" }, "PATCH");
  await call(root + "/payees/" + ordinary, { name: "Renamed bakery" }, "PATCH");
  await page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await expect(value.locator(`option[value="${transferPayee.id}"]`)).toHaveText(
    "Renamed card",
  );
  await expect(value.locator(`option[value="${ordinary}"]`)).toHaveText(
    "Renamed bakery",
  );
  expect(
    (await call(root + "/payees")).data.some((p) => p.transfer_account_id),
  ).toBe(false);
  expect(errors).toEqual([]);
  console.log(
    "PASS transfer identities in register filters; ordinary expense references remain limited",
  );
} finally {
  await browser?.close();
  await server.stop();
}

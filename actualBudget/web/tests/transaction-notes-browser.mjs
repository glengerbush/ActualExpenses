import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const url = server.url.replace("actualbudget.test", "127.0.0.1");
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const response = await fetch(url + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return (await response.json()).data;
}
let browser;
try {
  const budget = (await call("/budget")).id;
  const root = `/budgets/${budget}`;
  const account = randomUUID();
  await call(root + "/accounts", {
    id: account,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const id = randomUUID();
  const notes = "Receipt for café ☕\nShared groceries <not HTML>";
  await call(root + "/transactions", {
    id,
    account_id: account,
    amount: -250,
    date: "2026-01-02",
    notes,
  });
  expect(
    (await call(root + `/transactions/${id}`, { amount: -350 }, "PATCH")).notes,
  ).toBe(notes);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
    args: ["--no-sandbox"],
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  await page.goto(url);
  const row = page.locator(`[data-transaction-id="${id}"]`);
  await expect(row).toContainText("Shared groceries <not HTML>");
  await row.click();
  const field = page.getByLabel("Notes (optional)", { exact: true });
  await expect(field).toHaveValue(notes);
  await field.fill("Edited receipt\nSecond line");
  await page.reload();
  await expect(field).toHaveValue("Edited receipt\nSecond line");
  await mkdir("test-results/transaction-notes", { recursive: true });
  await page.screenshot({ path: "test-results/transaction-notes/desktop.png" });
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".transaction-form")).toHaveCount(0);
  expect((await call(root + `/transactions/${id}`)).notes).toBe(
    "Edited receipt\nSecond line",
  );
  await row.click();
  await field.fill("");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".transaction-form")).toHaveCount(0);
  expect((await call(root + `/transactions/${id}`)).notes).toBe("");
  const mobile = await browser.newContext({
    userAgent:
      "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148 Safari/604.1",
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  await mobile.addInitScript(() =>
    Object.defineProperty(navigator, "standalone", { value: true }),
  );
  const phone = await mobile.newPage();
  await phone.goto(url);
  await expect(
    phone.getByRole("button", { name: "+ Add expense", exact: true }),
  ).toBeEnabled();
  await mobile.setOffline(true);
  await phone
    .getByRole("button", { name: "+ Add expense", exact: true })
    .click();
  await phone.getByLabel("Amount", { exact: true }).fill("4.56");
  await phone
    .getByLabel("Notes (optional)", { exact: true })
    .fill("Offline receipt ☕");
  await phone.screenshot({
    path: "test-results/transaction-notes/iphone.png",
    fullPage: true,
  });
  await phone
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(phone.locator(".transaction-form")).toHaveCount(0);
  await expect(
    phone.getByText("Offline receipt ☕", { exact: true }),
  ).toBeVisible();
  await mobile.setOffline(false);
  await phone
    .getByRole("button", { name: "Refresh expenses", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/transactions")).find((row) => row.amount === -456)
          ?.notes,
    )
    .toBe("Offline receipt ☕");
  await mobile.close();
  console.log(
    "PASS transaction notes: create, preserve on unrelated edit, multiline/Unicode, recover draft, edit, clear, iPhone offline delivery",
  );
} finally {
  await browser?.close();
  await server.stop();
}

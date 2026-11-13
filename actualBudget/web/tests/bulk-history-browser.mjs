import { transactionAction } from "./register-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = "POST", headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: r.status, ...(await r.json()) };
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    ids = [randomUUID(), randomUUID(), randomUUID()];
  expect(
    (await call("/budgets", { id: budget, name: "Bulk history" })).status,
  ).toBe(201);
  await call(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Spending",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (let i = 0; i < ids.length; i++)
    await call(`/budgets/${budget}/transactions`, {
      id: ids[i],
      account_id: account,
      amount: [-1234, -5678, -90][i],
      date: `2026-01-0${i + 2}`,
      notes: ["Groceries", "Rent", "Coffee"][i],
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
      viewport: { width: 1440, height: 1000 },
    }),
    page = await context.newPage(),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
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
  await expect(page.locator(".register-table tbody tr")).toHaveCount(3);
  await expect(
    page.getByRole("button", { name: "Refresh", exact: true }),
  ).toBeEnabled();
  const boxes = page.locator(".register-table tbody input[type=checkbox]");
  await boxes.first().focus();
  await page.keyboard.press("Space");
  await boxes.nth(1).click({ modifiers: ["Shift"] });
  await expect(page.getByText("2 selected", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { name: "Edit selected", exact: true })
    .click();
  await page.getByLabel("Field", { exact: true }).selectOption("notes");
  await page.getByLabel("New value", { exact: true }).fill("Reviewed together");
  await page
    .getByRole("button", { name: "Apply to 2 entries", exact: true })
    .click();
  await expect(page.locator(".bulk-actions")).toContainText(
    "2 entries affected",
  );
  expect(
    (await call(`/budgets/${budget}/transactions`)).data.filter(
      (r) => r.notes === "Reviewed together",
    ),
  ).toHaveLength(2);
  await page.locator("h2#register-title").click();
  await page.keyboard.press("Control+z");
  await expect(
    page.getByRole("button", { name: "Redo", exact: true }),
  ).toBeEnabled();
  expect(
    (await call(`/budgets/${budget}/transactions`)).data.filter(
      (r) => r.notes === "Reviewed together",
    ),
  ).toHaveLength(0);
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Redo", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect(page.locator(".bulk-actions")).toContainText("entries affected");
  // A committed response can vanish. Reload and exact retry must not duplicate twice.
  let captured;
  await page.route("**/transactions/bulk", async (route) => {
    captured = {
      body: route.request().postData(),
      headers: route.request().headers(),
    };
    const { origin, ...headers } = captured.headers;
    const response = await fetch(
      route.request().url().replace("actualbudget.test", "127.0.0.1"),
      { method: "POST", headers, body: captured.body },
    );
    expect(response.status).toBe(200);
    await route.abort("failed");
  });
  await page.getByLabel("Select all loaded entries").check();
  await page
    .getByRole("button", { name: "Duplicate selected", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved bulk action", exact: true }),
  ).toBeEnabled();
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(6);
  await page.unroute("**/transactions/bulk");
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved bulk action", exact: true }),
  ).toBeEnabled();
  let retried;
  page.on("request", (r) => {
    if (r.url().endsWith("/transactions/bulk"))
      retried = { body: r.postData(), headers: r.headers() };
  });
  await page
    .getByRole("button", { name: "Retry saved bulk action", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved bulk action", exact: true }),
  ).toHaveCount(0);
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(6);
  expect(retried.body).toBe(captured.body);
  for (const header of [
    "idempotency-key",
    "x-actual-edit-clock",
    "x-actual-budget-epoch",
    "x-actual-history-owner",
  ])
    expect(retried.headers[header]).toBe(captured.headers[header]);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.locator(".register-table tbody tr")).toHaveCount(3);
  // Durable quota failure blocks even a retry/cancel network send.
  await page.route("**/transactions/bulk", (route) => route.abort("failed"));
  await page.getByLabel("Select all loaded entries").check();
  await page
    .getByRole("button", { name: "Delete selected", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm bulk delete", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved bulk action", exact: true }),
  ).toBeEnabled();
  await page.unroute("**/transactions/bulk");
  let sends = 0;
  page.on("request", (r) => {
    if (r.url().endsWith("/transactions/bulk")) sends++;
  });
  await page.evaluate(() => {
    window.originalBulkPut = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (...args) {
      if (this.name === "intents")
        throw new DOMException("Quota full", "QuotaExceededError");
      return window.originalBulkPut.apply(this, args);
    };
  });
  await page
    .getByRole("button", { name: "Retry saved bulk action", exact: true })
    .click();
  await expect(page.locator(".bulk-actions .error")).toContainText("Quota");
  expect(sends).toBe(0);
  await page
    .getByRole("button", { name: "Cancel saved action safely", exact: true })
    .click();
  await expect(page.locator(".bulk-actions .error")).toContainText("Quota");
  expect(sends).toBe(0);
  await mkdir("test-results/bulk-history", { recursive: true });
  await page.screenshot({
    path: "test-results/bulk-history/quota-failure.png",
    fullPage: true,
  });
  await page.evaluate(
    () => (IDBObjectStore.prototype.put = window.originalBulkPut),
  );
  await page
    .getByRole("button", { name: "Cancel saved action safely", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved bulk action", exact: true }),
  ).toHaveCount(0);
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(3);
  // Select allocations independently; validate their aggregate once after the batch.
  const childA = randomUUID(),
    childB = randomUUID();
  const splitSnapshot = await call(`/budgets/${budget}/transactions`);
  expect(
    (
      await call(`/budgets/${budget}/transactions/${ids[0]}/splits`, {
        snapshot: splitSnapshot.snapshot,
        children: [
          { id: childA, amount: -600, notes: "Split A1" },
          { id: childB, amount: -634, notes: "Split B2" },
        ],
      })
    ).status,
  ).toBe(200);
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await (
    await transactionAction(page, { name: "Select split entries", exact: true })
  ).click();
  await page
    .locator(`tr[data-transaction-id="${ids[0]}"]`)
    .getByRole("checkbox")
    .uncheck();
  const childBoxes = page.locator(".split-child input[type=checkbox]");
  await childBoxes.nth(0).check();
  await childBoxes.nth(1).check();
  await page
    .getByRole("button", { name: "Edit selected", exact: true })
    .click();
  await page.getByLabel("Field", { exact: true }).selectOption("amount");
  await page.getByLabel("New value", { exact: true }).fill("-6.17");
  await page
    .getByRole("button", { name: "Apply to 2 entries", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit selected", exact: true }),
  ).toHaveCount(0);
  expect(
    (await call(`/budgets/${budget}/transactions/${childA}`)).data.amount,
  ).toBe(-617);
  expect(
    (await call(`/budgets/${budget}/transactions/${childB}`)).data.amount,
  ).toBe(-617);
  await childBoxes.first().check();
  await page
    .getByRole("button", { name: "Edit selected", exact: true })
    .click();
  await page.getByLabel("Field", { exact: true }).selectOption("notes");
  await page.getByLabel("Notes action", { exact: true }).selectOption("append");
  await page.getByLabel("New value", { exact: true }).fill(" extra");
  await page
    .getByRole("button", { name: "Apply to 1 entry", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit selected", exact: true }),
  ).toHaveCount(0);
  expect(
    (await call(`/budgets/${budget}/transactions/${childA}`)).data.notes,
  ).toBe("Split A1 extra");
  await childBoxes.first().check();
  await page
    .getByRole("button", { name: "Edit selected", exact: true })
    .click();
  await page
    .getByLabel("Notes action", { exact: true })
    .selectOption("find_replace");
  await page.getByLabel("Find in notes", { exact: true }).fill("(\\d)");
  await page.getByLabel("Use regular expression", { exact: true }).check();
  await page.getByLabel("New value", { exact: true }).fill("[$1]");
  await page
    .getByRole("button", { name: "Apply to 1 entry", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit selected", exact: true }),
  ).toHaveCount(0);
  expect(
    (await call(`/budgets/${budget}/transactions/${childA}`)).data.notes,
  ).toBe("Split A[1] extra");
  // Full page renders for review: selection, error, narrow, dark and 200% text.
  await mkdir("test-results/bulk-history", { recursive: true });
  await page.getByLabel("Select all loaded entries").check();
  await page
    .getByRole("button", { name: "Edit selected", exact: true })
    .click();
  await page.screenshot({
    path: "test-results/bulk-history/desktop.png",
    fullPage: true,
  });
  await page.evaluate(() => (document.documentElement.dataset.theme = "dark"));
  await page.screenshot({
    path: "test-results/bulk-history/dark.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/bulk-history/mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({
    path: "test-results/bulk-history/text-200.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  // Two fresh tabs initialize the same browser engine atomically, then recover each other's history.
  const raceContext = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  await raceContext.addInitScript(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget, account },
  );
  const left = await raceContext.newPage(),
    right = await raceContext.newPage(),
    owners = [];
  for (const tab of [left, right])
    tab.on("request", (r) => {
      if (r.url().endsWith("/history"))
        owners.push(r.headers()["x-actual-history-owner"]);
    });
  await Promise.all([left.goto(server.url), right.goto(server.url)]);
  await Promise.all([
    expect(
      left.getByRole("button", { name: "Refresh", exact: true }),
    ).toBeEnabled(),
    expect(
      right.getByRole("button", { name: "Refresh", exact: true }),
    ).toBeEnabled(),
  ]);
  await left
    .locator(".register-table tbody input[type=checkbox]")
    .first()
    .check();
  await left
    .getByRole("button", { name: "Edit selected", exact: true })
    .click();
  await left.getByLabel("Field", { exact: true }).selectOption("notes");
  await left.getByLabel("New value", { exact: true }).fill("Across tabs");
  await left
    .getByRole("button", { name: "Apply to 1 entry", exact: true })
    .click();
  await expect(
    left.getByRole("button", { name: "Undo", exact: true }),
  ).toBeEnabled();
  await right.reload();
  await expect(
    right.getByRole("button", { name: "Undo", exact: true }),
  ).toBeEnabled();
  await right.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(
    right.getByRole("button", { name: "Redo", exact: true }),
  ).toBeEnabled();
  expect(owners.length).toBeGreaterThanOrEqual(2);
  expect(new Set(owners).size).toBe(1);
  expect(owners[0]).toMatch(/^[0-9a-f-]{36}$/);
  expect(
    (await call(`/budgets/${budget}/transactions`)).data.some(
      (row) => row.notes === "Across tabs",
    ),
  ).toBe(false);
  await raceContext.close();
  expect(errors).toEqual([]);
  console.log(
    "PASS keyboard bulk/history, reload exact replay, quota-blocked retry/cancel, responsive/dark/200% renders",
  );
} finally {
  await browser?.close();
  await server.stop();
}

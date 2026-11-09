import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const r = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const result = await r.json();
  expect(r.ok, JSON.stringify(result)).toBe(true);
  return result.data;
}
try {
  const b = await call("/budgets", {
      id: randomUUID(),
      name: "Category household",
    }),
    base = `/budgets/${b.id}`;
  const a = await call(base + "/accounts", {
    id: randomUUID(),
    name: "Checking",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const group = await call(base + "/category-groups", {
    id: randomUUID(),
    name: "Daily",
    sort_order: 1,
  });
  const food = await call(base + "/categories", {
    id: randomUUID(),
    name: "Food",
    group_id: group.id,
  });
  const dining = await call(base + "/categories", {
    id: randomUUID(),
    name: "Dining",
    group_id: group.id,
  });
  for (const [amount, notes] of [
    [-1250, "purchase"],
    [250, "refund"],
    [400, "reimbursement"],
  ])
    await call(base + "/transactions", {
      id: randomUUID(),
      account_id: a.id,
      category_id: food.id,
      amount,
      date: "2026-01-01",
      notes,
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
    viewport: { width: 1280, height: 960 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page
    .locator(".budget-list button")
    .filter({ hasText: "Category household" })
    .click();
  await page.getByRole("button", { name: "Categories", exact: true }).click();
  await page.getByRole("button", { name: "Edit Food", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Meals");
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Keep receipts\nReimbursements stay here");
  await page.getByLabel("Hidden", { exact: true }).check();
  const attempts = [];
  await page.route(`**/api/v1${base}/categories/${food.id}`, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    attempts.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
      clock: route.request().headers()["x-actual-edit-clock"],
      epoch: route.request().headers()["x-actual-budget-epoch"],
    });
    const url = new URL(route.request().url()),
      headers = { ...route.request().headers(), host: url.host };
    url.hostname = "127.0.0.1";
    const response = await route.fetch({ url: url.href, headers });
    if (attempts.length === 1) await route.abort("connectionreset");
    else await route.fulfill({ response });
  });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  await page.reload();
  await page.getByRole("button", { name: "Categories", exact: true }).click();
  await page.getByRole("button", { name: "Retry unchanged request" }).click();
  await expect(
    page.getByRole("button", { name: "Edit Meals", exact: true }),
  ).toBeVisible();
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  await page.getByLabel("Show hidden").uncheck();
  await expect(
    page.getByRole("button", { name: "Edit Meals", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Show hidden").check();
  await page.getByRole("button", { name: "Edit Meals", exact: true }).click();
  await page.getByRole("button", { name: "Merge or delete" }).click();
  await page.getByLabel("Reassign transactions to").selectOption(dining.id);
  await page
    .getByRole("button", { name: "Delete and reassign", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit Meals", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Transactions Dining", exact: true })
    .click();
  await expect(page.locator(".category-history article")).toHaveCount(3);
  await expect(page.getByText("refund", { exact: true })).toBeVisible();
  await expect(page.getByText("reimbursement", { exact: true })).toBeVisible();
  await expect(page.getByText("Total:")).toContainText("6.00");
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/categories-desktop.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Back to categories" }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "New group", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Travel");
  await page.getByLabel("Order", { exact: true }).fill("0.5");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Travel", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "New category", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Train");
  await page
    .getByLabel("Group", { exact: true })
    .selectOption({ label: "Travel" });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Edit Train", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Edit Train", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Name", { exact: true })).toBeFocused();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/categories-mobile.png",
    fullPage: true,
  });
  await page.getByText("Retained category notes", { exact: true }).click();
  await expect(
    page.getByText("Keep receipts\nReimbursements stay here", { exact: true }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  console.log(
    "PASS categories browser: durable lost-response retry, merge drilldown, hidden filtering, groups, keyboard, mobile plain HTTP",
  );
} finally {
  await browser?.close();
  await server.stop();
}

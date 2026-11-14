import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
// Seed the released editor journal: full category forms now exist for recovery.
async function editCategory(tab, name, kind = "categories") {
  await tab.evaluate(
    async ({ name, kind }) => {
      const budget = JSON.parse(
        localStorage.getItem("actualBudget.selection"),
      ).budgetId;
      const root = `/api/v1/budgets/${budget}`;
      const rows = (
        await (await fetch(`${root}/${kind}?include_deleted=true`)).json()
      ).data;
      const row = rows.find((row) => row.name === name);
      if (!row) throw Error(`Missing fixture ${name}`);
      const checkpoint = (await (await fetch(root + "/checkpoint")).json())
        .data;
      const key = `actualBudget.categories.v1.${budget}`;
      if (localStorage.getItem(key))
        throw Error("Cannot overwrite saved category work");
      localStorage.setItem(
        key,
        JSON.stringify({
          draft: {
            kind,
            id: row.id,
            new_id: row.id,
            name: row.name,
            group_id: row.group_id ?? "",
            color: row.color,
            is_income: row.is_income,
            sort_order: row.sort_order,
            epoch: checkpoint.epoch,
            original: row,
          },
          pending: null,
          deleting: false,
          replacement: "",
          revision: `fixture-${Date.now()}`,
          owner: "released-client-fixture",
        }),
      );
    },
    { name, kind },
  );
  await tab.reload();
  await tab.getByRole("button", { name: "Budget", exact: true }).click();
  await tab.getByLabel(/Months displayed/).selectOption("1");
  await expect(tab.getByLabel("Name", { exact: true })).toHaveValue(name);
}

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
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await editCategory(page, "Food");
  await page.getByLabel("Name", { exact: true }).fill("Meals");
  await expect(page.getByLabel("Notes", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Hidden", { exact: true })).toHaveCount(0);
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
  const uncertainJournal = await page.evaluate(
    (key) => localStorage.getItem(key),
    `actualBudget.categories.v1.${b.id}`,
  );
  await page.context().setOffline(true);
  await expect(
    page.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeEnabled();
  expect(
    await page.evaluate(
      (key) => localStorage.getItem(key),
      `actualBudget.categories.v1.${b.id}`,
    ),
  ).toBe(uncertainJournal);
  await page.context().setOffline(false);
  await page.reload();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await page.getByRole("button", { name: "Retry unchanged request" }).click();
  await expect(
    page.getByRole("textbox", { name: "Category name Meals", exact: true }),
  ).toBeVisible();
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);

  await editCategory(page, "Meals");
  await page.getByRole("button", { name: /^Delete category / }).click();
  await page.getByLabel("Reassign transactions to").selectOption(dining.id);
  await page
    .getByRole("button", { name: "Delete and reassign", exact: true })
    .click();
  await expect(
    page.getByRole("textbox", { name: "Category name Meals", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Budget month", { exact: true }).fill("Jan-26");
  await page.getByLabel("Budget month", { exact: true }).press("Enter");
  await page
    .getByRole("button", { name: "Edit Dining for January 2026", exact: true })
    .click();
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await expect(
    page.locator(".register-table tbody tr[data-transaction-id]"),
  ).toHaveCount(3);
  await expect(page.getByTestId("filtered-totals")).toContainText("-6.00");
  await expect(page.locator(".register-table")).toContainText("+2.50");
  await expect(page.locator(".register-table")).toContainText("+4.00");
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/categories-desktop.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Add group", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Travel");
  await expect(page.getByLabel("Order", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Toggle Travel", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Add category to Travel", exact: true })
    .click();
  await page.getByLabel("Name", { exact: true }).fill("Train");
  await expect(page.getByLabel("Group", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("textbox", { name: "Category name Train", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("textbox", { name: "Category name Train", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("textbox", { name: "Category name Train", exact: true })
    .focus();
  await expect(
    page.getByRole("textbox", { name: "Category name Train", exact: true }),
  ).toBeFocused();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/categories-mobile.png",
    fullPage: true,
  });
  expect(await call(base + "/categories/" + food.id)).not.toHaveProperty(
    "notes",
  );
  const nullGroup = await call(base + "/category-groups", {
    id: randomUUID(),
    name: "Null group",
    sort_order: null,
  });
  const nullCategory = await call(base + "/categories", {
    id: randomUUID(),
    name: "Null order",
    group_id: nullGroup.id,
    sort_order: null,
  });
  await page
    .getByRole("button", { name: "Refresh budget", exact: true })
    .click();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await editCategory(page, "Null order");
  await expect(page.getByLabel("Order", { exact: true })).toHaveCount(0);
  await page.getByLabel("Name", { exact: true }).fill("Null order renamed");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("textbox", {
      name: "Category name Null order renamed",
      exact: true,
    }),
  ).toBeVisible();
  expect(
    (await call(base + "/categories/" + nullCategory.id)).sort_order,
  ).toBeNull();
  await editCategory(page, "Null group", "category-groups");
  await page.getByLabel("Name", { exact: true }).fill("Null group renamed");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("textbox", {
      name: "Group name Null group renamed",
      exact: true,
    }),
  ).toBeVisible();
  expect(
    (await call(base + "/category-groups/" + nullGroup.id)).sort_order,
  ).toBeNull();
  expect(
    await call(base + "/categories/" + nullCategory.id),
  ).not.toHaveProperty("notes");
  expect(
    await call(base + "/category-groups/" + nullGroup.id),
  ).not.toHaveProperty("notes");
  await editCategory(page, "Null order renamed");
  await page
    .getByRole("button", { name: /^Delete category /, exact: true })
    .click();
  await page
    .getByRole("button", { name: "Delete and reassign", exact: true })
    .click();
  expect((await call(base + "/categories/" + nullCategory.id)).deleted).toBe(
    true,
  );
  // Category metadata is online-only. Synthetic submits must not create intents.
  await editCategory(page, "Train");
  await page.getByLabel("Name", { exact: true }).fill("Offline rename");
  const journalKey = `actualBudget.categories.v1.${b.id}`;
  const unsent = await page.evaluate(
    (key) => localStorage.getItem(key),
    journalKey,
  );
  const metadataWrites = [];
  page.on("request", (request) => {
    if (
      request.url().includes(`/api/v1${base}/`) &&
      ["POST", "PATCH", "DELETE"].includes(request.method())
    )
      metadataWrites.push(request.url());
  });
  await page.context().setOffline(true);
  await expect(
    page.getByRole("button", { name: "Save", exact: true }),
  ).toBeDisabled();
  await page.locator(".category-editor").dispatchEvent("submit");
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Reconnect before changing categories or groups." }),
  ).toBeVisible();
  expect(
    await page.evaluate((key) => localStorage.getItem(key), journalKey),
  ).toBe(unsent);
  expect(metadataWrites).toEqual([]);
  await expect(
    page.getByRole("button", { name: /^Delete category /, exact: true }),
  ).toBeDisabled();
  await page.screenshot({
    path: "test-results/categories-offline-mobile.png",
    fullPage: true,
  });
  await page.context().setOffline(false);
  await expect(
    page.getByRole("button", { name: "Save", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  // A stale tab cannot replace or clear another tab's durable request.
  const shared = await browser.newContext();
  const first = await shared.newPage(),
    second = await shared.newPage();
  async function openCategories(tab) {
    await tab.goto(server.url);
    const budgetButton = tab
      .locator(".budget-list button")
      .filter({ hasText: "Category household" });
    const categoriesButton = tab.getByRole("button", {
      name: "Budget",
      exact: true,
    });
    await expect(budgetButton.or(categoriesButton)).toBeVisible();
    if (await budgetButton.isVisible()) await budgetButton.click();
    await tab.getByRole("button", { name: "Budget", exact: true }).click();
    await tab.getByLabel(/Months displayed/).selectOption("1");
    await expect(tab.locator(".budget-month")).toHaveCount(1);
    await expect(
      tab.getByRole("textbox", { name: "Category name Train", exact: true }),
    ).toBeVisible();
  }
  await openCategories(first);
  await openCategories(second);
  const concurrentAttempts = [];
  let loseResponse = true;
  await first.route(`**/api/v1${base}/categories/*`, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    concurrentAttempts.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
      clock: route.request().headers()["x-actual-edit-clock"],
      epoch: route.request().headers()["x-actual-budget-epoch"],
    });
    const url = new URL(route.request().url());
    const headers = { ...route.request().headers(), host: url.host };
    url.hostname = "127.0.0.1";
    const response = await route.fetch({ url: url.href, headers });
    if (loseResponse) {
      loseResponse = false;
      await route.abort("connectionreset");
    } else await route.fulfill({ response });
  });
  await editCategory(first, "Train");
  await first.getByLabel("Category shade", { exact: true }).fill("#337755");
  await first.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    first.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  const originalJournal = await first.evaluate(
    (key) => localStorage.getItem(key),
    journalKey,
  );
  await second
    .getByRole("button", { name: "Add group", exact: true })
    .dispatchEvent("click");
  await expect(
    second.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  expect(
    await second.evaluate((key) => localStorage.getItem(key), journalKey),
  ).toBe(originalJournal);
  await second.reload();
  await second.getByRole("button", { name: "Budget", exact: true }).click();
  await second.getByLabel(/Months displayed/).selectOption("1");
  await expect(second.locator(".budget-month")).toHaveCount(1);
  await expect(
    second.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  await first.getByRole("button", { name: "Retry unchanged request" }).click();
  await expect(
    first.getByRole("textbox", { name: "Category name Train", exact: true }),
  ).toBeVisible();
  expect(concurrentAttempts[1]).toEqual(concurrentAttempts[0]);
  await editCategory(first, "Train");
  await first.getByLabel("Category shade", { exact: true }).fill("#446688");
  loseResponse = true;
  await first.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    first.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  const newerJournal = await first.evaluate(
    (key) => localStorage.getItem(key),
    journalKey,
  );
  expect(JSON.parse(newerJournal).pending.key).not.toBe(
    JSON.parse(originalJournal).pending.key,
  );
  await second
    .getByRole("button", { name: "Retry unchanged request" })
    .dispatchEvent("click");
  expect(
    await second.evaluate((key) => localStorage.getItem(key), journalKey),
  ).toBe(newerJournal);
  await first.reload();
  await first.getByRole("button", { name: "Budget", exact: true }).click();
  await first.getByLabel(/Months displayed/).selectOption("1");
  await expect(first.locator(".budget-month")).toHaveCount(1);
  await first.getByRole("button", { name: "Retry unchanged request" }).click();
  await expect(
    first.getByRole("textbox", { name: "Category name Train", exact: true }),
  ).toBeVisible();
  expect(concurrentAttempts[3]).toEqual(concurrentAttempts[2]);
  await shared.close();
  expect(errors).toEqual([]);
  console.log(
    "PASS categories browser: durable retries and stale-tab ownership, offline boundaries, null order, notes retirement, merges, groups, keyboard, desktop/mobile plain HTTP",
  );
} finally {
  await browser?.close();
  await server.stop();
}

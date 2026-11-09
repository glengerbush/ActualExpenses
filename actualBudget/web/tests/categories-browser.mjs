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
    page.locator("details .notes").filter({ hasText: "Keep receipts" }),
  ).toBeVisible();
  await page.route("https://example.com/**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "image/png",
      body: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aBZkAAAAASUVORK5CYII=",
        "base64",
      ),
    }),
  );
  const nullGroup = await call(base + "/category-groups", {
    id: randomUUID(),
    name: "Null group",
    sort_order: null,
    notes: "**Group note**",
  });
  const nullCategory = await call(base + "/categories", {
    id: randomUUID(),
    name: "Null order",
    group_id: nullGroup.id,
    sort_order: null,
    notes:
      "**Formatted note** and [Receipt](https://example.com/receipt)\n\n<script>window.notesExecuted=true</script>\n\n[Unsafe](javascript:alert(1))\n\n![Receipt image](https://example.com/receipt.png)\n\n~~Old plan~~\n\n- [x] Reimbursed\n- [ ] Pending\n\n| Month | Total |\n| --- | --- |\n| Jan | 12.50 |\n\nhttps://example.com/autolink\n\nLine one\nLine two\n\n\nAfter blank lines\n\n![Unsafe image](javascript:alert(2))",
  });
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: "Categories", exact: true }).click();
  await page
    .getByRole("button", { name: "Edit Null order", exact: true })
    .click();
  await expect(page.getByLabel("Order", { exact: true })).toHaveValue("");
  await page.getByLabel("Name", { exact: true }).fill("Null order renamed");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Edit Null order renamed", exact: true }),
  ).toBeVisible();
  expect(
    (await call(base + "/categories/" + nullCategory.id)).sort_order,
  ).toBeNull();
  await page
    .getByRole("button", { name: "Edit group Null group", exact: true })
    .click();
  await page.getByLabel("Name", { exact: true }).fill("Null group renamed");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Edit group Null group renamed",
      exact: true,
    }),
  ).toBeVisible();
  expect(
    (await call(base + "/category-groups/" + nullGroup.id)).sort_order,
  ).toBeNull();
  await expect(
    page.locator(".notes strong").filter({ hasText: "Formatted note" }),
  ).toBeVisible();
  await expect(
    page.locator(".notes strong").filter({ hasText: "Group note" }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Receipt", exact: true }),
  ).toHaveAttribute("href", "https://example.com/receipt");
  await expect(
    page.getByRole("link", { name: "Unsafe", exact: true }),
  ).toHaveCount(0);
  expect(await page.evaluate(() => window.notesExecuted)).toBeUndefined();
  expect(await page.locator(".notes script, .notes [onerror]").count()).toBe(0);
  await expect(
    page.getByRole("link", { name: "Receipt", exact: true }),
  ).toHaveAttribute("target", "_blank");
  await expect(
    page.getByRole("link", { name: "Receipt", exact: true }),
  ).toHaveAttribute("rel", "noopener noreferrer");
  await expect(
    page.getByRole("img", { name: "Receipt image", exact: true }),
  ).toHaveAttribute("src", "https://example.com/receipt.png");
  await expect(
    page.getByRole("img", { name: "Receipt image", exact: true }),
  ).toHaveAttribute("loading", "lazy");
  expect(
    await page
      .locator(".notes [src^='javascript:'], .notes [href^='javascript:']")
      .count(),
  ).toBe(0);
  await expect(page.locator(".notes del")).toHaveText("Old plan");
  await expect(
    page.locator(".notes input[type=checkbox]").first(),
  ).toBeChecked();
  await expect(
    page.locator(".notes input[type=checkbox]").first(),
  ).toBeDisabled();
  await expect(page.locator(".notes table")).toContainText("12.50");
  await expect(
    page.getByRole("link", {
      name: "https://example.com/autolink",
      exact: true,
    }),
  ).toHaveAttribute("href", "https://example.com/autolink");
  expect(await page.locator(".notes br").count()).toBeGreaterThanOrEqual(3);
  await page.screenshot({
    path: "test-results/categories-markdown-mobile.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 960 });
  await page.screenshot({
    path: "test-results/categories-markdown-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByRole("button", { name: "Edit Null order renamed", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Merge or delete", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Delete and reassign", exact: true })
    .click();
  await page.getByText("Retained category notes", { exact: true }).click();
  await expect(
    page.locator("details .notes strong").filter({ hasText: "Formatted note" }),
  ).toBeVisible();
  // Category metadata is online-only. Synthetic submits must not create intents.
  await page.getByRole("button", { name: "Edit Train", exact: true }).click();
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
    page.getByRole("button", { name: "Merge or delete", exact: true }),
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
      name: "Categories",
      exact: true,
    });
    await expect(budgetButton.or(categoriesButton)).toBeVisible();
    if (await budgetButton.isVisible()) await budgetButton.click();
    await tab.getByRole("button", { name: "Categories", exact: true }).click();
    await expect(
      tab.getByRole("button", { name: "Edit Train", exact: true }),
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
  await first.getByRole("button", { name: "Edit Train", exact: true }).click();
  await first
    .getByLabel("Notes", { exact: true })
    .fill("First tab retained note");
  await first.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    first.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  const originalJournal = await first.evaluate(
    (key) => localStorage.getItem(key),
    journalKey,
  );
  await second
    .getByRole("button", { name: "New group", exact: true })
    .dispatchEvent("click");
  await expect(
    second
      .getByRole("alert")
      .filter({ hasText: "Another tab changed the saved category draft" }),
  ).toBeVisible();
  expect(
    await second.evaluate((key) => localStorage.getItem(key), journalKey),
  ).toBe(originalJournal);
  await second.reload();
  await second.getByRole("button", { name: "Categories", exact: true }).click();
  await expect(
    second.getByRole("button", { name: "Retry unchanged request" }),
  ).toBeVisible();
  await first.getByRole("button", { name: "Retry unchanged request" }).click();
  await expect(
    first.getByRole("button", { name: "Edit Train", exact: true }),
  ).toBeVisible();
  expect(concurrentAttempts[1]).toEqual(concurrentAttempts[0]);
  await first.getByRole("button", { name: "Edit Train", exact: true }).click();
  await first
    .getByLabel("Notes", { exact: true })
    .fill("Newer durable request");
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
  await first.getByRole("button", { name: "Categories", exact: true }).click();
  await first.getByRole("button", { name: "Retry unchanged request" }).click();
  await expect(
    first.getByRole("button", { name: "Edit Train", exact: true }),
  ).toBeVisible();
  expect(concurrentAttempts[3]).toEqual(concurrentAttempts[2]);
  await shared.close();
  expect(errors).toEqual([]);
  console.log(
    "PASS categories browser: durable retries and stale-tab ownership, offline boundaries, null order, safe GFM notes, merges, groups, keyboard, desktop/mobile plain HTTP",
  );
} finally {
  await browser?.close();
  await server.stop();
}

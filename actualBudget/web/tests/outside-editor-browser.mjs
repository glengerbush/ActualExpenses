import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function raw(path, body) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
}
async function call(path, body) {
  return (await raw(path, body)).data;
}
let browser;
try {
  const b = randomUUID(),
    a = randomUUID(),
    other = randomUUID(),
    c = randomUUID(),
    row = randomUUID(),
    split = randomUUID(),
    transfer = randomUUID(),
    root = `/budgets/${b}`;
  await call("/budgets", { id: b, name: "Outside editors" });
  for (const [id, name] of [
    [a, "History"],
    [other, "Other"],
  ])
    await call(root + "/accounts", {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-11-01",
    });
  await call(root + "/categories", { id: c, name: "Food" });
  for (const [id, notes] of [
    [row, "Regular"],
    [split, "Split"],
  ])
    await call(root + "/transactions", {
      id,
      account_id: a,
      category_id: c,
      date: "2026-11-01",
      amount: -1000,
      notes,
    });
  const snapshot = await raw(root + `/transactions/${split}/splits`);
  await call(root + `/transactions/${split}/splits`, {
    snapshot: snapshot.snapshot,
    children: [
      { id: randomUUID(), amount: -600, category_id: c, notes: "First" },
      { id: randomUUID(), amount: -400, category_id: c, notes: "Second" },
    ],
  });
  await call(root + "/transfers", {
    id: transfer,
    counterpart_id: randomUUID(),
    account_id: a,
    to_account_id: other,
    amount: -500,
    date: "2026-11-01",
    notes: "Transfer",
  });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
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
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByText("Outside editors", { exact: true }).click();
  const outside = () => page.locator(".brand").click();
  const transaction = () => page.locator(`[data-transaction-id="${row}"]`);
  await transaction().click();
  await expect(page.locator(".transaction-form")).toBeVisible();
  await page.getByLabel("Amount", { exact: true }).click();
  await expect(page.locator(".transaction-form")).toBeVisible();
  await page
    .getByLabel("Search transactions", { exact: true })
    .click({ force: true });
  await expect(page.locator(".transaction-form")).toHaveCount(0);
  await expect(
    page.getByLabel("Search transactions", { exact: true }),
  ).toBeEnabled();
  await transaction().click();
  await page.getByLabel("Notes", { exact: true }).fill("Keep this edit");
  await outside();
  await expect(page.locator(".transaction-form")).toBeVisible();
  await page.reload();
  await expect(page.getByLabel("Notes", { exact: true })).toHaveValue(
    "Keep this edit",
  );
  await outside();
  await expect(page.locator(".transaction-form")).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.locator(`[data-transaction-id="${split}"]`).click();
  await expect(page.locator(".split-editor")).toBeVisible();
  await expect(page.getByLabel("Split 1 notes", { exact: true })).toHaveValue(
    "First",
  );
  await outside();
  await expect(page.locator(".split-editor")).toHaveCount(0);
  await page.locator(`[data-transaction-id="${split}"]`).click();
  await page.getByLabel("Split 1 notes", { exact: true }).fill("Changed split");
  await outside();
  await expect(page.locator(".split-editor")).toBeVisible();
  await page
    .getByRole("button", { name: "Discard split draft", exact: true })
    .click();
  await page.locator(`[data-transaction-id="${transfer}"]`).click();
  await expect(page.locator(".transfer-form")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Save transfer", exact: true }),
  ).toBeEnabled();
  await outside();
  await expect(page.locator(".transfer-form")).toHaveCount(0);
  await page.locator(`[data-transaction-id="${transfer}"]`).click();
  await page.getByLabel("Notes", { exact: true }).fill("Changed transfer");
  await outside();
  await expect(page.locator(".transfer-form")).toBeVisible();
  await page
    .getByRole("button", { name: "Close transfer form", exact: true })
    .click();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  const category = () =>
    page
      .locator(".budget-month")
      .first()
      .locator(".budget-category")
      .filter({ hasText: "Food" });
  await category().click();
  await expect(page.locator(".inline-plan-row")).toBeVisible();
  await page.getByLabel("Amount", { exact: true }).click();
  await expect(page.locator(".inline-plan-row")).toBeVisible();
  await outside();
  await expect(page.locator(".inline-plan-row")).toHaveCount(0);
  await category().click();
  await page.getByLabel("Amount", { exact: true }).fill("42");
  await outside();
  await expect(page.locator(".inline-plan-row")).toBeVisible();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("42");
  await page.reload();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("42");
  await outside();
  await expect(page.locator(".inline-plan-row")).toBeVisible();
  let release;
  const paused = new Promise((resolve) => (release = resolve));
  await page.route("**/api/v1" + root + "/budget", async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    await paused;
    await route.continue();
  });
  await page.getByRole("button", { name: "Save plan", exact: true }).click();
  await outside();
  await expect(page.locator(".inline-plan-row")).toBeVisible();
  release();
  await expect(page.locator(".inline-plan-row")).toHaveCount(0);

  await page.setViewportSize({ width: 390, height: 844 });
  await category().click();
  await outside();
  await expect(page.locator(".inline-plan-row")).toHaveCount(0);
  expect((await call(root + `/transactions/${row}`)).notes).toBe("Regular");
  expect(errors).toEqual([]);
  console.log(
    "PASS pristine outside dismissal for budget, transaction, split and transfer rows; inside clicks, dirty/recovered drafts and mobile",
  );
} finally {
  if (browser) await browser.close();
  await server.stop();
}

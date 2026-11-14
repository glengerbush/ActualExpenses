import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const directory = await mkdtemp(tmpdir() + "/budget-rows-");
let server = await startServer({ dataDirectory: directory }),
  browser;
const api = () =>
  server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const r = await fetch(api() + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
try {
  const budget = randomUUID(),
    account = randomUUID(),
    group = randomUUID(),
    category = randomUUID(),
    root = `/budgets/${budget}`;
  const date = new Date().toISOString().slice(0, 10),
    month = date.slice(0, 7);
  await call("/budgets", { id: budget, name: "Editable budget" });
  await call(root + "/accounts", {
    id: account,
    name: "Ledger",
    opening_balance: 0,
    opening_date: date,
  });
  await call(root + "/category-groups", {
    id: group,
    name: "Everyday",
    color: "#397c8c",
  });
  await call(root + "/categories", {
    id: category,
    name: "Groceries",
    group_id: group,
  });
  await call(root + "/transactions", {
    id: randomUUID(),
    account_id: account,
    category_id: category,
    date,
    amount: -2500,
  });
  await server.stop();
  // Actual pre-retirement storage: both category and group hidden.
  execFileSync("python3", [
    "tests/historical-schema.py",
    directory + "/actualbudget.sqlite3",
    "29",
  ]);
  execFileSync("python3", [
    "-c",
    `import sqlite3,sys
c=sqlite3.connect(sys.argv[1]+'/actualbudget.sqlite3')
for t in ['categories','category_groups']:
 c.execute('UPDATE '+t+' SET hidden=1')
c.commit()`,
    directory,
  ]);
  server = await startServer({ dataDirectory: directory });
  expect((await call(root + "/categories/" + category)).hidden).toBe(false);
  expect((await call(root + "/category-groups/" + group)).hidden).toBe(false);
  expect(
    (await call(root + "/budget?month=" + month)).months[month].summary
      .total_spent,
  ).toBe(-2500);
  const denied = await fetch(api() + root + "/categories/" + category, {
    method: "PATCH",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify({ hidden: true }),
  });
  expect(denied.status).toBe(400);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
      viewport: { width: 1440, height: 1000 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel("Months displayed").selectOption("3");
  await expect(page.locator(".budget-month")).toHaveCount(3);
  await expect(
    page.getByText("Show hidden categories", { exact: true }),
  ).toHaveCount(0);
  const bounds = await page.locator(".budget-month").evaluateAll((es) =>
    es.map((e) => {
      const r = e.getBoundingClientRect();
      return { x: r.x, y: r.y };
    }),
  );
  expect(bounds[1].x).toBeGreaterThan(bounds[0].x);
  expect(Math.abs(bounds[1].y - bounds[0].y)).toBeLessThan(2);
  const row = page
    .locator(".budget-month")
    .first()
    .locator(`.budget-category[data-id="${category}"]`);
  const planned = row.getByRole("textbox", { name: /^Planned Groceries for/ });
  await expect(planned).toBeVisible();
  await planned.fill("125");
  await planned.press("Enter");
  await expect(planned).toHaveValue("125.00");
  expect(
    (await call(root + "/budget?month=" + month)).months[month].categories.find(
      (c) => c.id === category,
    ).budgeted,
  ).toBe(12500);
  const edit = row.getByRole("button", { name: /^Edit Groceries for/ });
  await edit.focus();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("region", { name: "Budget options for Groceries" }),
  ).toBeVisible();
  await expect(
    page.getByRole("switch", { name: "Carryover excess from previous month" }),
  ).toBeVisible();
  await expect(
    row.getByRole("textbox", { name: "Category name Groceries", exact: true }),
  ).toHaveValue("Groceries");
  await expect(page.getByLabel("Hidden", { exact: true })).toHaveCount(0);
  await page.locator(".brand").click();
  await mkdir("test-results/budget-rows", { recursive: true });
  await page.screenshot({
    path: "test-results/budget-rows/desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/budget-rows/mobile.png",
    fullPage: true,
  });

  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  const mobile = await page
    .locator(".budget-month")
    .evaluateAll((es) => es.map((e) => e.getBoundingClientRect().y));
  expect(Math.abs(mobile[0] - mobile[1])).toBeLessThan(2);
  await page.screenshot({
    path: "test-results/budget-rows/mobile.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS row click/keyboard, inline save/settings, horizontal months desktop/mobile, hidden migration, totals and rejected hiding",
  );
} finally {
  await browser?.close();
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}

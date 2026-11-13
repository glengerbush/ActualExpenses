import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const r = await fetch(api + path, {
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
let browser;
try {
  const b = randomUUID(),
    c = randomUUID(),
    other = randomUUID(),
    tx = randomUUID(),
    root = `/budgets/${b}`;
  const month = new Date().toISOString().slice(0, 7);
  const previous = new Date(month + "-01T12:00:00Z");
  previous.setUTCMonth(previous.getUTCMonth() - 1);
  const prev = previous.toISOString().slice(0, 7);
  await call("/budgets", { id: b, name: "Aligned editing" });
  const a = (await call(root + "/ledger", {})).id;
  await call(root + "/categories", { id: c, name: "Food" });
  await call(root + "/categories", { id: other, name: "Travel" });
  for (const [id, category, date] of [
    [tx, c, month + "-01"],
    [randomUUID(), other, month + "-01"],
    [randomUUID(), c, prev + "-01"],
  ])
    await call(root + "/transactions", {
      id,
      account_id: a,
      category_id: category,
      date,
      amount: -1250,
    });
  const plan = () => call(root + `/budget?month=${month}&count=2`);
  await call(root + "/budget", {
    action: "set",
    month: prev,
    category: c,
    amount: 5000,
    snapshot: (await plan()).snapshot,
  });
  const category = async () =>
    (await plan()).months[month].categories.find((row) => row.id === c);
  expect((await category()).carryover_from_previous).toBe(true);
  expect((await category()).balance).toBe(2500);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } }),
    errors = [];
  p.on("pageerror", (e) => errors.push(e.message));
  await p.goto(server.url);
  await p.getByText("Aligned editing", { exact: true }).click();
  async function aligned() {
    await expect(p.locator(".aligned-field").first()).toBeVisible();
    const order = await p
      .locator(".aligned-field")
      .evaluateAll((nodes) => nodes.map((n) => n.dataset.editorColumn));
    const headers = await p
      .locator(".register-table th[data-column]")
      .evaluateAll((nodes) => nodes.map((n) => n.dataset.column));
    expect(order).toEqual(headers);
    for (const id of ["date", "payee", "category", "amount"]) {
      const field = p.locator(`[data-editor-column="${id}"]`);
      if (await field.count()) {
        const box = await field.boundingBox(),
          input = await field.locator(`#transaction-${id}`).boundingBox();
        expect(input.x).toBeGreaterThanOrEqual(box.x);
        expect(input.x + input.width).toBeLessThanOrEqual(
          box.x + box.width + 1,
        );
      }
    }
  }
  await p.locator(`[data-transaction-id="${tx}"]`).click();
  await aligned();
  const rowBounds = await p.evaluate(() => {
    const ids = [
      "#transaction-date",
      "#transaction-amount",
      ".transaction-form .direction-toggle",
      '.transaction-form [role="switch"]',
      '.transaction-form button[aria-label="Save transaction"]',
      '.transaction-form button[aria-label="Cancel"]',
    ];
    return ids.map((selector) => {
      const r = document.querySelector(selector).getBoundingClientRect();
      return { x: r.x, y: r.y, height: r.height };
    });
  });
  expect(
    Math.max(...rowBounds.map((r) => r.y + r.height / 2)) -
      Math.min(...rowBounds.map((r) => r.y + r.height / 2)),
  ).toBeLessThan(3);
  expect(rowBounds[2].x).toBeLessThan(rowBounds[1].x);
  const widths = await p.evaluate(() => {
    const box = (selector) =>
      document.querySelector(selector).getBoundingClientRect().toJSON();
    return {
      date: box('[data-editor-column="date"]'),
      payee: box('[data-editor-column="payee"]'),
      category: box('[data-editor-column="category"]'),
      amount: box('[data-editor-column="amount"]'),
      editor: box(".inline-editor"),
    };
  });
  expect(Math.abs(widths.date.x - widths.editor.x)).toBeLessThan(2);
  expect(widths.payee.width).toBeGreaterThan(widths.amount.width);
  expect(widths.category.width).toBeGreaterThan(widths.amount.width);
  await p.screenshot({
    path: "../../.scratch/aligned-transaction-desktop.png",
  });
  await p.getByRole("button", { name: "Cancel", exact: true }).click();
  await p
    .getByRole("button", { name: "Add Transaction", exact: false })
    .click();
  await aligned();
  await p.getByRole("button", { name: "Cancel", exact: true }).click();
  await p.locator('summary[aria-label="Filters and columns"]').click();
  await p.getByRole("button", { name: "Columns", exact: true }).click();
  await p.getByLabel("Show Category column", { exact: true }).uncheck();
  await p.getByText("Column order", { exact: true }).click();
  await p
    .getByRole("button", { name: "Move Amount earlier", exact: true })
    .click();
  await p
    .getByRole("button", { name: "Apply filters and columns", exact: true })
    .click();
  await p.locator('summary[aria-label="Filters and columns"]').click();
  await p.locator(`[data-transaction-id="${tx}"]`).click();
  await aligned();
  expect(
    await p
      .locator(".aligned-field")
      .evaluateAll((nodes) => nodes.map((n) => n.dataset.editorColumn)),
  ).toEqual(["date", "amount", "payee", "balance"]);
  await expect(
    p.locator(".additional-fields").getByLabel("Category", { exact: true }),
  ).toBeVisible();
  await p.setViewportSize({ width: 390, height: 844 });
  await expect(p.getByLabel("Date", { exact: true })).toBeVisible();
  expect(
    await p.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  await p.screenshot({ path: "../../.scratch/aligned-transaction-mobile.png" });
  await p.getByRole("button", { name: "Cancel", exact: true }).click();
  await p.setViewportSize({ width: 1440, height: 1000 });
  const nav = p.getByRole("complementary", { name: "Budget navigation" });
  await nav.getByRole("button", { name: "Budget", exact: true }).click();
  await p
    .getByRole("button", { name: /^Edit Food for/ })
    .first()
    .click();
  const toggle = p.getByRole("switch", {
    name: "Carryover excess from previous month",
    exact: true,
  });
  await expect(toggle).toBeChecked();
  await expect(
    p.getByRole("button", { name: "Save plan", exact: true }),
  ).toHaveCount(0);
  await toggle.uncheck();
  await expect.poll(async () => (await category()).balance).toBe(-1250);
  await expect(toggle).toBeEnabled();
  await toggle.check();
  await expect.poll(async () => (await category()).balance).toBe(2500);
  await p.getByRole("button", { name: "Copy previous", exact: true }).click();
  await expect.poll(async () => (await category()).budgeted).toBe(5000);
  await expect(
    p.getByRole("button", { name: "Transactions", exact: true }),
  ).toBeEnabled();
  await p.screenshot({ path: "../../.scratch/immediate-category.png" });
  await p.getByRole("button", { name: "Transactions", exact: true }).click();
  await expect(
    p.getByRole("heading", { name: "Transaction history", exact: true }),
  ).toBeVisible();
  await expect(
    p.locator(".register-table tbody tr[data-transaction-id]"),
  ).toHaveCount(1);
  await expect(p.locator(`[data-transaction-id="${tx}"]`)).toBeVisible();
  expect(new URL(p.url()).searchParams.get("category")).toBe(c);
  await p.goBack();
  await expect(
    p.getByRole("heading", { name: "Budget", exact: true }),
  ).toBeVisible();
  await p.goForward();
  await expect(
    p.locator(".register-table tbody tr[data-transaction-id]"),
  ).toHaveCount(1);
  await p.reload();
  await expect(
    p.locator(".register-table tbody tr[data-transaction-id]"),
  ).toHaveCount(1);
  expect(errors).toEqual([]);
  console.log(
    "PASS: aligned add/edit, reordered/hidden columns, mobile, immediate carryover/copy, filtered history navigation",
  );
} finally {
  await browser?.close();
  await server.stop();
}

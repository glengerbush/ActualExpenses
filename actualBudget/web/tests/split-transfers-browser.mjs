import { transactionAction, transactionRows } from "./register-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
async function call(path, body) {
  const r = await fetch(api + "/api/v1" + path, {
    method: body ? "POST" : "GET",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const result = await r.json();
  expect(r.ok, JSON.stringify(result)).toBe(true);
  return result;
}
try {
  const b = randomUUID(),
    cash = randomUUID(),
    card = randomUUID(),
    parent = randomUUID(),
    base = `/budgets/${b}`;
  await call("/budgets", { id: b, name: "Split transfers" });
  for (const [id, name] of [
    [cash, "Checking"],
    [card, "Credit card"],
  ])
    await call(base + "/accounts", {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
  await call(base + "/transactions", {
    id: parent,
    account_id: cash,
    amount: -1000,
    date: "2026-01-01",
    notes: "Mixed payment",
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
    .filter({ hasText: "Split transfers" })
    .click();
  await page.getByRole("button", { name: "Checking", exact: true }).click();
  await (
    await transactionAction(page, { name: "Split transaction", exact: true })
  ).click();
  await page.getByLabel("Split 1 amount", { exact: true }).fill("-6.00");
  await page.getByLabel("Split 2 amount", { exact: true }).fill("-4.00");
  await page
    .getByLabel("Split 1 transfer to", { exact: true })
    .selectOption(card);
  await page
    .getByLabel("Split 1 notes", { exact: true })
    .fill("Card allocation");
  await expect(
    page.getByLabel("Split 1 payee", { exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByLabel("Split 1 category", { exact: true }),
  ).toBeDisabled();
  await page.getByLabel("Split 1 transfer to", { exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Split 1 notes", { exact: true })).toBeFocused();
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/split-transfers-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/split-transfers-mobile.png",
    fullPage: true,
  });
  const attempts = [];
  let drop = true;
  await page.route("**/transactions/*/splits", async (route) => {
    const req = route.request();
    if (req.method() !== "POST") return route.continue();
    attempts.push([
      req.postData(),
      req.headers()["idempotency-key"],
      req.headers()["x-actual-edit-clock"],
      req.headers()["x-actual-budget-epoch"],
    ]);
    const url = new URL(req.url()),
      headers = { ...req.headers(), host: url.host };
    url.hostname = "127.0.0.1";
    const response = await route.fetch({ url: url.href, headers });
    if (drop) {
      drop = false;
      await route.abort();
    } else await route.fulfill({ response });
  });
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Split 1 transfer to", { exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Retry saved split", exact: true })
    .click();
  await expect(page.locator(".split-editor")).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  let row = (await call(base + `/transactions/${parent}`)).data;
  const child = row.children[0],
    peer = (await call(base + `/transactions/${child.transfer_id}`)).data;
  expect(peer.amount).toBe(600);
  expect(peer.transfer_id).toBe(child.id);
  await expect(
    page.getByRole("button", { name: "Link transfer", exact: true }),
  ).toHaveCount(0);
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  await expect(
    page.getByLabel("Split 1 transfer to", { exact: true }),
  ).toHaveValue(card);
  await page.getByLabel("Split 1 amount", { exact: true }).fill("-7.00");
  await page.getByLabel("Split 2 amount", { exact: true }).fill("-3.00");
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(page.locator(".split-editor")).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  expect((await call(base + `/transactions/${peer.id}`)).data.amount).toBe(700);
  await page.getByRole("button", { name: "Credit card", exact: true }).click();
  await expect(
    await transactionAction(page, { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  await transactionRows(page, { name: /^Edit / }).click();
  await page.getByLabel("Amount", { exact: true }).fill("8.00");
  await page
    .getByRole("button", { name: "Save transfer", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Edit transfer", exact: true }),
  ).toHaveCount(0);
  row = (await call(base + `/transactions/${parent}`)).data;
  expect(row.amount).toBe(-1100);
  expect(row.children.map((c) => c.amount)).toEqual([-800, -300]);
  const existing = randomUUID();
  await call(base + "/transactions", {
    id: existing,
    account_id: card,
    amount: 300,
    date: "2026-01-01",
    notes: "Existing deposit",
  });
  await page.reload();
  await page
    .getByRole("button", { name: "Link transfer", exact: true })
    .click();
  await page.getByLabel("To account", { exact: true }).selectOption(cash);
  await page
    .getByLabel("Matching entry", { exact: true })
    .selectOption(row.children[1].id);
  await page.getByRole("button", { name: "Link entries", exact: true }).click();
  await expect(page.getByLabel("Matching entry", { exact: true })).toHaveCount(
    0,
  );
  expect(
    (await call(base + `/transactions/${existing}`)).data.transfer_id,
  ).toBe(row.children[1].id);
  expect(errors).toEqual([]);
  console.log(
    "PASS split transfer creation/replay, child editor, parent total and responsive keyboard flow",
  );
} finally {
  await browser?.close();
  await server.stop();
}

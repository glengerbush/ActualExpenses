import { transactionRows } from "./register-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body) {
  const r = await fetch(api + "/api/v1" + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
async function get(path) {
  return (await fetch(api + "/api/v1" + path)).json();
}
let browser;
try {
  const budget = randomUUID(),
    cash = randomUUID(),
    card = randomUUID();
  const root = `/budgets/${budget}`;
  await write("/budgets", { id: budget, name: "Transfer browser" });
  for (const [id, name] of [
    [cash, "Cash"],
    [card, "Credit card"],
  ])
    await write(root + "/accounts", {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-01-01",
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
    viewport: { width: 1280, height: 900 },
  });
  page.on("pageerror", (error) => console.log("PAGE ERROR", error.message));
  await page.goto(server.url);
  await page
    .getByRole("button", { name: "Transfer browser Tracking Open" })
    .click();
  await page.getByRole("button", { name: "Move money", exact: true }).click();

  await page.getByLabel("From account", { exact: true }).selectOption(cash);
  await page.getByLabel("To account", { exact: true }).selectOption(card);
  await page.getByLabel("Amount", { exact: true }).fill("-25.00");
  await page.getByLabel("Date", { exact: true }).fill("2026-01-01");
  await page.getByLabel("Notes", { exact: true }).fill("Card payment");
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/transfers-desktop.png",
    fullPage: true,
  });
  const requests = [];
  let lose = true;
  await page.route("**/api/v1" + root + "/transfers", async (route) => {
    const request = route.request();
    requests.push({
      body: request.postData(),
      key: request.headers()["idempotency-key"],
      clock: request.headers()["x-actual-edit-clock"],
      epoch: request.headers()["x-actual-budget-epoch"],
    });
    const target = new URL(request.url());
    const headers = { ...request.headers(), host: target.host };
    target.hostname = "127.0.0.1";
    const response = await route.fetch({ url: target.href, headers });
    if (lose) {
      lose = false;
      await route.abort();
    } else await route.fulfill({ response });
  });
  await page
    .getByRole("button", { name: "Save transfer", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved transfer" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Retry saved transfer" }).click();
  await expect(
    page.getByRole("heading", { name: "Move money", exact: true }),
  ).toHaveCount(0);
  expect(requests).toHaveLength(2);
  expect(requests[0]).toEqual(requests[1]);
  let rows = (await get(root + "/transactions")).data;
  expect(rows).toHaveLength(2);
  let source = rows.find((r) => r.account_id === cash);
  let other = rows.find((r) => r.account_id === card);
  expect(source.transfer_id).toBe(other.id);
  expect(other.amount).toBe(2500);
  await transactionRows(
    page
      .locator(".register-table tbody tr")
      .filter({ hasText: "Card payment" })
      .first(),
    { name: /^Edit / },
  ).click();
  await expect(
    page.getByRole("heading", { name: "Edit transfer" }),
  ).toBeVisible();
  await page.getByLabel("Date", { exact: true }).fill("2026-02-01");
  await page.getByLabel("Sync both transfer dates").check();
  await page
    .getByRole("button", { name: "Save transfer", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Edit transfer" }),
  ).toHaveCount(0);
  rows = (await get(root + "/transactions")).data;
  expect(rows.map((r) => r.date)).toEqual(["2026-02-01", "2026-02-01"]);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Move money", exact: true }).click();
  await expect(page.getByLabel("To account", { exact: true })).toBeEnabled();
  await page.screenshot({
    path: "test-results/transfers-mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByLabel("From account", { exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("To account", { exact: true })).toBeFocused();
  await page.getByRole("button", { name: "Close transfer form" }).click();
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Card payment" })
    .first()
    .getByRole("button", { name: /^Delete / })
    .click();
  await expect(
    page.getByText(
      /Delete this transfer\? Its paired entry is also deleted unless it belongs to a split/,
    ),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Confirm delete", exact: true })
    .click();
  await expect(page.locator(".register-table")).toHaveCount(0);
  expect((await get(root + "/transactions")).data).toHaveLength(0);

  const sourceId = randomUUID(),
    targetId = randomUUID();
  await write(root + "/transactions", {
    id: sourceId,
    account_id: cash,
    amount: -600,
    date: "2026-04-01",
    notes: "Link source",
  });
  await write(root + "/transactions", {
    id: targetId,
    account_id: card,
    amount: 600,
    date: "2026-04-04",
    notes: "Link destination",
  });
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Link source" })
    .getByRole("button", { name: "Link transfer", exact: true })
    .click();
  await page.getByLabel("To account", { exact: true }).selectOption(card);
  await page
    .getByLabel("Matching entry", { exact: true })
    .selectOption(targetId);
  await page.getByRole("button", { name: "Link entries", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Link existing entries" }),
  ).toHaveCount(0);
  expect((await get(root + "/transactions/" + sourceId)).data.transfer_id).toBe(
    targetId,
  );
  await page
    .getByRole("navigation", { name: "Accounts", exact: true })
    .getByRole("button", { name: /^Cash/ })
    .click();
  await page
    .getByRole("button", { name: "Transfer balance and close", exact: true })
    .click();
  await page.getByLabel("To account", { exact: true }).selectOption(card);
  await page
    .getByRole("button", { name: "Transfer and close", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transfer balance and close account" }),
  ).toHaveCount(0);
  expect(
    (await get(root + "/accounts")).data.find((a) => a.id === cash).closed,
  ).toBe(true);
  expect(
    (await get(root + "/transactions?account_id=" + cash)).balances.total,
  ).toBe(0);
  console.log(
    "PASS transfer plain HTTP create, lost acknowledgement identity, edit dates, paired deletion, existing-entry linking, account closure, desktop/mobile and keyboard",
  );
} finally {
  await browser?.close();
  await server.stop();
}

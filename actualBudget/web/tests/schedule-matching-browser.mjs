import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";

const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return response.json();
}
let browser, page;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    payee = randomUUID();
  const root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Recurring expenses" });
  await call(root + "/accounts", {
    id: account,
    name: "Checking",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await call(root + "/payees", { id: payee, name: "Weekly bill" });
  // Independently pinned weekly-exact discovery seed, schedule-matching.json.
  const dates = ["2026-10-19", "2026-10-26", "2026-11-02"];
  for (const [index, date] of dates.entries()) {
    await call(root + "/transactions", {
      id: randomUUID(),
      account_id: account,
      payee_id: payee,
      amount: -10000,
      date,
      cleared: true,
      notes: `History ${index}`,
    });
  }
  const before = await call(root + "/transactions");
  const checkpoint = await call(root + "/checkpoint");
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
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
  await page.getByRole("button", { name: "Schedules", exact: true }).click();
  await page
    .getByRole("button", { name: "Find recurring payments", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Review suggestion", exact: true }),
  ).toHaveCount(1);
  expect(await call(root + "/checkpoint")).toEqual(checkpoint);
  await page
    .getByRole("button", { name: "Review suggestion", exact: true })
    .click();
  const matches = page.getByRole("region", {
    name: "Matching payments",
    exact: true,
  });
  await expect(matches.getByRole("checkbox")).toHaveCount(3);
  for (const box of await matches.getByRole("checkbox").all())
    await box.check();
  await matches
    .getByRole("button", { name: "Use selected payments", exact: true })
    .click();
  await page.getByLabel("Schedule name", { exact: true }).fill("Weekly bill");
  expect(await call(root + "/checkpoint")).toEqual(checkpoint);
  await page
    .getByRole("button", { name: "Save schedule", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit Weekly bill", exact: true }),
  ).toBeVisible();
  const schedule = (await call(root + "/schedules")).data.schedules[0];
  let ledger = await call(root + "/transactions");
  expect(ledger.data).toHaveLength(3);
  expect(ledger.balances).toEqual(before.balances);
  expect(ledger.data.every((row) => row.schedule_id === schedule.id)).toBe(
    true,
  );
  const originalFinancial = (row) => ({
    id: row.id,
    date: row.date,
    amount: row.amount,
    account: row.account_id,
    payee: row.payee_id,
    category: row.category_id,
    notes: row.notes,
    cleared: row.cleared,
    reconciled: row.reconciled,
  });
  expect(ledger.data.map(originalFinancial)).toEqual(
    before.data.map(originalFinancial),
  );

  // Capture selection across real server pages, including existing linked rows.
  for (let index = 0; index < 38; index++) {
    await call(root + "/transactions", {
      id: randomUUID(),
      account_id: account,
      payee_id: payee,
      amount: -10000,
      date: dates[index % dates.length],
      cleared: true,
      notes: `Extra ${index}`,
    });
  }
  await page
    .getByRole("button", { name: "Refresh schedules", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Match payments", exact: true })
    .click();
  await expect(matches.getByRole("checkbox")).toHaveCount(30);
  const first = matches.getByRole("checkbox", { name: /Extra/ }).first();
  const firstLabel = await first.getAttribute("aria-label");
  await first.check();
  await matches
    .getByRole("button", { name: "Next payments", exact: true })
    .click();
  await expect(matches.getByRole("checkbox")).toHaveCount(11);
  await matches.getByRole("checkbox", { name: /Extra/ }).first().check();
  await expect(matches).toContainText("2 selected");
  await matches
    .getByRole("button", { name: "Previous payments", exact: true })
    .click();
  await expect(
    matches.getByRole("checkbox", { name: firstLabel, exact: true }),
  ).toBeChecked();
  await mkdir("test-results/schedule-matching", { recursive: true });
  await matches.getByRole("heading").scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/schedule-matching/matches-desktop.png",
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await matches.getByRole("heading").scrollIntoViewIfNeeded();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/schedule-matching/matches-mobile.png",
  });
  await page.emulateMedia({ colorScheme: "dark" });
  const largeText = await page.addStyleTag({
    content: "html { font-size: 200% !important; }",
  });
  await matches.getByRole("heading").scrollIntoViewIfNeeded();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/schedule-matching/matches-dark-large.png",
  });
  await matches.getByRole("checkbox").first().scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/schedule-matching/payment-dark-large.png",
  });
  await matches
    .getByRole("button", { name: "Review selected links", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm schedule change", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/schedule-matching/confirm-dark-large.png",
  });
  await page
    .getByRole("button", { name: "Confirm schedule change", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit Weekly bill", exact: true }),
  ).toBeEnabled();
  ledger = await call(root + "/transactions");
  expect(ledger.data).toHaveLength(41);
  expect(ledger.balances.total).toBe(-410000);
  expect(
    ledger.data.filter((row) => row.schedule_id === schedule.id),
  ).toHaveLength(5);
  await largeText.evaluate((element) => element.remove());
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 1440, height: 1000 });

  await page
    .getByRole("button", { name: "Linked payments", exact: true })
    .click();
  const linked = page.getByRole("region", {
    name: "Linked payments",
    exact: true,
  });
  await expect(linked.getByRole("checkbox")).toHaveCount(5);
  await linked.getByRole("checkbox", { name: /Extra/ }).first().check();
  await linked
    .getByRole("button", { name: "Review selected unlinks", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm schedule change", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Edit Weekly bill", exact: true }),
  ).toBeEnabled();
  const unlinked = await call(root + "/transactions");
  expect(
    unlinked.data.filter((row) => row.schedule_id === schedule.id),
  ).toHaveLength(4);
  expect(unlinked.data.map(originalFinancial)).toEqual(
    ledger.data.map(originalFinancial),
  );
  expect(unlinked.balances).toEqual(ledger.balances);

  // A stale review must retain the user's selection, without silently linking it.
  await page
    .getByRole("button", { name: "Match payments", exact: true })
    .click();
  await expect(matches.getByRole("checkbox")).toHaveCount(30);
  await matches.getByRole("checkbox", { name: /Extra/ }).first().check();
  await call(root + "/categories", {
    id: randomUUID(),
    name: "Concurrent change",
    is_income: false,
  });
  await matches
    .getByRole("button", { name: "Review selected links", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm schedule change", exact: true })
    .click();
  await expect(page.locator(".schedules [role=alert]")).toBeVisible();
  expect((await call(root + "/transactions")).data).toEqual(unlinked.data);
  await expect(
    page.getByRole("button", { name: "Retry original request", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Cancel saved request", exact: true }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  console.log(
    "Schedule discovery, selective links across pages, unlink, stale review and responsive matches: PASS",
  );
} catch (error) {
  console.error(
    await page
      ?.locator(".schedules")
      .innerText({ timeout: 1500 })
      .catch(() => "No schedule view"),
  );
  throw error;
} finally {
  await browser?.close();
  await server.stop();
}

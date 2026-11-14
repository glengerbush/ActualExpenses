import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
}
let browser, page;
try {
  const budget = randomUUID(),
    account = randomUUID();
  await call("/budgets", { id: budget, name: "Scheduled bills" });
  await call(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Checking",
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
  page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on("pageerror", (e) => {
    errors.push(e.message);
    console.error("Browser error", e.message);
  });
  await page.goto(`${server.url}/?budget=${budget}&view=schedules`);
  await expect(
    page.getByRole("heading", { name: "Scheduled transactions", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Create schedule", exact: true })
    .click();
  await page.getByLabel("Schedule name", { exact: true }).fill("Monthly rent");
  await page.getByLabel(/^Amount value 3/).fill("-12.34");
  await page
    .getByRole("button", { name: "Preview calendar", exact: true })
    .click();
  await expect(page.locator(".dates li")).toHaveCount(12);
  await mkdir("test-results/schedules", { recursive: true });
  await page.screenshot({
    path: "test-results/schedules/builder-desktop.png",
    fullPage: true,
  });
  await page.reload();
  await expect(page.getByLabel("Schedule name", { exact: true })).toHaveValue(
    "Monthly rent",
  );
  await expect(page.getByLabel(/^Amount value 3/)).toHaveValue("-12.34");
  await page
    .getByRole("button", { name: "Save schedule", exact: true })
    .click();
  await expect(page.getByText("Monthly rent", { exact: true })).toBeVisible();
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(0);
  await page.getByRole("button", { name: "Post now", exact: true }).click();
  await page
    .getByRole("button", { name: "Confirm schedule change", exact: true })
    .click();
  await expect(page.locator(".schedules [role=status]")).toContainText(
    "Schedule change saved",
  );
  const posted = await call(`/budgets/${budget}/transactions`);
  expect(posted.data).toHaveLength(1);
  expect(posted.data[0].amount).toBe(-1234);
  expect(posted.balances.total).toBe(-1234);
  await page.getByRole("button", { name: "Occurrences", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Occurrences", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Edit Monthly rent", exact: true })
    .click();
  await page.getByRole("button", { name: "Add action", exact: true }).click();
  await page.getByLabel(/^Action 1/).selectOption("set");
  await page.getByLabel(/^Set field 1/).selectOption("amount");
  await page.getByLabel(/^Set amount 1/).fill("-15.00");
  await page.getByLabel("Completed", { exact: true }).check();
  await page
    .getByRole("button", { name: "Save schedule", exact: true })
    .click();
  await expect(page.getByText(/completed\s*·\s*Manual/)).toBeVisible();
  const revised = (await call(`/budgets/${budget}/schedules`)).data
    .schedules[0];
  expect(revised.actions).toEqual([
    { op: "link-schedule", value: revised.id },
    { op: "set", field: "amount", value: -1500 },
  ]);
  await page.screenshot({
    path: "test-results/schedules/list-desktop.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Edit Monthly rent", exact: true })
    .click();
  await page
    .getByLabel("Schedule name", { exact: true })
    .fill("Draft survives remote edits");
  const before = (await call(`/budgets/${budget}/schedules`)).data;
  await call(`/budgets/${budget}/schedules/commands`, {
    action: "update",
    id: before.schedules[0].id,
    snapshot: before.snapshot,
    name: "Remote winner",
  });
  await page
    .getByRole("button", { name: "Refresh schedules", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Save schedule", exact: true })
    .click();
  await expect(page.getByRole("alert")).toBeVisible();
  expect(
    (await call(`/budgets/${budget}/schedules`)).data.schedules[0].name,
  ).toBe("Remote winner");
  await expect(page.getByLabel("Schedule name", { exact: true })).toHaveValue(
    "Draft survives remote edits",
  );
  await page
    .getByRole("button", { name: "Cancel saved request", exact: true })
    .click();
  await expect(page.locator(".schedules [role=status]")).toContainText(
    "cancelled",
  );
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Create schedule", exact: true })
    .click();
  await page
    .getByLabel("Schedule name", { exact: true })
    .fill("Calendar details");
  await page.getByLabel(/^Move weekend occurrences 1/).check();
  await page
    .getByRole("button", { name: "Add specific day 1", exact: true })
    .click();
  await page.getByLabel("Day number 1.1", { exact: true }).fill("-1");
  await page
    .getByRole("button", { name: "Preview calendar", exact: true })
    .click();
  await expect(page.locator(".dates li")).toHaveCount(12);
  await page.setViewportSize({ width: 390, height: 900 });
  await page.screenshot({
    path: "test-results/schedules/builder-mobile.png",
    fullPage: true,
  });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({
    path: "test-results/schedules/builder-large-text.png",
    fullPage: true,
  });
  await page
    .locator("#schedule-editor")
    .evaluate((node) => node.scrollIntoView({ block: "start" }));
  await page.screenshot({
    path: "test-results/schedules/builder-large-text-viewport.png",
  });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({
    path: "test-results/schedules/builder-dark-large-text.png",
    fullPage: true,
  });
  await page.screenshot({
    path: "test-results/schedules/builder-dark-large-text-viewport.png",
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => (document.documentElement.style.fontSize = "100%"));
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click();
  const current = (await call(`/budgets/${budget}/schedules/due`)).data;
  const nextDay = new Date(current.today + "T12:00:00Z");
  nextDay.setUTCDate(nextDay.getUTCDate() + 1);
  const tomorrow = nextDay.toISOString().slice(0, 10);
  for (const [name, date, horizon] of [
    [
      "Daily forecast",
      { start: current.today, frequency: "daily", endMode: "never" },
      "14",
    ],
    ["Tomorrow forecast", tomorrow, "14"],
  ]) {
    const cp = (await call(`/budgets/${budget}/schedules/due`)).data;
    await call(`/budgets/${budget}/schedules/commands`, {
      action: "create",
      id: randomUUID(),
      name,
      custom_upcoming_length: horizon,
      snapshot: cp.snapshot,
      conditions: [
        { field: "account", op: "is", value: account },
        { field: "amount", op: "is", value: -100 },
        { field: "date", op: "is", value: date },
      ],
    });
  }
  await page
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  const upcoming = page.locator(
    'section[aria-labelledby="scheduled-preview-title"]',
  );
  await expect(upcoming.getByText(/Tomorrow forecast/)).toBeVisible();
  const forecastDates = (await upcoming.locator("li").allTextContents()).map(
    (text) => text.trim().slice(0, 10),
  );
  expect(forecastDates).toEqual([...forecastDates].sort());
  expect((await call(`/budgets/${budget}/transactions`)).balances.total).toBe(
    -1234,
  );
  expect(errors).toEqual([]);
  console.log(
    "Schedule creation, retained drafts, posting, completion, stale review and responsive calendars: PASS",
  );
} catch (e) {
  console.error(
    await page
      ?.locator(".schedules")
      .innerText({ timeout: 1500 })
      .catch(() => "No schedule view"),
  );
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

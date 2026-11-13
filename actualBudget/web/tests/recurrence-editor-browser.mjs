import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function request(path, body, method = "POST", headers = {}) {
  return fetch(base + path, {
    method: body === undefined ? "GET" : method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}
async function call(path, body, method, headers) {
  const r = await request(path, body, method, headers);
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    id = randomUUID();
  const root = "/budgets/" + budget;
  const today = new Date().toISOString().slice(0, 10),
    yesterday = new Date(Date.now() - 86400000).toISOString().slice(0, 10);
  await call("/budgets", { id: budget, name: "Recurring editor" });
  await call(root + "/accounts", {
    id: account,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const recurrence = {
    enabled: true,
    config: {
      start: yesterday,
      frequency: "daily",
      interval: 1,
      endMode: "never",
    },
  };
  const body = {
    id,
    account_id: account,
    date: yesterday,
    amount: -1200,
    notes: "Daily coffee",
    recurrence,
  };
  const key = randomUUID();
  const created = await call(root + "/transactions", body, "POST", {
    "Idempotency-Key": key,
  });
  expect(created.recurrence).toEqual(recurrence);
  expect(
    (
      await call(root + "/transactions", body, "POST", {
        "Idempotency-Key": key,
      })
    ).schedule_id,
  ).toBe(created.schedule_id);
  let schedules = await call(root + "/schedules");
  expect(schedules.schedules).toHaveLength(1);
  expect(schedules.schedules[0].next_date).toBe(today);
  await call(root + "/schedules/commands", {
    action: "catch-up",
    snapshot: schedules.snapshot,
  });
  const transactions = await call(root + "/transactions");
  expect(transactions).toHaveLength(2);
  const repeated = transactions.find((t) => t.id !== id);
  expect(repeated.notes).toBe("Daily coffee");
  expect(repeated.amount).toBe(-1200);
  schedules = await call(root + "/schedules");
  await call(root + "/schedules/commands", {
    action: "catch-up",
    snapshot: schedules.snapshot,
  });
  expect(await call(root + "/transactions")).toHaveLength(2);
  const ruleSnapshot = (await call(root + "/schedules")).snapshot;
  const explicit = await call(root + "/rules/transactions", {
    ...body,
    id: randomUUID(),
    snapshot: ruleSnapshot,
  });
  expect(explicit.schedule_id).not.toBe(created.schedule_id);
  expect(explicit.recurrence.enabled).toBe(true);
  await call(
    root + "/transactions/" + id,
    { recurrence: { ...recurrence, enabled: false } },
    "PATCH",
  );
  expect((await call(root + "/transactions/" + id)).recurrence.enabled).toBe(
    false,
  );
  const invalidId = randomUUID();
  expect(
    (
      await request(root + "/transactions", {
        ...body,
        id: invalidId,
        recurrence: {
          ...recurrence,
          config: { ...recurrence.config, interval: 0 },
        },
      })
    ).ok,
  ).toBe(false);
  expect((await request(root + "/transactions/" + invalidId)).status).toBe(404);
  const oneOff = randomUUID(),
    oneOffSchedule = randomUUID();
  await call(root + "/transactions", {
    id: oneOff,
    account_id: account,
    date: today,
    amount: -99,
    notes: "One-time payment",
  });
  await call(root + "/schedules/commands", {
    action: "create",
    id: oneOffSchedule,
    snapshot: (await call(root + "/schedules")).snapshot,
    posts_transaction: false,
    conditions: [
      { field: "date", op: "is", value: "2030-12-01" },
      { field: "account", op: "is", value: account },
      { field: "amount", op: "is", value: -99 },
    ],
    transaction_ids: [oneOff],
  });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
      viewport: { width: 1440, height: 1000 },
    }),
    page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByText("Recurring editor", { exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Schedules", exact: true }),
  ).toHaveCount(0);
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "One-time payment" })
    .click();
  await expect(
    page.getByRole("switch", { name: "Recurring", exact: true }),
  ).not.toBeChecked();
  await page
    .getByLabel("Notes", { exact: true })
    .fill("One-time payment edited");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".register-table")).toContainText(
    "One-time payment edited",
  );
  expect((await call(root + "/transactions/" + oneOff)).recurrence.config).toBe(
    "2030-12-01",
  );
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(
    page.getByRole("switch", { name: "Recurring", exact: true }),
  ).not.toBeChecked();
  await expect(
    page.getByRole("combobox", { name: "Frequency", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Amount", { exact: true }).fill("25");
  await page.getByLabel("Notes", { exact: true }).fill("Monthly membership");
  await page.getByRole("switch", { name: "Recurring", exact: true }).check();
  await expect(
    page.getByRole("combobox", { name: "Frequency", exact: true }),
  ).toHaveValue("monthly");
  await page
    .getByRole("combobox", { name: "Ends", exact: true })
    .selectOption("after_n_occurrences");
  await page.getByLabel("Occurrences", { exact: true }).fill("6");
  await mkdir("test-results/recurrence", { recursive: true });
  await page
    .locator(".transaction-form")
    .screenshot({ path: "test-results/recurrence/desktop.png" });
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  let row = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Monthly membership" });
  await expect(row).toBeVisible();
  await row.click();
  await expect(
    page.getByRole("switch", { name: "Recurring", exact: true }),
  ).toBeChecked();
  await expect(page.getByLabel("Occurrences", { exact: true })).toHaveValue(
    "6",
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page
    .locator(".inline-editor")
    .screenshot({ path: "test-results/recurrence/mobile.png" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByRole("switch", { name: "Recurring", exact: true }).uncheck();
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await row.click();
  await expect(
    page.getByRole("switch", { name: "Recurring", exact: true }),
  ).not.toBeChecked();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  const membership = (await call(root + "/transactions")).find(
    (t) => t.notes === "Monthly membership",
  );
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect
    .poll(
      async () =>
        (await call(root + "/transactions/" + membership.id)).recurrence
          .enabled,
    )
    .toBe(true);
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect
    .poll(
      async () =>
        (await call(root + "/transactions/" + membership.id)).recurrence
          .enabled,
    )
    .toBe(false);
  // Queue one recurring expense offline, reload, deliver exactly once on reconnect.
  await page.route("**/api/v1/**", (route) =>
    route.abort("internetdisconnected"),
  );
  await page.evaluate(() => window.dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("3");
  await page.getByLabel("Notes", { exact: true }).fill("Offline recurring");
  await page.getByRole("switch", { name: "Recurring", exact: true }).check();
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page
      .locator(".register-table")
      .getByText("Offline recurring", { exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(page.locator(".register-table")).toContainText(
    "Offline recurring",
  );
  await page.unroute("**/api/v1/**");
  await page.evaluate(() => window.dispatchEvent(new Event("online")));
  await expect
    .poll(
      async () =>
        (await call(root + "/transactions")).filter(
          (t) => t.notes === "Offline recurring",
        ).length,
    )
    .toBe(1);
  const offline = (await call(root + "/transactions")).find(
    (t) => t.notes === "Offline recurring",
  );
  expect(offline.recurrence.enabled).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "PASS atomic recurrence, replay, catch-up, stop, rollback, editor reopen, mobile, offline delivery",
  );
} finally {
  await browser?.close();
  await server.stop();
}

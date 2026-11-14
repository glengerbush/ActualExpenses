import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
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
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser, page;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    food = randomUUID(),
    income = randomUUID();
  await call("/budgets", {
    id: budget,
    name: "Monthly planning",
    budget_type: "tracking",
  });
  await call(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Cash",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const [id, name, is_income] of [
    [food, "Food", false],
    [income, "Salary", true],
  ])
    await call(`/budgets/${budget}/categories`, { id, name, is_income });
  for (const [amount, category_id] of [
    [50000, income],
    [-12000, food],
    [2000, food],
  ])
    await call(`/budgets/${budget}/transactions`, {
      id: randomUUID(),
      account_id: account,
      amount,
      category_id,
      date: "2026-01-02",
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
  });
  page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
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
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel("Budget month", { exact: true }).fill("Jan-26");
  await page
    .getByLabel("Budget month", { exact: true })
    .dispatchEvent("change");
  await expect(page.getByTestId("2026-01-real_saved")).toHaveText("400.00");
  // An overspent total must not look like a positive balance.
  await expect(page.locator(".summary .balance-badge")).toHaveClass(/negative/);
  await expect(
    page.getByRole("button", {
      name: "Budget",
      exact: true,
    }),
  ).toBeVisible();
  await page.getByLabel("Months displayed").selectOption("2");
  const nextMonth = page.locator(".budget-month").nth(1);
  await nextMonth.locator(".month-metrics summary").click();
  await expect(nextMonth.getByTestId("2026-02-real_saved")).toBeVisible();
  await expect(
    nextMonth.getByTestId("2026-02-total_budget_income"),
  ).toBeVisible();
  await page.getByLabel("Months displayed").selectOption("1");
  const amountInput = () =>
    page.getByLabel("Planned Food for January 2026", { exact: true });
  // Current entry autosaves inline; released durable drafts remain recoverable.
  await amountInput().fill("100.00");
  await amountInput().press("Enter");
  await expect(page.getByTestId("2026-01-total_budgeted")).toHaveText("100.00");
  await expect(page.locator(".summary .balance-badge")).not.toHaveClass(
    /negative/,
  );
  async function savedDraft(amount) {
    const plan = await call(`/budgets/${budget}/budget?month=2026-01`);
    await page.evaluate(
      ({ budget, food, epoch, amount }) =>
        localStorage.setItem(
          `actualBudget.planning.v1.${budget}`,
          JSON.stringify({
            version: 1,
            epoch,
            month: "2026-01",
            action: "set",
            category: food,
            amount,
            method: "tracking",
            enabled: true,
            format: { decimal_places: 2, number_format: "comma-dot" },
            pending: null,
          }),
        ),
      { budget, food, epoch: plan.epoch, amount },
    );
    await page.reload();
    await expect(
      page.getByRole("region", { name: "Saved budget change", exact: true }),
    ).toBeVisible();
  }
  const save = () =>
    page.getByRole("button", {
      name: "Retry saved budget change",
      exact: true,
    });
  await page
    .getByRole("button", { name: "Edit Food for January 2026", exact: true })
    .click();
  await page
    .getByRole("button", { name: "View transactions", exact: true })
    .click();
  await expect(
    page.locator(".register-table tr[data-transaction-id]"),
  ).toHaveCount(2);
  await expect(page.locator(".register-table")).toContainText("Food");
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await savedDraft("150.00");
  let captured, delivered;
  await page.route("**/api/v1/budgets/*/budget", async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    captured = {
      body: route.request().postData(),
      headers: route.request().headers(),
    };
    // Simulate native delivery through the public seam. Preserve all receipt
    // identity headers; browser Origin cannot accompany the IP transport URL.
    const response = await fetch(
      route.request().url().replace("actualbudget.test", "127.0.0.1"),
      {
        method: "POST",
        headers: Object.fromEntries(
          [
            "content-type",
            "idempotency-key",
            "x-actual-edit-clock",
            "x-actual-budget-epoch",
            "x-actual-history-owner",
          ].map((name) => [name, captured.headers[name]]),
        ),
        body: captured.body,
      },
    );
    delivered = { status: response.status, body: await response.text() };
    await route.abort("failed");
    await page.unroute("**/api/v1/budgets/*/budget");
  });
  await save().click();
  await expect(
    page.getByRole("button", {
      name: "Retry saved budget request",
      exact: true,
    }),
  ).toBeVisible();
  await expect.poll(() => delivered?.status).not.toBeUndefined();
  expect(delivered.status, delivered.body).toBe(200);

  await page.reload();
  await expect(page.getByTestId("2026-01-real_saved")).toHaveText("400.00");
  let retry;
  await page.route("**/api/v1/budgets/*/budget", async (route) => {
    if (route.request().method() === "POST")
      retry = {
        body: route.request().postData(),
        headers: route.request().headers(),
      };
    await route.continue();
  });
  await page
    .getByRole("button", { name: "Retry saved budget request", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Edit allocation", exact: true }),
  ).toHaveCount(0);
  expect(retry.body).toBe(captured.body);
  for (const h of [
    "idempotency-key",
    "x-actual-edit-clock",
    "x-actual-budget-epoch",
    "x-actual-history-owner",
  ])
    expect(retry.headers[h]).toBe(captured.headers[h]);
  await page.unroute("**/api/v1/budgets/*/budget");
  await expect(page.getByTestId("2026-01-real_saved")).toHaveText("400.00");
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByTestId("2026-01-total_budgeted")).toHaveText("100.00");
  // Quota errors preserve typed values and block transmission.
  await savedDraft("80.00");
  await page.evaluate(() => {
    window.__planningSet = Storage.prototype.setItem;
    Storage.prototype.setItem = function (k, v) {
      if (k.startsWith("actualBudget.planning."))
        throw new DOMException("quota", "QuotaExceededError");
      return window.__planningSet.call(this, k, v);
    };
  });

  await save().click();
  await expect(page.getByRole("alert")).toContainText("quota");
  expect(
    (await call(`/budgets/${budget}/budget?month=2026-01`)).months[
      "2026-01"
    ].categories.find((c) => c.id === food).budgeted,
  ).toBe(10000);
  await page.evaluate(() => (Storage.prototype.setItem = window.__planningSet));
  // Independent draft changes fence this form before any request dispatch.
  await page.evaluate((budget) => {
    const k = `actualBudget.planning.v1.${budget}`,
      d = JSON.parse(localStorage.getItem(k));
    d.amount = "90.00";
    localStorage.setItem(k, JSON.stringify(d));
  }, budget);
  await save().click();
  await expect(page.getByRole("alert")).toContainText("Another tab");
  await page.reload();
  expect(
    await page.evaluate(
      (b) =>
        JSON.parse(localStorage.getItem(`actualBudget.planning.v1.${b}`))
          .amount,
      budget,
    ),
  ).toBe("90.00");
  await page.route("**/api/v1/budgets/*/budget", async (route) =>
    route.request().method() === "POST"
      ? route.abort("failed")
      : route.continue(),
  );
  await save().click();
  await expect(
    page.getByRole("button", {
      name: "Cancel saved budget request safely",
      exact: true,
    }),
  ).toBeVisible();
  await page.unroute("**/api/v1/budgets/*/budget");
  await page
    .getByRole("button", {
      name: "Cancel saved budget request safely",
      exact: true,
    })
    .click();
  await expect(
    page.getByText(
      "The saved request was canceled. Review the retained fields before submitting a new plan.",
      { exact: true },
    ),
  ).toBeVisible();
  expect(
    await page.evaluate(
      (b) =>
        JSON.parse(localStorage.getItem(`actualBudget.planning.v1.${b}`))
          .amount,
      budget,
    ),
  ).toBe("90.00");
  expect(
    (await call(`/budgets/${budget}/budget?month=2026-01`)).months[
      "2026-01"
    ].categories.find((c) => c.id === food).budgeted,
  ).toBe(10000);
  await page
    .getByRole("button", { name: "Discard saved change", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Saved budget change", exact: true }),
  ).toHaveCount(0);
  await mkdir("../../test-results/budgeting", { recursive: true });
  await page.screenshot({
    path: "../../test-results/budgeting/desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "../../test-results/budgeting/mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  const region = page.getByRole("region", {
    name: "Budget categories for January 2026",
    exact: true,
  });
  await region.focus();
  await page.keyboard.press("End");
  await expect(
    page.getByRole("button", {
      name: "Edit Food for January 2026",
      exact: true,
    }),
  ).toBeInViewport();
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({
    path: "../../test-results/budgeting/dark-mobile.png",
    fullPage: true,
  });
  await page.addStyleTag({ content: "html{font-size:200%}" });
  for (const name of ["Previous month", "Next month"]) {
    const size = await page
      .getByRole("button", { name, exact: true })
      .evaluate((el) => ({ width: el.clientWidth, content: el.scrollWidth }));
    expect(size.content).toBeLessThanOrEqual(size.width);
  }
  await page.screenshot({
    path: "../../test-results/budgeting/large-text.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByLabel("Budget month", { exact: true })
    .screenshot({ path: "../../test-results/budgeting/large-text-month.png" });
  await page.screenshot({
    path: "../../test-results/budgeting/large-text-viewport.png",
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS tracking budget: filtered transactions, released draft recovery, immutable lost-ack retry, undo, quota/ownership/cancellation, responsive view",
  );
} catch (e) {
  if (page) {
    await mkdir("../../test-results/budgeting", { recursive: true });
    await writeFile(
      "../../test-results/budgeting/failure.html",
      await page.content(),
    );
    await page.screenshot({
      path: "../../test-results/budgeting/failure.png",
      fullPage: true,
    });
  }
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

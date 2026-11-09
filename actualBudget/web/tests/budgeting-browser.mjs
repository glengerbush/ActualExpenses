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
    budget_type: "envelope",
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
  async function remoteMethod(method) {
    const current = await call(`/budgets/${budget}/budget?month=2026-01`);
    await call(`/budgets/${budget}/budget`, {
      action: "method",
      month: "2026-01",
      method,
      snapshot: current.snapshot,
    });
  }
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel("Budget month", { exact: true }).fill("2026-01");
  await page
    .getByLabel("Budget month", { exact: true })
    .dispatchEvent("change");
  await expect(page.getByTestId("2026-01-to_budget")).toHaveText("500.00");
  const edit = () =>
      page.getByRole("button", {
        name: "Edit Food allocation for January 2026",
        exact: true,
      }),
    save = () => page.getByRole("button", { name: "Save plan", exact: true });
  await edit().click();
  await page.getByLabel("Amount", { exact: true }).fill("100.00");
  await page.reload();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue(
    "100.00",
  );
  await save().click();
  await expect(
    page.getByRole("heading", { name: "Edit allocation", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByTestId("2026-01-to_budget")).toHaveText("400.00");
  await page
    .getByRole("button", {
      name: "View Food transactions for January 2026",
      exact: true,
    })
    .click();
  await expect(
    page.getByText("2 contributing transactions · -100.00", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Close category detail", exact: true })
    .click();
  await edit().click();
  await page.getByLabel("Amount", { exact: true }).fill("150.00");
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
  await remoteMethod("tracking");
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
  await remoteMethod("envelope");
  await page
    .getByRole("button", { name: "Refresh budget", exact: true })
    .click();
  await expect(page.getByTestId("2026-01-to_budget")).toHaveText("350.00");
  await page.getByRole("button", { name: /^Undo / }).click();
  await save().click();
  await expect(page.getByTestId("2026-01-to_budget")).toHaveText("400.00");
  await page
    .getByRole("button", { name: "Change budgeting method", exact: true })
    .click();
  await page
    .getByLabel("Budgeting method", { exact: true })
    .selectOption("tracking");
  await save().click();
  await expect(page.getByTestId("2026-01-real_saved")).toHaveText("400.00");
  await edit().click();
  await page.getByLabel("Amount", { exact: true }).fill("60.00");
  await save().click();
  await expect(page.getByTestId("2026-01-total_budgeted")).toHaveText("60.00");
  await edit().click();
  await page.getByLabel("Amount", { exact: true }).fill("123.00");
  await remoteMethod("envelope");
  await page
    .getByRole("button", { name: "Refresh budget", exact: true })
    .click();
  await expect(page.getByTestId("2026-01-to_budget")).toHaveText("400.00");
  for (let attempt = 0; attempt < 2; attempt++) {
    if (attempt) await page.reload();
    await expect(page.getByLabel("Amount", { exact: true })).toHaveValue(
      "123.00",
    );
    await save().click();
    await expect(page.getByRole("alert")).toContainText(
      "draft still belongs to Tracking",
    );
    const current = await call(`/budgets/${budget}/budget?month=2026-01`);
    expect(current.method).toBe("envelope");
    expect(
      current.months["2026-01"].categories.find((c) => c.id === food).budgeted,
    ).toBe(10000);
    const saved = await page.evaluate(
      (budget) =>
        JSON.parse(localStorage.getItem(`actualBudget.planning.v1.${budget}`)),
      budget,
    );
    expect(saved.method).toBe("tracking");
    expect(saved.pending).toBeNull();
  }
  await page
    .getByRole("button", { name: "Discard budget draft", exact: true })
    .click();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Retry connection", exact: true }),
  ).toHaveCount(0);
  // Quota errors preserve typed values and block transmission.
  await edit().click();
  await page.evaluate(() => {
    window.__planningSet = Storage.prototype.setItem;
    Storage.prototype.setItem = function (k, v) {
      if (k.startsWith("actualBudget.planning."))
        throw new DOMException("quota", "QuotaExceededError");
      return window.__planningSet.call(this, k, v);
    };
  });
  await page.getByLabel("Amount", { exact: true }).fill("80.00");
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("80.00");
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
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("90.00");
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
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("90.00");
  expect(
    (await call(`/budgets/${budget}/budget?month=2026-01`)).months[
      "2026-01"
    ].categories.find((c) => c.id === food).budgeted,
  ).toBe(10000);
  await page
    .getByRole("button", { name: "Discard budget draft", exact: true })
    .click();
  await expect(page.getByRole("alert")).toHaveCount(0);
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
    page.getByRole("button", { name: "Edit Food notes", exact: true }),
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
    "PASS budgeting: both methods, actual leaf drilldown, reload draft, immutable lost-ack retry, undo, retained method allocations, responsive view",
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

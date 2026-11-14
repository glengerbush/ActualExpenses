import {
  savedWork,
  expectSavedWork,
  openSavedWork,
  history,
} from "./saved-work-ui.mjs";
import { transactionRows, transactionAction } from "./register-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer, transactionCreatePattern } from "./server.mjs";
const artifacts = fileURLToPath(new URL("../test-results/", import.meta.url));
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
const budget = randomUUID(),
  account = randomUUID();
async function create(path, body) {
  const response = await fetch(`${api}/api/v1${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(response.status).toBe(201);
  return (await response.json()).data;
}
await create("/budgets", { id: budget, name: "Register browser" });
await create(`/budgets/${budget}/accounts`, {
  id: account,
  name: "Spending",
  opening_balance: 100000,
  opening_date: "2026-01-01",
});
let browser;
try {
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
  const page = await context.newPage();
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
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("10+2.34");
  await page.getByLabel("Date", { exact: true }).fill("01/02/2026");
  await page.getByRole("combobox", { name: "Payee", exact: true }).fill("Café");
  await page.getByRole("option", { name: /Create payee/ }).click();
  await page.getByLabel("Category", { exact: true }).selectOption("new");
  await page.getByLabel("New category name").fill("Food");
  await page.reload();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue(
    "10+2.34",
  );
  await expect(
    page.getByRole("combobox", { name: "Payee", exact: true }),
  ).toHaveValue("Café");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("987.66");
  let row = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Café" });
  await expect(row).toContainText("Food");
  await expect(row).toContainText("Café");
  await transactionRows(row, { name: /^Edit / }).click();
  await page.getByLabel("Date", { exact: true }).fill("01/04/2026");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("987.66");
  row = page.locator(".register-table tbody tr").filter({ hasText: "Café" });
  await (await transactionAction(row, { name: /^Duplicate / })).click();
  await expect(page.getByTestId("register-total")).toHaveText("975.32");
  await expect(page.getByTestId("register-cleared")).toHaveCount(0);
  await expect(page.getByTestId("register-uncleared")).toHaveCount(0);
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Café" })
    .first()
    .getByRole("button", { name: /^Delete / })
    .click();
  await expect(page.getByText(/Delete this transaction\?/)).toContainText(
    "Café",
  );
  await page
    .getByRole("button", { name: "Confirm delete", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("987.66");
  // Lose the response after a real commit, reload, then replay the same saved operation.
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("2.34");
  await page.getByRole("radio", { name: "Income", exact: true }).check();
  await page.getByLabel("Date", { exact: true }).fill("01/03/2026");
  let dropped = false;
  await page.route(transactionCreatePattern(budget), async (route) => {
    if (!dropped && route.request().method() === "POST") {
      dropped = true;
      const target = new URL(route.request().url());
      const headers = { ...route.request().headers(), host: target.host };
      target.hostname = "127.0.0.1";
      const response = await route.fetch({ url: target.href, headers });
      expect(response.ok()).toBe(true);
      await route.abort("failed");
    } else await route.continue();
  });
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expectSavedWork(page, 1);

  await page.reload();
  await expectSavedWork(page, 0);
  await expect(page.getByTestId("register-total")).toHaveText("990.00");
  const rows = await (
    await fetch(
      `${api}/api/v1/budgets/${budget}/transactions?account_id=${account}`,
    )
  ).json();
  expect(rows.data.filter((row) => row.amount === 234)).toHaveLength(1);

  // Two tabs cannot replace an unconfirmed edit; quota failures retain its original operation.
  let refundRow = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "+2.34" });
  await transactionRows(refundRow, { name: /^Edit / }).click();
  await page.getByLabel("Date", { exact: true }).fill("01/05/2026");
  const second = await context.newPage();
  await second.goto(server.url);
  await expect(second.getByLabel("Date", { exact: true })).toHaveValue(
    "01/05/2026",
  );
  await second.route("**/api/v1/**", (route) => route.abort());
  await second.evaluate(() => dispatchEvent(new Event("offline")));
  let editAttempts = [];
  let lostEdit = false;
  await page.route(
    new RegExp(`/api/v1/budgets/${budget}/transactions/[^/]+$`),
    async (route) => {
      if (route.request().method() !== "PATCH") {
        await route.continue();
        return;
      }
      editAttempts.push(route.request().headers()["idempotency-key"]);
      if (!lostEdit) {
        lostEdit = true;
        const target = new URL(route.request().url());
        const headers = { ...route.request().headers(), host: target.host };
        target.hostname = "127.0.0.1";
        const response = await route.fetch({ url: target.href, headers });
        expect(response.ok()).toBe(true);
        await route.abort("failed");
      } else await route.continue();
    },
  );
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expectSavedWork(page, 1);
  await expect(second.getByLabel("Date", { exact: true })).toBeDisabled();
  await expect(
    second
      .getByRole("alert")
      .filter({ hasText: "Another tab saved this draft" }),
  ).toBeVisible();
  await second.close();
  await page.evaluate(() => {
    window.originalOfflineTransaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (...args) {
      if (this.name === "actualBudget.offline.v1" && args[1] === "readwrite")
        throw new DOMException("Full", "QuotaExceededError");
      return window.originalOfflineTransaction.apply(this, args);
    };
  });
  await openSavedWork(page);
  await page
    .getByRole("button", { name: "Retry saved expense", exact: true })
    .click();
  await expect(
    page.getByRole("alert").filter({ hasText: "Free device space" }).first(),
  ).toBeVisible();
  expect(editAttempts).toHaveLength(1);
  await page.evaluate(() => {
    IDBDatabase.prototype.transaction = window.originalOfflineTransaction;
  });
  await history(page);
  await page.reload();
  await expectSavedWork(page, 0);
  await expect(
    page.locator(".register-table tbody tr").filter({ hasText: "01/05/2026" }),
  ).toHaveCount(1);
  expect(new Set(editAttempts).size).toBe(1);
  // Duplicate and deletion acknowledgements can also be recovered after a reload.
  for (const action of ["duplicate", "delete"]) {
    let attempts = [];
    let droppedAction = false;
    const predicate = (url) =>
      url.pathname.startsWith(`/api/v1/budgets/${budget}/transactions/`) &&
      (action === "duplicate"
        ? url.pathname.endsWith("/duplicate")
        : !url.pathname.endsWith("/duplicate"));
    await page.route(predicate, async (route) => {
      const expected = action === "duplicate" ? "POST" : "DELETE";
      if (route.request().method() !== expected) {
        await route.continue();
        return;
      }
      attempts.push(route.request().headers()["idempotency-key"]);
      if (!droppedAction) {
        droppedAction = true;
        const target = new URL(route.request().url());
        const headers = { ...route.request().headers(), host: target.host };
        target.hostname = "127.0.0.1";
        const response = await route.fetch({ url: target.href, headers });
        expect(response.ok()).toBe(true);
        await route.abort("failed");
      } else await route.continue();
    });
    const row = page
      .locator(".register-table tbody tr")
      .filter({ hasText: "01/05/2026" })
      .first();
    await (
      action === "duplicate"
        ? await transactionAction(row, { name: /^Duplicate / })
        : row.getByRole("button", { name: /^Delete / })
    ).click();
    if (action === "delete")
      await page
        .getByRole("button", { name: "Confirm delete", exact: true })
        .click();
    await expectSavedWork(page, 1);
    await expect(
      page
        .getByText(
          "Cannot reach the server. Check your connection and retry.",
          { exact: true },
        )
        .first(),
    ).toBeVisible();
    await page.reload();
    await expectSavedWork(page, 0);
    await expect(
      page.getByRole("button", { name: "Retry pending expense" }),
    ).toHaveCount(0);
    expect(new Set(attempts).size).toBe(1);
    await page.unroute(predicate);
  }
  await page.evaluate(
    (budget) =>
      localStorage.setItem(
        `actualBudget.register-action.v1.${budget}`,
        JSON.stringify({
          origin: location.origin,
          budgetId: budget,
          pending: { method: "PUT", path: "/wrong", key: "invalid" },
        }),
      ),
    budget,
  );
  await page.reload();
  await expect(page.getByRole("alert")).toContainText("could not be validated");
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeDisabled();
  await page.evaluate(
    (budget) =>
      localStorage.removeItem(`actualBudget.register-action.v1.${budget}`),
    budget,
  );
  await page.getByRole("button", { name: "Retry saved work recovery" }).click();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  // Exercise real locale/currency normalization, rather than feeding pre-normalized API strings.
  for (const [number_format, expression, expected] of [
    ["comma-dot", "$1,222.45 + $2.55", "1225.00"],
    ["space-comma", "€1\u202f222,45 + 2,55", "1225,00"],
    ["apostrophe-dot", "1’222.45 + 2.55", "1225.00"],
  ]) {
    const response = await fetch(
      `${api}/api/v1/budgets/${budget}/preferences`,
      {
        method: "PATCH",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": randomUUID(),
        },
        body: JSON.stringify({ number_format }),
      },
    );
    expect(response.ok).toBe(true);
    await page.reload();
    // Cached views are intentionally usable before refresh. Wait until the new
    // server format is visible before opening a form, which captures its format.
    await expect(page.getByTestId("register-total")).toContainText(
      number_format === "space-comma" ? /,\d{2}$/ : /\.\d{2}$/,
    );
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill(expression);
    await page.getByRole("button", { name: "Calculate", exact: true }).click();
    await expect(page.getByLabel("Amount", { exact: true })).toHaveValue(
      expected,
    );
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
  }
  const hidden = await fetch(`${api}/api/v1/budgets/${budget}/preferences`, {
    method: "PATCH",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify({ hide_fraction: true, number_format: "comma-dot" }),
  });
  expect(hidden.ok).toBe(true);
  await page.reload();
  await transactionRows(
    page.locator(".register-table tbody tr").filter({ hasText: "01/05/2026" }),
    { name: /^Edit / },
  ).click();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("2.34");
  await page.getByLabel("Date", { exact: true }).fill("01/06/2026");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.locator(".register-table tbody tr").filter({ hasText: "01/06/2026" }),
  ).toHaveCount(1);
  const afterHidden = await (
    await fetch(
      `${api}/api/v1/budgets/${budget}/transactions?account_id=${account}`,
    )
  ).json();
  expect(afterHidden.data.find((row) => row.date === "2026-01-06").amount).toBe(
    234,
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1/0");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("1/0");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.locator('summary[aria-label="Filters and columns"]').click();
  await page.getByLabel("Show through date").fill("01/01/2026");
  await page.getByRole("button", { name: "Apply date", exact: true }).click();
  await expect(page.getByTestId("register-total")).toHaveText("1,000");
  await expect(page.locator(".register-table tbody tr")).toHaveCount(1);
  await page.getByRole("button", { name: "All dates", exact: true }).click();
  await expect(page.getByTestId("register-total")).toHaveText("990");
  await mkdir(artifacts, { recursive: true });
  await page.screenshot({
    path: artifacts + "register-mobile.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.screenshot({
    path: artifacts + "register-desktop.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.screenshot({
    path: artifacts + "register-form.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.addStyleTag({ content: "html{font-size:200% !important}" });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: artifacts + "register-mobile-large-text.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: artifacts + "register-form-large-text.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS register CRUD, calculator, references, completed entries, exact balances, draft/retry recovery, mobile and failure retention",
  );
} finally {
  await browser?.close();
  await server.stop();
}

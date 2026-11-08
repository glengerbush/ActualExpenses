import { chromium, expect } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
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
  await page.getByLabel("Payee", { exact: true }).selectOption("new");
  await page.getByLabel("New payee name").fill("Café");
  await page.getByLabel("Category", { exact: true }).selectOption("new");
  await page.getByLabel("New category name").fill("Food");
  await page.getByLabel("Notes", { exact: true }).fill("Receipt");
  await page.getByLabel("Cleared", { exact: true }).check();
  await page.reload();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue(
    "10+2.34",
  );
  await expect(page.getByLabel("New payee name")).toHaveValue("Café");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("987.66");
  let row = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Receipt" });
  await expect(row).toContainText("Food");
  await expect(row).toContainText("Café");
  await row.getByRole("button", { name: /^Edit / }).click();
  await page.getByLabel("Notes", { exact: true }).fill("Edited receipt");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("987.66");
  row = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Edited receipt" });
  await row.getByRole("button", { name: /^Duplicate / }).click();
  await expect(page.getByTestId("register-total")).toHaveText("975.32");
  await expect(page.getByTestId("register-cleared")).toHaveText("987.66");
  await expect(page.getByTestId("register-uncleared")).toHaveText("-12.34");
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Edited receipt" })
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
  await page.getByLabel("Transaction type").selectOption("income");
  await page.getByLabel("Date", { exact: true }).fill("01/03/2026");
  await page.getByLabel("Notes", { exact: true }).fill("Refund");
  let dropped = false;
  await page.route(
    `**/api/v1/budgets/${budget}/transactions`,
    async (route) => {
      if (!dropped && route.request().method() === "POST") {
        dropped = true;
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
  await expect(
    page.getByRole("button", { name: "Retry saved transaction" }),
  ).toBeVisible();
  await page.reload();
  await expect(page.getByLabel("Amount", { exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Retry saved transaction" }).click();
  await expect(page.getByTestId("register-total")).toHaveText("990.00");
  const rows = await (
    await fetch(
      `${api}/api/v1/budgets/${budget}/transactions?account_id=${account}`,
    )
  ).json();
  expect(rows.data.filter((row) => row.notes === "Refund")).toHaveLength(1);

  // Two tabs cannot replace an unconfirmed edit; quota failures retain its original operation.
  let refundRow = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Refund" });
  await refundRow.getByRole("button", { name: /^Edit / }).click();
  await page.getByLabel("Notes", { exact: true }).fill("Edit survives");
  const second = await context.newPage();
  await second.goto(server.url);
  await expect(second.getByLabel("Notes", { exact: true })).toHaveValue(
    "Edit survives",
  );
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
  await expect(
    page.getByRole("button", { name: "Retry saved transaction" }),
  ).toBeVisible();
  await second.getByLabel("Notes", { exact: true }).fill("Conflicting tab");
  await expect(second.getByRole("alert")).toContainText("unconfirmed");
  await second
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(second.getByRole("alert")).toContainText("unconfirmed");
  expect(
    await second.evaluate(
      (budget) =>
        JSON.parse(
          localStorage.getItem(`actualBudget.transaction.v1.${budget}`),
        ).pending.body.notes,
      budget,
    ),
  ).toBe("Edit survives");
  await second.close();
  await page.evaluate(() => {
    window.originalStorageSet = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key.startsWith("actualBudget.transaction.v1."))
        throw new Error("Simulated storage failure");
      return window.originalStorageSet.call(this, key, value);
    };
  });
  await page.getByRole("button", { name: "Retry saved transaction" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Simulated storage failure",
  );
  expect(editAttempts).toHaveLength(1);
  await page.evaluate(
    () => (Storage.prototype.setItem = window.originalStorageSet),
  );
  await page.reload();
  await page.getByRole("button", { name: "Retry saved transaction" }).click();
  await expect(
    page
      .locator(".register-table tbody tr")
      .filter({ hasText: "Edit survives" }),
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
      .filter({ hasText: "Edit survives" })
      .first();
    await row
      .getByRole("button", {
        name: action === "duplicate" ? /^Duplicate / : /^Delete /,
      })
      .click();
    if (action === "delete")
      await page
        .getByRole("button", { name: "Confirm delete", exact: true })
        .click();
    await expect(
      page.getByRole("button", { name: "Retry saved action" }),
    ).toBeVisible();
    await page.reload();
    await page.getByRole("button", { name: "Retry saved action" }).click();
    await expect(
      page.getByRole("button", { name: "Retry saved action" }),
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
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Edit survives" })
    .getByRole("button", { name: /^Edit / })
    .click();
  await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("2.34");
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Hidden fractions preserved");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page
      .locator(".register-table tbody tr")
      .filter({ hasText: "Hidden fractions preserved" }),
  ).toHaveCount(1);
  const afterHidden = await (
    await fetch(
      `${api}/api/v1/budgets/${budget}/transactions?account_id=${account}`,
    )
  ).json();
  expect(
    afterHidden.data.find((row) => row.notes === "Hidden fractions preserved")
      .amount,
  ).toBe(234);
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
  await page.getByLabel("Show through date").fill("01/01/2026");
  await page.getByRole("button", { name: "Apply", exact: true }).click();
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
    "PASS register CRUD, calculator, references, statuses, exact balances, draft/retry recovery, mobile and failure retention",
  );
} finally {
  await browser?.close();
  await server.stop();
}

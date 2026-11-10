import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const r = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser, page;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    repairs = randomUUID(),
    salary = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", {
    id: budget,
    name: "Template goals",
    budget_type: "envelope",
  });
  await call(root + "/accounts", {
    id: account,
    name: "Checking",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await call(root + "/categories", {
    id: salary,
    name: "Salary",
    is_income: true,
  });
  await call(root + "/categories", {
    id: repairs,
    name: "Repairs",
    is_income: false,
    notes: "Keep this plan description\n#template 72.99",
  });
  await call(root + "/transactions", {
    id: randomUUID(),
    account_id: account,
    category_id: salary,
    amount: 100000,
    date: "2026-11-01",
    notes: "Confirmed salary",
    cleared: true,
  });
  await call(
    root + "/preferences",
    { experimental: { goalTemplatesEnabled: true } },
    "PATCH",
  );
  const financial = await call(root + "/transactions");
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
  await page.getByLabel("Budget month", { exact: true }).fill("2026-11");
  await page.getByLabel("Budget month", { exact: true }).press("Tab");
  await page
    .getByRole("button", { name: "Goal templates", exact: true })
    .click({ timeout: 2500 });
  await page
    .getByRole("button", { name: "Set Repairs savings target", exact: true })
    .click();
  await page.getByLabel("Savings target", { exact: true }).fill("100.00");
  await page
    .getByRole("button", { name: "Use savings target", exact: true })
    .click();
  await expect(
    page.getByLabel("Category template notes", { exact: true }),
  ).toHaveValue("Keep this plan description\n#template 72.99\n#goal 100");
  await page.reload();
  await expect(
    page.getByLabel("Category template notes", { exact: true }),
  ).toHaveValue("Keep this plan description\n#template 72.99\n#goal 100");
  await page
    .getByRole("button", { name: "Save template notes", exact: true })
    .click();
  await page
    .getByRole("checkbox", { name: "Select Repairs", exact: true })
    .check();
  await page
    .getByRole("button", { name: "Preview selected templates", exact: true })
    .click();
  await expect(page.locator(".template-preview")).toContainText("72.99");
  await expect(page.locator(".template-preview")).toContainText("100.00");
  await mkdir("test-results/budget-templates", { recursive: true });
  await page.screenshot({
    path: "test-results/budget-templates/preview-desktop.png",
    fullPage: true,
  });
  let dropped = false;
  const captured = [];
  await page.route(
    `**/budgets/${budget}/budget/templates/commands`,
    async (route) => {
      const req = route.request();
      if (!["apply", "overwrite"].includes(req.postDataJSON()?.action))
        return route.continue();
      captured.push({
        body: req.postData(),
        key: req.headers()["idempotency-key"],
        clock: req.headers()["x-actual-edit-clock"],
        epoch: req.headers()["x-actual-budget-epoch"],
        owner: req.headers()["x-actual-history-owner"],
      });
      if (!dropped) {
        dropped = true;
        const response = await route.fetch({
          url: req.url().replace("actualbudget.test", "127.0.0.1"),
          headers: { ...req.headers(), host: new URL(req.url()).host },
        });
        expect(response.status(), await response.text()).toBe(200);
        return route.abort("connectionfailed");
      }
      return route.continue();
    },
  );
  await page
    .getByRole("button", { name: "Apply captured templates", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Retry saved template change",
      exact: true,
    }),
  ).toBeVisible();
  expect(
    (await call(root + "/budget?month=2026-11")).months[
      "2026-11"
    ].categories.find((c) => c.id === repairs).budgeted,
  ).toBe(7299);
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved template change", exact: true })
    .click();
  await expect(page.locator(".budget-templates")).toContainText(
    "Template change saved",
  );
  expect(captured).toHaveLength(2);
  expect(captured[1]).toEqual(captured[0]);
  await page
    .getByRole("button", { name: "Close goal templates", exact: true })
    .click();
  const row = page.locator("table tbody tr").filter({ hasText: "Repairs" });
  await expect(row).toContainText("Goal: 100.00");
  await expect(row).toContainText("27.01 to goal");
  const after = await call(root + "/transactions");
  expect(after.data).toEqual(financial.data);
  expect(after.balances).toEqual(financial.balances);
  await page
    .getByRole("button", { name: "Goal templates", exact: true })
    .click();
  // Storage failure retains entered notes and blocks dispatch until saved.
  await page
    .getByRole("button", { name: "Edit Repairs template notes", exact: true })
    .click();
  await page.evaluate(() => {
    window.templateOriginalSet = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key.startsWith("actualBudget.templates.v1."))
        throw new DOMException("Fixture quota", "QuotaExceededError");
      return window.templateOriginalSet.call(this, key, value);
    };
  });
  await page
    .getByLabel("Category template notes", { exact: true })
    .fill("Retained after quota\n#template 72.99\n#goal 100");
  await expect(
    page.getByLabel("Category template notes", { exact: true }),
  ).toHaveValue("Retained after quota\n#template 72.99\n#goal 100");
  await expect(
    page.getByRole("button", { name: "Save template notes", exact: true }),
  ).toBeDisabled();
  await page.evaluate(
    () => (Storage.prototype.setItem = window.templateOriginalSet),
  );
  await page
    .getByRole("button", {
      name: "Retry saving retained template work",
      exact: true,
    })
    .click();
  await page
    .getByRole("button", { name: "Save template notes", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save template notes", exact: true }),
  ).toHaveCount(0);
  // Refresh cannot retarget an already captured allocation to another method.
  await page
    .getByRole("checkbox", { name: "Select Repairs", exact: true })
    .check();
  await page
    .getByRole("button", { name: "Preview selected templates", exact: true })
    .click();
  let cp = await call(root + "/checkpoint");
  await call(root + "/budget", {
    action: "method",
    month: "2026-11",
    method: "tracking",
    snapshot: cp.snapshot,
  });
  await page
    .getByRole("button", { name: "Refresh template definitions", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Apply captured templates", exact: true })
    .click();
  await expect(page.locator(".budget-templates [role=alert]")).toContainText(
    "budgeting method changed",
  );
  expect(
    await page.evaluate(
      (b) =>
        JSON.parse(localStorage.getItem(`actualBudget.templates.v1.${b}`))
          .pending,
      budget,
    ),
  ).toBeNull();
  await page
    .getByRole("button", { name: "Discard template review", exact: true })
    .click();
  cp = await call(root + "/checkpoint");
  await call(root + "/budget", {
    action: "method",
    month: "2026-11",
    method: "envelope",
    snapshot: cp.snapshot,
  });
  await page
    .getByRole("button", { name: "Refresh template definitions", exact: true })
    .click();
  await expect(page.locator(".budget-templates [role=alert]")).toHaveCount(0);
  for (const [name, dark, large] of [
    ["mobile", false, false],
    ["large-text", false, true],
    ["dark-large-text", true, true],
  ]) {
    await page.setViewportSize({ width: 390, height: 1000 });
    await page.emulateMedia({ colorScheme: dark ? "dark" : "light" });
    await page.evaluate(
      (large) =>
        (document.documentElement.style.fontSize = large ? "200%" : ""),
      large,
    );
    await page.locator("#goal-template-title").scrollIntoViewIfNeeded();
    await page.screenshot({
      path: `test-results/budget-templates/${name}.png`,
      fullPage: true,
    });
    await page.screenshot({
      path: `test-results/budget-templates/${name}-viewport.png`,
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  expect(errors).toEqual([]);
  console.log(
    "PASS notes+savings target, retaineddraft, exactallocation+progress, postcommit lostACK replay, unchangedledger and responsive goals",
  );
} catch (e) {
  if (page) {
    await mkdir("test-results/budget-templates", { recursive: true });
    await writeFile(
      "test-results/budget-templates/failure.html",
      await page.content(),
    );
    await page.screenshot({
      path: "test-results/budget-templates/failure.png",
      fullPage: true,
    });
  }
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

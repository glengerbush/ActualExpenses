import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { startServer } from "./server.mjs";

const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
const artifacts = "test-results/budget-automations";
async function request(
  path,
  body,
  method = body === undefined ? "GET" : "POST",
) {
  const response = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  // Retain the envelope: transaction data is an array; balances live beside it.
  return response.json();
}
let browser, page;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    repairs = randomUUID(),
    buffer = randomUUID(),
    salary = randomUUID();
  const root = `/budgets/${budget}`;
  await request("/budgets", {
    id: budget,
    name: "Visual automations",
    budget_type: "envelope",
  });
  await request(root + "/accounts", {
    id: account,
    name: "Checking",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const category of [
    { id: salary, name: "Salary", is_income: true },
    {
      id: repairs,
      name: "Repairs",
      is_income: false,
      notes: "Keep this plan description\n#template 25",
    },
    {
      id: buffer,
      name: "Buffer",
      is_income: false,
      notes: "Keep the buffer description",
    },
  ])
    await request(root + "/categories", category);
  for (const [category_id, amount, notes] of [
    [salary, 100000, "Salary"],
    [repairs, -1200, "Repair expense"],
  ]) {
    await request(root + "/transactions", {
      id: randomUUID(),
      account_id: account,
      category_id,
      amount,
      date: "2026-11-01",
      notes,
      cleared: true,
    });
  }
  await request(
    root + "/preferences",
    {
      experimental: {
        goalTemplatesEnabled: true,
        goalTemplatesUIEnabled: true,
      },
    },
    "PATCH",
  );
  const ledger = async () => {
    const value = await request(root + "/transactions");
    delete value.snapshot;
    return value;
  };
  const financial = await ledger();
  const plan = async () =>
    (await request(root + "/budget?month=2026-11")).data.months["2026-11"];
  const definitions = async () =>
    (await request(root + "/budget/templates?month=2026-11")).data;
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
  const button = (name) => page.getByRole("button", { name, exact: true });
  const field = (name) => page.getByLabel(name, { exact: true });
  const save = async (name) => {
    await button(name).click();
    await expect(button(name)).toBeHidden();
  };
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
  await button("Budget").click();
  await field("Budget month").fill("2026-11");
  await field("Budget month").press("Tab");
  await button("Goal templates").click();
  await button("Migrate Repairs visual automation").click();
  await save("Save source migration");
  await button("Edit Repairs visual automation").click();
  await field("Monthly amount").fill("-");
  await expect(button("Save visual automation")).toBeDisabled();
  await page.reload();
  await expect(field("Monthly amount")).toHaveValue("-");
  await expect(button("Refresh template definitions")).toBeEnabled();
  await expect(button("Save visual automation")).toBeDisabled();
  expect(
    (await definitions()).categories.find((row) => row.id === repairs)
      .templates[0].monthly,
  ).toBe(25);
  await field("Monthly amount").fill("50.00");
  let dropped = false;
  const captured = [];
  await page.route(
    `**/budgets/${budget}/budget/templates/commands`,
    async (route) => {
      const req = route.request();
      if (
        req.postDataJSON()?.action !== "configure" ||
        req.postDataJSON()?.category_id !== repairs ||
        captured.length >= 2
      )
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
  await button("Save visual automation").click();
  await expect(button("Retry saved template change")).toBeVisible();
  expect(
    (await definitions()).categories.find((row) => row.id === repairs)
      .templates[0].monthly,
  ).toBe(50);
  await page.reload();
  await button("Retry saved template change").click();
  await expect(page.locator(".budget-templates")).toContainText(
    "Template change saved",
  );
  expect(captured).toHaveLength(2);
  expect(captured[1]).toEqual(captured[0]);
  await page.unroute(`**/budgets/${budget}/budget/templates/commands`);
  await button("Create named cleanup pool").click();
  await field("Pool name").fill("Reserve");
  await save("Save cleanup pool");
  const pool = (await definitions()).pools.find(
    (row) => row.name === "Reserve",
  );
  expect(pool).toBeTruthy();
  for (const [name, role] of [
    ["Repairs", "source"],
    ["Buffer", "sink"],
  ]) {
    if (name === "Buffer") {
      await button("Migrate Buffer visual automation").click();
      await save("Save source migration");
    }
    await button(`Edit ${name} visual automation`).click();
    await button("Add cleanup role").click();
    const cleanup = page.getByRole("group", {
      name: "Month-end cleanup",
      exact: true,
    });
    await cleanup.getByLabel(/^Cleanup role/).selectOption(role);
    await cleanup.getByLabel(/^Pool/).selectOption({ label: "Reserve" });
    if (role === "sink")
      await cleanup.getByLabel("Share weight", { exact: true }).fill("1");
    await save("Save visual automation");
  }
  await page
    .getByRole("checkbox", { name: "Select Repairs", exact: true })
    .check();
  await button("Preview selected templates").click();
  await button("Apply captured templates").click();
  await expect
    .poll(
      async () =>
        (await plan()).categories.find((row) => row.id === repairs).budgeted,
    )
    .toBe(5000);
  const beforeCleanup = await plan();
  await button("Preview month-end cleanup").click();
  await expect(page.locator(".template-preview")).toContainText("38.00");
  await expect(page.locator(".template-preview")).not.toContainText(
    /cleanup-|up.to.date|no changes/i,
  );
  expect(await plan()).toEqual(beforeCleanup);
  await mkdir(artifacts, { recursive: true });
  await page.screenshot({
    path: `${artifacts}/cleanup-desktop.png`,
    fullPage: true,
  });
  await button("Apply captured cleanup").click();
  await expect
    .poll(
      async () =>
        (await plan()).categories.find((row) => row.id === repairs).budgeted,
    )
    .toBe(1200);
  expect(
    (await plan()).categories.find((row) => row.id === buffer).budgeted,
  ).toBe(3800);
  expect(await ledger()).toEqual(financial);
  const beforeMigration = await plan();
  await button("Move Repairs automation to notes").click();
  await expect(field("Proposed notes")).toHaveValue(
    /Keep this plan description/,
  );
  await save("Save source migration");
  expect(
    (await definitions()).categories.find((row) => row.id === repairs).source,
  ).toBe("notes");
  await button("Migrate Repairs visual automation").click();
  await save("Save source migration");
  // Note text can change, but migration never applies its hypothetical plan.
  const money = (view) =>
    view.categories.map(
      ({ id, budgeted, actual, balance, goal, long_goal }) => ({
        id,
        budgeted,
        actual,
        balance,
        goal,
        long_goal,
      }),
    );
  expect(money(await plan())).toEqual(money(beforeMigration));
  await button("Edit Buffer visual automation").click();
  await button("Add allocation rule").click();
  let rules = page.getByRole("group", {
    name: "Visual allocation rules",
    exact: true,
  });
  await rules.getByLabel(/^Rule type/).selectOption("remainder");
  await rules.getByLabel("Share weight", { exact: true }).fill("1");
  await button("Add allocation rule").click();
  await rules
    .getByLabel(/^Rule type/)
    .nth(1)
    .selectOption("limit");
  await rules.getByLabel("Maximum balance", { exact: true }).fill("50.00");
  await page.screenshot({
    path: `${artifacts}/editor-desktop.png`,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 1000 });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await rules.scrollIntoViewIfNeeded();
  await page.screenshot({
    path: `${artifacts}/editor-dark-large-text.png`,
    fullPage: true,
  });
  await page.screenshot({
    path: `${artifacts}/editor-dark-large-text-viewport.png`,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => (document.documentElement.style.fontSize = ""));
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.emulateMedia({ colorScheme: "light" });
  await save("Save visual automation");
  await button("Move Buffer automation to notes").click();
  await expect(page.locator(".budget-templates")).toContainText(
    "changes an allocation rule or goal indicator",
  );
  await expect(button("Save source migration")).toBeDisabled();
  expect(money(await plan())).toEqual(money(beforeMigration));
  expect(await ledger()).toEqual(financial);
  // A source with no sink must show its actionable server warning, both before
  // applying and in the confirmed result. A fresh browser context keeps this
  // scenario independent of the deliberately incompatible migration review.
  await request(root + "/categories", {
    id: randomUUID(),
    name: "Unassigned remainder",
    is_income: false,
    notes: "#cleanup Unassigned source",
  });
  await page.close();
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
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
  await button("Budget").click();
  await field("Budget month").fill("2026-11");
  await field("Budget month").press("Tab");
  await button("Goal templates").click();
  await button("Preview month-end cleanup").click();
  const warning = 'Cleanup group "Unassigned" has no matching sink categories.';
  await expect(page.locator(".template-preview")).toContainText(warning);
  await expect(page.locator(".template-preview")).not.toContainText(
    "cleanup-no-funds",
  );
  await page.screenshot({
    path: `${artifacts}/cleanup-warning.png`,
    fullPage: true,
  });
  await button("Apply captured cleanup").click();
  await expect(button("Apply captured cleanup")).toBeHidden();
  await expect(field("Confirmed cleanup outcome")).toContainText(warning);
  await expect(field("Confirmed cleanup outcome")).not.toContainText(
    "cleanup-no-funds",
  );
  expect(await ledger()).toEqual(financial);
  expect(errors).toEqual([]);
  await writeFile(
    `${artifacts}/lost-ack.json`,
    JSON.stringify(captured, null, 2),
  );
  console.log(
    "PASS typed automation, scoped pool cleanup, lossless migration/refusal, exact lost-200-ack replay and unchanged complete ledger",
  );
} catch (error) {
  if (page) {
    await mkdir(artifacts, { recursive: true });
    await writeFile(`${artifacts}/failure.html`, await page.content());
    await page.screenshot({ path: `${artifacts}/failure.png`, fullPage: true });
  }
  throw error;
} finally {
  await browser?.close();
  await server.stop();
}

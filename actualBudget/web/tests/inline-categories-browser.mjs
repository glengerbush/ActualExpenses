import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api + path, {
    method: body ? "POST" : "GET",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Inline categories" });
  await call(root + "/accounts", {
    id: randomUUID(),
    name: "History",
    opening_balance: 0,
    opening_date: "2026-11-01",
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
      viewport: { width: 1440, height: 1000 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByText("Inline categories", { exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Categories", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  const editCategory = async (name) => {
    const row = page.locator(".budget-category").filter({ hasText: name });
    await row
      .getByRole("button", { name: new RegExp(`^Edit ${name} for `) })
      .click();
    await page
      .getByRole("button", { name: "Category settings", exact: true })
      .click();
  };
  await page.getByRole("button", { name: "Add group", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Everyday");
  await page.getByLabel("Group color", { exact: true }).fill("#397c8c");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Add category to Everyday", exact: true }),
  ).toBeEnabled();
  await expect(
    page.getByRole("textbox", { name: "Group name Everyday", exact: true }),
  ).toBeVisible();
  const group = (await call(root + "/category-groups")).find(
    (g) => g.name === "Everyday",
  );
  await page
    .getByRole("button", { name: "Add category to Everyday", exact: true })
    .click();
  await expect(page.getByLabel("Name", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Group", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Type", { exact: true })).toHaveCount(0);
  await page.getByLabel("Name", { exact: true }).fill("Groceries");
  const shade = await page
    .getByLabel("Category shade", { exact: true })
    .inputValue();
  await page.getByLabel("Category shade", { exact: true }).fill("#ff0000");
  await page
    .getByRole("button", { name: "Use automatic shade", exact: true })
    .click();
  await expect(page.getByLabel("Category shade", { exact: true })).toHaveValue(
    shade,
  );
  // Restore the in-progress draft through the Budget route.
  await page.reload();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await expect(page.getByLabel("Name", { exact: true })).toHaveValue(
    "Groceries",
  );
  await expect(page.getByLabel("Group", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.locator(".budget-category").filter({ hasText: "Groceries" }),
  ).toBeEnabled();
  let category = (await call(root + "/categories")).find(
    (c) => c.name === "Groceries",
  );
  expect(category.group_id).toBe(group.id);
  expect(category.is_income).toBe(false);
  expect(category.color).toBeNull();
  expect(category.resolved_color.toLowerCase()).toBe(shade);
  await editCategory("Groceries");
  await page.getByLabel("Category shade", { exact: true }).fill("#cc2244");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.locator(".budget-category").filter({ hasText: "Groceries" }),
  ).toBeEnabled();
  await editCategory("Groceries");
  await page
    .getByRole("button", { name: "Use automatic shade", exact: true })
    .click();
  await expect(page.getByLabel("Category shade", { exact: true })).toHaveValue(
    shade,
  );
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.locator(".budget-category").filter({ hasText: "Groceries" }),
  ).toBeEnabled();
  expect((await call(root + "/categories/" + category.id)).color).toBeNull();
  await page.getByRole("button", { name: "Add group", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Income");
  await page
    .getByLabel("Type", { exact: true })
    .selectOption({ label: "Income" });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Add category to Income", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Add category to Income", exact: true })
    .click();
  await page.getByLabel("Name", { exact: true }).fill("Salary");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.locator(".budget-category").filter({ hasText: "Salary" }),
  ).toBeEnabled();
  expect(
    (await call(root + "/categories")).find((c) => c.name === "Salary")
      .is_income,
  ).toBe(true);
  await mkdir("test-results/inline-categories", { recursive: true });
  await page.screenshot({
    path: "test-results/inline-categories/desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/inline-categories/mobile.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS inline group/category creation, empty groups, inherited type/group, draft reload, automatic preview/server match, reset and mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
}

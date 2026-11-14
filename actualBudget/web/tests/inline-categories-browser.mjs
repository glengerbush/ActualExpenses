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
  await expect(
    page.getByRole("button", { name: "Categories", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel(/Months displayed/).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
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
    page.getByRole("textbox", { name: "Category name Groceries", exact: true }),
  ).toBeEnabled();
  let category = (await call(root + "/categories")).find(
    (c) => c.name === "Groceries",
  );
  expect(category.group_id).toBe(group.id);
  expect(category.is_income).toBe(false);
  expect(category.color).toBeNull();
  expect(category.resolved_color.toLowerCase()).toBe(shade);
  await page
    .getByRole("button", { name: "Choose color for Groceries", exact: true })
    .click();
  const color = page.getByLabel("Color for Groceries", { exact: true });
  await color.fill("#cc2244");
  await expect
    .poll(async () => (await call(root + "/categories/" + category.id)).color)
    .toBe("#cc2244");
  await expect(
    page.getByRole("button", {
      name: "Use automatic shade for Groceries",
      exact: true,
    }),
  ).toHaveCount(0);
  await expect(color).toHaveValue("#cc2244");
  await page.keyboard.press("Escape");
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
    page.getByRole("textbox", { name: "Category name Salary", exact: true }),
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
  // Destructive actions appear only after the pencil, not while typing names or amounts.
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect(
    page.getByRole("button", { name: /^Delete (category|group)/ }),
  ).toHaveCount(0);
  await page
    .getByRole("textbox", { name: "Category name Groceries", exact: true })
    .focus();
  await expect(
    page.getByRole("button", { name: /^Delete (category|group)/ }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: /^Edit Groceries for/ }).click();
  await expect(
    page.getByRole("button", {
      name: "Delete category Groceries",
      exact: true,
    }),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/inline-categories/category-editor.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Delete category Groceries", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Delete and reassign", exact: true }),
  ).toBeVisible();
  expect((await call(root + "/categories/" + category.id)).deleted).toBe(false);
  await page
    .getByRole("button", { name: "Delete and reassign", exact: true })
    .click();
  await expect(
    page.getByRole("textbox", { name: "Category name Groceries", exact: true }),
  ).toHaveCount(0);
  expect((await call(root + "/categories/" + category.id)).deleted).toBe(true);
  await page
    .getByRole("button", { name: "Edit group Everyday", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Delete group Everyday", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/inline-categories/group-editor.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/inline-categories/group-editor-mobile.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Delete group Everyday", exact: true })
    .click();
  expect((await call(root + "/category-groups/" + group.id)).deleted).toBe(
    false,
  );
  await page
    .getByRole("button", { name: "Delete and reassign", exact: true })
    .click();
  await expect(
    page.getByRole("textbox", { name: "Group name Everyday", exact: true }),
  ).toHaveCount(0);
  expect((await call(root + "/category-groups/" + group.id)).deleted).toBe(
    true,
  );
  expect(errors).toEqual([]);
  console.log(
    "PASS inline group/category creation, empty groups, inherited type/group, draft reload, automatic preview/server match, compact colors, pencil-only deletion and mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
}

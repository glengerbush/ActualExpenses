import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const response = await fetch(base + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return response.json();
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    payee = randomUUID(),
    transaction = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Calculated expenses" });
  await call(root + "/accounts", {
    id: account,
    name: "Spending",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await call(root + "/payees", { id: payee, name: "Market" });
  await call(root + "/transactions", {
    id: transaction,
    account_id: account,
    payee_id: payee,
    amount: -1234,
    date: "2026-11-09",
    notes: "Coffee",
    cleared: true,
  });
  await call(
    root + "/preferences",
    { experimental: { formulaMode: true, actionTemplating: true } },
    "PATCH",
  );
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
      viewport: { width: 1280, height: 900 },
    }),
    errors = [];
  page.setDefaultTimeout(15000);
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
  const open = async () => {
    if (
      !(await page
        .getByRole("heading", { name: "Rules and category learning" })
        .isVisible())
    )
      await page.getByRole("button", { name: "Rules", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: "Rules and category learning" }),
    ).toBeVisible();
  };
  await open();
  await page.getByRole("button", { name: "Create rule", exact: true }).click();
  await page.getByLabel(/^Value 1/).selectOption(payee);
  await page.getByLabel(/^Set field 1/).selectOption("amount");
  await page
    .getByLabel("Value method 1", { exact: true })
    .selectOption("formula");
  await page.getByLabel("Formula 1", { exact: true }).fill("=amount/100+1");
  await page.getByRole("button", { name: "Add action", exact: true }).click();
  await page.getByLabel(/^Set field 2/).selectOption("notes");
  await page
    .getByLabel("Value method 2", { exact: true })
    .selectOption("template");
  await page
    .getByLabel("Template 2", { exact: true })
    .fill("{{notes}} / tracked");
  await page.reload();
  await open();
  await expect(page.getByLabel("Formula 1", { exact: true })).toHaveValue(
    "=amount/100+1",
  );
  await expect(page.getByLabel("Template 2", { exact: true })).toHaveValue(
    "{{notes}} / tracked",
  );
  await page
    .getByRole("button", { name: "Preview draft rule", exact: true })
    .click();
  await expect(page.locator(".preview-list")).toContainText("Coffee / tracked");
  await mkdir("test-results/rule-expressions", { recursive: true });
  await page
    .locator("fieldset")
    .last()
    .screenshot({ path: "test-results/rule-expressions/editor-desktop.png" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByLabel("Formula 1", { exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/rule-expressions/editor-mobile.png",
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByRole("button", { name: "Save rule", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Rule saved." }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Preview all saved rules", exact: true })
    .click();
  await page.locator(".preview-list input[type=checkbox]").first().check();
  await page
    .getByRole("button", { name: "Apply rules to 1 selected", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/transactions/" + transaction)).data.amount,
    )
    .toBe(-1134);
  expect((await call(root + "/transactions/" + transaction)).data.notes).toBe(
    "Coffee / tracked",
  );
  await call(
    root + "/preferences",
    { experimental: { formulaMode: false, actionTemplating: false } },
    "PATCH",
  );
  await page.reload();
  await open();
  await page.getByRole("button", { name: "Edit rule", exact: true }).click();
  await expect(page.getByLabel("Formula 1", { exact: true })).toHaveValue(
    "=amount/100+1",
  );
  await page.getByLabel("Formula 1", { exact: true }).fill("=1/0");
  await page
    .getByRole("button", { name: "Preview draft rule", exact: true })
    .click();
  await expect(
    page.getByText(/Error executing formula for "amount"/),
  ).toBeVisible();
  await page
    .getByLabel("Value method 1", { exact: true })
    .selectOption("fixed");
  // Existing template action still exposes its mode when the feature flag is off.
  await page.getByLabel(/^Set field 2/).selectOption("amount");
  await page.getByLabel("Template 2", { exact: true }).fill("-12.5 pounds");
  await page
    .getByRole("button", { name: "Preview draft rule", exact: true })
    .click();
  await expect(
    page.getByText("Cannot save: amounts must be whole minor units.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.locator(".preview-list")).toContainText(
    "-12.5 minor units",
  );
  await page.locator(".preview-list").screenshot({
    path: "test-results/rule-expressions/invalid-preview-mobile.png",
  });
  await call(root + "/preferences", { privacy_mode: true }, "PATCH");
  await page.reload();
  await open();
  await page
    .getByRole("button", { name: "Preview draft rule", exact: true })
    .click();
  await expect(page.locator(".preview-list")).not.toContainText("-12.5");
  await expect(page.locator(".preview-list")).toContainText("••••");
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click();
  await call(
    root + "/preferences",
    { privacy_mode: false, experimental: { formulaMode: true } },
    "PATCH",
  );
  await page.reload();
  await open();
  await page.getByRole("button", { name: "Create rule", exact: true }).click();
  await page.getByLabel(/^Value 1/).selectOption(payee);
  await page.getByLabel(/^Action 1/).selectOption("set-split-amount");
  await page.getByLabel(/^Allocation method 1/).selectOption("formula");
  const splitFormula = page.getByLabel("Formula 1", { exact: true });
  await splitFormula.fill("=parent_amount/200");
  await splitFormula.press("Tab");
  await expect(page.getByLabel(/^Split target 1/)).toBeFocused();
  await page.getByRole("button", { name: "Add action", exact: true }).click();
  await page.getByLabel(/^Action 2/).selectOption("set-split-amount");
  await page.getByLabel(/^Allocation method 2/).selectOption("remainder");
  await page.getByLabel(/^Split target 2/).fill("2");
  await page.getByLabel(/^Split target 2/).press("Tab");
  await page
    .getByRole("button", { name: "Preview draft rule", exact: true })
    .click();
  await expect(page.locator(".preview-list")).toContainText("-5.67");
  await page.addStyleTag({ content: "html { font-size: 200%; }" });
  await splitFormula.scrollIntoViewIfNeeded();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/rule-expressions/split-large-text-mobile.png",
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS calculated rule editing, reload, preview, apply, opt-out retention, errors, privacy, split formulas and keyboard/large-text mobile on HTTP",
  );
} finally {
  await browser?.close();
  await server.stop();
}

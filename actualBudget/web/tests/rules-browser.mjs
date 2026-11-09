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
  return r.json();
}
let browser, page;
const output = "test-results/rules";
try {
  const budget = randomUUID(),
    account = randomUUID(),
    food = randomUUID(),
    travel = randomUUID(),
    shop = randomUUID(),
    first = randomUUID(),
    second = randomUUID();
  const root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Everyday rules" });
  await call(root + "/accounts", {
    id: account,
    name: "Cash",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const [id, name] of [
    [food, "Food"],
    [travel, "Travel"],
  ])
    await call(root + "/categories", { id, name, is_income: false });
  await call(root + "/payees", { id: shop, name: "Shop" });
  for (const [id, notes] of [
    [first, "First"],
    [second, "Second"],
  ])
    await call(root + "/transactions", {
      id,
      account_id: account,
      date: "2026-01-01",
      amount: -100,
      payee_id: shop,
      notes,
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
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Rules and category learning" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Create rule", exact: true }).click();
  await page.getByLabel(/^Value 1/).selectOption(shop);
  await page.getByLabel(/^Set value 1/).selectOption(food);
  await page.getByRole("button", { name: "Add action", exact: true }).click();
  await page.getByLabel(/^Action 2/).selectOption("append-notes");
  await page.getByLabel(/^Notes text 2/).fill(" / checked");
  await mkdir(output, { recursive: true });
  await page.screenshot({
    path: output + "/builder-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: output + "/builder-mobile.png",
    fullPage: true,
  });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.getByLabel(/^Field 1/).scrollIntoViewIfNeeded();
  await page.screenshot({ path: output + "/builder-large-text-viewport.png" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => (document.documentElement.style.fontSize = ""));
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.getByRole("button", { name: "Save rule", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Rule saved." }),
  ).toBeVisible();
  let rules = await call(root + "/rules"),
    rule = rules.data[0];
  expect(rule.actions).toHaveLength(2);
  await page
    .getByRole("button", { name: "Preview all saved rules", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Rule preview" }),
  ).toBeVisible();
  await expect(page.locator(".preview-list li")).toHaveCount(2);
  await page.locator(".preview-list input[type=checkbox]").first().check();
  await page.screenshot({
    path: output + "/preview-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: output + "/preview-mobile.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page
    .getByRole("button", { name: "Apply rules to 1 selected", exact: true })
    .click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Selected rule actions saved." }),
  ).toBeVisible();
  const rows = (await call(root + "/transactions?limit=100")).data;
  expect(rows.filter((r) => r.notes.endsWith(" / checked"))).toHaveLength(1);
  expect(rows.filter((r) => r.category_id === food)).toHaveLength(1);
  // An untouched remote condition survives an actions-only edit, including refresh.
  await page.getByRole("button", { name: "Edit rule", exact: true }).click();
  await page.getByLabel(/^Notes text 2/).fill(" / local");
  rules = await call(root + "/rules");
  await call(
    root + "/rules/" + rule.id,
    {
      snapshot: rules.snapshot,
      conditions: [{ field: "amount", op: "lt", value: 0 }],
    },
    "PATCH",
  );
  await page
    .getByRole("button", { name: "Refresh rules", exact: true })
    .click();
  await page.getByRole("button", { name: "Save rule", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Rule saved." }),
  ).toBeVisible();
  rules = await call(root + "/rules");
  expect(rules.data[0].conditions).toEqual([
    { field: "amount", op: "lt", value: 0 },
  ]);
  expect(rules.data[0].actions[1].value).toBe(" / local");
  // A remote edit of the same field refuses silent overwrite after refresh/reload.
  await page.getByRole("button", { name: "Edit rule", exact: true }).click();
  await page.getByLabel(/^Notes text 2/).fill(" / retained");
  await call(
    root + "/rules/" + rule.id,
    {
      snapshot: rules.snapshot,
      actions: [
        { op: "set", field: "category", value: food },
        { op: "append-notes", value: " / remote" },
      ],
    },
    "PATCH",
  );
  await page
    .getByRole("button", { name: "Refresh rules", exact: true })
    .click();
  await page.getByRole("button", { name: "Save rule", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "actions changed elsewhere",
  );
  await page.reload();
  await expect(page.getByLabel(/^Notes text 2/)).toHaveValue(" / retained");
  await page.getByRole("button", { name: "Save rule", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "actions changed elsewhere",
  );
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click();
  // A committed lost acknowledgement retries the exact owner/epoch/clock/body/key.
  await page.getByRole("button", { name: "Edit rule", exact: true }).click();
  await page.getByLabel(/^Notes text 2/).fill(" / captured");
  const attempts = [];
  let drop = true;
  await page.route("**/api/v1" + root + "/rules/" + rule.id, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    const r = route.request();
    attempts.push({
      body: r.postData(),
      headers: Object.fromEntries(
        Object.entries(r.headers()).filter(([k]) =>
          [
            "idempotency-key",
            "x-actual-edit-clock",
            "x-actual-budget-epoch",
            "x-actual-history-owner",
          ].includes(k),
        ),
      ),
    });
    if (drop) {
      drop = false;
      const delivered = await fetch(
        r.url().replace("actualbudget.test", "127.0.0.1"),
        {
          method: "PATCH",
          headers: Object.fromEntries(
            [
              "content-type",
              "idempotency-key",
              "x-actual-edit-clock",
              "x-actual-budget-epoch",
              "x-actual-history-owner",
            ].map((k) => [k, r.headers()[k]]),
          ),
          body: r.postData(),
        },
      );
      expect(delivered.status, await delivered.clone().text()).toBe(200);
      return route.abort("failed");
    }
    return route.continue();
  });
  await page.getByRole("button", { name: "Save rule", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Unconfirmed request" }),
  ).toBeVisible();
  rules = await call(root + "/rules");
  await call(
    root + "/rules/" + rule.id,
    {
      snapshot: rules.snapshot,
      conditions: [{ field: "payee", op: "is", value: shop }],
    },
    "PATCH",
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved request", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Unconfirmed request" }),
  ).toHaveCount(0);
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  expect(attempts[0].headers["x-actual-history-owner"]).toMatch(
    /^[0-9a-f-]{36}$/,
  );
  await expect(page.getByRole("alert")).toHaveCount(0);
  // Automatic new entry protects manual fields; existing edits do not rerun rules.
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("12.34");
  await page.getByLabel("Notes", { exact: true }).fill("Manual");
  await page.getByLabel("Category", { exact: true }).selectOption(travel);
  await page.getByLabel("Payee", { exact: true }).selectOption(shop);
  await expect(page.getByLabel("Notes", { exact: true })).toHaveValue(
    "Manual / captured",
  );
  await expect(page.getByLabel("Category", { exact: true })).toHaveValue(
    travel,
  );
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  let saved = (await call(root + "/transactions?limit=100")).data.find(
    (r) => r.amount === -1234,
  );
  expect(saved.notes).toBe("Manual / captured");
  expect(saved.category_id).toBe(travel);
  // Empty new categories remain fillable, while explicit existing clearing stays clear.
  const edit = page
    .locator(".register-table tr")
    .filter({ hasText: "Manual / captured" })
    .getByRole("button", { name: /^Edit / });
  await edit.click();
  await page.getByLabel("Category", { exact: true }).selectOption("");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  expect(
    (await call(root + "/transactions/" + saved.id)).data.category_id,
  ).toBe(null);
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await page.getByLabel(/^Associated payee/).selectOption(shop);
  await expect(
    page.getByRole("heading", { name: /Associated rules/ }),
  ).toContainText("(1)");
  await page
    .getByLabel("Allow category learning for Shop", { exact: true })
    .uncheck();
  await expect(
    page.getByRole("status").filter({ hasText: "Learning setting saved." }),
  ).toBeVisible();
  expect((await call(root + "/payees/" + shop)).data.learn_categories).toBe(
    false,
  );
  await mkdir(output, { recursive: true });
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.screenshot({ path: output + "/desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: output + "/mobile.png", fullPage: true });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({ path: output + "/large-text.png", fullPage: true });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({
    path: output + "/dark-large-text.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS typed rules, selective apply, retained-field conflict, exact committed retry, automatic manual protection, learning controls",
  );
} catch (e) {
  if (page) {
    await mkdir(output, { recursive: true });
    await writeFile(output + "/failure.html", await page.content());
    await page.screenshot({ path: output + "/failure.png", fullPage: true });
  }
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

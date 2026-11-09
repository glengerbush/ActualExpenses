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
  // Narrow through the existing register query seam; the preview carries bounded IDs.
  const reserve = randomUUID(),
    otherPayee = randomUUID(),
    detailed = randomUUID(),
    detailRule = randomUUID(),
    splitEntry = randomUUID(),
    splitRule = randomUUID();
  await call(root + "/accounts", {
    id: reserve,
    name: "Reserve",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await call(root + "/payees", { id: otherPayee, name: "Other shop" });
  await call(root + "/transactions", {
    id: detailed,
    account_id: account,
    date: "2026-01-01",
    amount: -250,
    payee_id: otherPayee,
    notes: "Detailed review",
  });
  let checkpoint = (await call(root + "/rules")).snapshot;
  await call(root + "/rules", {
    id: detailRule,
    stage: null,
    conditionsOp: "and",
    conditions: [{ field: "notes", op: "contains", value: "Detailed review" }],
    actions: [
      { op: "set", field: "date", value: "2026-02-03" },
      { op: "set", field: "account", value: reserve },
      { op: "set", field: "payee", value: shop },
      { op: "set", field: "cleared", value: true },
    ],
    snapshot: checkpoint,
  });
  await page
    .getByRole("button", { name: "Refresh rules", exact: true })
    .click();
  await page.getByLabel(/^Preview account/).selectOption(account);
  await page
    .locator(`.rule-list li[data-rule-id="${detailRule}"] input[type=checkbox]`)
    .check();
  const accountPreview = page.waitForRequest(
    (r) => r.method() === "POST" && r.url().endsWith(root + "/rules/preview"),
  );
  await page
    .getByRole("button", { name: "Preview selected saved rules", exact: true })
    .click();
  expect((await accountPreview).postDataJSON().account_id).toBe(account);
  await page
    .getByLabel("Find transactions", { exact: true })
    .fill("Detailed review");
  await page
    .getByRole("button", { name: "Find transactions", exact: true })
    .click();
  await expect(page.locator(".scope-list li")).toHaveCount(1);
  await page.locator(".scope-list input[type=checkbox]").check();
  const narrowed = page.waitForRequest(
    (r) => r.method() === "POST" && r.url().endsWith(root + "/rules/preview"),
  );
  await page
    .getByRole("button", { name: "Preview selected saved rules", exact: true })
    .click();
  const narrowedBody = (await narrowed).postDataJSON();
  expect(narrowedBody.ids).toEqual([detailed]);
  expect(narrowedBody.account_id).toBeUndefined();
  const detail = page
    .locator(".preview-list li")
    .filter({ hasText: "Detailed review" });
  await expect(detail).toContainText("Date: 2026-01-01 → 2026-02-03");
  await expect(detail).toContainText("Account: Cash → Reserve");
  await expect(detail).toContainText("Payee: Other shop → Shop");
  await expect(detail).toContainText("Cleared: No → Yes");
  expect((await call(root + "/transactions/" + detailed)).data.date).toBe(
    "2026-01-01",
  );
  await page.screenshot({
    path: output + "/preview-fields-desktop.png",
    fullPage: true,
  });
  await detail.getByRole("checkbox").check();
  await page
    .getByRole("button", { name: "Apply rules to 1 selected", exact: true })
    .click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Selected rule actions saved." }),
  ).toBeVisible();
  const changed = (await call(root + "/transactions/" + detailed)).data;
  expect(changed.account_id).toBe(reserve);
  expect(changed.date).toBe("2026-02-03");
  expect(changed.payee_id).toBe(shop);
  expect(changed.cleared).toBe(true);
  await page
    .getByRole("button", { name: "Clear transaction selection", exact: true })
    .click();
  await page
    .locator(`.rule-list li[data-rule-id="${detailRule}"] input[type=checkbox]`)
    .uncheck();
  // New allocations show every proposed amount/reference/status before application.
  await call(root + "/transactions", {
    id: splitEntry,
    account_id: account,
    date: "2026-01-01",
    amount: -300,
    payee_id: otherPayee,
    category_id: travel,
    notes: "Split review",
  });
  checkpoint = (await call(root + "/rules")).snapshot;
  await call(root + "/rules", {
    id: splitRule,
    stage: null,
    conditionsOp: "and",
    conditions: [{ field: "notes", op: "contains", value: "Split review" }],
    actions: [
      {
        op: "set-split-amount",
        value: -100,
        options: { splitIndex: 1, method: "fixed-amount" },
      },
      {
        op: "set-split-amount",
        value: null,
        options: { splitIndex: 2, method: "remainder" },
      },
    ],
    snapshot: checkpoint,
  });
  await page
    .getByRole("button", { name: "Refresh rules", exact: true })
    .click();
  await page.getByLabel(/^Associated payee/).selectOption("");
  await page
    .locator(`.rule-list li[data-rule-id="${splitRule}"] input[type=checkbox]`)
    .check();
  await page
    .getByLabel("Find transactions", { exact: true })
    .fill("Split review");
  await page
    .getByRole("button", { name: "Find transactions", exact: true })
    .click();
  await expect(page.locator(".scope-list li")).toHaveCount(1);
  await page.locator(".scope-list input[type=checkbox]").check();
  await page
    .getByRole("button", { name: "Preview selected saved rules", exact: true })
    .click();
  const allocations = page.getByRole("region", {
    name: "Split allocation changes",
  });
  await expect(allocations).toContainText("Allocation 1 · Added");
  await expect(allocations).toContainText("Amount: Not present → -1.00");
  await expect(allocations).toContainText("Amount: Not present → -2.00");
  await expect(allocations).toContainText("Category: Not present → Travel");
  await expect(allocations).toContainText("Payee: Not present → Other shop");
  await expect(allocations).toContainText("Cleared: Not present → No");
  await expect(page.locator(".preview-list")).toContainText(
    "Ordinary transaction → Split transaction",
  );
  const originalSplitCandidate = (
    await call(root + "/transactions/" + splitEntry)
  ).data;
  expect(originalSplitCandidate.is_parent ?? false).toBe(false);
  expect(originalSplitCandidate.children ?? []).toEqual([]);
  expect(originalSplitCandidate.amount).toBe(-300);
  await page.screenshot({
    path: output + "/preview-allocations-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: output + "/preview-allocations-mobile.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page
    .getByRole("button", { name: "Clear transaction selection", exact: true })
    .click();
  await page
    .locator(`.rule-list li[data-rule-id="${splitRule}"] input[type=checkbox]`)
    .uncheck();
  // One protected match must not block selection of an unrelated unlocked match.
  const lockedRow = randomUUID(),
    unlockedRow = randomUUID();
  await call(root + "/transactions", {
    id: lockedRow,
    account_id: account,
    date: "2026-01-01",
    amount: -10,
    payee_id: shop,
    notes: "Protected preview",
    cleared: true,
  });
  await call(root + "/transactions", {
    id: unlockedRow,
    account_id: account,
    date: "2026-01-01",
    amount: -20,
    payee_id: shop,
    notes: "Unlocked preview",
  });
  const reconciliation = (
    await call(root + `/accounts/${account}/reconciliation`)
  ).data;
  await call(root + `/accounts/${account}/reconciliation`, {
    action: "lock",
    statement_balance: reconciliation.balances.cleared,
    snapshot: reconciliation.snapshot,
    date: "2026-02-28",
  });
  await page
    .getByRole("button", { name: "Refresh rules", exact: true })
    .click();
  await page
    .locator(`.rule-list li[data-rule-id="${rule.id}"] input[type=checkbox]`)
    .check();
  await page
    .getByRole("button", { name: "Preview selected saved rules", exact: true })
    .click();
  const lockedMatch = page
      .locator(".preview-list>li")
      .filter({ hasText: "Protected preview" }),
    unlockedMatch = page
      .locator(".preview-list>li")
      .filter({ hasText: "Unlocked preview" });
  await expect(lockedMatch.getByRole("checkbox")).toBeDisabled();
  await expect(unlockedMatch.getByRole("checkbox")).toBeEnabled();
  await unlockedMatch.getByRole("checkbox").check();
  await expect(
    page.getByRole("button", {
      name: "Apply rules to 1 selected",
      exact: true,
    }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Apply rules to 1 selected", exact: true })
    .click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Selected rule actions saved." }),
  ).toBeVisible();
  expect((await call(root + "/transactions/" + lockedRow)).data.notes).toBe(
    "Protected preview",
  );
  expect(
    (await call(root + "/transactions/" + lockedRow)).data.reconciled,
  ).toBe(true);
  expect((await call(root + "/transactions/" + unlockedRow)).data.notes).toBe(
    "Unlocked preview / captured",
  );
  await mkdir(output, { recursive: true });
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.screenshot({ path: output + "/desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: output + "/mobile.png", fullPage: true });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({ path: output + "/large-text.png", fullPage: true });
  const headingBox = await page
    .getByRole("heading", { name: "Rules and category learning", exact: true })
    .boundingBox();
  const closeBox = await page
    .getByRole("button", { name: "Close", exact: true })
    .boundingBox();
  expect(headingBox.width).toBeGreaterThan(300);
  expect(closeBox.y).toBeGreaterThanOrEqual(headingBox.y + headingBox.height);
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

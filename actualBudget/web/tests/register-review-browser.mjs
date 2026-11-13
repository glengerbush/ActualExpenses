import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = "POST", headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
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
  async function setup(all = false) {
    const budget = randomUUID(),
      account = randomUUID();
    await call("/budgets", { id: budget, name: "Register review" });
    await call(`/budgets/${budget}/accounts`, {
      id: account,
      name: "Spending",
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto(server.url);
    await page.evaluate(
      ({ budget, account }) =>
        localStorage.setItem(
          "actualBudget.selection",
          JSON.stringify({ budgetId: budget, accountId: account }),
        ),
      { budget, account: all ? null : account },
    );
    await page.reload();
    await expect(page.getByLabel("Search transactions")).toBeEnabled();
    await page.locator('summary[aria-label="Filters and columns"]').click();
    return { budget, account, context, page };
  }
  const run = process.argv[2];
  if (!run || run === "recovery") {
    const { budget, context, page: a } = await setup();
    const b = await context.newPage();
    await b.goto(server.url);
    await expect(b.getByLabel("Search transactions")).toBeEnabled();
    await b.locator('summary[aria-label="Filters and columns"]').click();
    await a.getByLabel("Search transactions").fill("Keep my draft");
    await b.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      b.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    await a.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(a.getByRole("alert")).toContainText("another tab");
    await b
      .getByRole("button", { name: "Retry register settings", exact: true })
      .click();
    await expect(b.getByRole("alert")).toContainText("another tab");
    await b.getByRole("button", { name: "Reload register settings" }).click();
    await b.getByLabel("Search transactions").fill("Other tab committed");
    let sent;
    await b.route("**/register-settings", async (route) => {
      if (route.request().method() !== "PATCH") return route.continue();
      sent = {
        key: route.request().headers()["idempotency-key"],
        body: route.request().postData(),
      };
      const url = new URL(route.request().url()),
        headers = { ...route.request().headers(), host: url.host };
      url.hostname = "127.0.0.1";
      expect((await route.fetch({ url: url.href, headers })).ok()).toBe(true);
      await route.abort();
    });
    await b.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      b.getByRole("button", { name: "Retry register settings", exact: true }),
    ).toBeVisible();
    const pending = await b.evaluate(
      (budget) =>
        Object.entries(localStorage).filter(([key]) =>
          key.startsWith("actualBudget.register-settings.v1." + budget + "."),
        ),
      budget,
    );
    expect(pending).toHaveLength(1);
    await a.getByRole("button", { name: "Reload register settings" }).click();
    expect(
      await a.evaluate((key) => localStorage.getItem(key), pending[0][0]),
    ).toBe(pending[0][1]);
    await expect(a.getByLabel("Search transactions")).toHaveValue(
      "Keep my draft",
    );
    await b.unroute("**/register-settings");
    await b.route("**/register-settings", (route) => {
      if (route.request().method() === "PATCH") {
        expect(route.request().headers()["idempotency-key"]).toBe(sent.key);
        expect(route.request().postData()).toBe(sent.body);
      }
      return route.continue();
    });
    await b
      .getByRole("button", { name: "Retry register settings", exact: true })
      .click();
    await expect(
      b.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect((await call(`/budgets/${budget}/register-settings`)).revision).toBe(
      2,
    );
    await context.close();
    console.log(
      "PASS rejection belongs to original request; newer uncertain request and local draft survive",
    );
  }
  if (!run || run === "amount") {
    const { budget, account, context, page } = await setup();
    await call(`/budgets/${budget}/transactions`, {
      id: randomUUID(),
      account_id: account,
      date: "2026-01-02",
      amount: 7036874417766401,
      notes: "Exact cent",
    });
    await page
      .getByRole("button", { name: "Add condition", exact: true })
      .click();
    await page.getByLabel(/^Field 1/).selectOption("amount");
    await page.getByLabel(/^Operator 1/).selectOption("eq");
    await page.getByLabel("Amount value 1", { exact: true }).fill("");
    await page
      .getByLabel("Amount value 1", { exact: true })
      .pressSequentially("70368744177664.01");
    await page
      .getByRole("button", { name: "Apply filters and columns" })
      .click();
    await expect(page.getByTestId("filtered-totals")).toContainText(
      "1 matching transaction",
    );
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect(
      (await call(`/budgets/${budget}/register-settings`)).settings.views.all
        .query.filter.conditions[0].value,
    ).toBe(7036874417766401);
    await page.getByLabel(/^Operator 1/).selectOption("isbetween");
    await page
      .getByLabel("From amount 1", { exact: true })
      .fill("70368744177664.01");
    await page
      .getByLabel("To amount 1", { exact: true })
      .fill("70368744177664.01");
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect(
      (await call(`/budgets/${budget}/register-settings`)).settings.views.all
        .query.filter.conditions[0].value,
    ).toEqual({ num1: 7036874417766401, num2: 7036874417766401 });
    await page.reload();
    await page.locator('summary[aria-label="Filters and columns"]').click();
    await expect(page.getByLabel("From amount 1", { exact: true })).toHaveValue(
      "70368744177664.01",
    );
    await expect(page.getByLabel("To amount 1", { exact: true })).toHaveValue(
      "70368744177664.01",
    );
    await context.close();
    console.log(
      "PASS exact large amount and range input, results and persisted display",
    );
  }
  if (!run || run === "scope") {
    const { budget, context, page } = await setup(true);
    await expect(page.getByLabel(/^Account scope/)).toHaveCount(0);
    await expect(
      page.getByLabel("Show Account column", { exact: true }),
    ).toHaveCount(0);
    await page.getByLabel("Show Notes column", { exact: true }).uncheck();
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect(
      (await call(`/budgets/${budget}/register-settings`)).settings.views.all
        .columns,
    ).not.toContain("notes");
    await page.reload();
    await page.locator('summary[aria-label="Filters and columns"]').click();
    await expect(page.getByLabel(/^Account scope/)).toHaveCount(0);
    await expect(
      page.getByLabel("Show Notes column", { exact: true }),
    ).not.toBeChecked();
    await page.getByLabel("Show Notes column", { exact: true }).check();
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    await page.reload();
    await page.locator('summary[aria-label="Filters and columns"]').click();
    await expect(
      page.getByLabel("Show Notes column", { exact: true }),
    ).toBeChecked();
    await context.close();
    console.log(
      "PASS unified history view columns persist and restore without account controls",
    );
  }
  if (!run || run === "regex") {
    const { budget, account, context, page } = await setup();
    for (const notes of ["Safe", "a".repeat(64) + "!"])
      await call(`/budgets/${budget}/transactions`, {
        id: randomUUID(),
        account_id: account,
        date: "2026-01-02",
        amount: -100,
        notes,
      });
    await page.getByLabel("Search transactions").fill("Safe");
    await page
      .getByRole("button", { name: "Find transactions", exact: true })
      .click();
    await expect(page.getByTestId("filtered-totals")).toContainText(
      "1 matching transaction",
    );
    await page.getByLabel("Search transactions").fill("");
    await page
      .getByRole("button", { name: "Add condition", exact: true })
      .click();
    await page.getByLabel(/^Operator 1/).selectOption("matches");
    await page.getByLabel(/^Value\s+1/).fill("^(a+)+$");
    await page
      .getByRole("button", { name: "Apply filters and columns" })
      .click();
    await expect(page.getByRole("alert")).toContainText("work limit reached");
    await expect(page.getByLabel(/^Value\s+1/)).toHaveValue("^(a+)+$");
    await expect(page.getByTestId("filtered-totals")).toContainText(
      "1 matching transaction",
    );
    await expect(page.locator(".register-table")).toContainText("Safe");
    await page.getByLabel(/^Value\s+1/).fill("^safe$");
    await page
      .getByRole("button", { name: "Apply filters and columns" })
      .click();
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(page.getByTestId("filtered-totals")).toContainText(
      "1 matching transaction",
    );
    await context.close();
    console.log(
      "PASS regex work-limit failure preserves draft and confirmed rows; corrected pattern recovers",
    );
  }
  if (!run || run === "boundary") {
    const { budget, context, page } = await setup(true);
    for (const [amount, date] of [
      [-9007199254740991, "2026-03-01"],
      [9007199254740991, "2026-01-01"],
      [9007199254740991, "2026-01-02"],
    ]) {
      const account = randomUUID();
      await call(`/budgets/${budget}/accounts`, {
        id: account,
        name: date,
        opening_balance: 0,
        opening_date: "2025-01-01",
      });
      await call(`/budgets/${budget}/transactions`, {
        id: randomUUID(),
        account_id: account,
        date,
        amount,
        notes: "Boundary",
      });
    }
    await page.getByRole("button", { name: "Refresh", exact: true }).click();
    await expect(
      page.getByText("Outside exact range", { exact: true }),
    ).toBeVisible();
    await page.getByLabel("Search transactions").fill("Boundary");
    await page
      .getByRole("button", { name: "Find transactions", exact: true })
      .click();
    await expect(
      page.getByText("Outside exact range", { exact: true }),
    ).toBeVisible();
    await expect(page.getByTestId("filtered-totals")).toContainText(
      "90,071,992,547,409.91",
    );
    await context.close();
    console.log(
      "PASS intermediate running amount unavailable with reason; exact final total remains visible",
    );
  }
  if (!run || run === "identity") {
    const archive = await readFile(
      new URL(
        "../../tests/fixtures/register-settings-26.10.zip",
        import.meta.url,
      ),
    );
    const upload = await fetch(api + "/imports/actual", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: archive,
    });
    expect(upload.status).toBe(201);
    const staged = (await upload.json()).data;
    // Container suites share one server; the public import endpoint deduplicates
    // this same fixture after register-query has already activated it.
    expect(["ready", "activated"]).toContain(staged.status);
    const imported = await call(`/imports/${staged.id}/activate`, {});
    const source = await call(`/budgets/${imported.id}`);
    const copy = await call(
      `/budgets/${imported.id}/duplicate`,
      { id: randomUUID(), name: "Imported identity filter review" },
      "POST",
      { "X-Actual-Budget-Epoch": source.epoch },
    );
    const budget = copy.id;
    const context = await browser.newContext(),
      page = await context.newPage();
    await page.goto(server.url);
    await page.evaluate(
      (budget) =>
        localStorage.setItem(
          "actualBudget.selection",
          JSON.stringify({ budgetId: budget, accountId: null }),
        ),
      budget,
    );
    await page.reload();
    await expect(page.getByLabel("Search transactions")).toBeEnabled();
    await page.locator('summary[aria-label="Filters and columns"]').click();
    await page
      .getByRole("combobox", { name: /^Saved filter/ })
      .selectOption({ label: "Identity" });
    await expect(page.getByLabel(/^Field 1/)).toHaveValue("account");
    await expect(
      page.getByLabel(/^Field 1/).locator("option:checked"),
    ).toHaveText("Historical account");
    await expect(page.getByLabel(/^Value\s+1/)).toHaveValue(
      "0165d0a7-99e2-4719-a102-c007b7a8daaa",
    );
    await expect(
      page.getByLabel(/^Value\s+1/).locator("option:checked"),
    ).toHaveText("Spending");
    await mkdir(new URL("../test-results/", import.meta.url), {
      recursive: true,
    });
    await page.screenshot({
      path: "test-results/register-historical-filter-desktop.png",
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: "test-results/register-historical-filter-mobile.png",
      fullPage: true,
    });
    await page.setViewportSize({ width: 1280, height: 720 });
    const groupA = randomUUID(),
      groupB = randomUUID(),
      catA = randomUUID(),
      catB = randomUUID();
    for (const [id, name] of [
      [groupA, "House"],
      [groupB, "Travel"],
    ])
      await call(`/budgets/${budget}/category-groups`, {
        id,
        name,
        is_income: false,
      });
    for (const [id, group_id] of [
      [catA, groupA],
      [catB, groupB],
    ])
      await call(`/budgets/${budget}/categories`, {
        id,
        name: "Food",
        is_income: false,
        group_id,
      });
    await page.reload();
    await page.locator('summary[aria-label="Filters and columns"]').click();
    await page
      .getByRole("combobox", { name: /^Saved filter/ })
      .selectOption({ label: "Identity" });
    await page.getByLabel(/^Field 1/).selectOption("category");
    await expect(
      page.getByLabel(/^Field 1/).locator('option[value="account"]'),
    ).toHaveCount(0);
    const value = page.getByLabel(/^Value\s+1/);
    await expect(value.locator(`option[value="${catA}"]`)).toHaveText(
      "House / Food",
    );
    await expect(value.locator(`option[value="${catB}"]`)).toHaveText(
      "Travel / Food",
    );
    await value.selectOption(catB);
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect(
      (await call(`/budgets/${budget}/register-settings`)).settings.views.all
        .query.filter.conditions[0].value,
    ).toBe(catB);
    await value.selectOption("");
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect(
      (await call(`/budgets/${budget}/register-settings`)).settings.views.all
        .query.filter.conditions[0].value,
    ).toBe(null);
    await page.getByLabel(/^Field 1/).selectOption("category_group");
    await value.selectOption(groupA);
    await page.getByRole("button", { name: "Save view", exact: true }).click();
    await expect(
      page.getByText("Register settings saved.", { exact: true }),
    ).toBeVisible();
    expect(
      (await call(`/budgets/${budget}/register-settings`)).settings.views.all
        .query.filter.conditions[0],
    ).toMatchObject({ field: "category_group", value: groupA });
    await mkdir(new URL("../test-results/", import.meta.url), {
      recursive: true,
    });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.screenshot({
      path: new URL(
        "../test-results/register-identity-desktop.png",
        import.meta.url,
      ).pathname,
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({
      path: new URL(
        "../test-results/register-identity-mobile.png",
        import.meta.url,
      ).pathname,
      fullPage: true,
    });
    await page.addStyleTag({ content: "html{font-size:200% !important}" });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth + 1,
      ),
    ).toBe(true);
    await context.close();
    console.log(
      "PASS imported ID filter editing, duplicate category labels, null category and group authoring",
    );
  }
} finally {
  await browser?.close();
  await server.stop();
}

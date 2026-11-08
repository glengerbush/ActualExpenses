import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
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
    viewport: { width: 1280, height: 900 },
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  const api = async (path, body, method = "POST", key = randomUUID()) =>
    page.evaluate(
      async ({ path, body, method, key }) => {
        const response = await fetch(`/api/v1${path}`, {
          method,
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
          },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        if (!response.ok) throw Error(await response.text());
        return response.json();
      },
      { path, body, method, key },
    );
  const budget = randomUUID();
  await api("/budgets", {
    id: budget,
    name: "Preference oracle",
    budget_type: "tracking",
  });
  const fixture = JSON.parse(
    await readFile(
      new URL("./fixtures/preferences-format-26.10.0.json", import.meta.url),
      "utf8",
    ),
  );
  const amounts = [...new Set(fixture.cases.map((c) => c.amount))];
  for (const amount of amounts)
    await api(`/budgets/${budget}/accounts`, {
      id: randomUUID(),
      name: `Amount ${amount}`,
      opening_balance: amount,
      opening_date: "2026-11-08",
      off_budget: false,
    });
  await page.evaluate(
    (budget) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget }),
      ),
    budget,
  );
  for (const sample of fixture.cases.filter((c) => c.amount === 0)) {
    await api(
      `/budgets/${budget}/preferences`,
      {
        number_format: sample.number_format,
        hide_fraction: sample.hide_fraction,
        currency_code: sample.decimal_places === 0 ? "JPY" : "",
        currency_symbol_position: "after",
        currency_space: false,
      },
      "PATCH",
    );
    await page.reload();
    await expect(
      page.getByRole("heading", { name: "Accounts", exact: true }),
    ).toBeVisible();
    for (const expected of fixture.cases.filter(
      (c) =>
        c.number_format === sample.number_format &&
        c.hide_fraction === sample.hide_fraction &&
        c.decimal_places === sample.decimal_places,
    )) {
      const row = page.locator("tbody tr").filter({
        has: page.getByRole("button", {
          name: `Amount ${expected.amount}`,
          exact: true,
        }),
      });
      await expect(row.locator("td.amount")).toHaveText(
        expected.expected + (sample.decimal_places === 0 ? "¥" : ""),
      );
    }
  }
  console.log(
    "PASS 220 pinned Actual format results, including negative half ties and JPY",
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  for (const theme of ["dark", "midnight", "light"]) {
    await page.locator("#theme").selectOption(theme);
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await page.screenshot({
      path: `test-results/preferences-${theme}.png`,
      fullPage: true,
    });
  }
  await page.locator("#preferred-dark-theme").selectOption("midnight");
  await page.locator("#theme").selectOption("system");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "midnight");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.route("**/fr-*.json", (route) => route.abort());
  await page.locator("#language").selectOption("fr");
  await expect(
    page.getByRole("alert").filter({ hasText: "Language preference is saved" }),
  ).toBeVisible();
  await page.unroute("**/fr-*.json");
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "fr");
  await expect(page.locator("#settings-title")).toHaveText("Paramètres");
  await expect(
    page.getByRole("alert").filter({ hasText: "Language preference is saved" }),
  ).toHaveCount(0);
  await page.locator("#language").selectOption("en");
  await expect(page.locator("#settings-title")).toHaveText("Settings");
  await page.locator("#currency").selectOption("");
  await page.locator("#number-format").selectOption("dot-comma");
  await page.locator("#date-format").selectOption("dd/MM/yyyy");
  await page.locator("#week-start").selectOption("1");
  await page.getByLabel("Hide decimal places").uncheck();
  await page.getByRole("button", { name: "Save shared preferences" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "Shared preferences saved.",
  );
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await page.getByLabel("Opening date").fill("08/11/2026");
  await page.getByRole("button", { name: "Choose date" }).click();
  await expect(page.getByRole("columnheader").first()).toHaveText("Mon");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Opening date")).toHaveValue("09/11/2026");
  await page.getByLabel("Account name").fill("Locale entered");
  await page.getByLabel("Opening balance", { exact: true }).fill("1.234,56");
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(page.getByTestId("account-balance")).toHaveText("1.234,56");
  const saved = (
    await api(`/budgets/${budget}/accounts`, undefined, "GET")
  ).data.find((a) => a.name === "Locale entered");
  expect(saved.balance).toBe(123456);
  console.log(
    "PASS themes/system changes, shipped French catalog, localized entry and week-start keyboard calendar",
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.locator("#number-format").selectOption("space-comma");
  let writes = [];
  let lose = true;
  await page.route(`**/api/v1/budgets/${budget}/preferences`, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    writes.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
    });
    if (lose) {
      lose = false;
      const url = new URL(route.request().url());
      const host = url.host;
      url.hostname = "127.0.0.1";
      await route.fetch({
        url: url.href,
        headers: { ...route.request().headers(), host },
      });
      await route.abort();
    } else await route.continue();
  });
  await page.getByRole("button", { name: "Save shared preferences" }).click();
  await expect(
    page.getByRole("button", { name: "Retry saving" }),
  ).toBeVisible();
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.locator("#number-format")).toHaveValue("space-comma");
  expect(writes.length).toBe(1);
  await page.getByRole("button", { name: "Retry saving" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "Shared preferences saved.",
  );
  expect(writes.length).toBe(2);
  expect(writes[1]).toEqual(writes[0]);
  await page.unroute(`**/api/v1/budgets/${budget}/preferences`);
  await page.locator("#date-format").selectOption("dd.MM.yyyy");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.locator("#date-format")).toHaveValue("dd.MM.yyyy");
  await page.getByRole("button", { name: "Discard changes" }).click();
  console.log(
    "PASS persisted settings drafts and lost committed response recovery with stable identity",
  );
  const other = await browser.newContext();
  const second = await other.newPage();
  await second.goto(server.url);
  await second.evaluate(
    (budget) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget }),
      ),
    budget,
  );
  await second.reload();
  await second.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(second.locator("#number-format")).toHaveValue("space-comma");
  await expect(second.locator("#preferred-dark-theme")).toHaveValue("dark");
  await other.close();
  // A new browser with failed preferences must not guess currency precision.
  const unavailable = await browser.newContext();
  const blocked = await unavailable.newPage();
  await blocked.route("**/preferences", (route) => route.abort());
  await blocked.goto(server.url);
  await blocked.evaluate(
    (budget) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget }),
      ),
    budget,
  );
  await blocked.reload();
  await expect(
    blocked.getByRole("heading", {
      name: "Display preferences are unavailable",
    }),
  ).toBeVisible();
  await expect(
    blocked.getByRole("button", { name: "Add account", exact: true }),
  ).toHaveCount(0);
  await unavailable.close();

  // Device storage failure must be visible and must not mutate shared settings.
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.evaluate(() => {
    window.savedStorageWrite = Storage.prototype.setItem;
    Storage.prototype.setItem = function () {
      throw new DOMException("Full", "QuotaExceededError");
    };
  });
  let blockedWrites = 0;
  await page.route("**/preferences", (route) => {
    if (route.request().method() === "PATCH") blockedWrites++;
    return route.continue();
  });
  await page.locator("#number-format").selectOption("dot-comma");
  await page.getByRole("button", { name: "Save shared preferences" }).click();
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Cannot save preference changes" }),
  ).toBeVisible();
  expect(blockedWrites).toBe(0);
  await page.evaluate(() => {
    Storage.prototype.setItem = window.savedStorageWrite;
  });
  await page.unroute("**/preferences");
  await page.getByRole("button", { name: "Discard changes" }).click();
  // Original account body remains retryable after reload even if preferences are unavailable.
  await page.getByRole("button", { name: "All accounts", exact: true }).click();
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await page.getByLabel("Account name").fill("Recover without preferences");
  await page.getByLabel("Opening balance", { exact: true }).fill("12,34");
  let accountWrites = [];
  let loseAccount = true;
  await page.route(`**/api/v1/budgets/${budget}/accounts`, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    accountWrites.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
    });
    if (loseAccount) {
      loseAccount = false;
      const url = new URL(route.request().url());
      const host = url.host;
      url.hostname = "127.0.0.1";
      await route.fetch({
        url: url.href,
        headers: { ...route.request().headers(), host },
      });
      return route.abort();
    }
    return route.continue();
  });
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry creation" }),
  ).toBeVisible();
  await page.evaluate(
    (budget) =>
      localStorage.removeItem(`actualBudget.preferencesCache.v1.${budget}`),
    budget,
  );
  await page.route("**/preferences", (route) => route.abort());
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry creation" }),
  ).toBeVisible();
  await expect(page.getByLabel("Account name")).toHaveValue(
    "Recover without preferences",
  );
  expect(accountWrites.length).toBe(1);
  await page.getByRole("button", { name: "Retry creation" }).click();
  await expect(
    page.getByRole("heading", { name: "Display preferences are unavailable" }),
  ).toBeVisible();
  expect(accountWrites.length).toBe(2);
  expect(accountWrites[1]).toEqual(accountWrites[0]);
  expect(JSON.parse(accountWrites[0].body).opening_balance).toBe(1234);
  await page.unroute("**/preferences");
  await page.unroute(`**/api/v1/budgets/${budget}/accounts`);
  await page.getByRole("button", { name: "Retry preferences" }).click();
  await expect(page.getByTestId("account-balance")).toHaveText("12,34");
  console.log(
    "PASS storage failure prevents PATCH and original account body retries without cached preferences",
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.setViewportSize({ width: 320, height: 900 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addStyleTag({ content: "html{font-size:200%}" });
  for (const theme of ["light", "dark", "midnight"]) {
    await page.locator("#theme").selectOption(theme);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `test-results/preferences-${theme}-320-large-text.png`,
      fullPage: true,
    });
  }
  expect(errors).toEqual([]);
  console.log(
    "PASS shared/device scope, unavailable-preference gate, 320px/200% text and reduced motion",
  );
  await context.close();
} finally {
  await browser?.close();
  await server.stop();
}

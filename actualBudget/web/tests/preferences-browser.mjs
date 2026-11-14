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
  const destination = (await api(`/budgets/${budget}/ledger`, {})).data.id;
  const transactionIds = new Map();
  for (const amount of amounts) {
    const id = randomUUID();
    transactionIds.set(amount, id);
    await api(`/budgets/${budget}/transactions`, {
      id,
      account_id: destination,
      amount,
      date: "2026-11-08",
    });
  }
  await page.reload();
  await page.waitForLoadState("networkidle");
  await expect(page.getByTestId("register-total")).toBeVisible();
  for (const sample of fixture.cases.filter(
    (c) => c.amount === 0 && c.decimal_places === 2,
  )) {
    await api(
      `/budgets/${budget}/preferences`,
      {
        number_format: sample.number_format,
        hide_fraction: sample.hide_fraction,
      },
      "PATCH",
    );
    await page.reload();
    await page.waitForLoadState("networkidle");
    await expect(
      page.getByRole("heading", { name: "Transactions", exact: true }),
    ).toBeVisible();
    for (const expected of fixture.cases.filter(
      (c) =>
        c.number_format === sample.number_format &&
        c.hide_fraction === sample.hide_fraction &&
        c.decimal_places === sample.decimal_places,
    )) {
      const row = page.locator(
        `[data-transaction-id="${transactionIds.get(expected.amount)}"]`,
      );
      await expect(row.locator("td.amount").first()).toHaveText(
        (expected.amount > 0 ? "+" : "") + expected.expected,
      );
    }
  }
  console.log(
    "PASS retained pinned Actual number formats and negative half ties",
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.locator("#currency")).toHaveCount(0);
  await expect(
    page.getByText("Experimental features", { exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByLabel("Currency display", { exact: true }),
  ).toHaveCount(0);
  for (const theme of ["dark", "light"]) {
    await page.locator("#theme").selectOption(theme);
    await expect(page.locator("html")).toHaveAttribute(
      "data-theme",
      theme === "dark" ? "midnight" : "light",
    );
    await page.screenshot({
      path: `test-results/preferences-${theme}.png`,
      fullPage: true,
    });
  }
  await expect(page.locator("#preferred-dark-theme")).toHaveCount(0);
  await expect(page.locator("#theme option")).toHaveCount(3);
  // Both former palettes migrate to the only dark palette without resetting other device settings.
  await page.emulateMedia({ colorScheme: "dark" });
  for (const oldTheme of ["dark", "midnight", "system"]) {
    await page.evaluate(
      (theme) =>
        localStorage.setItem(
          "actualBudget.devicePreferences.v1",
          JSON.stringify({
            version: 1,
            values: {
              theme,
              preferred_dark_theme: "dark",
              language: "",
              show_release_notices: false,
              seen_release: "0.5.0",
            },
          }),
        ),
      oldTheme,
    );
    await page.reload();
    const mode = oldTheme === "system" ? "system" : "dark";
    await expect(page.locator("#theme")).toHaveValue(mode);
    await expect(page.locator("html")).toHaveAttribute(
      "data-theme",
      "midnight",
    );
    const saved = await page.evaluate(
      () =>
        JSON.parse(localStorage.getItem("actualBudget.devicePreferences.v1"))
          .values,
    );
    expect(saved).toEqual({
      theme: mode,
      language: "",
      show_release_notices: false,
      seen_release: "0.5.0",
    });
  }
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
  await page.locator("#number-format").selectOption("dot-comma");
  await page.locator("#date-format").selectOption("dd/MM/yyyy");
  await page.locator("#week-start").selectOption("1");
  await page.getByLabel("Hide decimal places").uncheck();
  await page.getByRole("button", { name: "Save shared preferences" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Shared preferences saved." }),
  ).toHaveText("Shared preferences saved.");
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Date", { exact: true }).fill("08/11/2026");
  await page
    .getByRole("region", { name: "Add transaction", exact: true })
    .getByRole("button", { name: "Choose date", exact: true })
    .click();
  await expect(
    page.getByRole("grid").getByRole("columnheader").first(),
  ).toHaveText("Mon");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Date", { exact: true })).toHaveValue(
    "09/11/2026",
  );
  await page.getByLabel("Amount", { exact: true }).fill("1.234,56");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect
    .poll(async () =>
      (
        await api(`/budgets/${budget}/transactions`, undefined, "GET")
      ).data.some((row) => row.amount === -123456 && row.date === "2026-11-09"),
    )
    .toBe(true);
  const saved = (
    await api(`/budgets/${budget}/transactions`, undefined, "GET")
  ).data.find((row) => row.amount === -123456 && row.date === "2026-11-09");
  expect(saved.amount).toBe(-123456);
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
  await page.waitForLoadState("networkidle");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.locator("#number-format")).toHaveValue("space-comma");
  expect(writes.length).toBe(1);
  await page.getByRole("button", { name: "Retry saving" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Shared preferences saved." }),
  ).toHaveText("Shared preferences saved.");
  expect(writes.length).toBe(2);
  expect(writes[1]).toEqual(writes[0]);
  await page.unroute(`**/api/v1/budgets/${budget}/preferences`);
  await page.locator("#date-format").selectOption("dd.MM.yyyy");
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await page.reload();
  await page.waitForLoadState("networkidle");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.locator("#date-format")).toHaveValue("dd.MM.yyyy");
  await page.getByRole("button", { name: "Discard changes" }).click();
  console.log(
    "PASS persisted settings drafts and lost committed response recovery with stable identity",
  );
  const other = await browser.newContext();
  const second = await other.newPage();
  await second.goto(server.url);
  await second.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(second.locator("#number-format")).toHaveValue("space-comma");
  await expect(second.locator("#preferred-dark-theme")).toHaveCount(0);
  await expect(second.locator("#theme")).toHaveValue("system");
  await other.close();
  // A new browser with failed preferences must not guess number formatting.
  const unavailable = await browser.newContext();
  const blocked = await unavailable.newPage();
  await blocked.route("**/preferences**", (route) => route.abort());
  await blocked.goto(server.url);
  await expect(
    blocked.getByRole("heading", {
      name: "Display preferences are unavailable",
    }),
  ).toBeVisible();
  await expect(
    blocked.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeDisabled();
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
  await page.route("**/preferences**", (route) => {
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
  await page.unroute("**/preferences**");
  await page.getByRole("button", { name: "Discard changes" }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.setViewportSize({ width: 320, height: 900 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addStyleTag({ content: "html{font-size:200%}" });
  for (const theme of ["light", "dark"]) {
    await page.locator("#theme").selectOption(theme);
    const tools = page.getByRole("button", {
      name: "Search commands",
      exact: true,
    });
    expect(
      await tools.evaluate(
        (el) =>
          el.getBoundingClientRect().height /
          parseFloat(getComputedStyle(el).lineHeight),
      ),
    ).toBeLessThanOrEqual(3);
    await page.screenshot({
      path: `test-results/preferences-${theme}-320-large-text.png`,
      fullPage: true,
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
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

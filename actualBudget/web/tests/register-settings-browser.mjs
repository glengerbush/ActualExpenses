import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function post(path, body) {
  const r = await fetch(api + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.status).toBe(201);
  return (await r.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID();
  await post("/budgets", { id: budget, name: "Find transactions" });
  await post(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Spending",
    opening_balance: 10000,
    opening_date: "2026-01-01",
  });
  for (const [amount, notes, cleared] of [
    [-1250, "Coffee", true],
    [-375, "Coffee beans", false],
    [2400, "Refund", true],
  ])
    await post(`/budgets/${budget}/transactions`, {
      id: randomUUID(),
      account_id: account,
      date: "2026-01-02",
      amount,
      notes,
      cleared,
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
    }),
    page = await context.newPage(),
    errors = [];
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
  await expect(page.getByLabel("Search transactions")).toBeEnabled();
  await page.getByLabel("Search transactions").fill("Coffee");
  await page
    .getByRole("button", { name: "Find transactions", exact: true })
    .click();
  await expect(page.getByTestId("filtered-totals")).toContainText(
    "2 matching transactions",
  );
  await expect(page.getByTestId("filtered-totals")).toContainText("-16.25");
  await page.getByText("Filters and columns", { exact: true }).click();
  await page.getByLabel("Sort by").selectOption("amount");
  await page.getByLabel(/^Order/).selectOption("asc");
  await page.getByLabel("Show Notes column", { exact: true }).uncheck();
  await page.getByRole("button", { name: "Apply filters and columns" }).click();
  await expect(page.locator(".register-table tbody tr").first()).toContainText(
    "-12.50",
  );
  await expect(
    page.getByRole("columnheader", { name: "Notes", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Filter name", { exact: true }).fill("Coffee review");
  await page.getByRole("button", { name: "Save filter", exact: true }).click();
  await expect(
    page.getByText("Register settings saved.", { exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(page.getByLabel("Search transactions")).toHaveValue("Coffee");
  await expect(page.getByTestId("filtered-totals")).toContainText(
    "2 matching transactions",
  );
  // Keyboard navigation goes row-to-row without changing transaction values.
  const edits = page
    .locator(".register-table tbody")
    .getByRole("button", { name: /^Edit / });
  await edits.first().focus();
  await page.keyboard.press("ArrowDown");
  await expect(edits.nth(1)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(edits.first()).toBeFocused();
  await page.getByText("Filters and columns", { exact: true }).click();
  await page.getByLabel("Hide cleared transactions", { exact: true }).check();
  await page.getByRole("button", { name: "Apply filters and columns" }).click();
  await expect(page.getByTestId("filtered-totals")).toContainText(
    "1 matching transactions",
  );
  await expect(page.getByTestId("filtered-totals")).toContainText("-3.75");
  const stale = await context.newPage();
  await stale.goto(server.url);
  await expect(stale.getByLabel("Search transactions")).toBeEnabled();
  // Lose response after commit; reload and retry the exact operation.
  const attempts = [];
  let lost = false;
  await page.route("**/register-settings", async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    attempts.push([
      route.request().headers()["idempotency-key"],
      route.request().postData(),
    ]);
    if (!lost) {
      lost = true;
      const url = new URL(route.request().url()),
        headers = { ...route.request().headers(), host: url.host };
      url.hostname = "127.0.0.1";
      const response = await route.fetch({ url: url.href, headers });
      expect(response.ok()).toBe(true);
      await route.abort();
    } else await route.continue();
  });
  await page.getByRole("button", { name: "Save view", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry register settings", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Retry register settings", exact: true }),
  ).toBeEnabled();
  await expect(stale.getByLabel("Search transactions")).toBeDisabled();
  await page.reload();
  await page
    .getByRole("button", { name: "Retry register settings", exact: true })
    .click();
  await expect(
    page.getByText("Register settings saved.", { exact: true }),
  ).toBeVisible();
  expect(attempts.length).toBe(2);
  expect(attempts[0]).toEqual(attempts[1]);
  await stale.getByText("Filters and columns", { exact: true }).click();
  await stale.getByRole("button", { name: "Save view", exact: true }).click();
  await expect(stale.getByRole("alert")).toContainText("another tab");
  await stale.getByRole("button", { name: "Reload register settings" }).click();
  await expect(
    stale.getByLabel("Hide cleared transactions", { exact: true }),
  ).not.toBeChecked();
  await stale.close();
  await page.getByText("Filters and columns", { exact: true }).click();
  await page
    .getByRole("combobox", { name: /^Saved filter/ })
    .selectOption({ label: "Coffee review" });
  await expect(page.getByTestId("filtered-totals")).toContainText(
    "2 matching transactions",
  );
  await page
    .getByRole("button", { name: "Add condition", exact: true })
    .click();
  await page.getByLabel(/^Field 1/).selectOption("amount");
  await page.getByLabel(/^Operator 1/).selectOption("lt");
  await page.getByLabel("Amount value 1", { exact: true }).fill("0");
  await page.getByRole("button", { name: "Apply filters and columns" }).click();
  await expect(page.getByTestId("filtered-totals")).toContainText("-16.25");
  await mkdir(new URL("../test-results/", import.meta.url), {
    recursive: true,
  });
  await page.screenshot({
    path: new URL(
      "../test-results/register-settings-desktop.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await page.screenshot({
    path: new URL(
      "../test-results/register-settings-mobile.png",
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
  await page.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await expect(
    page.getByRole("button", { name: "Save view", exact: true }),
  ).toBeDisabled();
  await expect(page.getByLabel("Search transactions")).toBeDisabled();
  expect(errors).toEqual([]);
  console.log(
    "PASS register search/sort/filter/columns, persistence, lost response retry, stale-tab CAS, keyboard, mobile/large text, HTTP and offline boundary",
  );
} finally {
  await browser?.close();
  await server.stop();
}

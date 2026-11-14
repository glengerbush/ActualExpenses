import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function raw(path, body, method = body === undefined ? "GET" : "POST") {
  return fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}
async function call(path, body, method) {
  const r = await raw(path, body, method);
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
}
let browser;
try {
  const b = randomUUID(),
    a = randomUUID(),
    g = randomUUID(),
    c = randomUUID(),
    tx = randomUUID(),
    root = `/budgets/${b}`;
  await call("/budgets", { id: b, name: "Compact editing" });
  await call(root + "/accounts", {
    id: a,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  await call(root + "/category-groups", {
    id: g,
    name: "Living",
    color: "#397C8C",
  });
  await call(root + "/categories", { id: c, name: "Food", group_id: g });
  await call(root + "/transactions", {
    id: tx,
    account_id: a,
    category_id: c,
    date: "2026-11-01",
    amount: -1250,
    notes: "Retired note",
  });
  expect((await call(root + `/transactions/${tx}`)).data.notes).toBe("");
  for (const [path, body, method] of [
    [root + `/categories/${c}`, { notes: "No" }, "PATCH"],
    [root + `/category-groups/${g}`, { notes: "No" }, "PATCH"],
    [root + "/notes/commands", { id: c, note: "No" }],
  ])
    expect((await raw(path, body, method)).status).toBe(400);
  expect((await call(root + `/categories/${c}`)).data).not.toHaveProperty(
    "notes",
  );
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByText("Compact editing", { exact: true }).click();
  await page.locator(`[data-transaction-id="${tx}"]`).click();
  await expect(page.getByLabel("Notes", { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole("radio", { name: "Expense", exact: true }),
  ).toBeChecked();
  await page.getByRole("radio", { name: "Income", exact: true }).check();
  await expect(
    page.getByRole("radio", { name: "Income", exact: true }),
  ).toBeChecked();
  await page.getByRole("switch", { name: "Recurring" }).check();
  await expect(page.getByLabel("Repeat interval")).toBeVisible();
  await page.getByRole("switch", { name: "Recurring" }).uncheck();
  await page.screenshot({ path: "../../.scratch/compact-transaction.png" });
  await page.getByRole("switch", { name: "Recurring" }).check();
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".transaction-form")).toHaveCount(0);
  expect(
    (await call(root + `/transactions/${tx}`)).data.recurrence.enabled,
  ).toBe(true);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  const month = page.getByRole("textbox", {
    name: "Budget month",
    exact: true,
  });
  const originalMonth = await month.inputValue();
  expect(originalMonth).toMatch(/^[A-Z][a-z]{2}-\d{2}$/);
  await page
    .getByRole("button", { name: "Previous month", exact: true })
    .click();
  await expect(month).not.toHaveValue(originalMonth);
  await page.getByRole("button", { name: "This month", exact: true }).click();
  await expect(month).toHaveValue(originalMonth);
  await month.fill("Nov-26");
  await month.press("Enter");
  const color = page
    .locator(`.budget-category[data-id="${c}"]`)
    .first()
    .getByLabel("Color for Food", { exact: true });
  await expect(color).toBeVisible();
  await color.evaluate((input) => {
    input.value = "#112233";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await expect
    .poll(async () => (await call(root + `/categories/${c}`)).data.color)
    .toBe("#112233");
  await expect(color).toHaveValue("#112233");
  await expect(
    page.getByRole("button", {
      name: "Use automatic shade for Food",
      exact: true,
    }),
  ).toHaveCount(0);
  await expect(color).toHaveValue("#112233");
  const input = page.getByRole("textbox", {
    name: "Planned Food for November 2026",
    exact: true,
  });
  await expect(input).toBeVisible();
  let posts = [];
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith(root + "/budget"))
      posts.push(r.postDataJSON());
  });
  await input.focus();
  await input.blur();
  await page.waitForTimeout(1100);
  expect(posts).toHaveLength(0);
  await input.fill("20.00");
  await expect.poll(() => posts.length).toBe(1);
  await expect(
    page.getByText("Budget change saved.", { exact: true }),
  ).toBeVisible();
  await expect(input).toHaveValue("20.00");
  expect(posts[0].amount).toBe(2000);
  await input.fill("30.00");
  await input.blur();
  await expect.poll(() => posts.length).toBe(2);
  await expect(input).toHaveValue("30.00");
  await expect(
    page.getByRole("button", {
      name: "Edit Food for November 2026",
      exact: true,
    }),
  ).toBeEnabled();
  // Typing through an in-flight write must retain and subsequently save the latest amount.
  let release;
  let entered;
  const held = new Promise((r) => (entered = r)),
    gate = new Promise((r) => (release = r));
  await page.route("**/api/v1" + root + "/budget", async (route) => {
    if (route.request().method() === "POST") {
      entered();
      await gate;
    }
    await route.continue();
  });
  await input.fill("40.00");
  await held;
  await input.fill("50.00");
  await input.blur();
  release();
  await expect
    .poll(
      async () =>
        (await call(root + "/budget?month=2026-11&count=1")).data.months[
          "2026-11"
        ].categories.find((x) => x.id === c).budgeted,
    )
    .toBe(5000);
  await expect(
    page.getByRole("button", {
      name: "Edit Food for November 2026",
      exact: true,
    }),
  ).toBeEnabled();
  await page.unroute("**/api/v1" + root + "/budget");
  await expect(input).toHaveValue("50.00");
  // An unknown network outcome is kept across reload and retried with the same operation key.
  let failedKey;
  await page.route("**/api/v1" + root + "/budget", async (route) => {
    if (route.request().method() === "POST") {
      failedKey = route.request().headers()["idempotency-key"];
      await route.abort("failed");
    } else await route.continue();
  });
  await input.fill("60.00");
  await input.blur();
  await expect(
    page.getByRole("button", { name: "Retry saved amount", exact: true }),
  ).toBeEnabled();
  await page.unroute("**/api/v1" + root + "/budget");
  await page.reload();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await expect(input).toHaveValue("60.00");
  let retryKey;
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith(root + "/budget"))
      retryKey = r.headers()["idempotency-key"];
  });
  await page
    .getByRole("button", { name: "Retry saved amount", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Edit Food for November 2026",
      exact: true,
    }),
  ).toBeEnabled();
  expect(retryKey).toBe(failedKey);
  await expect
    .poll(
      async () =>
        (await call(root + "/budget?month=2026-11&count=1")).data.months[
          "2026-11"
        ].categories.find((x) => x.id === c).budgeted,
    )
    .toBe(6000);
  const alignment = await page.evaluate(() => {
    const box = (s) => document.querySelector(s).getBoundingClientRect();
    const a = box(".month-picker"),
      b = box(".months-count");
    return {
      delta: Math.abs(a.y + a.height / 2 - b.y - b.height / 2),
      margin: getComputedStyle(
        document.querySelector("#planning-title").closest(".page-heading"),
      ).marginBottom,
    };
  });
  expect(alignment.delta).toBeLessThan(1);
  expect(alignment.margin).toBe("0px");
  await expect(
    page.getByText("Compare planned income and expenses with actuals."),
  ).toHaveCount(0);
  // Invalid text remains editable, without a write.
  const before = posts.length;
  await input.fill("not money");
  await input.blur();
  await expect(page.getByRole("alert")).toBeVisible();
  expect(posts).toHaveLength(before);
  await page.waitForTimeout(5200);
  await expect(page.locator(".toast.failure")).toBeVisible();
  await page
    .getByRole("button", { name: "Dismiss error", exact: true })
    .click();
  await expect(page.locator(".toast.failure")).toHaveCount(0);
  await expect(input).toHaveValue("not money");
  await page
    .getByRole("button", { name: "Discard amount", exact: true })
    .click();
  await expect(page.locator(".toast.success")).toContainText(
    "Budget draft discarded.",
  );
  expect(
    await page
      .locator(".notifications")
      .evaluate((e) => getComputedStyle(e).position),
  ).toBe("fixed");
  await page.screenshot({ path: "../../.scratch/budget-toast.png" });
  await expect(page.locator(".toast.success")).toHaveCount(0, {
    timeout: 6500,
  });
  await page
    .getByRole("button", { name: "Edit Food for November 2026", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Budget options for Food" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Notes", exact: true }),
  ).toHaveCount(0);
  for (const name of ["Category settings", "Copy year", "Allocation"]) {
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(
      0,
    );
  }
  await expect(
    page.locator(".plan-form fieldset > p").filter({ hasText: /^Food$/ }),
  ).toHaveCount(0);
  await page.locator(".brand").click();
  await expect(
    page.getByRole("region", { name: "Budget options for Food" }),
  ).toHaveCount(0);
  await page.screenshot({ path: "../../.scratch/compact-budget.png" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: "../../.scratch/compact-budget-mobile.png" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "PASS compact editors, notes retirement, month controls, debounce, blur and in-flight typing",
  );
} finally {
  await browser?.close();
  await server.stop();
}

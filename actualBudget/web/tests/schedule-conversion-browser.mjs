import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return response.json();
}
const budget = randomUUID(),
  account = randomUUID(),
  destination = randomUUID(),
  future = new Date();
future.setUTCDate(future.getUTCDate() + 8);
const date = future.toISOString().slice(0, 10),
  inputDate = `${date.slice(5, 7)}/${date.slice(8)}/${date.slice(0, 4)}`;
let browser, page;
try {
  await call("/budgets", { id: budget, name: "Future conversion" });
  for (const [id, name] of [
    [account, "Checking"],
    [destination, "Savings"],
  ])
    await call(`/budgets/${budget}/accounts`, {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
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
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("12.34");
  await page.getByLabel("Date", { exact: true }).fill(inputDate);
  await page.getByLabel("Payee", { exact: true }).selectOption("new");
  await page.getByLabel("New payee name").fill("Future merchant");
  await page.getByLabel("Category", { exact: true }).selectOption("new");
  await page.getByLabel("New category name").fill("Future category");
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Keep original conversion identity");
  await page.getByLabel("Cleared", { exact: true }).check();
  const original = await page.evaluate(
    (b) => JSON.parse(localStorage.getItem(`actualBudget.transaction.v1.${b}`)),
    budget,
  );
  expect(original.pending).toBeNull();
  await page
    .getByRole("button", { name: "Schedule future transaction", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Confirm schedule conversion",
      exact: true,
    }),
  ).toBeEnabled();
  await expect(page.locator(".conversion")).toContainText("Future merchant");
  await expect(page.locator(".conversion")).toContainText("Future category");
  await expect(page.locator(".conversion")).toContainText("12.34");
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(0);
  expect(
    (await call(`/budgets/${budget}/payees`)).data.some(
      (p) => p.name === "Future merchant",
    ),
  ).toBe(false);
  await mkdir("test-results/schedule-conversion", { recursive: true });
  await page.screenshot({
    path: "test-results/schedule-conversion/preview-desktop.png",
    fullPage: true,
  });
  await page.reload();
  await expect(page.locator(".conversion")).toContainText(
    "Keep original conversion identity",
  );
  const captured = [];
  let dropped = false;
  await page.route(`**/budgets/${budget}/schedules/commands`, async (route) => {
    const req = route.request();
    if (req.postDataJSON()?.action !== "convert") return route.continue();
    captured.push({
      body: req.postData(),
      key: req.headers()["idempotency-key"],
      clock: req.headers()["x-actual-edit-clock"],
      epoch: req.headers()["x-actual-budget-epoch"],
      owner: req.headers()["x-actual-history-owner"],
    });
    if (!dropped) {
      dropped = true;
      const forwarded = await route.fetch({
        url: req.url().replace("actualbudget.test", "127.0.0.1"),
        headers: { ...req.headers(), host: new URL(req.url()).host },
      });
      expect(forwarded.status(), await forwarded.text()).toBe(201);
      return route.abort("connectionfailed");
    }
    return route.continue();
  });
  await page
    .getByRole("button", { name: "Confirm schedule conversion", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved conversion", exact: true }),
  ).toBeVisible();
  const pending = await page.evaluate(
    (b) =>
      JSON.parse(
        localStorage.getItem(`actualBudget.future-conversion.v1.${b}`),
      ),
    budget,
  );
  expect(pending.pending.body.transaction.id).toBe(original.id);
  expect(pending.pending.body.transaction.new_payee.name).toBe(
    "Future merchant",
  );
  const committedBeforeRetry = (await call(`/budgets/${budget}/schedules`)).data
    .schedules;
  expect(committedBeforeRetry).toHaveLength(1);
  expect(committedBeforeRetry[0].id).toBe(pending.pending.body.id);
  const reservedBeforeRetry = await fetch(
    api + `/budgets/${budget}/transactions`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body: JSON.stringify({
        id: original.id,
        account_id: account,
        date,
        amount: -1234,
        notes: "Original save before retry",
      }),
    },
  );
  expect(reservedBeforeRetry.status).toBe(409);
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved conversion", exact: true })
    .click();
  await expect(
    page.getByRole("heading", {
      name: "Schedule a future transaction",
      exact: true,
    }),
  ).toHaveCount(0);
  expect(captured).toHaveLength(2);
  expect(captured[1]).toEqual(captured[0]);
  expect(
    await page.evaluate(
      (b) => localStorage.getItem(`actualBudget.transaction.v1.${b}`),
      budget,
    ),
  ).toBeNull();
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(0);
  const schedules = (await call(`/budgets/${budget}/schedules`)).data.schedules;
  expect(schedules).toHaveLength(1);
  expect(schedules[0].conditions.find((c) => c.field === "amount").value).toBe(
    -1234,
  );
  // A pre-conversion tab cannot record the original draft identity afterward.
  const late = await fetch(api + `/budgets/${budget}/transactions`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify({
      id: original.id,
      account_id: account,
      date,
      amount: -1234,
      notes: "Late original save",
    }),
  });
  expect(late.status).toBe(409);
  // An existing future entry shows its complete replacement graph before conversion.
  const existing = randomUUID();
  await call(`/budgets/${budget}/transactions`, {
    id: existing,
    account_id: account,
    date,
    amount: -2000,
    notes: "Saved future entry",
    cleared: true,
  });
  await page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Saved future entry" })
    .getByRole("button", { name: /^Schedule future / })
    .click();
  await expect(page.locator(".conversion")).toContainText(
    "Existing entries replaced",
  );
  await expect(page.locator(".conversion")).toContainText(
    "Cleared · Not reconciled",
  );
  const remote = await fetch(
    api + `/budgets/${budget}/transactions/${existing}`,
    {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body: JSON.stringify({ notes: "Newer remote future note" }),
    },
  );
  expect(remote.ok, await remote.clone().text()).toBe(true);
  await page
    .getByRole("button", { name: "Confirm schedule conversion", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved conversion", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".conversion [role=alert]")).toBeVisible();
  const stale = await page.evaluate(
    (b) =>
      JSON.parse(localStorage.getItem(`actualBudget.future-conversion.v1.${b}`))
        .pending,
    budget,
  );
  await page.reload();
  expect(
    await page.evaluate(
      (b) =>
        JSON.parse(
          localStorage.getItem(`actualBudget.future-conversion.v1.${b}`),
        ).pending,
      budget,
    ),
  ).toEqual(stale);
  await page
    .getByRole("button", {
      name: "Cancel saved conversion safely",
      exact: true,
    })
    .click();
  await expect(page.locator(".conversion")).toContainText(
    "saved conversion was cancelled",
  );
  await page
    .getByRole("button", { name: "Keep original transaction", exact: true })
    .click();
  expect(
    (await call(`/budgets/${budget}/transactions/${existing}`)).data.notes,
  ).toBe("Newer remote future note");
  // A new expense draft preserves its original identity during schedule conversion.
  await page.getByRole("button", { name: "Add transaction", exact: true }).click();
  await page.getByLabel("Amount", { exact: true }).fill("5.00");
  await page.getByLabel("Date", { exact: true }).fill(inputDate);
  await page
    .getByRole("button", { name: "Schedule future transaction", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Confirm schedule conversion",
      exact: true,
    }),
  ).toBeEnabled();
  for (const [name, dark, large] of [
    ["preview-mobile", false, false],
    ["preview-large-text", false, true],
    ["preview-dark-large-text", true, true],
  ]) {
    await page.setViewportSize({ width: 390, height: 1000 });
    await page.emulateMedia({ colorScheme: dark ? "dark" : "light" });
    await page.evaluate(
      (large) =>
        (document.documentElement.style.fontSize = large ? "200%" : ""),
      large,
    );
    await page.screenshot({
      path: `test-results/schedule-conversion/${name}.png`,
      fullPage: true,
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.locator("#future-conversion-title").scrollIntoViewIfNeeded();
    await page.screenshot({
      path: `test-results/schedule-conversion/${name}-viewport.png`,
    });
    await page
      .getByRole("button", { name: "Confirm schedule conversion", exact: true })
      .scrollIntoViewIfNeeded();
    await page.screenshot({
      path: `test-results/schedule-conversion/${name}-controls.png`,
    });
  }
  await page
    .getByRole("button", { name: "Confirm schedule conversion", exact: true })
    .click();
  await expect(
    page.getByRole("heading", {
      name: "Schedule a future transaction",
      exact: true,
    }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      (b) => localStorage.getItem(`actualBudget.transaction.v1.${b}`),
      budget,
    ),
  ).toBeNull();
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(1);
  expect(
    (await call(`/budgets/${budget}/schedules`)).data.schedules,
  ).toHaveLength(2);
  expect(errors).toEqual([]);
  console.log(
    "PASS future draft/existing conversion, write-free review, immutable lost-ACK and stale-snapshot cancellation recovery, original-ID reservation and responsive review",
  );
} catch (e) {
  if (page) {
    await mkdir("test-results/schedule-conversion", { recursive: true });
    await writeFile(
      "test-results/schedule-conversion/failure.html",
      await page.content(),
    );
    await page.screenshot({
      path: "test-results/schedule-conversion/failure.png",
      fullPage: true,
    });
  }
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

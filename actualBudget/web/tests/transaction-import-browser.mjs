import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  budget = randomUUID(),
  account = randomUUID();
async function call(path, body, status = 201) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(r.status, await r.clone().text()).toBe(
    body === undefined ? 200 : status,
  );
  return r.json();
}
await call("/budgets", { id: budget, name: "Bank file browser" });
await call(`/budgets/${budget}/accounts`, {
  id: account,
  name: "Checking",
  opening_balance: 0,
  opening_date: "2026-01-01",
});
let browser, page;
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
  await page
    .getByRole("button", { name: "Transaction files", exact: true })
    .click();
  await page.getByLabel("Transaction file").setInputFiles({
    name: "purchases.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(
      "Date,Payee,Amount,Notes\n2026-11-01,BRAND NEW SHOP,-12.50,First purchase\n2026-11-02,BRAND NEW SHOP,-7.25,Second purchase\n",
    ),
  });
  await page.getByRole("button", { name: "Read file", exact: true }).click();
  await expect(
    page.getByText("2 records parsed.", { exact: false }),
  ).toBeVisible();
  await page.getByLabel("Skip first lines").fill("-");
  await expect(
    page.getByRole("button", { name: "Parse with these options" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Close", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await expect(page.getByLabel("Skip first lines")).toHaveValue("-");
  await page.getByLabel("Skip first lines").fill("0");
  await page.getByRole("button", { name: "Read retained file" }).click();
  await expect(
    page.getByText("2 records parsed.", { exact: false }),
  ).toBeVisible();
  for (const [field, value] of [
    ["Date", "Date"],
    ["Payee", "Payee"],
    ["Amount", "Amount"],
    ["Notes", "Notes"],
  ])
    await page
      .getByLabel(`${field} column`, { exact: false })
      .selectOption(value);
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Import and export transactions" }),
  ).toBeVisible();
  await expect(page.getByText("purchases.csv", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Read retained file" }).click();
  await expect(page.getByLabel("Amount column", { exact: false })).toHaveValue(
    "Amount",
  );
  await page.getByRole("button", { name: "Preview selected import" }).click();
  await expect(
    page.getByRole("heading", { name: "Review proposed import" }),
  ).toBeVisible();
  await expect(
    page.getByText("File after rules: First purchase", { exact: true }),
  ).toBeVisible();
  await mkdir(new URL("../test-results/transaction-import/", import.meta.url), {
    recursive: true,
  });
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/review-desktop.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 900 });
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/review-mobile.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/review-dark-large.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page
    .getByRole("heading", { name: "Review proposed import" })
    .scrollIntoViewIfNeeded();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/review-dark-large-viewport.png",
      import.meta.url,
    ).pathname,
  });
  await page.emulateMedia({ colorScheme: "light" });
  await page.evaluate(() => (document.documentElement.style.fontSize = ""));
  await page.setViewportSize({ width: 1440, height: 1000 });
  const before = await call(`/budgets/${budget}/transactions`);
  expect(before.data).toEqual([]);
  // Forward the original request to a real commit, lose only its acknowledgement.
  let original,
    committed = false;
  const pattern = `**/api/v1/budgets/${budget}/transaction-imports/*/commit`;
  await page.route(pattern, async (route) => {
    original = {
      body: route.request().postData(),
      headers: route.request().headers(),
    };
    const target = new URL(route.request().url()),
      headers = { ...route.request().headers(), host: target.host };
    target.hostname = "127.0.0.1";
    const response = await route.fetch({ url: target.href, headers });
    expect(response.status(), await response.text()).toBe(200);
    const ledger = await call(`/budgets/${budget}/transactions`);
    expect(ledger.data).toHaveLength(2);
    expect(ledger.balances.total).toBe(-1975);
    committed = true;
    await route.abort("failed");
  });
  await page.getByRole("button", { name: "Import reviewed records" }).click();
  await expect(
    page.getByRole("heading", { name: "Original import pending" }),
  ).toBeVisible();
  await expect.poll(() => committed).toBe(true);
  await page.unroute(pattern);
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Original import pending" }),
  ).toBeVisible();
  let retry;
  page.on("request", (r) => {
    if (r.url().endsWith("/commit"))
      retry = { body: r.postData(), headers: r.headers() };
  });
  await page.getByRole("button", { name: "Retry original import" }).click();
  await expect(page.getByText("Import saved:", { exact: false })).toBeVisible();
  expect(retry.body).toBe(original.body);
  for (const header of [
    "idempotency-key",
    "x-actual-edit-clock",
    "x-actual-budget-epoch",
    "x-actual-history-owner",
  ])
    expect(retry.headers[header]).toBe(original.headers[header]);
  const ledger = await call(`/budgets/${budget}/transactions`);
  expect(ledger.data).toHaveLength(2);
  expect(ledger.balances.total).toBe(-1975);
  expect(new Set(ledger.data.map((r) => r.id)).size).toBe(2);
  // A fresh review cannot silently rebase a prepared stale command.
  await page.getByLabel("Transaction file").setInputFiles({
    name: "repeat.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(
      "Date,Payee,Amount,Notes\n2026-11-01,BRAND NEW SHOP,-12.50,First purchase\n2026-11-02,BRAND NEW SHOP,-7.25,Second purchase\n",
    ),
  });
  await page.getByRole("button", { name: "Read file", exact: true }).click();
  await expect(page.getByLabel("Amount column", { exact: false })).toHaveValue(
    "Amount",
  );
  await page.getByRole("button", { name: "Preview selected import" }).click();
  await expect(
    page.getByRole("heading", { name: "Review proposed import" }),
  ).toBeVisible();
  await call(`/budgets/${budget}/accounts`, {
    id: randomUUID(),
    name: "Concurrent account",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await page.getByRole("button", { name: "Import reviewed records" }).click();
  await expect(
    page.getByRole("heading", { name: "Original import pending" }),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toBeVisible();
  const staleBody = retry.body,
    staleHeaders = retry.headers;
  await page.reload();
  await page.getByRole("button", { name: "Retry original import" }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  expect(retry.body).toBe(staleBody);
  expect(retry.headers["idempotency-key"]).toBe(
    staleHeaders["idempotency-key"],
  );
  await page
    .getByRole("button", { name: "Cancel original import safely" })
    .click();
  await expect(
    page.getByText("Original import canceled.", { exact: false }),
  ).toBeVisible();
  expect((await call(`/budgets/${budget}/transactions`)).data).toEqual(
    ledger.data,
  );
  await page.getByRole("button", { name: "Discard unsubmitted draft" }).click();
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download account CSV" }).click();
  expect((await downloadPromise).suggestedFilename()).toBe("transactions.csv");
  await mkdir(new URL("../test-results/transaction-import/", import.meta.url), {
    recursive: true,
  });
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/desktop.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/dark-large-text.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page.evaluate(() => (document.documentElement.style.fontSize = ""));
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 1440, height: 1000 });
  const first = ledger.data.find((r) => r.notes === "First purchase");
  await call(`/budgets/${budget}/transactions`, {
    id: randomUUID(),
    account_id: account,
    amount: first.amount,
    date: first.date,
    payee_id: first.payee_id,
    category_id: null,
    notes: "Duplicate purchased",
    cleared: false,
  });
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Duplicate purchased" })
    .getByRole("checkbox")
    .check();
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "First purchase" })
    .getByRole("checkbox")
    .check();
  await page.getByRole("button", { name: "Review duplicate merge" }).click();
  await expect(
    page.getByRole("heading", { name: "Merge two duplicate purchases" }),
  ).toBeVisible();
  await expect(page.getByLabel("Duplicate merge review")).toContainText(
    "Duplicate purchased",
  );
  await page.screenshot({
    path: new URL(
      "../test-results/transaction-import/merge-desktop.png",
      import.meta.url,
    ).pathname,
    fullPage: true,
  });
  await page.getByRole("button", { name: "Merge reviewed duplicates" }).click();
  await expect(
    page.getByText("Merge duplicate entries saved.", { exact: false }),
  ).toBeVisible();
  expect((await call(`/budgets/${budget}/transactions`)).balances.total).toBe(
    -1975,
  );
  expect((await call(`/budgets/${budget}/transactions`)).data).toHaveLength(2);
  expect(errors).toEqual([]);
  console.log(
    "PASS file mapping/reload, source purchases, committed lost ACK/exact retry and CSV download",
  );
} catch (e) {
  if (page) {
    console.error((await page.locator("body").innerText()).slice(-14000));
    await page
      .screenshot({
        path: "/tmp/transaction-import-browser-failure.png",
        fullPage: true,
      })
      .catch(() => {});
  }
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

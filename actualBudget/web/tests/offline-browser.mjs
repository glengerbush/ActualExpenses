import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer, transactionCreatePattern } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
const budget = randomUUID(),
  account = randomUUID();
async function create(path, body) {
  const response = await fetch(`${api}/api/v1${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(response.status).toBe(201);
  return (await response.json()).data;
}
await create("/budgets", { id: budget, name: "Offline browser" });
await create(`/budgets/${budget}/accounts`, {
  id: account,
  name: "Spending",
  opening_balance: 100000,
  opening_date: "2026-01-01",
});
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
  const context = await browser.newContext();
  let page = await context.newPage();

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
  await expect(page.getByTestId("register-total")).toHaveText("1,000.00");
  await context.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("12.34");
  await page.getByLabel("Notes", { exact: true }).fill("Offline purchase");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByText("Saved on this device. Waiting to sync.", { exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("register-total")).toHaveText("1,000.00");
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Offline purchase",
  );
  await page
    .getByRole("button", { name: "Edit pending expense", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("5.67");
  await page.getByLabel("Date", { exact: true }).fill("01/01/2020");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText("-5.67");
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("2.00");
  await page.getByLabel("Notes", { exact: true }).fill("Delete before send");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await page
    .locator(".pending-expense")
    .filter({ hasText: "Delete before send" })
    .getByRole("button", { name: "Delete pending expense", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).not.toContainText(
    "Delete before send",
  );
  // Static app code remains available; only API connectivity is absent. Cold offline navigation is optional on HTTP.
  await page.close();
  page = await context.newPage();
  await page.goto(server.url);
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Offline purchase",
  );
  await expect(page.getByTestId("register-total")).toHaveText("1,000.00");
  await expect(
    page
      .locator(".register-table")
      .getByRole("button", { name: /^Edit / })
      .first(),
  ).toBeDisabled();
  await context.unroute("**/api/v1/**");
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect(page.getByTestId("register-total"))
    .toHaveText("994.33")
    .catch(async (error) => {
      console.log(await page.locator("body").innerText());
      throw error;
    });
  await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
  console.log(
    "PASS offline capture durable pending and reconnect single delivery",
  );
  const rows = await (
    await fetch(`${api}/api/v1/budgets/${budget}/transactions`)
  ).json();
  expect(
    rows.data.filter((row) => row.notes === "Offline purchase"),
  ).toHaveLength(1);
  expect(rows.data.find((row) => row.notes === "Offline purchase").amount).toBe(
    -567,
  );
  expect(rows.data.find((row) => row.notes === "Offline purchase").date).toBe(
    "2020-01-01",
  );
  expect(rows.data.some((row) => row.notes === "Delete before send")).toBe(
    false,
  );
  console.log(
    "PASS offline edit/delete, backdated pending, restarted loaded page and unchanged confirmed balances",
  );
  const attempts = [];
  const onlineCreate = transactionCreatePattern(budget);
  let lost = false;
  await context.route(
    onlineCreate,
    async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      attempts.push({
        body: route.request().postData(),
        key: route.request().headers()["idempotency-key"],
        clock: route.request().headers()["x-actual-edit-clock"],
        epoch: route.request().headers()["x-actual-budget-epoch"],
      });
      if (!lost) {
        lost = true;
        const target = new URL(route.request().url());
        const headers = { ...route.request().headers(), host: target.host };
        target.hostname = "127.0.0.1";
        const response = await route.fetch({ url: target.href, headers });
        expect(response.ok()).toBe(true);
        return route.abort();
      }
      return route.continue();
    },
  );
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1.23");
  await page.getByLabel("Notes", { exact: true }).fill("Lost acknowledgement");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Lost acknowledgement",
  );
  await expect(
    page
      .getByText("Cannot reach the server. Check your connection and retry.", {
        exact: true,
      })
      .first(),
  ).toBeVisible();
  const second = await context.newPage();
  await second.goto(server.url);
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect(second.getByTestId("pending-expenses")).toHaveCount(0);
  await expect(page.getByTestId("register-total")).toHaveText("993.10");
  expect(attempts.length).toBeGreaterThanOrEqual(2);
  for (const attempt of attempts) expect(attempt).toEqual(attempts[0]);
  const confirmed = await (
    await fetch(`${api}/api/v1/budgets/${budget}/transactions`)
  ).json();
  expect(
    confirmed.data.filter((row) => row.notes === "Lost acknowledgement"),
  ).toHaveLength(1);
  await second.close();
  await context.unroute(onlineCreate);
  console.log(
    "PASS lost commit acknowledgement + two-tab replay keeps body/key/clock/epoch and one financial effect",
  );

  let writes = 0;
  const countWrites = (request) => {
    if (
      request.method() === "POST" &&
      onlineCreate.test(request.url())
    )
      writes++;
  };
  page.on("request", countWrites);
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1.00");
  await page.getByLabel("Notes", { exact: true }).fill("Quota retained");
  await page.evaluate(() => {
    const original = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (...args) {
      if (this.name === "actualBudget.offline.v1" && args[1] === "readwrite")
        throw new DOMException("Full", "QuotaExceededError");
      return original.apply(this, args);
    };
  });
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByRole("alert").filter({ hasText: "Free device space" }).first(),
  ).toBeVisible();
  expect(writes).toBe(0);
  await expect(page.getByLabel("Notes", { exact: true })).toHaveValue(
    "Quota retained",
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("992.10");
  page.off("request", countWrites);
  console.log(
    "PASS failed durable queue commit makes no mutation request; original draft retries after reload",
  );
  const editRow = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Quota retained" });
  await editRow.getByRole("button", { name: /^Edit / }).click();
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Interrupted local note");
  await context.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Interrupted local note",
  );
  const list = await (
    await fetch(`${api}/api/v1/budgets/${budget}/transactions`)
  ).json();
  const target = list.data.find((row) => row.notes === "Quota retained");
  const checkpoint = (
    await (await fetch(`${api}/api/v1/budgets/${budget}/checkpoint`)).json()
  ).data;
  const remote = await fetch(
    `${api}/api/v1/budgets/${budget}/transactions/${target.id}`,
    {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
        "X-Actual-Budget-Epoch": checkpoint.epoch,
        "X-Actual-Edit-Clock":
          new Date(Date.now() + 1000).toISOString() + "-0000-00000000000000ff",
      },
      body: JSON.stringify({ notes: "Newer remote note" }),
    },
  );
  expect(remote.status).toBe(200);
  await context.unroute("**/api/v1/**");
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect(page.locator(".register-table")).toContainText(
    "Newer remote note",
  );
  await expect(
    page.getByText("A newer edit replaced these fields: Notes", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
  console.log(
    "PASS online edit interrupted offline retains intent; newer remote field wins with feedback",
  );
  await page.addInitScript(() => {
    window.cacheWrites = 0;
    const put = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (...args) {
      if (this.name === "cache") window.cacheWrites++;
      return put.apply(this, args);
    };
  });
  let releaseReads;
  const heldReads = new Promise((resolve) => (releaseReads = resolve));
  let referenceReads = 0,
    heldCount = 0;
  await page.route("**/api/v1/**", async (route) => {
    heldCount++;
    if (
      /\/(payees|categories|accounts)(\?|$)/.test(
        new URL(route.request().url()).pathname,
      )
    )
      referenceReads++;
    await heldReads;
    await route.continue();
  });
  await page.reload();
  await expect(page.getByTestId("register-total")).toHaveText("992.10");
  await expect(page.getByText(/Saved data from/).first()).toBeVisible();
  await expect.poll(() => heldCount).toBeGreaterThan(0);
  releaseReads();
  await page.waitForLoadState("networkidle");
  expect(referenceReads).toBe(0);
  expect(await page.evaluate(() => window.cacheWrites)).toBe(0);
  await page.unroute("**/api/v1/**");
  console.log(
    "PASS cache renders before held API responses; unchanged reopen downloads no references and rewrites no cache",
  );
  let idleRequests = 0;
  const countIdle = (request) => {
    if (request.url().includes("/api/v1/")) idleRequests++;
  };
  page.on("request", countIdle);
  await page.clock.install({ time: new Date() });
  await page.clock.runFor(120_000);
  expect(idleRequests).toBe(0);
  await page.clock.setSystemTime(new Date());
  page.off("request", countIdle);
  console.log(
    "PASS empty outbox causes no API polling during two minutes of idle browser time",
  );
  // An unknown change kind must cause a guarded bootstrap, never a silently advanced cache cursor.
  const newPayee = randomUUID();
  await create(`/budgets/${budget}/payees`, {
    id: newPayee,
    name: "New reference",
  });
  let guardedReads = 0,
    unknownChanges = 0;
  await page.route("**/api/v1/**", async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/changes")) {
      const headers = { ...route.request().headers(), host: url.host };
      url.hostname = "127.0.0.1";
      const response = await route.fetch({ url: url.href, headers });
      const body = await response.json();
      if (body.data?.length) {
        body.data[0].entity = "future_unsupported_entity";
        unknownChanges++;
      }
      return route.fulfill({ response, json: body });
    }
    if (
      /\/(accounts|payees|categories)$/.test(url.pathname) &&
      url.searchParams.has("snapshot")
    )
      guardedReads++;
    return route.continue();
  });
  await page.reload();
  await expect(page.getByTestId("register-total")).toHaveText("992.10");
  await expect.poll(() => guardedReads).toBeGreaterThanOrEqual(3);
  expect(unknownChanges).toBeGreaterThan(0);
  await page.unroute("**/api/v1/**");
  await context.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(
    page
      .getByLabel("Payee", { exact: true })
      .locator("option")
      .filter({ hasText: "New reference" }),
  ).toHaveCount(1);
  await page.getByLabel("Amount", { exact: true }).fill("2");
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Mobile pending expense");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await page.setViewportSize({ width: 320, height: 740 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await mkdir("test-results", { recursive: true });
  await page.getByTestId("pending-expenses").scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/offline-pending-mobile-200.png",
  });
  await page
    .getByRole("button", { name: "Edit pending expense", exact: true })
    .focus();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Notes", { exact: true })).toHaveValue(
    "Mobile pending expense",
  );
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page
    .getByRole("button", { name: "Delete pending expense", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
  console.log(
    "PASS unknown change entity forces guarded reference bootstrap; offline references, keyboard editing and 320px/200% pending controls remain usable",
  );
  await context.close();
  const legacyContext = await browser.newContext(),
    legacy = await legacyContext.newPage();
  await legacy.goto(server.url);
  await legacy.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget, account },
  );
  await legacy.reload();
  await legacy
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await legacy.getByLabel("Amount", { exact: true }).fill("4.56");
  await legacy
    .getByLabel("Notes", { exact: true })
    .fill("Legacy pending record");
  const original = await legacy.evaluate((budget) => {
    const key = `actualBudget.transaction.v1.${budget}`,
      draft = JSON.parse(localStorage.getItem(key));
    delete draft.budgetEpoch;
    delete draft.beganOnline;
    delete draft.editClock;
    draft.pending = {
      path: `/budgets/${budget}/transactions`,
      method: "POST",
      key: draft.draftId,
      body: {
        id: draft.id,
        account_id: draft.fields.account_id,
        date: draft.fields.date,
        amount: -456,
        notes: draft.fields.notes,
        payee_id: null,
        category_id: null,
        cleared: true,
      },
    };
    localStorage.setItem(key, JSON.stringify(draft));
    return draft.pending;
  }, budget);
  const legacyAttempts = [];
  legacy.on("request", (r) => {
    if (
      r.method() === "POST" &&
      r.url().endsWith(`/budgets/${budget}/transactions`)
    )
      legacyAttempts.push({
        body: r.postDataJSON(),
        key: r.headers()["idempotency-key"],
        clock: r.headers()["x-actual-edit-clock"],
      });
  });
  await legacy.reload();
  await legacy
    .getByRole("button", { name: "Retry saved transaction", exact: true })
    .click();
  await expect(legacy.locator(".register-table")).toContainText(
    "Legacy pending record",
  );
  expect(legacyAttempts).toHaveLength(1);
  expect(legacyAttempts[0]).toEqual({
    body: original.body,
    key: original.key,
    clock: undefined,
  });
  expect(
    await legacy.evaluate(
      (budget) => localStorage.getItem(`actualBudget.transaction.v1.${budget}`),
      budget,
    ),
  ).toBe(null);
  console.log(
    "PASS genuine pre-outbox pending draft without clock/epoch adopts durable delivery without changing original body/key",
  );
  await legacyContext.close();
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer, transactionCreatePattern } from "./server.mjs";

// Real HTTP and browser seams: only transport responses are interrupted.
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
const failures = [];
async function request(path, body, method = "POST") {
  const response = await fetch(`${api}/api/v1${path}`, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return (await response.json()).data;
}
async function fixture(name, opening = 0) {
  const budget = randomUUID(),
    account = randomUUID();
  await request("/budgets", { id: budget, name });
  await request(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Spending",
    opening_balance: opening,
    opening_date: "2026-01-01",
  });
  const transactions = `/budgets/${budget}/transactions`;
  const add = (body) =>
    request(transactions, {
      id: randomUUID(),
      account_id: account,
      date: "2026-01-01",
      ...body,
    });
  return { budget, account, transactions, add };
}
async function open(fixture) {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  page.on("pageerror", (error) => failures.push(error.message));
  await page.goto(server.url);
  await page.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget: fixture.budget, account: fixture.account },
  );
  await page.reload();
  return { context, page };
}
async function loseCommittedResponse(page, predicate, method) {
  const attempts = [];
  await page.route(predicate, async (route) => {
    if (route.request().method() !== method) return route.continue();
    try {
      attempts.push({
        key: route.request().headers()["idempotency-key"],
        body: route.request().postData(),
      });
      const target = new URL(route.request().url());
      const headers = { ...route.request().headers(), host: target.host };
      if (target.hostname === "actualbudget.test")
        target.hostname = "127.0.0.1";
      const response = await route.fetch({ url: target.href, headers });
      expect(response.ok()).toBe(true);
      if (attempts.length === 1) await route.abort("connectionreset");
      else await route.fulfill({ response });
    } catch (error) {
      failures.push(String(error));
      await route.abort("failed").catch(() => {});
    }
  });
  return attempts;
}
async function hidePreferences(page, budget) {
  await page.evaluate(
    (budget) =>
      localStorage.removeItem(`actualBudget.preferencesCache.v1.${budget}`),
    budget,
  );
  await page.evaluate(
    (budget) =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("actualBudget.offline.v1", 1);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("cache", "readwrite");
          tx.objectStore("cache").delete(`budget:${budget}`);
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      }),
    budget,
  );
  await page.route("**/preferences?**", (route) => route.abort("failed"));
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Display preferences are unavailable" }),
  ).toBeVisible();
  await expect(page.getByTestId("register-total")).toHaveCount(0);
  await expect(page.getByTestId("account-balance")).toHaveCount(0);
  await expect(page.locator(".register-table tbody tr")).toHaveCount(0);
}
async function restorePreferences(page) {
  await page.unroute("**/preferences?**");
  await page.getByRole("button", { name: "Retry connection" }).click();
}
try {
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  {
    const data = await fixture("Cutoff response recovery", 10000);
    await data.add({
      amount: 5000,
      date: "2026-12-01",
      notes: "Future income",
    });
    const { context, page } = await open(data);
    await expect(page.getByTestId("register-total")).toHaveText("150.00");
    const confirmedLabel = await page
      .locator(".register-balances dt")
      .first()
      .textContent();
    await page.route("**/transactions?**", (route) => route.abort("failed"));
    await page.getByLabel("Show through date").fill("06/01/2026");
    await page.getByRole("button", { name: "Apply", exact: true }).click();
    await expect(
      page
        .getByText(
          "Cannot reach the server. Check your connection and retry.",
          { exact: true },
        )
        .first(),
    ).toBeVisible();
    await expect(page.getByTestId("register-total")).toHaveText("150.00");
    await expect(page.locator(".register-balances dt").first()).toHaveText(
      confirmedLabel,
    );
    await expect(
      page.getByText("Future income", { exact: true }),
    ).toBeVisible();
    await page.unroute("**/transactions?**");
    await page.getByRole("button", { name: "Apply", exact: true }).click();
    await expect(page.getByTestId("register-total")).toHaveText("100.00");
    await expect(page.getByText("Future income", { exact: true })).toHaveCount(
      0,
    );
    await expect(page.locator(".register-balances dt").first()).not.toHaveText(
      confirmedLabel,
    );
    await context.close();
    console.log(
      "PASS failed cutoff retains confirmed label, rows and totals; retry commits all together",
    );
  }
  {
    const data = await fixture("Concurrent register pagination");
    for (let index = 0; index < 101; index++)
      await data.add({ amount: 100, notes: `Seed ${index}` });
    const { context, page } = await open(data);
    const rows = page.locator(".register-table tbody tr");
    const more = page.getByRole("button", {
      name: "Load more transactions",
      exact: true,
    });
    await expect(rows).toHaveCount(100);
    const inserted = await data.add({
      amount: 100,
      date: "2026-12-01",
      notes: "Other device insertion",
    });
    await more.click();
    await expect(page.getByTestId("register-total")).toHaveText("102.00");
    await expect(rows).toHaveCount(100);
    await expect(
      page.getByText("Other device insertion", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByText("Data changed. The transaction list was refreshed."),
    ).toBeVisible();
    await more.click();
    await expect(rows).toHaveCount(102);
    expect(new Set(await rows.allTextContents()).size).toBe(102);
    await page.reload();
    await expect(rows).toHaveCount(100);
    await request(
      `${data.transactions}/${inserted.id}`,
      { amount: 500, notes: "Edited on other device" },
      "PATCH",
    );
    await more.click();
    await expect(page.getByTestId("register-total")).toHaveText("106.00");
    await expect(rows).toHaveCount(100);
    await expect(
      page.getByText("Edited on other device", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByText("Other device insertion", { exact: true }),
    ).toHaveCount(0);
    await request(`${data.transactions}/${inserted.id}`, {}, "DELETE");
    await more.click();
    await expect(page.getByTestId("register-total")).toHaveText("101.00");
    await expect(rows).toHaveCount(100);
    await expect(
      page.getByText("Edited on other device", { exact: true }),
    ).toHaveCount(0);
    await more.click();
    await expect(rows).toHaveCount(101);
    expect(new Set(await rows.allTextContents()).size).toBe(101);
    await context.close();
    console.log(
      "PASS concurrent insertion/edit/deletion refresh coherent pages; stable continuation has no duplicates",
    );
  }
  {
    const data = await fixture(
      "Pending transaction without preferences",
      10000,
    );
    const { context, page } = await open(data);
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("12.34");
    await page.getByLabel("Date", { exact: true }).fill("01/02/2026");
    await page
      .getByLabel("Notes", { exact: true })
      .fill("Recover original expense");
    const createRoute = transactionCreatePattern(data.budget);
    const creates = await loseCommittedResponse(page, createRoute, "POST");
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Retry pending expense" }),
    ).toBeVisible();
    await hidePreferences(page, data.budget);
    await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
    expect(creates).toHaveLength(2);
    expect(creates[1]).toEqual(creates[0]);
    await expect(page.getByTestId("register-total")).toHaveCount(0);
    const actual = await request(
      `${data.transactions}?account_id=${data.account}`,
      undefined,
      "GET",
    );
    expect(
      actual.filter((row) => row.notes === "Recover original expense"),
    ).toHaveLength(1);
    await restorePreferences(page);
    await expect(page.getByTestId("register-total")).toHaveText("87.66");
    await page.unroute(createRoute);
    const entry = actual.find(
      (row) => row.notes === "Recover original expense",
    );
    const deleteRoute = `**/api/v1${data.transactions}/${entry.id}`;
    const deletes = await loseCommittedResponse(page, deleteRoute, "DELETE");
    await page
      .locator(".register-table tbody tr")
      .filter({ hasText: "Recover original expense" })
      .getByRole("button", { name: /^Delete / })
      .click();
    await page
      .getByRole("button", { name: "Confirm delete", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Retry pending expense" }),
    ).toBeVisible();
    await hidePreferences(page, data.budget);
    await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Retry pending expense" }),
    ).toHaveCount(0);
    expect(deletes).toHaveLength(2);
    expect(deletes[1]).toEqual(deletes[0]);
    await expect(page.getByTestId("register-total")).toHaveCount(0);
    expect(
      (
        await request(
          `${data.transactions}?account_id=${data.account}`,
          undefined,
          "GET",
        )
      ).filter((row) => row.id === entry.id),
    ).toHaveLength(0);
    await restorePreferences(page);
    await expect(page.getByTestId("register-total")).toHaveText("100.00");
    await context.close();
    console.log(
      "PASS pending create and deletion replay exact original operations without preferences; financial views stay hidden",
    );
  }
  expect(failures).toEqual([]);
} finally {
  await browser?.close();
  await server.stop();
}

// External-server compatible: retired navigation/recovery never contacts providers.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  budget = randomUUID(),
  account = randomUUID(),
  name = "Manual-only " + budget.slice(0, 8);
async function call(path, body, status = 201) {
  const r = await fetch(api + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.status, await r.clone().text()).toBe(status);
  return r.json();
}
await call("/budgets", { id: budget, name, budget_type: "tracking" });
await call(`/budgets/${budget}/accounts`, {
  id: account,
  name: "Manual checking",
  opening_balance: 12345,
  opening_date: "2026-11-10",
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
  const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    }),
    page = await context.newPage(),
    requests = [],
    errors = [];
  page.on("request", (r) => requests.push(r.url()));
  page.on("pageerror", (e) => errors.push(e.message));
  async function open() {
    await page.goto(server.url);
    const button = page.getByRole("button", {
      name: new RegExp(name + ".*Open"),
    });
    const manual = page.getByRole("heading", { name: "Accounts", exact: true });
    await expect(button.or(manual)).toBeVisible();
    if (await button.isVisible()) await button.click();
    await expect(
      page.getByRole("heading", { name: "Accounts", exact: true }),
    ).toBeVisible();
  }
  await open();
  await expect(
    page.getByRole("button", { name: "Bank connections", exact: true }),
  ).toHaveCount(0);
  const retained = {
    version: 1,
    budgetId: budget,
    pending: {
      key: "original-retired-key",
      path: `/budgets/${budget}/banking/connections`,
      body: { setup_token_sha256: "a".repeat(64) },
    },
    marker: "original recovery artifact",
  };
  await page.evaluate(
    async ({ budget, retained }) => {
      localStorage.setItem(
        `actualBudget.navigation.v1.${budget}`,
        JSON.stringify({ version: 1, view: "banking" }),
      );
      await new Promise((resolve, reject) => {
        const req = indexedDB.open("actualBudget.banking.v1", 1);
        req.onupgradeneeded = () =>
          req.result.createObjectStore("drafts", { keyPath: "budgetId" });
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const db = req.result,
            tx = db.transaction("drafts", "readwrite");
          tx.objectStore("drafts").put(retained);
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      });
    },
    { budget, retained },
  );
  await open();
  await expect(
    page.getByRole("button", { name: "Manual checking", exact: true }),
  ).toBeVisible();
  await expect(page.locator("td.amount").first()).toHaveText("123.45");
  await page.keyboard.press("Control+k");
  await page.getByLabel("Search commands").fill("bank");
  await expect(page.locator("#command-results button")).toHaveCount(0);
  await page.getByRole("button",{name:"Close search",exact:true}).click();
  await expect(page.getByRole("dialog")).not.toBeVisible();
  const recovered = await page.evaluate(
    (budget) =>
      new Promise((resolve, reject) => {
        const r = indexedDB.open("actualBudget.banking.v1", 1);
        r.onsuccess = () => {
          const db = r.result,
            tx = db.transaction("drafts"),
            q = tx.objectStore("drafts").get(budget);
          q.onsuccess = () => {
            resolve(q.result);
            db.close();
          };
          q.onerror = () => reject(q.error);
        };
        r.onerror = () => reject(r.error);
      }),
    budget,
  );
  expect(recovered).toEqual(retained);
  const directory = new URL("../test-results/manual-only/", import.meta.url);
  await mkdir(directory, { recursive: true });
  await page.screenshot({
    path: new URL("desktop.png", directory).pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: new URL("mobile.png", directory).pathname,
    fullPage: true,
  });
  await expect(
    page.getByRole("button", { name: "Transaction files", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Transaction files", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transaction files", exact: true }),
  ).toBeVisible();
  expect(
    requests.filter((url) =>
      /\/banking(?:\/|$)|simplefin|gocardless|nordigen|akahu|pluggy/i.test(url),
    ),
  ).toEqual([]);
  expect(errors).toEqual([]);
  console.log(
    "PASS manual-only navigation, old recovery bytes retained without execution, manual ledger/files and zero connector requests",
  );
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, webkit, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { savedWork, expectSavedWork } from "./saved-work-ui.mjs";
const server = await startServer(),
  url = server.url.replace("actualbudget.test", "127.0.0.1"),
  api = url + "/api/v1";
const iphone =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
const useWebKit = process.env.PWA_BROWSER === "webkit";
let browser, page;
async function call(
  path,
  body,
  method = body === undefined ? "GET" : "POST",
  headers = {},
) {
  const r = await fetch(api + path, {
    method,
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
try {
  const budget = randomUUID(),
    second = randomUUID(),
    account = randomUUID(),
    category = randomUUID(),
    payee = randomUUID();
  const root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Everyday" });
  await call("/budgets", { id: second, name: "Travel" });
  await call(root + "/accounts", {
    id: account,
    name: "Imported history",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await call(root + "/categories", {
    id: category,
    name: "Food",
    is_income: false,
  });
  await call(root + "/payees", { id: payee, name: "Corner café" });
  const ids = [];
  for (let day = 1; day <= 12; day++) {
    const id = randomUUID();
    ids.push(id);
    await call(root + "/transactions", {
      id,
      account_id: account,
      amount: -day * 100,
      date: `2026-01-${String(day).padStart(2, "0")}`,
      payee_id: payee,
      category_id: category,
    });
  }
  await call(root + "/transactions", {
    id: randomUUID(),
    account_id: account,
    amount: 50000,
    date: "2026-02-01",
  });
  browser = await (useWebKit ? webkit : chromium).launch(
    useWebKit
      ? {}
      : { executablePath: "/usr/bin/chromium", args: ["--no-sandbox"] },
  );
  // Installation and iPhone are both necessary. A narrow screen alone never selects companion mode.
  for (const [agent, standalone] of [
    [iphone, false],
    [iphone.replace("(iPhone; CPU iPhone OS", "(iPad; CPU OS"), true],
    [
      "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/125.0 Mobile Safari/537.36",
      true,
    ],
  ]) {
    const context = await browser.newContext({
      userAgent: agent,
      viewport: { width: 390, height: 844 },
    });
    await context.addInitScript(
      (value) =>
        Object.defineProperty(navigator, "standalone", {
          value,
          configurable: true,
        }),
      standalone,
    );
    const regular = await context.newPage();
    await regular.goto(url);
    await expect(
      regular.getByRole("heading", { name: "Your budgets", exact: true }),
    ).toBeVisible();
    await expect(
      regular.getByRole("button", { name: "Settings", exact: true }),
    ).toBeVisible();
    await expect(
      regular.getByRole("button", { name: "Refresh expenses", exact: true }),
    ).toHaveCount(0);
    await context.close();
  }
  const context = await browser.newContext({
    userAgent: iphone,
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  await context.addInitScript(() =>
    Object.defineProperty(navigator, "standalone", {
      value: true,
      configurable: true,
    }),
  );
  page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(url);
  await page.getByRole("button", { name: "Everyday", exact: true }).click();
  const rows = page.locator("[data-expense-id]");
  await expect(rows).toHaveCount(10);
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "Settings", exact: true }),
  ).toHaveCount(0);
  expect(
    await rows.evaluateAll((nodes) => nodes.map((n) => n.dataset.expenseId)),
  ).toEqual(ids.slice(2).reverse());
  await mkdir("test-results/iphone-pwa", { recursive: true });
  await page.screenshot({
    path: `test-results/iphone-pwa/${useWebKit ? "webkit" : "chromium"}-recent.png`,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "+ Add expense", exact: true })
    .click();
  await expect(page.getByRole("radio")).toHaveCount(0);
  await expect(page.getByText("Recurring", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Account", { exact: true })).toHaveCount(0);
  await page.getByLabel("Date", { exact: true }).fill("01/01/2020");
  await page.getByLabel("Amount", { exact: true }).fill("4.50");
  await page
    .getByRole("combobox", { name: "Payee", exact: true })
    .fill("Corner");
  await page.getByRole("option", { name: "Corner café", exact: true }).click();
  await page.getByLabel("Category", { exact: true }).selectOption(category);
  // An older full-app draft has its own namespace and must never be rewritten or sent.
  const legacy = await page.evaluate(
    ({ budget, account }) => {
      const draft = JSON.parse(
        localStorage.getItem(`actualExpenses.transaction.v1.${budget}`),
      );
      delete draft.companion;
      draft.id = crypto.randomUUID();
      draft.draftId = crypto.randomUUID();
      draft.fields.direction = "income";
      draft.fields.amount = "99.00";
      draft.pending = {
        path: `/budgets/${budget}/transactions`,
        method: "POST",
        key: draft.draftId,
        body: {
          id: draft.id,
          account_id: account,
          amount: 9900,
          date: "2026-01-01",
        },
      };
      const raw = JSON.stringify(draft);
      localStorage.setItem(`actualBudget.transaction.v1.${budget}`, raw);
      return { raw, id: draft.id };
    },
    { budget, account },
  );

  await page.screenshot({
    path: `test-results/iphone-pwa/${useWebKit ? "webkit" : "chromium"}-editor.png`,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(rows).toHaveCount(11);
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  const added = (await call(root + "/companion?retain=")).recent;
  expect(added).toHaveLength(10);
  const backdated = await rows.last().getAttribute("data-expense-id");
  expect((await call(root + "/transactions/" + backdated)).date).toBe(
    "2020-01-01",
  );
  // Edit only a changed field, with the companion expense precondition attached.
  await rows
    .last()
    .getByRole("button", { name: /^Edit / })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("5.75");
  const patch = page.waitForRequest(
    (r) =>
      r.method() === "PATCH" && r.url().endsWith("/transactions/" + backdated),
  );
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  const sent = await patch;
  expect(sent.headers()["x-actual-expense-only"]).toBe("true");
  expect(sent.postDataJSON()).toEqual({ amount: -575 });
  await expect(rows.last()).toContainText("-5.75");
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await expect(rows).toHaveCount(11);
  await expect(rows.last()).toContainText("-5.75");
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  // Cache the actual loaded shell; nothing under /api is cached by the worker.
  await page.evaluate(async () => {
    const reg = await navigator.serviceWorker.ready;
    await new Promise((resolve) => {
      const channel = new MessageChannel();
      channel.port1.onmessage = resolve;
      reg.active.postMessage(
        {
          type: "CACHE_SHELL",
          urls: performance.getEntriesByType("resource").map((e) => e.name),
        },
        [channel.port2],
      );
    });
  });
  expect(
    await page.evaluate(async () => {
      const cache = await caches.open("actualExpenses-shell-v1");
      return (await cache.keys()).some((r) =>
        new URL(r.url).pathname.startsWith("/api/"),
      );
    }),
  ).toBe(false);
  await context.setOffline(true);
  await page.reload();
  await expect(rows).toHaveCount(11);
  await expect(
    rows.first().getByRole("button", { name: /^Edit / }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "+ Add expense", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("2.25");
  await page
    .getByRole("combobox", { name: "Payee", exact: true })
    .fill("Street cart");
  await page.getByRole("option", { name: /Create payee/ }).click();

  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expectSavedWork(page, 1);
  const pending = (await savedWork(page))[0];
  const queuedRow = page.locator(`[data-expense-id="${pending.row.id}"]`);
  await expect(queuedRow).toBeVisible();
  await queuedRow.getByRole("button", { name: /^Edit / }).click();
  await page.getByLabel("Amount", { exact: true }).fill("3.25");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expectSavedWork(page, 1);
  await expect(queuedRow).toContainText("-3.25");
  await context.setOffline(false);
  await expectSavedWork(page, 0);
  await expect
    .poll(
      async () => (await call(root + "/transactions/" + pending.row.id)).amount,
    )
    .toBe(-325);
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Refresh expenses", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  expect(
    (await call(root + "/transactions/" + pending.row.id)).payee_name,
  ).toBe("Street cart");
  // An edit started online can finish offline; a later server edit still wins.
  const conflictId = ids.at(-1);
  await page
    .locator(`[data-expense-id="${conflictId}"]`)
    .getByRole("button", { name: /^Edit / })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("8.88");
  await context.setOffline(true);
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expectSavedWork(page, 1);
  const queuedEdit = (await savedWork(page))[0];
  const newer =
    new Date(
      Date.parse(queuedEdit.pending.edit_clock.slice(0, 24)) + 1000,
    ).toISOString() + "-0000-0000000000000001";
  await call(root + "/transactions/" + conflictId, { amount: -999 }, "PATCH", {
    "X-Actual-Edit-Clock": newer,
  });
  await context.setOffline(false);
  await expectSavedWork(page, 0);
  await expect(
    page.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeEnabled();
  expect((await call(root + "/transactions/" + conflictId)).amount).toBe(-999);
  expect(
    await page.evaluate(
      (b) => localStorage.getItem(`actualBudget.transaction.v1.${b}`),
      budget,
    ),
  ).toBe(legacy.raw);
  expect((await fetch(api + root + "/transactions/" + legacy.id)).status).toBe(
    404,
  );
  await queuedRow.getByRole("button", { name: /^Delete / }).click();
  await queuedRow
    .getByRole("button", { name: "Delete expense", exact: true })
    .click();
  await expect(queuedRow).toHaveCount(0);
  await expectSavedWork(page, 0);
  expect(
    (await fetch(api + root + "/transactions/" + pending.row.id)).status,
  ).toBe(404);
  await page.getByRole("button", { name: "Budgets", exact: true }).click();
  await page.getByRole("button", { name: "Travel", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "No expenses yet", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Budgets", exact: true }).click();
  await page.getByRole("button", { name: "Everyday", exact: true }).click();
  await expect(rows).toHaveCount(10);
  await expect(
    page.getByRole("button", { name: "Refresh expenses", exact: true }),
  ).toBeEnabled();
  await context.route("**/api/v1/**", (route) => route.abort());
  await page
    .getByRole("button", { name: "Refresh expenses", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh expenses", exact: true }),
  ).toBeEnabled();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(rows).toHaveCount(10);
  await context.unroute("**/api/v1/**");
  await page
    .getByRole("button", { name: "Refresh expenses", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh expenses", exact: true }),
  ).toBeEnabled();
  const idleRequests = [];
  page.on("request", (r) => {
    if (r.url().includes("/api/v1/")) idleRequests.push(r.url());
  });
  await page.clock.install();
  await page.clock.fastForward(120_000);
  expect(idleRequests).toEqual([]);
  await page.setViewportSize({ width: 320, height: 690 });
  await page.addStyleTag({ content: "html{font-size:200%}" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: `test-results/iphone-pwa/${useWebKit ? "webkit" : "chromium"}-large-text.png`,
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS iPhone installed-only routing, ten recent expenses, retained backdated entry, guarded edit/delete, offline cold launch/capture/edit/silent delivery, budget selection and mobile layout",
  );
} catch (e) {
  if (page) {
    await mkdir("test-results/iphone-pwa", { recursive: true });
    await page.screenshot({
      path: "test-results/iphone-pwa/failure.png",
      fullPage: true,
    });
    await writeFile(
      "test-results/iphone-pwa/failure.html",
      await page.content(),
    );
  }
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const artifacts = fileURLToPath(new URL("../test-results/", import.meta.url));
await mkdir(artifacts, { recursive: true });
import { startServer } from "./server.mjs";
const server = await startServer(),
  native = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, clock) {
  const response = await fetch(native + "/api/v1" + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...(clock ? { "X-Actual-Edit-Clock": clock } : {}),
    },
    body: JSON.stringify(body),
  });
  expect(response.status).toBe(201);
  return (await response.json()).data;
}
const budget = randomUUID(),
  account = randomUUID(),
  transaction = randomUUID();
let browser;
try {
  await write("/budgets", { id: budget, name: "Concurrent clients" });
  await write(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Spending",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await write(
    `/budgets/${budget}/transactions`,
    {
      id: transaction,
      account_id: account,
      amount: -100,
      date: "2026-01-01",
      notes: "Initial",
    },
    "2026-01-01T00:00:00.000Z-0000-0000000000000001",
  );

  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const errors = [];
  async function client(time) {
    const context = await browser.newContext();
    const page = await context.newPage();
    page.on("pageerror", (e) => errors.push(e.message));
    if (time) await page.clock.install({ time: new Date(time) });
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
    await expect(page.getByTestId("register-total")).toHaveText("-1.00");
    return { context, page };
  }
  const a = await client(Date.now() - 1000),
    b = await client();
  const attempts = [];
  await a.page.route(
    `**/api/v1/budgets/${budget}/transactions/${transaction}`,
    async (route) => {
      const request = route.request();
      if (!["PATCH", "DELETE"].includes(request.method()))
        return route.continue();
      const same = attempts.filter((a) => a.method === request.method());
      attempts.push({
        method: request.method(),
        clock: request.headers()["x-actual-edit-clock"],
        key: request.headers()["idempotency-key"],
        body: request.postDataJSON(),
      });
      if (same.length === 0) return route.abort("failed");
      await route.continue();
    },
  );
  await a.page.getByRole("button", { name: /^Edit / }).click();
  await a.page.getByLabel("Notes", { exact: true }).fill("Older client note");
  await a.page.getByLabel("Amount", { exact: true }).fill("2");
  await a.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    a.page.getByRole("button", {
      name: "Retry saved transaction",
      exact: true,
    }),
  ).toBeVisible();
  await b.page.getByRole("button", { name: /^Edit / }).click();
  await b.page.getByLabel("Notes", { exact: true }).fill("Newer client note");
  await b.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(b.page.locator(".register-table")).toContainText(
    "Newer client note",
  );
  await a.page.reload();
  await a.page
    .getByRole("button", { name: "Retry saved transaction", exact: true })
    .click();
  await expect(
    a.page.getByText("A newer edit replaced these fields: Notes", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(a.page.getByTestId("register-total")).toHaveText("-2.00");
  expect(attempts[1]).toEqual(attempts[0]);
  await a.page.screenshot({
    path: artifacts + "concurrency-conflict-desktop.png",
    fullPage: true,
  });
  // A queued unpublished deletion loses to an edit made after its original clock.
  await a.page.getByRole("button", { name: /^Delete / }).click();
  await a.page
    .getByRole("button", { name: "Confirm delete", exact: true })
    .click();
  await expect(
    a.page.getByRole("button", { name: "Retry saved action", exact: true }),
  ).toBeVisible();
  await b.page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await expect(b.page.getByTestId("register-total")).toHaveText("-2.00");
  await b.page.getByRole("button", { name: /^Edit / }).click();
  await b.page.getByLabel("Notes", { exact: true }).fill("Newer than deletion");
  await b.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(b.page.locator(".register-table")).toContainText(
    "Newer than deletion",
  );
  await a.page.reload();
  await a.page
    .getByRole("button", { name: "Retry saved action", exact: true })
    .click();
  await expect(
    a.page.getByText(
      "A newer edit kept this transaction. The pending deletion was canceled.",
      { exact: true },
    ),
  ).toBeVisible();
  await expect(a.page.locator(".register-table")).toContainText(
    "Newer than deletion",
  );
  const deletes = attempts.filter((a) => a.method === "DELETE");
  expect(deletes[1]).toEqual(deletes[0]);
  // Unchanged refresh uses one cursor request, not the full register/reference endpoints.
  const requests = [];
  a.page.on("request", (r) => {
    if (r.method() === "GET" && r.url().includes("/api/v1/"))
      requests.push(r.url());
  });
  await a.page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await expect(
    a.page.getByText("Transactions are up to date.", { exact: true }),
  ).toBeVisible();
  expect(requests).toHaveLength(1);
  expect(requests[0]).toContain("/changes?");
  // Newer fields remain hidden after publication of a tombstone.
  await b.page.getByRole("button", { name: /^Edit / }).click();
  await b.page.getByLabel("Notes", { exact: true }).fill("Hidden edit");
  await a.page.unroute(
    `**/api/v1/budgets/${budget}/transactions/${transaction}`,
  );
  await a.page.getByRole("button", { name: /^Delete / }).click();
  await a.page
    .getByRole("button", { name: "Confirm delete", exact: true })
    .click();
  await expect(
    a.page.getByText(
      "No transactions yet. Add an income or expense to begin.",
      { exact: true },
    ),
  ).toBeVisible();
  await b.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    b.page.getByText(
      "This transaction remains deleted. Its newer field edits were retained without restoring it.",
      { exact: true },
    ),
  ).toBeVisible();
  expect(
    (
      await (
        await fetch(native + `/api/v1/budgets/${budget}/transactions`)
      ).json()
    ).data,
  ).toHaveLength(0);
  await a.context.close();
  await b.context.close();
  // Clock correction never retimestamps an immutable rejected request.
  const context = await browser.newContext(),
    page = await context.newPage();
  await page.clock.install({ time: new Date(Date.now() + 600_000) });
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
  await page.getByLabel("Amount", { exact: true }).fill("1");
  const clockRequests = [];
  page.on("request", (r) => {
    if (
      r.method() === "POST" &&
      r.url().endsWith(`/budgets/${budget}/transactions`)
    )
      clockRequests.push({
        key: r.headers()["idempotency-key"],
        clock: r.headers()["x-actual-edit-clock"],
        body: r.postDataJSON(),
      });
  });
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Discard rejected edit", exact: true }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: artifacts + "concurrency-clock-mobile.png",
    fullPage: true,
  });
  await page.clock.setSystemTime(new Date());
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved transaction", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Discard rejected edit", exact: true }),
  ).toBeVisible();
  expect(clockRequests).toHaveLength(2);
  expect(clockRequests[1]).toEqual(clockRequests[0]);
  await page
    .getByRole("button", { name: "Discard rejected edit", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("-1.00");
  expect(clockRequests).toHaveLength(3);
  expect(clockRequests[2].clock).not.toBe(clockRequests[0].clock);
  // Rejected register actions retain their clocks too; explicit discard changes no ledger rows.
  await page.clock.setSystemTime(new Date(Date.now() + 600_000));
  const actionRequests = [];
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith("/duplicate"))
      actionRequests.push({
        key: r.headers()["idempotency-key"],
        clock: r.headers()["x-actual-edit-clock"],
        body: r.postDataJSON(),
      });
  });
  await page.getByRole("button", { name: /^Duplicate / }).click();
  await expect(
    page.getByRole("button", { name: "Discard rejected action", exact: true }),
  ).toBeVisible();
  await page.clock.setSystemTime(new Date());
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved action", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Discard rejected action", exact: true }),
  ).toBeVisible();
  expect(actionRequests).toHaveLength(2);
  expect(actionRequests[1]).toEqual(actionRequests[0]);
  await page
    .getByRole("button", { name: "Discard rejected action", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("-1.00");
  await page.getByRole("button", { name: /^Duplicate / }).click();
  await expect(page.getByTestId("register-total")).toHaveText("-2.00");
  expect(actionRequests).toHaveLength(3);
  expect(actionRequests[2].clock).not.toBe(actionRequests[0].clock);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  expect(errors).toEqual([]);
  await context.close();
  // A rejection authorizes discard only until another attempt can reach the server.
  for (const kind of ["transaction", "action"]) {
    const recoveryContext = await browser.newContext();
    const page = await recoveryContext.newPage();
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
    if (kind === "transaction") {
      await page
        .getByRole("button", { name: "Add transaction", exact: true })
        .click();
      await page.getByLabel("Amount", { exact: true }).fill("1");
    }
    const horizon = Date.now() + 5000;
    await page.evaluate(
      (horizon) =>
        localStorage.setItem(
          "actualBudget.clock.v1",
          new Date(horizon).toISOString() + "-FFFE-0000000000000001",
        ),
      horizon,
    );
    const sent = [];
    let dropAcknowledgement = false;
    const pattern =
      kind === "transaction"
        ? `**/api/v1/budgets/${budget}/transactions`
        : `**/api/v1/budgets/${budget}/transactions/*/duplicate`;
    await page.route(pattern, async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      const request = route.request();
      sent.push({
        key: request.headers()["idempotency-key"],
        clock: request.headers()["x-actual-edit-clock"],
        body: request.postDataJSON(),
      });
      const url = new URL(request.url());
      const headers = { ...request.headers(), host: url.host };
      url.hostname = "127.0.0.1";
      const response = await route.fetch({ url: url.href, headers });
      if (dropAcknowledgement) {
        expect(response.status()).toBe(201);
        dropAcknowledgement = false;
        await route.abort("failed");
      } else await route.fulfill({ response });
    });
    const discardName =
      kind === "transaction"
        ? "Discard rejected edit"
        : "Discard rejected action";
    const retryName =
      kind === "transaction" ? "Retry saved transaction" : "Retry saved action";
    const discard = page.getByRole("button", {
      name: discardName,
      exact: true,
    });
    const retry = page.getByRole("button", { name: retryName, exact: true });
    if (kind === "transaction")
      await page
        .getByRole("button", { name: "Save transaction", exact: true })
        .click();
    else
      await page
        .getByRole("button", { name: /^Duplicate / })
        .first()
        .click();
    await expect(discard).toBeVisible();
    expect(sent).toHaveLength(1);
    expect(sent[0].clock.slice(25, 29)).toBe("FFFF");
    const storageKey = `actualBudget.${kind === "transaction" ? "transaction" : "register-action"}.v1.${budget}`;
    // A failed durable authorization transition must prevent sending.
    await page.evaluate((key) => {
      const original = Storage.prototype.setItem;
      Storage.prototype.setItem = function (name, value) {
        if (name === key) throw new Error("Storage write blocked for test");
        return original.call(this, name, value);
      };
    }, storageKey);
    await retry.click();
    await expect(retry).toBeEnabled();
    expect(sent).toHaveLength(1);
    await page.reload();
    await expect(discard).toBeVisible();
    const stalePage = await recoveryContext.newPage();
    await stalePage.goto(server.url);
    await expect(
      stalePage.getByRole("button", { name: discardName, exact: true }),
    ).toBeVisible();
    // A delayed confirmed rejection from an older attempt cannot reauthorize discard.
    const delayedPage = await recoveryContext.newPage();
    await delayedPage.goto(server.url);
    let releaseRejection;
    const holdRejection = new Promise((resolve) => {
      releaseRejection = resolve;
    });
    let rejectionReceived;
    const receivedRejection = new Promise((resolve) => {
      rejectionReceived = resolve;
    });
    await delayedPage.route(pattern, async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      const url = new URL(route.request().url());
      const headers = { ...route.request().headers(), host: url.host };
      url.hostname = "127.0.0.1";
      const response = await route.fetch({ url: url.href, headers });
      expect(response.status()).toBe(400);
      expect((await response.json()).error.code).toBe("clock_overflow");
      rejectionReceived();
      await holdRejection;
      await route.fulfill({ response });
    });
    await delayedPage
      .getByRole("button", { name: retryName, exact: true })
      .click();
    await receivedRejection;
    await new Promise((resolve) =>
      setTimeout(resolve, Math.max(0, horizon + 50 - Date.now())),
    );
    dropAcknowledgement = true;
    await retry.click();
    await expect(retry).toBeEnabled();
    await expect(discard).toHaveCount(0);
    expect(sent).toHaveLength(2);
    expect(sent[1]).toEqual(sent[0]);
    releaseRejection();
    await expect(
      delayedPage.getByRole("button", { name: retryName, exact: true }),
    ).toBeEnabled();
    await expect(
      delayedPage.getByRole("button", { name: discardName, exact: true }),
    ).toHaveCount(0);
    await delayedPage.close();
    // A tab still showing the earlier rejection cannot discard the new attempt.
    await stalePage
      .getByRole("button", { name: discardName, exact: true })
      .click();
    expect(
      await page.evaluate((key) => localStorage.getItem(key), storageKey),
    ).not.toBeNull();
    await stalePage.close();
    await page.reload();
    await expect(retry).toBeVisible();
    await expect(discard).toHaveCount(0);
    await retry.click();
    await expect(retry).toHaveCount(0);
    expect(sent).toHaveLength(3);
    expect(sent[2]).toEqual(sent[0]);
    const rows = (
      await (
        await fetch(native + `/api/v1/budgets/${budget}/transactions`)
      ).json()
    ).data;
    expect(rows.filter((row) => row.id === sent[0].body.id)).toHaveLength(1);
    await recoveryContext.close();
  }
  // Editing unrelated fields must not erase a latent off-budget category.
  const off = randomUUID(),
    category = randomUUID(),
    latent = randomUUID();
  await write(`/budgets/${budget}/accounts`, {
    id: off,
    name: "Off budget",
    off_budget: true,
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  await write(`/budgets/${budget}/categories`, {
    id: category,
    name: "Food",
    is_income: false,
  });
  await write(
    `/budgets/${budget}/transactions`,
    {
      id: latent,
      account_id: account,
      category_id: category,
      amount: -100,
      date: "2026-01-01",
    },
    "2026-01-01T00:00:00.000Z-0000-0000000000000001",
  );
  const latentPath = `/api/v1/budgets/${budget}/transactions/${latent}`;
  expect(
    (
      await fetch(native + latentPath, {
        method: "PATCH",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": randomUUID(),
          "X-Actual-Edit-Clock":
            "2026-01-02T00:00:00.000Z-0000-0000000000000001",
        },
        body: JSON.stringify({ account_id: off }),
      })
    ).status,
  ).toBe(200);
  const latentContext = await browser.newContext();
  const latentPage = await latentContext.newPage();
  await latentPage.goto(server.url);
  await latentPage.evaluate(
    ({ budget, off }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: off }),
      ),
    { budget, off },
  );
  await latentPage.reload();
  await latentPage.getByRole("button", { name: /^Edit / }).click();
  await latentPage
    .getByLabel("Notes", { exact: true })
    .fill("Only notes changed");
  const sentPatch = latentPage.waitForRequest(
    (request) =>
      request.method() === "PATCH" && request.url().endsWith(latentPath),
  );
  await latentPage
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  expect((await sentPatch).postDataJSON()).toEqual({
    notes: "Only notes changed",
  });
  await expect(latentPage.locator(".register-table")).toContainText(
    "Only notes changed",
  );
  expect(
    (await (await fetch(native + latentPath)).json()).data.category_id,
  ).toBe(category);
  await latentContext.close();
  console.log(
    "PASS two-client field merge, stable queued retry/delete conflict, cursor-only unchanged refresh, hidden tombstone edits and explicit clock-skew recovery",
  );
} finally {
  await browser?.close();
  await server.stop();
}

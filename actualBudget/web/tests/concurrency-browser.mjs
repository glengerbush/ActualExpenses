import {
  savedWork,
  expectSavedWork,
  openSavedWork,
  history,
} from "./saved-work-ui.mjs";
import { transactionRows, transactionAction } from "./register-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const artifacts = fileURLToPath(new URL("../test-results/", import.meta.url));
await mkdir(artifacts, { recursive: true });
import { startServer, transactionCreatePattern } from "./server.mjs";
const recoveryKinds = process.argv.slice(2).map((argument) => {
  const match = /^--recovery-kind=(transaction|action)$/.exec(argument);
  if (!match) throw new Error(`Unknown concurrency scenario: ${argument}`);
  return match[1];
});
if (!recoveryKinds.length) recoveryKinds.push("transaction", "action");
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
  await transactionRows(a.page, { name: /^Edit / }).click();
  await a.page.getByLabel("Date", { exact: true }).fill("02/01/2026");
  await a.page.getByLabel("Amount", { exact: true }).fill("2");
  await a.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expectSavedWork(a.page, 1);
  await transactionRows(b.page, { name: /^Edit / }).click();
  await b.page.getByLabel("Date", { exact: true }).fill("02/02/2026");
  await b.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(b.page.locator(".register-table")).toContainText("02/02/2026");
  await a.page.reload();
  await expectSavedWork(a.page, 0);
  await expect(
    a.page.getByText("A newer edit replaced these fields: Date", {
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
  await expectSavedWork(a.page, 1);
  await b.page.getByRole("button", { name: "Refresh", exact: true }).click();
  await expect(b.page.getByTestId("register-total")).toHaveText("-2.00");
  await transactionRows(b.page, { name: /^Edit / }).click();
  await b.page.getByLabel("Date", { exact: true }).fill("02/03/2026");
  await b.page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(b.page.locator(".register-table")).toContainText("02/03/2026");
  await a.page.reload();
  await expectSavedWork(a.page, 0);
  await expect(
    a.page.getByText(
      "A newer edit kept this transaction. The pending deletion was canceled.",
      { exact: true },
    ),
  ).toBeVisible();
  await expect(a.page.locator(".register-table")).toContainText("02/03/2026");
  const deletes = attempts.filter((a) => a.method === "DELETE");
  expect(deletes[1]).toEqual(deletes[0]);
  // Unchanged refresh uses one cursor request, not the full register/reference endpoints.
  const requests = [];
  a.page.on("request", (r) => {
    if (
      r.method() === "GET" &&
      r.url().includes("/api/v1/") &&
      !r.url().endsWith("/history")
    )
      requests.push(r.url());
  });
  await a.page.getByRole("button", { name: "Refresh", exact: true }).click();
  await expect(
    a.page.getByText("Transactions are up to date.", { exact: true }),
  ).toBeVisible();
  expect(requests).toHaveLength(1);
  expect(requests[0]).toContain("/changes?");
  // Newer fields remain hidden after publication of a tombstone.
  await transactionRows(b.page, { name: /^Edit / }).click();
  await b.page.getByLabel("Date", { exact: true }).fill("02/04/2026");
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
      !r.headers()["x-actual-cancel-if-uncommitted"] &&
      transactionCreatePattern(budget).test(r.url())
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
    page.getByRole("button", {
      name: "Cancel saved change safely",
      exact: true,
    }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: artifacts + "concurrency-clock-mobile.png",
    fullPage: true,
  });
  await page.clock.setSystemTime(new Date());
  await page.reload();
  await page.getByRole("button", { name: "Retry sync", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Cancel saved change safely",
      exact: true,
    }),
  ).toBeVisible();
  await expect.poll(() => clockRequests.length).toBe(2);
  expect(clockRequests[1]).toEqual(clockRequests[0]);
  await page
    .getByRole("button", { name: "Cancel saved change safely", exact: true })
    .click();
  await expect(page.getByTestId("sync-issues"))
    .toHaveCount(0)
    .catch(async (error) => {
      console.log(await page.locator("body").innerText());
      throw error;
    });
  await expectSavedWork(page, 0);
  await page.waitForLoadState("networkidle");
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
    if (
      r.method() === "POST" &&
      !r.headers()["x-actual-cancel-if-uncommitted"] &&
      r.url().endsWith("/duplicate")
    )
      actionRequests.push({
        key: r.headers()["idempotency-key"],
        clock: r.headers()["x-actual-edit-clock"],
        body: r.postDataJSON(),
      });
  });
  await (await transactionAction(page, { name: /^Duplicate / })).click();
  await expect(
    page.getByRole("button", {
      name: "Cancel saved change safely",
      exact: true,
    }),
  ).toBeVisible();
  await page.clock.setSystemTime(new Date());
  await page.reload();
  await page.getByRole("button", { name: "Retry sync", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Cancel saved change safely",
      exact: true,
    }),
  ).toBeVisible();
  await expect.poll(() => actionRequests.length).toBe(2);
  expect(actionRequests[1]).toEqual(actionRequests[0]);
  await page
    .getByRole("button", { name: "Cancel saved change safely", exact: true })
    .click();
  await expectSavedWork(page, 0);
  await expect(page.getByTestId("register-total")).toHaveText("-1.00");
  await (await transactionAction(page, { name: /^Duplicate / })).click();
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
  for (const kind of recoveryKinds) {
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
    // Allow the quota/reload/three-tab setup to finish before this real server
    // clock boundary, including while other build workers contend for CPU.
    const horizon = Date.now() + 30000;
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
        ? transactionCreatePattern(budget)
        : `**/api/v1/budgets/${budget}/transactions/*/duplicate`;
    await recoveryContext.route(pattern, async (route) => {
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
        ? "Cancel saved change safely"
        : "Cancel saved change safely";
    const retryName = kind === "transaction" ? "Retry sync" : "Retry sync";
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
      await (
        await transactionAction(page, { name: /^Duplicate / })
      )
        .first()
        .click();
    await expect(discard).toBeVisible();
    expect(sent).toHaveLength(1);
    expect(sent[0].clock.slice(25, 29)).toBe("FFFF");
    await page.evaluate(() => {
      const original = IDBDatabase.prototype.transaction;
      IDBDatabase.prototype.transaction = function (...args) {
        if (this.name === "actualBudget.offline.v1" && args[1] === "readwrite")
          throw new DOMException(
            "Storage write blocked for test",
            "QuotaExceededError",
          );
        return original.apply(this, args);
      };
    });
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
    let holdNextRejection = true;
    await recoveryContext.route(pattern, async (route) => {
      if (route.request().method() !== "POST" || !holdNextRejection)
        return route.fallback();
      holdNextRejection = false;
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
    await expect(discard).toHaveCount(0);
    await expect.poll(() => sent.length).toBeGreaterThanOrEqual(2);
    expect(sent[1]).toEqual(sent[0]);
    releaseRejection();
    await expect(
      delayedPage.getByRole("button", { name: discardName, exact: true }),
    ).toHaveCount(0);
    await delayedPage.close();
    await stalePage.close();
    await page.reload();
    // Reload can interrupt the old tab before it releases its delivery claim.
    // Recovery must honor that 60-second lease, then replay the same request.
    try {
      await expect(retry).toHaveCount(0, { timeout: 65_000 });
    } catch (error) {
      // Failure-only evidence: do not perturb claim/reload timing on passing runs.
      let diagnosticTimer;
      try {
        const state = await Promise.race([
          page.evaluate(async (budgetId) => {
            const db = await new Promise((resolve, reject) => {
              const request = indexedDB.open("actualBudget.offline.v1");
              request.onsuccess = () => resolve(request.result);
              request.onerror = () => reject(request.error);
              request.onblocked = () =>
                reject(new Error("IndexedDB open blocked"));
            });
            let operations;
            try {
              operations = await new Promise((resolve, reject) => {
                const request = db
                  .transaction("operations")
                  .objectStore("operations")
                  .getAll();
                request.onsuccess = () => resolve(request.result);
                request.onerror = () => reject(request.error);
              });
            } finally {
              db.close();
            }
            return {
              now: Date.now(),
              clock: localStorage.getItem("actualBudget.clock.v1"),
              online: navigator.onLine,
              visibility: document.visibilityState,
              pendingMessage: document.querySelector(
                '[data-testid="sync-issues"]',
              )?.textContent,
              operations: operations
                .filter((entry) => entry.budgetId === budgetId)
                .map((entry) => ({
                  key: entry.key,
                  revision: entry.revision,
                  attempted: entry.attempted,
                  claim: entry.claim,
                  blocked: entry.blocked,
                  dependsOn: entry.dependsOn,
                  cancelRequested: entry.cancelRequested,
                  pending: entry.pending,
                })),
            };
          }, budget),
          new Promise((_, reject) => {
            diagnosticTimer = setTimeout(
              () => reject(new Error("Recovery diagnostics timed out")),
              5000,
            );
          }),
        ]).catch((diagnosticError) => ({
          diagnosticError: String(diagnosticError),
        }));
        console.error(
          "Pending recovery failure",
          JSON.stringify({ kind, hostNow: Date.now(), attempts: sent, state }),
        );
      } finally {
        clearTimeout(diagnosticTimer);
      }
      throw error;
    }
    await expect(discard).toHaveCount(0);
    await expect.poll(() => sent.length).toBeGreaterThanOrEqual(3);
    for (const attempt of sent) expect(attempt).toEqual(sent[0]);
    const rows = (
      await (
        await fetch(native + `/api/v1/budgets/${budget}/transactions`)
      ).json()
    ).data;
    expect(rows.filter((row) => row.id === sent[0].body.id)).toHaveLength(1);
    await recoveryContext.close();
  }
  // A rejected newer retry cannot authorize a local discard while an older attempt is held.
  // Server cancellation either fences that older request or recovers its committed receipt.
  for (const kind of recoveryKinds)
    for (const alreadyCommitted of [false, true]) {
      const overlap = await browser.newContext();
      const a = await overlap.newPage();
      await a.goto(server.url);
      await a.evaluate(
        ({ budget, account }) =>
          localStorage.setItem(
            "actualBudget.selection",
            JSON.stringify({ budgetId: budget, accountId: account }),
          ),
        { budget, account },
      );
      await a.reload();
      if (kind === "transaction") {
        await a
          .getByRole("button", { name: "Add transaction", exact: true })
          .click();
        await a.getByLabel("Amount", { exact: true }).fill("1");
      }
      const horizon = Date.now() + 5000;
      await a.evaluate(
        (horizon) =>
          localStorage.setItem(
            "actualBudget.clock.v1",
            new Date(horizon).toISOString() + "-FFFE-0000000000000001",
          ),
        horizon,
      );
      const pattern =
        kind === "transaction"
          ? transactionCreatePattern(budget)
          : `**/api/v1/budgets/${budget}/transactions/*/duplicate`;
      const matches = (request) =>
        request.method() === "POST" &&
        (kind === "transaction"
          ? transactionCreatePattern(budget).test(request.url())
          : request.url().endsWith("/duplicate"));
      const firstRequest = a.waitForRequest(matches);
      const retryName = kind === "transaction" ? "Retry sync" : "Retry sync";
      const discardName =
        kind === "transaction"
          ? "Cancel saved change safely"
          : "Cancel saved change safely";
      if (kind === "transaction")
        await a
          .getByRole("button", { name: "Save transaction", exact: true })
          .click();
      else
        await (
          await transactionAction(a, { name: /^Duplicate / })
        )
          .first()
          .click();
      const original = await firstRequest;
      const identity = (request) => ({
        key: request.headers()["idempotency-key"],
        clock: request.headers()["x-actual-edit-clock"],
        epoch: request.headers()["x-actual-budget-epoch"],
        owner: request.headers()["x-actual-history-owner"],
        body: request.postDataJSON(),
      });
      await expect(
        a.getByRole("button", { name: discardName, exact: true }),
      ).toBeVisible();
      let release, held, delayedStatus;
      const gate = new Promise((resolve) => {
        release = resolve;
      });
      const intercepted = new Promise((resolve) => {
        held = resolve;
      });
      async function forward(route) {
        const request = route.request();
        expect(identity(request)).toEqual(identity(original));
        const url = new URL(request.url());
        const headers = { ...request.headers(), host: url.host };
        url.hostname = "127.0.0.1";
        return route.fetch({ url: url.href, headers });
      }
      await a.route(pattern, async (route) => {
        if (!matches(route.request())) return route.continue();
        if (route.request().headers()["x-actual-cancel-if-uncommitted"])
          return route.fallback();
        held();
        await gate;
        const response = await forward(route);
        delayedStatus = response.status();
        await route.abort("failed");
      });
      await a.getByRole("button", { name: retryName, exact: true }).click();
      await intercepted;
      const b = await overlap.newPage();
      await b.goto(server.url);
      await openSavedWork(b);
      await b
        .getByRole("button", { name: "Retry saved expense", exact: true })
        .click();
      await expect(
        b.getByRole("button", { name: discardName, exact: true }),
      ).toBeVisible();
      if (alreadyCommitted) {
        await new Promise((resolve) =>
          setTimeout(resolve, Math.max(0, horizon + 50 - Date.now())),
        );
        release();
        await expect.poll(() => delayedStatus).toBe(201);
      }
      let cancellationStatus,
        cancelAttempts = 0,
        lostCancellationStatus;
      await overlap.route(pattern, async (route) => {
        if (!route.request().headers()["x-actual-cancel-if-uncommitted"])
          return route.continue();
        expect(
          route.request().headers()["x-actual-cancel-if-uncommitted"],
        ).toBe("true");
        cancelAttempts++;
        if (!alreadyCommitted && cancelAttempts === 1)
          return route.abort("failed"); // Cancellation never reached server.
        const response = await forward(route);
        cancellationStatus = response.status();
        if (cancelAttempts === (alreadyCommitted ? 1 : 2)) {
          await route.abort("failed"); // Cancellation committed, acknowledgement lost.
          lostCancellationStatus = response.status();
        } else await route.fulfill({ response });
      });
      await b.getByRole("button", { name: discardName, exact: true }).click();
      await expect.poll(() => cancelAttempts).toBe(1);
      await expect(
        b
          .getByText(
            "Cannot reach the server. Check your connection and retry.",
            { exact: true },
          )
          .first(),
      ).toBeVisible();
      await expect(
        b.getByRole("button", { name: "Retry saved expense", exact: true }),
      ).toBeEnabled();
      if (!alreadyCommitted) {
        expect(cancellationStatus).toBeUndefined();
        await b.reload();
        await expect.poll(() => lostCancellationStatus).toBe(409);
        // Successful bootstrap reads may wake pending delivery after the lost
        // cancellation ACK. Exercise that valid retry explicitly; every attempt
        // above must preserve the exact identity and cancellation fence.
        await b.evaluate(() => dispatchEvent(new Event("online")));
        await expect.poll(() => cancelAttempts).toBeGreaterThanOrEqual(3);
        await expectSavedWork(b, 0);
      }
      await expect
        .poll(() => lostCancellationStatus)
        .toBe(alreadyCommitted ? 201 : 409);
      expect(cancellationStatus).toBe(alreadyCommitted ? 201 : 409);
      if (!alreadyCommitted) {
        await new Promise((resolve) =>
          setTimeout(resolve, Math.max(0, horizon + 50 - Date.now())),
        );
        release();
        await expect.poll(() => delayedStatus).toBe(409);
      }
      await b.reload();
      await b.waitForLoadState("networkidle");
      await expectSavedWork(b, 0);
      await history(b);
      await expect(
        b.getByRole("button", { name: "Add transaction", exact: true }),
      ).toBeEnabled();
      await expect(
        b.getByRole("button", { name: retryName, exact: true }),
      ).toHaveCount(0);
      const rows = (
        await (
          await fetch(native + `/api/v1/budgets/${budget}/transactions`)
        ).json()
      ).data;
      expect(
        rows.filter((row) => row.id === original.postDataJSON().id),
      ).toHaveLength(alreadyCommitted ? 1 : 0);
      // Toasts expire; the empty durable queue and unchanged ledger prove cancellation.
      await overlap.close();
    }
  // Older API1 servers must never receive an unrecognized cancellation header as an edit.
  const unsupportedContext = await browser.newContext();
  const unsupportedPage = await unsupportedContext.newPage();
  await unsupportedPage.clock.install({ time: new Date(Date.now() + 600_000) });
  await unsupportedPage.goto(server.url);
  await unsupportedPage.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget, account },
  );
  await unsupportedPage.reload();
  await unsupportedPage
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await unsupportedPage.getByLabel("Amount", { exact: true }).fill("1");
  await unsupportedPage
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(
    unsupportedPage.getByRole("button", {
      name: "Cancel saved change safely",
      exact: true,
    }),
  ).toBeVisible();
  await unsupportedPage.route("**/api/v1/info", (route) =>
    route.fulfill({ json: { data: { api_version: 1, capabilities: [] } } }),
  );
  let unsupportedWrites = 0;
  unsupportedPage.on("request", (request) => {
    if (
      request.method() === "POST" &&
      transactionCreatePattern(budget).test(request.url())
    )
      unsupportedWrites++;
  });
  await unsupportedPage
    .getByRole("button", { name: "Cancel saved change safely", exact: true })
    .click();
  await expect(
    unsupportedPage.getByText(
      "This server does not support safe cancellation. The saved request has been kept for retry.",
      { exact: true },
    ),
  ).toBeVisible();
  await unsupportedPage.reload();
  await unsupportedPage.waitForLoadState("networkidle");
  await openSavedWork(unsupportedPage);
  await unsupportedPage
    .getByRole("button", { name: "Retry saved expense", exact: true })
    .click();
  await expect(
    unsupportedPage.getByText(
      "This server does not support safe cancellation. The saved request has been kept for retry.",
      { exact: true },
    ),
  ).toBeVisible();
  expect(unsupportedWrites).toBe(0);
  await unsupportedContext.close();
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
  await latentPage
    .locator(`.register-table tr[data-transaction-id="${latent}"]`)
    .click();
  await latentPage.getByLabel("Date", { exact: true }).fill("02/05/2026");
  const sentPatch = latentPage.waitForRequest(
    (request) =>
      request.method() === "PATCH" && request.url().endsWith(latentPath),
  );
  await latentPage
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  expect((await sentPatch).postDataJSON()).toEqual({
    date: "2026-02-05",
  });
  await expect(latentPage.locator(".register-table")).toContainText(
    "02/05/2026",
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

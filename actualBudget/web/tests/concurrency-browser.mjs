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
  console.log(
    "PASS two-client field merge, stable queued retry/delete conflict, cursor-only unchanged refresh, hidden tombstone edits and explicit clock-skew recovery",
  );
} finally {
  await browser?.close();
  await server.stop();
}

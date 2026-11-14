import { savedWork, expectSavedWork } from "./saved-work-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
let server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
async function create(path, body) {
  const response = await fetch(api + "/api/v1" + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return (await response.json()).data;
}
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
  for (const mode of ["synced", "claimed", "handoff"]) {
    await server.stop();
    server = await startServer();
    api = server.url.replace("actualbudget.test", "127.0.0.1");
    const context = await browser.newContext(),
      page = await context.newPage(),
      budget = randomUUID(),
      account = randomUUID();
    await create("/budgets", { id: budget, name: `Pending edit ${mode}` });
    await create(`/budgets/${budget}/accounts`, {
      id: account,
      name: "Spending",
      opening_balance: 10000,
      opening_date: "2026-01-01",
    });
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
    await expect(page.getByTestId("register-total")).toHaveText("100.00");
    await context.route("**/api/v1/**", (route) => route.abort());
    await page.evaluate(() => dispatchEvent(new Event("offline")));
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("1");
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await page
      .locator(".register-table tr[data-transaction-id]")
      .filter({ hasText: /-[123]\.00/ })
      .first()
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("2");
    const sent = [];
    let release, claimed;
    const held = new Promise((resolve) => (release = resolve)),
      observed = new Promise((resolve) => (claimed = resolve));
    let holdFirst = mode === "claimed";
    const capture = async (route) => {
      const request = route.request();
      if (!["POST", "PATCH"].includes(request.method()))
        return route.continue();
      sent.push({
        method: request.method(),
        key: request.headers()["idempotency-key"],
        epoch: request.headers()["x-actual-budget-epoch"],
        body: request.postDataJSON(),
      });
      if (holdFirst && request.method() === "POST") {
        holdFirst = false;
        claimed();
        await held;
      }
      return route.continue();
    };
    if (mode !== "handoff") {
      await context.unroute("**/api/v1/**");
      await context.route(
        `**/api/v1/budgets/${budget}/transactions**`,
        capture,
      );
      await page.evaluate(() => dispatchEvent(new Event("online")));
      if (mode === "synced") await expectSavedWork(page, 0);
      else await observed;
    } else {
      await page.evaluate(() => {
        const remove = Storage.prototype.removeItem;
        Storage.prototype.removeItem = function (key) {
          if (key.startsWith("actualBudget.transaction.v1."))
            throw new DOMException("Interrupted handoff", "QuotaExceededError");
          return remove.call(this, key);
        };
      });
    }
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    if (mode === "claimed") {
      await expectSavedWork(page, 2);
      release();
      await expect(page.locator(".transaction-form")).toHaveCount(0);
    }
    if (mode === "handoff") {
      await expect(
        page.getByRole("alert").filter({ hasText: "Interrupted handoff" }),
      ).toBeVisible();
      await page.reload();
      await expect(page.locator(".transaction-form")).toHaveCount(0);
      await expect(page.locator(".register-table")).toContainText("-2.00");
      await context.unroute("**/api/v1/**");
      await context.route(
        `**/api/v1/budgets/${budget}/transactions**`,
        capture,
      );
      await page.evaluate(() => dispatchEvent(new Event("online")));
    }
    await expect(page.getByTestId("register-total"))
      .toHaveText("98.00")
      .catch(async (error) => {
        console.log(mode, sent, await page.locator("body").innerText());
        throw error;
      });
    await expectSavedWork(page, 0);
    await expect(page.locator(".transaction-form")).toHaveCount(0);
    const rows = (
      await (await fetch(api + `/api/v1/budgets/${budget}/transactions`)).json()
    ).data.filter((row) => !row.starting_balance);
    expect(rows).toHaveLength(1);
    expect(rows[0].amount).toBe(-200);
    if (mode === "handoff") {
      expect(sent).toHaveLength(1);
      expect(sent[0].method).toBe("POST");
      expect(sent[0].body.amount).toBe(-200);
    } else {
      expect(sent.map((r) => r.method)).toEqual(["POST", "PATCH"]);
      expect(sent[0].body.amount).toBe(-100);
      expect(sent[1].body).toEqual({ amount: -200 });
      expect(sent[1].key).not.toBe(sent[0].key);
      expect(sent[1].epoch).toBe(sent[0].epoch);
    }
    console.log(
      `PASS pending editor ${mode}: original identity immutable, edited amount retained, one financial row`,
    );
    await context.close();
  }
} finally {
  await browser?.close();
  await server.stop();
}

import { savedWork, expectSavedWork } from "./saved-work-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
let server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
async function create(path, body) {
  const r = await fetch(api + "/api/v1" + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
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
  for (const action of ["deliver", "delete"]) {
    await server.stop();
    server = await startServer();
    api = server.url.replace("actualbudget.test", "127.0.0.1");
    const context = await browser.newContext(),
      page = await context.newPage(),
      budget = randomUUID(),
      account = randomUUID();
    await create("/budgets", { id: budget, name: `Reload pending ${action}` });
    await create(`/budgets/${budget}/accounts`, {
      id: account,
      name: "Spending",
      opening_balance: 10000,
      opening_date: "2026-01-01",
    });
    // Reverse key order: IDB's UUID ordering must never select an older local edit.
    await context.addInitScript(() => {
      let serial = Number(sessionStorage.getItem("test-uuid-serial") ?? 999999);
      crypto.randomUUID = () => {
        sessionStorage.setItem("test-uuid-serial", String(--serial));
        return `f0000000-0000-4000-8000-${String(serial).padStart(12, "0")}`;
      };
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
    await page.reload();
    await expect(page.getByLabel("Amount", { exact: true })).toHaveValue("2");
    await page.evaluate(() => dispatchEvent(new Event("offline")));
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await expect(page.locator(".transaction-form")).toHaveCount(0);
    await expect(page.locator(".register-table")).toContainText("-2.00");
    await expect(
      page
        .locator(".register-table tr[data-transaction-id]")
        .filter({ hasText: /-[123]\.00/ })
        .first()
        .getByRole("button", { name: /^Delete / }),
    ).toBeVisible();
    await page
      .locator(".register-table tr[data-transaction-id]")
      .filter({ hasText: /-[123]\.00/ })
      .first()
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("3");
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await expect(page.locator(".register-table")).toContainText("-3.00");
    if (action === "delete") {
      await page
        .locator(".register-table tr[data-transaction-id]")
        .filter({ hasText: /-[123]\.00/ })
        .first()
        .getByRole("button", { name: /^Delete / })
        .click();
      await expectSavedWork(page, 0);
    }
    await context.unroute("**/api/v1/**");
    await page.evaluate(() => dispatchEvent(new Event("online")));
    await expectSavedWork(page, 0);
    await expect(page.getByTestId("register-total")).toHaveText(
      action === "delete" ? "100.00" : "97.00",
    );
    const rows = (
      await (await fetch(api + `/api/v1/budgets/${budget}/transactions`)).json()
    ).data.filter((row) => !row.starting_balance);
    expect(rows).toHaveLength(action === "delete" ? 0 : 1);
    if (action === "deliver") expect(rows[0].amount).toBe(-300);
    console.log(
      `PASS reloaded unsent editor retains subsequent offline edit/${action} eligibility`,
    );
    await context.close();
  }
  for (const mode of ["revert", "mixed", "claimed", "cancel", "handoff"]) {
    await server.stop();
    server = await startServer();
    api = server.url.replace("actualbudget.test", "127.0.0.1");
    const context = await browser.newContext(),
      page = await context.newPage(),
      budget = randomUUID(),
      account = randomUUID();
    await create("/budgets", { id: budget, name: `Pending revert ${mode}` });
    await create(`/budgets/${budget}/accounts`, {
      id: account,
      name: "Spending",
      opening_balance: 10000,
      opening_date: "2026-01-01",
    });
    // Reverse key order: IDB's UUID ordering must never select an older local edit.
    await context.addInitScript(() => {
      let serial = Number(sessionStorage.getItem("test-uuid-serial") ?? 999999);
      crypto.randomUUID = () => {
        sessionStorage.setItem("test-uuid-serial", String(--serial));
        return `f0000000-0000-4000-8000-${String(serial).padStart(12, "0")}`;
      };
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
    await expect(
      page.getByRole("radio", { name: "Income", exact: true }),
    ).toBeDisabled();
    await expect(
      page.getByRole("combobox", { name: "Payee", exact: true }),
    ).toBeVisible();
    await page.getByLabel("Amount", { exact: true }).fill("2");
    let stage = "first",
      committedSecond = false;
    const sent = [];
    await context.unroute("**/api/v1/**");
    await context.route(
      `**/api/v1/budgets/${budget}/transactions**`,
      async (route) => {
        const request = route.request();
        if (!["POST", "PATCH"].includes(request.method()))
          return route.continue();
        if (stage === "blocked") return route.abort();
        const record = {
          method: request.method(),
          key: request.headers()["idempotency-key"],
          epoch: request.headers()["x-actual-budget-epoch"],
          body: request.postDataJSON(),
        };
        sent.push(record);
        if (
          stage === "first" ||
          (stage === "second" && request.method() === "PATCH")
        ) {
          const target = new URL(request.url()),
            headers = { ...request.headers(), host: target.host };
          target.hostname = "127.0.0.1";
          const response = await route.fetch({ url: target.href, headers });
          expect(response.ok(), await response.text()).toBe(true);
          if (stage === "second") committedSecond = true;
          stage = "blocked";
          return route.abort();
        }
        return route.continue();
      },
    );
    await page.evaluate(() => dispatchEvent(new Event("online")));
    await expect.poll(() => stage).toBe("blocked");
    await page.evaluate(() => dispatchEvent(new Event("offline")));
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await expectSavedWork(page, 2);
    await page
      .locator(".register-table tr[data-transaction-id]")
      .filter({ hasText: "-2.00" })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("1");
    if (mode === "mixed")
      await page.getByLabel("Date", { exact: true }).fill("02/03/2026");
    if (mode === "claimed") {
      stage = "second";
      await page.evaluate(() => dispatchEvent(new Event("online")));
      await expect.poll(() => committedSecond).toBe(true);
      await page.evaluate(() => dispatchEvent(new Event("offline")));
    }
    if (mode === "handoff")
      await page.evaluate(() => {
        const remove = Storage.prototype.removeItem;
        Storage.prototype.removeItem = function (key) {
          if (key.startsWith("actualBudget.transaction.v1."))
            throw new DOMException(
              "Interrupted empty replacement",
              "QuotaExceededError",
            );
          return remove.call(this, key);
        };
      });
    await page
      .getByRole("button", {
        name: mode === "cancel" ? "Cancel" : "Save transaction",
        exact: true,
      })
      .click();
    if (mode === "handoff") {
      await expect(
        page
          .getByRole("alert")
          .filter({ hasText: "Interrupted empty replacement" }),
      ).toBeVisible();
      await page.reload();
    }
    await expect(page.locator(".transaction-form")).toHaveCount(0);
    if (["revert", "handoff"].includes(mode)) await expectSavedWork(page, 1);
    stage = "deliver";
    await page.evaluate(() => dispatchEvent(new Event("online")));
    await expectSavedWork(page, 0);
    await expect(page.getByTestId("register-total")).toHaveText(
      mode === "cancel" ? "98.00" : "99.00",
    );
    const rows = (
      await (await fetch(api + `/api/v1/budgets/${budget}/transactions`)).json()
    ).data.filter((row) => !row.starting_balance);
    expect(rows).toHaveLength(1);
    expect(rows[0].amount).toBe(mode === "cancel" ? -200 : -100);
    if (mode === "mixed") {
      expect(sent.map((request) => request.method)).toEqual([
        "POST",
        "POST",
        "PATCH",
      ]);
      expect(rows[0].date).toBe("2026-02-03");
      expect(
        sent.filter((r) => r.method === "PATCH").map((r) => r.body),
      ).toEqual([{ date: "2026-02-03" }]);
    }
    if (["revert", "handoff"].includes(mode))
      expect(sent.filter((r) => r.method === "PATCH")).toHaveLength(0);
    if (mode === "claimed") {
      const patches = sent.filter((r) => r.method === "PATCH");
      expect(patches.at(-1).body).toEqual({ amount: -100 });
      expect(patches.at(-1).key).not.toBe(patches[0].key);
      expect(patches.at(-1).epoch).toBe(patches[0].epoch);
    }
    console.log(
      `PASS pending PATCH ${mode}: displayed edit intent, dependency order and exact final financial effect`,
    );
    await context.close();
  }
} finally {
  await browser?.close();
  await server.stop();
}

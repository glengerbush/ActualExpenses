import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer(),
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
  for (const mode of ["revert", "mixed", "claimed", "cancel", "handoff"]) {
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
    await page.getByLabel("Notes", { exact: true }).fill("Original expense");
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Edit pending expense", exact: true })
      .click();
    await expect(
      page.getByLabel("Transaction type").locator('option[value="income"]'),
    ).toBeDisabled();
    await expect(
      page.getByLabel("Payee", { exact: true }).locator('option[value="new"]'),
    ).toBeDisabled();
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
    await expect(page.locator(".pending-expense")).toHaveCount(2);
    await page
      .locator(".pending-expense")
      .filter({ hasText: "-2.00" })
      .getByRole("button", { name: "Edit pending expense", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("1");
    if (mode === "mixed")
      await page
        .getByLabel("Notes", { exact: true })
        .fill("Changed note with reverted amount");
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
    if (["revert", "handoff"].includes(mode))
      await expect(page.locator(".pending-expense")).toHaveCount(1);
    stage = "deliver";
    await page.evaluate(() => dispatchEvent(new Event("online")));
    await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
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
      expect(rows[0].notes).toBe("Changed note with reverted amount");
      expect(
        sent.filter((r) => r.method === "PATCH").map((r) => r.body),
      ).toEqual([{ notes: "Changed note with reverted amount" }]);
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

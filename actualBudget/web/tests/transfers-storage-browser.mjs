import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
async function post(path, body) {
  const r = await fetch(api + "/api/v1" + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
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
  for (const scenario of [
    "retry",
    "reload",
    "cancel",
    "cancel_write",
    "stale",
  ]) {
    const context = await browser.newContext(),
      page = await context.newPage();
    const budget = randomUUID(),
      from = randomUUID(),
      to = randomUUID(),
      root = `/budgets/${budget}`,
      key = `actualBudget.transfers.v1.${budget}`;
    await post("/budgets", { id: budget, name: `Storage ${scenario}` });
    for (const [id, name] of [
      [from, "Checking"],
      [to, "Reserve"],
    ])
      await post(root + "/accounts", {
        id,
        name,
        opening_balance: 0,
        opening_date: "2026-01-01",
      });
    await page.goto(server.url);
    await page
      .locator(".budget-list button")
      .filter({ hasText: `Storage ${scenario}` })
      .click();
    await page.getByRole("button", { name: "Move money", exact: true }).click();
    await page.getByLabel("From account", { exact: true }).selectOption(from);
    await page.getByLabel("To account", { exact: true }).selectOption(to);
    await page.getByLabel("Amount", { exact: true }).fill("-12.34");
    await page.getByLabel("Notes", { exact: true }).fill("Keep entered fields");
    await page.evaluate((key) => {
      const write = Storage.prototype.setItem;
      window.failTransferStorage = true;
      window.failedTransferWrites = [];
      Storage.prototype.setItem = function (k, value) {
        if (
          k === key &&
          (window.failTransferStorage ||
            (window.failCancellationWrite &&
              JSON.parse(value).pending?.cancelRequested)) &&
          JSON.parse(value).pending
        ) {
          window.failedTransferWrites.push(JSON.parse(value));
          throw new DOMException(
            "Transfer storage quota exhausted",
            "QuotaExceededError",
          );
        }
        return write.call(this, k, value);
      };
    }, key);
    const requests = [];
    await page.route("**/api/v1" + root + "/transfers", async (route) => {
      const request = route.request();
      const durable = await page.evaluate(
        (key) => JSON.parse(localStorage.getItem(key)),
        key,
      );
      requests.push({
        body: JSON.parse(request.postData()),
        key: request.headers()["idempotency-key"],
        clock: request.headers()["x-actual-edit-clock"],
        epoch: request.headers()["x-actual-budget-epoch"],
        cancel: request.headers()["x-actual-cancel-if-uncommitted"],
        durable,
      });
      await route.continue();
    });
    await page
      .getByRole("button", { name: "Save transfer", exact: true })
      .click();
    const retry = page.getByRole("button", {
      name: "Retry saved transfer",
      exact: true,
    });
    await expect(retry).toBeEnabled();
    expect(requests).toHaveLength(0);
    expect(
      await page.evaluate(
        (key) => JSON.parse(localStorage.getItem(key)).pending,
        key,
      ),
    ).toBeNull();
    await retry.click();
    await expect(retry).toBeEnabled();
    expect(requests).toHaveLength(0);
    const original = (await page.evaluate(() => window.failedTransferWrites[0]))
      .pending;
    if (scenario === "stale") {
      const second = await context.newPage();
      await second.goto(page.url());
      await expect(second.getByLabel("Notes", { exact: true })).toHaveValue(
        "Keep entered fields",
      );
      await second
        .getByLabel("Notes", { exact: true })
        .fill("Owned by second tab");
      await page.evaluate(() => (window.failTransferStorage = false));
      await retry.click();
      await expect(page.getByRole("alert")).toContainText(
        "Another tab changed",
      );
      expect(requests).toHaveLength(0);
      expect(
        await page.evaluate(
          (key) => JSON.parse(localStorage.getItem(key)).notes,
          key,
        ),
      ).toBe("Owned by second tab");
    } else if (scenario === "reload") {
      await page.reload();
      await expect(page.getByLabel("Notes", { exact: true })).toHaveValue(
        "Keep entered fields",
      );
      await expect(
        page.getByRole("button", { name: "Save transfer", exact: true }),
      ).toBeEnabled();
      expect(requests).toHaveLength(0);
      await page
        .getByRole("button", { name: "Save transfer", exact: true })
        .click();
      await expect(
        page.getByRole("heading", { name: "Move money", exact: true }),
      ).toHaveCount(0);
      expect(requests).toHaveLength(1);
    } else {
      const action = scenario.startsWith("cancel")
        ? page.getByRole("button", {
            name: "Cancel pending transfer",
            exact: true,
          })
        : retry;
      if (scenario.startsWith("cancel")) {
        await action.click();
        await expect(retry).toBeEnabled();
        expect(requests).toHaveLength(0);
      }
      await page.evaluate(() => (window.failTransferStorage = false));
      if (scenario === "cancel_write") {
        await page.evaluate(() => (window.failCancellationWrite = true));
        await action.click();
        await expect(retry).toBeEnabled();
        expect(requests).toHaveLength(0);
        // Reload must retain the separately persisted cancellation fence even
        // when its draft write failed; retry may never turn into a create.
        await page.reload();
        await expect(retry).toBeEnabled();
        await retry.click();
      } else await action.click();
      if (scenario.startsWith("cancel"))
        await expect(page.getByRole("alert")).toContainText(
          "Transfer request canceled",
        );
      else
        await expect(
          page.getByRole("heading", { name: "Move money", exact: true }),
        ).toHaveCount(0);
      expect(requests).toHaveLength(1);
      if (scenario.startsWith("cancel"))
        expect(requests[0].cancel).toBe("true");
      expect([
        requests[0].body,
        requests[0].key,
        requests[0].clock,
        requests[0].epoch,
      ]).toEqual([
        original.body,
        original.key,
        original.edit_clock,
        original.budgetEpoch,
      ]);
    }
    for (const request of requests) {
      expect(request.durable.pending.body).toEqual(request.body);
      expect(request.durable.pending.key).toBe(request.key);
      expect(request.durable.pending.edit_clock).toBe(request.clock);
      expect(request.durable.pending.budgetEpoch).toBe(request.epoch);
    }
    const rows = await (
      await fetch(api + "/api/v1" + root + "/transactions")
    ).json();
    expect(rows.data).toHaveLength(
      ["retry", "reload"].includes(scenario) ? 2 : 0,
    );
    await context.close();
  }
  console.log(
    "PASS transfer quota failures block save/retry/cancel, preserve reload fields and stale-tab ownership; every dispatched intent is durable",
  );
} finally {
  await browser?.close();
  await server.stop();
}

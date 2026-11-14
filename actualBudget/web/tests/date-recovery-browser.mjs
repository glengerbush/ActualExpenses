import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer, transactionCreatePattern } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, method = "POST") {
  const response = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(response.ok).toBe(true);
  return (await response.json()).data;
}
const currentYear = new Date().getUTCFullYear();
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
  for (const [format, raw, expected] of [
    ["MM/dd/yyyy", "4/5", `${currentYear}-04-05`],
    ["dd/MM/yyyy", "4/5", `${currentYear}-05-04`],
    ["dd.MM.yyyy", "04.05.2026", "2026-05-04"],
    ["yyyy-MM-dd", "2026-4-5", "2026-04-05"],
  ]) {
    const context = await browser.newContext({ timezoneId: "UTC" }),
      page = await context.newPage();
    await page.clock.install({ time: new Date() });
    async function loseFirstAck(path) {
      const attempts = [];
      if (format !== "MM/dd/yyyy") return attempts;
      const pattern = typeof path === "string" ? "**/api/v1" + path : path;
      await page.route(pattern, async (route) => {
        const request = route.request();
        if (request.method() !== "POST") return route.continue();
        attempts.push({
          key: request.headers()["idempotency-key"],
          body: request.postDataJSON(),
        });
        if (attempts.length !== 1) return route.continue();
        const target = new URL(request.url());
        const headers = { ...request.headers(), host: target.host };
        target.hostname = "127.0.0.1";
        const result = await route.fetch({ url: target.href, headers });
        expect(result.status()).toBe(201);
        await route.abort("failed");
      });
      return attempts;
    }
    async function retryAfterYearChange(button, attempts) {
      if (format !== "MM/dd/yyyy") return;
      if (button === "Retry pending expense")
        await expect(
          page
            .getByText(
              "Cannot reach the server. Check your connection and retry.",
              { exact: true },
            )
            .first(),
        ).toBeVisible();
      await page.clock.setSystemTime(
        new Date(`${currentYear + 1}-11-08T12:00:00Z`),
      );
      await page.reload();
      if (button !== "Retry pending expense")
        await page.getByRole("button", { name: button, exact: true }).click();
      await expect.poll(() => attempts.length).toBe(2);
      expect(attempts[1]).toEqual(attempts[0]);
      await page.clock.setSystemTime(new Date());
    }
    const budget = randomUUID();
    await write("/budgets", { id: budget, name: "Recovered dates" });
    await write(
      `/budgets/${budget}/preferences`,
      { date_format: format },
      "PATCH",
    );
    await page.goto(server.url);
    await page.evaluate(
      (budget) =>
        localStorage.setItem(
          "actualBudget.selection",
          JSON.stringify({ budgetId: budget, accountId: null }),
        ),
      budget,
    );
    await page.reload();
    await expect(page.getByLabel("Show through date")).toHaveAttribute(
      "placeholder",
      format.toUpperCase(),
    );
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("1");
    await page.getByLabel("Date", { exact: true }).fill("");
    await page.getByLabel("Date", { exact: true }).pressSequentially(raw);
    // Captured draft format must win even after shared display format changes.
    await write(
      `/budgets/${budget}/preferences`,
      { date_format: format === "MM/dd/yyyy" ? "dd/MM/yyyy" : "MM/dd/yyyy" },
      "PATCH",
    );
    await page.reload();
    const transactionAttempts = await loseFirstAck(
      transactionCreatePattern(budget),
    );
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    await retryAfterYearChange("Retry pending expense", transactionAttempts);
    await expect(
      page.getByRole("button", { name: "Add transaction", exact: true }),
    ).toBeEnabled();
    const transactions = (
      await (await fetch(api + `/api/v1/budgets/${budget}/transactions`)).json()
    ).data;
    expect(transactions).toHaveLength(1);
    expect(transactions.find((t) => t.amount === -100).date).toBe(expected);
    await context.close();
  }
  console.log(
    "PASS four restored transaction dates with captured formats; lost acknowledgements preserve payloads across year changes",
  );
} finally {
  await browser?.close();
  await server.stop();
}

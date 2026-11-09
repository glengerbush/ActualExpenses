import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer(),
  native = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
async function write(method, path, body, clock, key = randomUUID()) {
  return fetch(native + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": key,
      ...(clock ? { "X-Actual-Edit-Clock": clock } : {}),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
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
  for (const hidden of [false, true]) {
    const budget = randomUUID(),
      account = randomUUID(),
      id = randomUUID(),
      path = `/budgets/${budget}/transactions/${id}`;
    expect(
      (
        await write("POST", "/budgets", {
          id: budget,
          name: "Receipt feedback",
        })
      ).status,
    ).toBe(201);
    expect(
      (
        await write("POST", `/budgets/${budget}/accounts`, {
          id: account,
          name: "Spending",
          opening_balance: 0,
          opening_date: "2026-01-01",
        })
      ).status,
    ).toBe(201);
    expect(
      (
        await write(
          "POST",
          `/budgets/${budget}/transactions`,
          { id, account_id: account, amount: -100, date: "2026-01-01" },
          "2026-01-01T00:00:00.000Z-0000-0000000000000001",
        )
      ).status,
    ).toBe(201);
    const context = await browser.newContext(),
      page = await context.newPage();
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
    await page.getByRole("button", { name: /^Edit / }).click();
    await page.getByLabel("Amount", { exact: true }).fill("2");
    await page.getByLabel("Notes", { exact: true }).fill("Original notes");
    const horizon = Date.now() + 2500;
    await page.evaluate(
      (horizon) =>
        localStorage.setItem(
          "actualBudget.clock.v1",
          new Date(horizon).toISOString() + "-FFFE-0000000000000001",
        ),
      horizon,
    );
    const captured = page.waitForRequest(
      (r) => r.method() === "PATCH" && r.url().endsWith(path),
    );
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    const request = await captured;
    await expect(
      page.getByRole("button", { name: "Discard rejected edit", exact: true }),
    ).toBeVisible();
    if (hidden)
      expect(
        (
          await write(
            "DELETE",
            path,
            undefined,
            "2026-01-02T00:00:00.000Z-0000-0000000000000001",
          )
        ).status,
      ).toBe(200);
    else
      expect(
        (
          await write(
            "PATCH",
            path,
            { notes: "Newer winning notes" },
            new Date(horizon + 1).toISOString() + "-0000-0000000000000002",
          )
        ).status,
      ).toBe(200);
    await new Promise((resolve) =>
      setTimeout(resolve, Math.max(0, horizon + 50 - Date.now())),
    );
    const committed = await write(
      "PATCH",
      path,
      request.postDataJSON(),
      request.headers()["x-actual-edit-clock"],
      request.headers()["idempotency-key"],
    );
    expect(committed.status).toBe(200);
    const outcome = await committed.json();
    if (hidden) expect(outcome.data.deleted).toBe(true);
    else
      expect(outcome.conflicts.map((field) => field.field)).toContain("notes");
    await page
      .getByRole("button", { name: "Discard rejected edit", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Add transaction", exact: true }),
    ).toBeEnabled();
    await expect(
      page.getByText(
        hidden
          ? "This transaction remains deleted. Its newer field edits were retained without restoring it."
          : "A newer edit replaced these fields: Notes",
        { exact: true },
      ),
    ).toBeVisible();
    if (hidden)
      await expect(page.getByTestId("register-total")).toHaveText("0.00");
    else {
      await expect(page.getByTestId("register-total")).toHaveText("-2.00");
      await expect(page.locator(".register-table")).toContainText(
        "Newer winning notes",
      );
    }
    await context.close();
  }
  console.log(
    "PASS cancellation recovers committed per-field conflict and hidden-row feedback",
  );
} finally {
  await browser?.close();
  await server.stop();
}

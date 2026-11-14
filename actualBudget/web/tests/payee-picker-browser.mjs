import { chromium, expect } from "@playwright/test";
import { startServer } from "./server.mjs";
import { randomUUID } from "node:crypto";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const b = randomUUID(),
    a = randomUUID(),
    root = `/budgets/${b}`;
  await call("/budgets", { id: b, name: "Payee search" });
  await call(root + "/accounts", {
    id: a,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  for (const name of ["Coffee House", "Coffee Corner", "Supermarket"])
    await call(root + "/payees", { id: randomUUID(), name });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  p.on("pageerror", (e) => errors.push(e.message));
  await p.goto(server.url);
  await p
    .getByRole("button", { name: "Add Transaction", exact: false })
    .click();
  await expect(
    p.locator("tbody tr.new-transaction-row .transaction-form"),
  ).toBeVisible();
  await expect(
    p.getByRole("radio", { name: "Expense", exact: true }),
  ).toBeChecked();
  const input = p.locator("#transaction-payee");
  await p.locator("#transaction-amount").fill("12.50");
  await input.click();
  await expect(p.locator(".payee-menu").getByRole("option").last()).toHaveText(
    "Create payee",
  );
  await expect(
    p.locator(".payee-menu").getByRole("option").last(),
  ).toBeDisabled();
  await input.fill("Coffee House");
  await expect(p.locator(".payee-menu").getByRole("option").last()).toHaveText(
    "Create payee “Coffee House”",
  );
  await expect(
    p.locator(".payee-menu").getByRole("option").last(),
  ).toBeDisabled();
  await input.fill("coFFee");
  await expect(p.locator(".payee-menu").getByRole("option").last()).toHaveText(
    "Create payee “coFFee”",
  );
  await expect(
    p.locator(".payee-menu").getByRole("option").last(),
  ).toBeEnabled();
  await expect(p.locator(".payee-menu").getByRole("option")).toHaveCount(3);
  await expect(
    p
      .locator(".payee-menu")
      .getByRole("option", { name: "Coffee House", exact: true }),
  ).toBeVisible();
  await p
    .locator(".payee-menu")
    .getByRole("option", { name: "Coffee House", exact: true })
    .click();
  await expect(input).toHaveValue("Coffee House");
  await expect(p.getByRole("listbox")).toHaveCount(0);
  await input.fill("New Bakery");
  await expect(p.locator(".payee-menu").getByRole("option")).toHaveCount(1);
  await expect(
    p
      .locator(".payee-menu")
      .getByRole("option", { name: "Create payee “New Bakery”", exact: true }),
  ).toBeVisible();
  await p.screenshot({ path: "../../.scratch/payee-create.png" });
  await p
    .locator(".payee-menu")
    .getByRole("option", { name: "Create payee “New Bakery”", exact: true })
    .click();
  await expect(input).toHaveValue("New Bakery");
  await expect(input).toBeEnabled();
  expect(
    (await call(root + "/payees")).filter((x) => x.name === "New Bakery"),
  ).toHaveLength(1);
  expect(await call(root + "/transactions")).toHaveLength(0);
  await expect(p.locator("#new-payee-name")).toHaveCount(0);
  await p
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(p.locator(".transaction-form")).toHaveCount(0);
  const tx = (await call(root + "/transactions"))[0];
  expect(tx.payee_name).toBe("New Bakery");
  // Existing rows use the same picker; arrow/Enter selects without submitting the transaction.
  await p.locator(`[data-transaction-id="${tx.id}"]`).click();
  await input.fill("super");
  await input.press("ArrowDown");
  await input.press("Enter");
  await expect(input).toHaveValue("Supermarket");
  await expect(p.locator(".transaction-form")).toBeVisible();
  await input.fill("Coffee");
  await p.screenshot({ path: "../../.scratch/payee-inline-search.png" });
  await input.press("Escape");
  await expect(input).toHaveValue("Supermarket");
  await p.getByRole("button", { name: "Cancel", exact: true }).click();
  // Lost response after commit: retry across reload must reuse the exact creation operation.
  await p
    .getByRole("button", { name: "Add Transaction", exact: false })
    .click();
  await p.locator("#transaction-amount").fill("9.00");
  let key;
  await p.route("**/api/v1" + root + "/payees", async (route) => {
    if (route.request().method() === "POST") {
      key = route.request().headers()["idempotency-key"];
      const response = await route.fetch({
        url: route.request().url().replace("actualbudget.test", "127.0.0.1"),
        headers: {
          ...route.request().headers(),
          host: new URL(route.request().url()).host,
        },
      });
      expect(response.ok(), await response.text()).toBe(true);
      await route.abort("failed");
    } else await route.continue();
  });
  await input.fill("Retry Bakery");
  await p
    .locator(".payee-menu")
    .getByRole("option", { name: "Create payee “Retry Bakery”", exact: true })
    .click();
  await expect(
    p.getByRole("button", { name: "Retry payee creation", exact: true }),
  ).toBeEnabled();
  await expect(
    p.getByRole("button", { name: "Save transaction", exact: true }),
  ).toBeDisabled();
  await p.unroute("**/api/v1" + root + "/payees");
  await p.reload();
  await expect(
    p.getByRole("button", { name: "Retry payee creation", exact: true }),
  ).toBeVisible();
  let retried;
  p.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith(root + "/payees"))
      retried = r.headers()["idempotency-key"];
  });
  await p
    .getByRole("button", { name: "Retry payee creation", exact: true })
    .click();
  await expect(input).toHaveValue("Retry Bakery");
  await expect(input).toBeEnabled();
  expect(retried).toBe(key);
  expect(
    (await call(root + "/payees")).filter((x) => x.name === "Retry Bakery"),
  ).toHaveLength(1);
  // Canceling a transaction does not lose an already-created payee from the next editor.
  await p.getByRole("button", { name: "Cancel", exact: true }).click();
  await p
    .getByRole("button", { name: "Add Transaction", exact: false })
    .click();
  await input.fill("Retry Bakery");
  await expect(
    p
      .locator(".payee-menu")
      .getByRole("option", { name: "Retry Bakery", exact: true }),
  ).toBeVisible();
  await p
    .locator(".payee-menu")
    .getByRole("option", { name: "Retry Bakery", exact: true })
    .click();
  await input.fill("");
  await input.press("Escape");
  await expect(input).toHaveValue("");
  await p.setViewportSize({ width: 390, height: 844 });
  await input.fill("Coffee");
  await expect(p.locator(".payee-menu").getByRole("option")).toHaveCount(3);
  await p.screenshot({ path: "../../.scratch/payee-mobile-search.png" });
  expect(
    await p.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  // Cached search stays available offline; creation is clearly unavailable.
  await input.press("Escape");
  await p.context().setOffline(true);
  await input.fill("Offline Shop");
  await expect(
    p
      .locator(".payee-menu")
      .getByRole("option", { name: "Create payee “Offline Shop”" }),
  ).toBeDisabled();
  expect(errors).toEqual([]);
  console.log(
    "PASS live matching, immediate creation, transaction save, keyboard selection, reload/retry without duplicates, mobile and offline",
  );
} finally {
  await browser?.close();
  await server.stop();
}

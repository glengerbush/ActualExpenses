import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, rm, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const original = await mkdtemp(`${tmpdir()}/recovery-original-`),
  copied = await mkdtemp(`${tmpdir()}/recovery-fault-copy-`);
let server, browser;
const budget = randomUUID(),
  account = randomUUID();
try {
  server = await startServer({ dataDirectory: original });
  const write = async (path, body) => {
    const r = await fetch(
      server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1" + path,
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": randomUUID(),
        },
        body: JSON.stringify(body),
      },
    );
    if (!r.ok) throw Error(await r.text());
    return r.json();
  };
  await write("/budgets", {
    id: budget,
    name: "Recovery browser",
    budget_type: "tracking",
  });
  await write(`/budgets/${budget}/accounts`, {
    id: account,
    name: "Spending",
    opening_balance: 1000,
    opening_date: "2026-11-09",
    off_budget: false,
  });
  await server.stop();
  // Labeled FAULT INJECTION on a disposable COPY of HTTP-created current data.
  // The original remains intact; this is not a fabricated old-schema fixture.
  execFileSync("python3", [
    "-c",
    'import sqlite3,sys; a=sqlite3.connect(sys.argv[1]); b=sqlite3.connect(sys.argv[2]); a.backup(b); b.execute("UPDATE ledger_totals SET total=9999,cleared=9999"); b.commit(); b.close(); a.close()',
    original + "/actualbudget.sqlite3",
    copied + "/actualbudget.sqlite3",
  ]);
  server = await startServer({ dataDirectory: copied });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    }),
    page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByRole("button", { name: /Recovery browser.*Open/ }).click();
  await expect(page.locator("td.amount").first()).toHaveText("99.99");
  await page
    .getByRole("button", { name: "Recovery tools", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Recovery tools", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Inspect budget", exact: true })
    .click();
  await expect(page.getByTestId("recovery-account")).toContainText("99.99");
  await expect(page.getByTestId("recovery-account")).toContainText("10.00");
  await mkdir("artifacts", { recursive: true });
  await page.screenshot({
    path: "artifacts/recovery-preview-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  const comparison = page.getByRole("region", {
    name: "Account balance comparison",
  });
  await comparison.focus();
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(() => comparison.evaluate((element) => element.scrollLeft))
    .toBeGreaterThan(0);
  await page.screenshot({
    path: "artifacts/recovery-preview-mobile.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.getByLabel("Confirm budget name").fill("Recovery browser");
  // Cancellation intent must survive an abort BEFORE either request reaches
  // the server. Reload/retry may only send the original cancellation fence.
  const canceledAttempts = [];
  await page.route(`**/api/v1/budgets/${budget}/recovery`, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    canceledAttempts.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
      epoch: route.request().headers()["x-actual-budget-epoch"],
      cancel:
        route.request().headers()["x-actual-cancel-if-uncommitted"] ?? "false",
    });
    if (canceledAttempts.length <= 2) await route.abort();
    else await route.continue();
  });
  await page
    .getByRole("button", { name: "Apply reviewed repair", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved repair", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Cancel saved repair safely", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    "Cannot reach the server",
  );
  await page.reload();
  await page
    .getByRole("button", { name: /^Retry (saved repair|cancellation)$/ })
    .click();
  await expect.poll(() => canceledAttempts.length).toBe(3);
  expect(canceledAttempts[2]).toEqual({
    ...canceledAttempts[0],
    cancel: "true",
  });
  expect(canceledAttempts[1]).toEqual(canceledAttempts[2]);
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Saved repair was safely canceled" }),
  ).toBeVisible();
  await page.unroute(`**/api/v1/budgets/${budget}/recovery`);
  const delayed = await fetch(
    server.url.replace("actualbudget.test", "127.0.0.1") +
      `/api/v1/budgets/${budget}/recovery`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": canceledAttempts[0].key,
        "X-Actual-Budget-Epoch": canceledAttempts[0].epoch,
      },
      body: canceledAttempts[0].body,
    },
  );
  expect(delayed.status).toBe(409);
  expect((await delayed.json()).error.code).toBe("operation_canceled");
  await page
    .getByRole("button", { name: "Inspect budget", exact: true })
    .click();
  await expect(page.getByTestId("recovery-account")).toContainText("99.99");
  await page.getByLabel("Confirm budget name").fill("Recovery browser");
  const writes = [];
  let lose = true;
  await page.route(`**/api/v1/budgets/${budget}/recovery`, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    writes.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
      epoch: route.request().headers()["x-actual-budget-epoch"],
    });
    if (lose) {
      lose = false;
      const url = new URL(route.request().url()),
        host = url.host;
      url.hostname = "127.0.0.1";
      await route.fetch({
        url: url.href,
        headers: { ...route.request().headers(), host },
      });
      await route.abort();
    } else await route.continue();
  });
  await page
    .getByRole("button", { name: "Apply reviewed repair", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved repair", exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved repair", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Reset downloaded data", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm cache reset", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved repair", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Retry saved repair", exact: true })
    .click();
  await expect(
    page.getByRole("status").filter({ hasText: "Repair confirmed" }),
  ).toBeVisible();
  expect(writes).toHaveLength(2);
  expect(writes[1]).toEqual(writes[0]);
  await expect(
    page.getByRole("link", { name: "Download pre-repair backup", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.locator("td.amount").first()).toHaveText("10.00");
  // Explicit HTTP fault injection: a stale cursor is rejected, then bootstrap
  // must recover real current references without changing pending-work identity.
  await expect(
    page.getByRole("button", { name: "Refresh transactions", exact: true }),
  ).toBeEnabled();
  const recoveredCategory = randomUUID();
  await write(`/budgets/${budget}/categories`, {
    id: recoveredCategory,
    name: "Recovered category",
  });
  let cursorRejected = false;
  await page.route("**/changes?*", async (route) => {
    if (!cursorRejected) {
      cursorRejected = true;
      await route.fulfill({
        status: 409,
        contentType: "application/json",
        body: JSON.stringify({
          error: { code: "cursor_invalid", message: "Injected stale cursor" },
        }),
      });
    } else await route.continue();
  });
  await page
    .getByRole("button", { name: "Recovery tools", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reconnect and refresh", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Accounts", exact: true }),
  ).toBeVisible();
  await expect.poll(() => cursorRejected).toBe(true);
  await expect(
    page.getByRole("button", { name: "Refresh transactions", exact: true }),
  ).toBeEnabled();

  await page.getByRole("button", { name: "Spending", exact: true }).click();
  await context.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1.23");
  await page
    .getByLabel("Category", { exact: true })
    .selectOption(recoveredCategory);
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Purchase survives cache recovery");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Purchase survives cache recovery",
  );
  await page
    .getByRole("button", { name: "Recovery tools", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reset downloaded data", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm cache reset", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Review pending expenses", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Purchase survives cache recovery",
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Review pending expenses", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Purchase survives cache recovery",
  );
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await mkdir("artifacts", { recursive: true });
  await page.screenshot({
    path: "artifacts/recovery-pending-mobile.png",
    fullPage: true,
  });
  await context.unroute("**/api/v1/**");
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
  const data = await (
    await fetch(
      server.url.replace("actualbudget.test", "127.0.0.1") +
        `/api/v1/budgets/${budget}/transactions`,
    )
  ).json();
  expect(
    data.data.filter((r) => r.notes === "Purchase survives cache recovery"),
  ).toHaveLength(1);
  // A legacy lifecycle request shares the cache store but is NOT downloaded data.
  // Its original body/key must remain recoverable after this new reset tool.
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: /Recovery browser.*Open/ }).click();
  await page
    .getByRole("button", { name: "Manage budget", exact: true })
    .click();
  const renamed = [];
  let loseRename = true;
  await page.route(`**/api/v1/budgets/${budget}`, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    renamed.push({
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
    });
    if (loseRename) {
      loseRename = false;
      const url = new URL(route.request().url()),
        host = url.host;
      url.hostname = "127.0.0.1";
      await route.fetch({
        url: url.href,
        headers: { ...route.request().headers(), host },
      });
      await route.abort();
    } else await route.continue();
  });
  await page
    .getByLabel("Budget name", { exact: true })
    .fill("Recovered household");
  await page
    .getByRole("button", { name: "Save budget name", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved operation", exact: true }),
  ).toBeVisible();
  await page.reload();
  await page
    .getByRole("button", { name: "Recovery tools", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reset downloaded data", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm cache reset", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Manage budget", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Retry saved operation", exact: true })
    .click();
  await expect(page.getByLabel("Budget name", { exact: true })).toHaveValue(
    "Recovered household",
  );
  await expect.poll(() => renamed.length).toBe(2);
  await expect(
    page.getByRole("button", { name: "Save budget name", exact: true }),
  ).toBeEnabled();
  expect(renamed[1]).toEqual(renamed[0]);
  expect(errors).toEqual([]);
  console.log(
    "PASS recovery preview/backup, lost receipt retry, cache reset and restarted offline outbox exactly once",
  );
} finally {
  await browser?.close();
  await server?.stop();
  await rm(original, { recursive: true, force: true });
  await rm(copied, { recursive: true, force: true });
}

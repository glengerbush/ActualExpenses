import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, readFile, readdir, rm, mkdir } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { startServer } from "./server.mjs";
const data = await mkdtemp(`${tmpdir()}/manual-complete-`);
let server, browser;
async function call(path, body, method = "POST", headers = {}) {
  const response = await fetch(
    server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1" + path,
    {
      method: body === undefined ? "GET" : method,
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
        ...headers,
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    },
  );
  expect(response.ok, await response.clone().text()).toBe(true);
  return (await response.json()).data;
}
try {
  server = await startServer({ dataDirectory: data });
  const budget = randomUUID(),
    account = randomUUID(),
    id = randomUUID();
  await call("/budgets", { id: budget, name: "Completed expenses" });
  await call(`/budgets/${budget}/accounts`, {
    id: account,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const root = `/budgets/${budget}`;
  await call(root + "/transactions", {
    id,
    account_id: account,
    date: "2026-01-02",
    amount: -2500,
    notes: "Imported purchase",
    cleared: false,
  });
  expect((await call(root + "/transactions/" + id)).cleared).toBe(true);
  await server.stop();
  // Rebuild genuine schema27 before restoring legacy status values.
  execFileSync("python3", [
    "tests/historical-schema.py",
    data + "/actualbudget.sqlite3",
    "27",
  ]);
  execFileSync("python3", [
    "-c",
    `import sqlite3,sys
c=sqlite3.connect(sys.argv[1])
c.execute('UPDATE transactions SET cleared=0,reconciled=1')
c.execute('UPDATE ledger_totals SET cleared=0,uncleared=total')
c.commit()`,
    data + "/actualbudget.sqlite3",
  ]);
  server = await startServer({ dataDirectory: data });
  const migrated = await call(root + "/transactions/" + id);
  expect(migrated.amount).toBe(-2500);
  expect(migrated.cleared).toBe(true);
  expect(migrated.reconciled).toBe(false);
  expect(
    (await readdir(data + "/backups")).some((name) =>
      name.startsWith("before-v27-"),
    ),
  ).toBe(true);
  const edited = await call(
    root + "/transactions/" + id,
    { amount: -2600, cleared: false },
    "PATCH",
  );
  expect(edited.amount).toBe(-2600);
  expect(edited.notes).toBe("");
  expect(edited.cleared).toBe(true);
  const statusQuery = {
    hide_cleared: true,
    hide_reconciled: true,
    filter: {
      op: "and",
      conditions: [{ field: "cleared", op: "is", value: false }],
    },
  };
  expect(
    (
      await call(
        root +
          "/transactions?query=" +
          encodeURIComponent(JSON.stringify(statusQuery)),
      )
    ).map((row) => row.id),
  ).toContain(id);
  const retired = await fetch(
    server.url.replace("actualbudget.test", "127.0.0.1") +
      `/api/v1${root}/accounts/${account}/reconciliation`,
  );
  expect(retired.status).toBe(404);
  const settings = await call(root + "/register-settings");
  await call(
    root + "/register-settings",
    {
      revision: settings.revision,
      settings: {
        views: {
          all: {
            query: {
              search: "",
              sort: "cleared",
              direction: "desc",
              ...statusQuery,
            },
            columns: ["date", "notes", "amount", "status"],
            as_of: "",
          },
        },
        saved_filters: [],
      },
    },
    "PATCH",
    { "X-Actual-Budget-Epoch": settings.epoch },
  );
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByRole("button", { name: /Completed expenses.*Open/ }).click();
  await expect(page.locator(".register-table")).toContainText("26.00");
  await expect(
    page.getByRole("columnheader", { name: "Status", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Review pending expenses", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByTestId("register-cleared")).toHaveCount(0);
  await expect(page.getByTestId("register-uncleared")).toHaveCount(0);
  await page.locator('summary[aria-label="Filters and columns"]').click();
  await expect(
    page.getByLabel(/Hide cleared|Hide reconciled|Show cleared/),
  ).toHaveCount(0);
  await page.locator('summary[aria-label="Filters and columns"]').click();
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await expect(page.getByLabel("Cleared", { exact: true })).toHaveCount(0);
  await page.getByLabel("Amount", { exact: true }).fill("3.21");
  await expect(page.getByLabel("Notes", { exact: true })).toHaveCount(0);
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".register-table")).toContainText("3.21");
  await page.route("**/api/v1/**", (route) =>
    route.abort("internetdisconnected"),
  );
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("4.56");

  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.locator(".register-table")).toContainText("4.56");
  await expect(
    page.getByRole("heading", { name: /Pending expenses/ }),
  ).toHaveCount(0);
  await page.reload();
  await expect(page.locator(".register-table")).toContainText("4.56");
  await page.unroute("**/api/v1/**");
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect
    .poll(
      async () =>
        (await call(root + "/transactions")).filter(
          (row) => row.amount === -456,
        ).length,
    )
    .toBe(1);
  await expect(
    page.locator(".register-table tbody tr").filter({ hasText: "4.56" }),
  ).toHaveCount(1);
  await mkdir("test-results/completed-expenses", { recursive: true });
  await page.screenshot({
    path: "test-results/completed-expenses/desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/completed-expenses/mobile.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS existing-data migration, legacy writes, editable imports, retired status filters/routes, immediate entry, offline reload and duplicate-free quiet sync",
  );
} finally {
  await browser?.close();
  await server?.stop();
  await rm(data, { recursive: true, force: true });
}

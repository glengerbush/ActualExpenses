import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
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
  const page = await browser.newPage({
    viewport: { width: 1440, height: 960 },
  });
  const errors = [],
    creations = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith("/api/v1/budgets"))
      creations.push(r.url());
  });
  await page.goto(server.url);
  await expect(
    page.getByRole("heading", { name: "Transactions", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  for (const name of [
    "New budget",
    "Create budget",
    "Budgets",
    "All budgets",
    "Manage budget",
  ])
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(
      0,
    );
  await expect(page.getByLabel("Budget name", { exact: true })).toHaveCount(0);
  expect(creations).toEqual([]);
  const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  const current = (await (await fetch(api + "/budget")).json()).data;
  const simultaneous = await Promise.all(
    Array.from(
      { length: 12 },
      async () => (await (await fetch(api + "/budget")).json()).data.id,
    ),
  );
  expect(new Set(simultaneous)).toEqual(new Set([current.id]));
  expect((await (await fetch(api + "/budgets")).json()).data).toHaveLength(1);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Budget", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await page.goBack();
  await expect(
    page.getByRole("button", { name: "Budget", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await page.goto(server.url + `/?budget=${randomUUID()}&view=transactions`);
  await expect(
    page.getByRole("heading", { name: "Transactions", exact: true }),
  ).toBeVisible();
  await expect
    .poll(() => new URL(page.url()).searchParams.get("budget"))
    .toBe(current.id);
  await mkdir("test-results/single-budget", { recursive: true });
  await page.screenshot({
    path: "test-results/single-budget/desktop.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Backups", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Create backup", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await page
    .getByRole("button", { name: "Transaction file", exact: true })
    .click();
  await expect(
    page.getByLabel("Transaction file", { exact: true }),
  ).toBeEnabled();
  await expect(page.getByLabel("A new budget", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Transactions", exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/single-budget/mobile.png",
    fullPage: true,
  });
  await page.route("**/api/v1/**", (route) => route.abort());
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Transactions", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Add transaction", exact: true }),
  ).toBeEnabled();
  expect(errors).toEqual([]);
  console.log(
    "PASS fresh singleton startup, concurrent discovery, no create/chooser UI, remembered views/history, stale-ID links, Settings imports/backups, offline cached reopening",
  );
} finally {
  await browser?.close();
  await server.stop();
}

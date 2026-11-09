import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, method = "POST") {
  const r = await fetch(api + "/api/v1" + path, {
    method,
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
  const budget = randomUUID(),
    account = randomUUID();
  await write("/budgets", { id: budget, name: "Account organization" });
  const root = `/budgets/${budget}`;
  await write(root + "/accounts", {
    id: account,
    name: "Daily account",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const amount of [10000, -10000])
    await write(root + "/transactions", {
      id: randomUUID(),
      account_id: account,
      amount,
      date: "2026-01-01",
    });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1280, height: 900 },
  });
  await page.goto(server.url);
  await page
    .getByRole("button", { name: "Account organization Tracking Open" })
    .click();
  await page
    .getByRole("button", { name: "Organize accounts", exact: true })
    .click();
  await page.getByLabel("Group name", { exact: true }).fill("Everyday");
  await page.getByRole("button", { name: "Create group", exact: true }).click();
  await page.getByLabel("Account name", { exact: true }).fill("Daily spending");
  await page
    .getByLabel("Account notes", { exact: true })
    .fill("Keep history\nFor travel");
  await page
    .getByLabel("Account group", { exact: true })
    .selectOption({ label: "Everyday" });
  await page.getByRole("button", { name: "Save account", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Account saved." }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Close account", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Reopen account", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await expect(
    page
      .getByRole("navigation", { name: "Accounts" })
      .getByRole("button", { name: /Daily spending/ }),
  ).toHaveCount(0);
  await page.getByLabel("Show closed accounts", { exact: true }).check();
  await page
    .getByRole("navigation", { name: "Accounts" })
    .getByRole("button", { name: /Daily spending/ })
    .click();
  await expect(
    page.getByRole("heading", { name: "Daily spending", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Closed account. History is retained.", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Organize accounts", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Reopen account", exact: true })
    .click();
  await page
    .getByLabel("Account notes", { exact: true })
    .fill("Lost response retained");
  let dropped = false;
  await page.route("**/api/v1/budgets/*/accounts/*", async (route) => {
    if (route.request().method() === "PATCH" && !dropped) {
      dropped = true;
      const target = new URL(route.request().url());
      target.hostname = "127.0.0.1";
      const headers = { ...route.request().headers() };
      delete headers.origin;
      delete headers.referer;
      await route.fetch({ url: target.href, headers });
      await route.abort("failed");
    } else await route.continue();
  });
  await page.getByRole("button", { name: "Save account", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved change", exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved change", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Retry saved change", exact: true })
    .click();
  await expect(page.getByLabel("Account notes", { exact: true })).toHaveValue(
    "Lost response retained",
  );
  const point = await write(root + "/backups", {
    label: "Before account draft",
  });
  await page
    .getByLabel("Account notes", { exact: true })
    .fill("Draft before restored budget");
  await write(root + `/backups/${point.id}/restore`, {
    confirm_name: "Account organization",
  });
  await page.getByRole("button", { name: "Save account", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Review current accounts", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Review current accounts", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Keep fields for a new change", exact: true })
    .click();
  await page.getByRole("button", { name: "Save account", exact: true }).click();
  await expect(page.getByLabel("Account notes", { exact: true })).toHaveValue(
    "Draft before restored budget",
  );
  await mkdir(new URL("../test-results/", import.meta.url), {
    recursive: true,
  });
  await page.screenshot({
    path: fileURLToPath(
      new URL("../test-results/accounts-desktop.png", import.meta.url),
    ),
    fullPage: true,
  });
  await page.setViewportSize({ width: 360, height: 800 });
  await mkdir(new URL("../test-results/", import.meta.url), {
    recursive: true,
  });
  await page.screenshot({
    path: fileURLToPath(
      new URL("../test-results/accounts-mobile.png", import.meta.url),
    ),
    fullPage: true,
  });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(360);
  await page
    .getByLabel("Type account name to delete", { exact: true })
    .fill("Daily spending");
  await page
    .getByRole("button", {
      name: "Delete account and transactions",
      exact: true,
    })
    .click();
  await expect(
    page.getByText("No accounts to organize.", { exact: true }),
  ).toBeVisible();
  const response = await fetch(api + "/api/v1" + root + "/transactions");
  expect((await response.json()).data).toHaveLength(0);
  await page
    .getByLabel("Group to organize", { exact: true })
    .selectOption({ label: "Everyday" });
  await page.getByLabel("Group name", { exact: true }).fill("Renamed group");
  await page.getByLabel("Group name", { exact: true }).press("Enter");
  await expect(
    page.getByRole("status").filter({ hasText: "Group saved." }),
  ).toBeVisible();
  await page
    .getByLabel("Group to organize", { exact: true })
    .selectOption({ label: "Renamed group" });
  await page.getByRole("button", { name: "Move group", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Group moved." }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Delete group", exact: true }).click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Group deleted. Accounts kept." }),
  ).toBeVisible();
  await page.addStyleTag({ content: "html {font-size:200%}" });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(360);
  console.log(
    "PASS plain HTTP account organization, closed history, reload-safe lost response, mobile deletion",
  );
} finally {
  await browser?.close();
  await server.stop();
}

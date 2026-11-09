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
  const shared = await browser.newContext();
  const sharedBudget = randomUUID(),
    sharedAccount = randomUUID();
  const sharedRoot = `/budgets/${sharedBudget}`;
  await write("/budgets", {
    id: sharedBudget,
    name: "Shared account recovery",
  });
  await write(sharedRoot + "/accounts", {
    id: sharedAccount,
    name: "Shared cash",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const first = await shared.newPage();
  await first.goto(server.url);
  await first
    .getByRole("button", { name: "Shared account recovery Tracking Open" })
    .click();
  await first
    .getByRole("button", { name: "Organize accounts", exact: true })
    .click();
  await expect(first.getByLabel("Account name", { exact: true })).toBeEnabled();
  const second = await shared.newPage();
  await second.goto(server.url);
  await expect(
    second.getByRole("heading", { name: "Organize accounts", exact: true }),
  ).toBeVisible();
  await expect(
    second.getByLabel("Account name", { exact: true }),
  ).toBeEnabled();
  const attempts = [];
  let loseNextResponse = true;
  await first.route("**/api/v1/budgets/*/accounts/*", async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    const request = route.request();
    attempts.push({
      body: request.postData(),
      key: request.headers()["idempotency-key"],
      clock: request.headers()["x-actual-edit-clock"],
      epoch: request.headers()["x-actual-budget-epoch"],
    });
    const target = new URL(request.url()),
      headers = { ...request.headers(), host: new URL(request.url()).host };
    target.hostname = "127.0.0.1";
    const response = await route.fetch({ url: target.href, headers });
    if (response.ok() && loseNextResponse) {
      loseNextResponse = false;
      await route.abort("connectionreset");
    } else await route.fulfill({ response });
  });
  await first
    .getByLabel("Account notes", { exact: true })
    .fill("Keep the other tab intent");
  await first
    .getByRole("button", { name: "Save account", exact: true })
    .click();
  await expect(
    first.getByRole("button", { name: "Retry saved change", exact: true }),
  ).toBeVisible();
  const journalKey = `actualBudget.accounts.v1.${sharedBudget}`;
  const journal = await first.evaluate(
    (key) => localStorage.getItem(key),
    journalKey,
  );
  expect(JSON.parse(journal).pending).toBeTruthy();
  await second.getByRole("button", { name: "Done", exact: true }).click();
  expect(
    await second.evaluate((key) => localStorage.getItem(key), journalKey),
  ).toBe(journal);
  await expect(
    second
      .getByRole("alert")
      .filter({ hasText: "Another tab has a pending account change" }),
  ).toBeVisible();
  await shared.setOffline(true);
  expect(
    await first.evaluate((key) => localStorage.getItem(key), journalKey),
  ).toBe(journal);
  await shared.setOffline(false);
  await first.reload();
  await first
    .getByRole("button", { name: "Retry saved change", exact: true })
    .click();
  await expect(
    first.getByRole("status").filter({ hasText: "Account saved." }),
  ).toBeVisible();
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  const sharedPoint = await write(sharedRoot + "/backups", {
    label: "Shared review",
  });
  await first
    .getByLabel("Account notes", { exact: true })
    .fill("Reviewed stale request");
  await write(sharedRoot + `/backups/${sharedPoint.id}/restore`, {
    confirm_name: "Shared account recovery",
  });
  await first
    .getByRole("button", { name: "Save account", exact: true })
    .click();
  await first
    .getByRole("button", { name: "Review current accounts", exact: true })
    .click();
  await expect(
    first.getByRole("button", { name: "Discard saved change", exact: true }),
  ).toBeVisible();
  await second.reload();
  await second
    .getByRole("button", { name: "Retry saved change", exact: true })
    .click();
  await second
    .getByRole("button", { name: "Review current accounts", exact: true })
    .click();
  await expect(
    second.getByRole("button", { name: "Discard saved change", exact: true }),
  ).toBeVisible();
  await first
    .getByRole("button", { name: "Keep fields for a new change", exact: true })
    .click();
  loseNextResponse = true;
  await first
    .getByRole("button", { name: "Save account", exact: true })
    .click();
  await expect(
    first.getByRole("button", { name: "Retry saved change", exact: true }),
  ).toBeVisible();
  const newerJournal = await first.evaluate(
    (key) => localStorage.getItem(key),
    journalKey,
  );
  expect(JSON.parse(newerJournal).pending.key).not.toBe(
    JSON.parse(journal).pending.key,
  );
  for (const name of ["Discard saved change", "Keep fields for a new change"]) {
    await second.getByRole("button", { name, exact: true }).click();
    await expect(
      second
        .getByRole("alert")
        .filter({ hasText: "Another tab has a pending account change" }),
    ).toBeVisible();
    expect(
      await second.evaluate((key) => localStorage.getItem(key), journalKey),
    ).toBe(newerJournal);
  }
  const latestAttempt = attempts.at(-1);
  await first.reload();
  await first
    .getByRole("button", { name: "Retry saved change", exact: true })
    .click();
  await expect(
    first.getByRole("status").filter({ hasText: "Account saved." }),
  ).toBeVisible();
  expect(attempts.at(-1)).toEqual(latestAttempt);
  await shared.close();
  console.log(
    "PASS another tab cannot discard a saved account request; reload replays the original identity",
  );

  const offline = await browser.newContext();
  const offlineBudget = randomUUID(),
    empty = randomUUID(),
    closed = randomUUID(),
    group = randomUUID();
  const offlineRoot = `/budgets/${offlineBudget}`;
  await write("/budgets", {
    id: offlineBudget,
    name: "Offline account actions",
  });
  for (const [id, name] of [
    [empty, "Empty cash"],
    [closed, "Closed history"],
  ])
    await write(offlineRoot + "/accounts", {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
  for (const amount of [1234, -1234])
    await write(offlineRoot + "/transactions", {
      id: randomUUID(),
      account_id: closed,
      amount,
      date: "2026-01-01",
    });
  await write(offlineRoot + `/accounts/${closed}/close`, {});
  await write(offlineRoot + "/account-groups", {
    id: group,
    name: "Existing group",
  });
  const offlinePage = await offline.newPage();
  await offlinePage.goto(server.url);
  await offlinePage
    .getByRole("button", { name: "Offline account actions Tracking Open" })
    .click();
  await offlinePage
    .getByRole("button", { name: "Organize accounts", exact: true })
    .click();
  await offlinePage
    .getByLabel("Account to organize", { exact: true })
    .selectOption(empty);
  await offlinePage
    .getByLabel("Account notes", { exact: true })
    .fill("Unsent edit");
  await offlinePage
    .getByLabel("Type account name to delete", { exact: true })
    .fill("Empty cash");
  await offlinePage
    .getByLabel("Group to organize", { exact: true })
    .selectOption(group);
  await offlinePage
    .getByLabel("Group name", { exact: true })
    .fill("Unsent group edit");
  const writes = [];
  offlinePage.on("request", (request) => {
    if (
      request.url().includes(`/api/v1${offlineRoot}/`) &&
      request.method() !== "GET"
    )
      writes.push(request.url());
  });
  const offlineKey = `actualBudget.accounts.v1.${offlineBudget}`;
  const unsent = await offlinePage.evaluate(
    (key) => localStorage.getItem(key),
    offlineKey,
  );
  await offline.setOffline(true);
  for (const name of [
    "Save account",
    "Move account",
    "Close account",
    "Delete account and transactions",
    "Save group",
    "Move group",
    "Delete group",
  ])
    await expect(
      offlinePage.getByRole("button", { name, exact: true }),
    ).toBeDisabled();
  // A submit already dispatched as connection state changes must also stop at
  // the public form handler, before creating a durable request or edit clock.
  await offlinePage
    .locator('section[aria-labelledby="organize-title"] form')
    .evaluateAll((forms) => forms.forEach((form) => form.requestSubmit()));
  await expect(
    offlinePage
      .getByRole("alert")
      .filter({ hasText: "Reconnect before changing accounts or groups." }),
  ).toBeVisible();
  expect(
    await offlinePage.evaluate((key) => localStorage.getItem(key), offlineKey),
  ).toBe(unsent);
  await offlinePage.setViewportSize({ width: 360, height: 800 });
  await offlinePage.screenshot({
    path: fileURLToPath(
      new URL("../test-results/accounts-offline-mobile.png", import.meta.url),
    ),
    fullPage: true,
  });
  expect(
    await offlinePage.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(360);
  await offlinePage
    .getByLabel("Account to organize", { exact: true })
    .selectOption(closed);
  await expect(
    offlinePage.getByRole("button", { name: "Reopen account", exact: true }),
  ).toBeDisabled();
  await offlinePage
    .getByLabel("Group to organize", { exact: true })
    .selectOption("");
  await expect(
    offlinePage.getByRole("button", { name: "Create group", exact: true }),
  ).toBeDisabled();
  expect(
    JSON.parse(
      await offlinePage.evaluate(
        (key) => localStorage.getItem(key),
        offlineKey,
      ),
    ).pending,
  ).toBeNull();
  expect(writes).toEqual([]);
  await offline.setOffline(false);
  await expect(
    offlinePage.getByRole("button", { name: "Reopen account", exact: true }),
  ).toBeEnabled();
  expect(writes).toEqual([]);
  const unchangedAccounts = (
    await (await fetch(api + "/api/v1" + offlineRoot + "/accounts")).json()
  ).data;
  expect(unchangedAccounts.find((a) => a.id === empty)).toMatchObject({
    name: "Empty cash",
    notes: "",
    closed: false,
  });
  expect(unchangedAccounts.find((a) => a.id === closed)).toMatchObject({
    closed: true,
  });
  const unchangedGroups = (
    await (
      await fetch(api + "/api/v1" + offlineRoot + "/account-groups")
    ).json()
  ).data;
  expect(unchangedGroups).toHaveLength(1);
  expect(unchangedGroups[0].name).toBe("Existing group");
  await offline.close();
  console.log(
    "PASS all new account/group mutations blocked offline without writes or pending requests; saved requests survive reconnect",
  );
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
async function call(path, body, method = body ? "POST" : "GET") {
  const r = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const v = await r.json();
  expect(r.ok, JSON.stringify(v)).toBe(true);
  return v;
}
let browser;
try {
  const b = randomUUID(),
    a = randomUUID(),
    z = randomUUID(),
    t = randomUUID(),
    p = randomUUID(),
    root = `/budgets/${b}`;
  await call("/budgets", { id: b, name: "Paired protection" });
  for (const [id, name] of [
    [a, "Cash"],
    [z, "Savings"],
  ])
    await call(root + "/accounts", {
      id,
      name,
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
  await call(root + "/transfers", {
    id: t,
    counterpart_id: p,
    account_id: a,
    to_account_id: z,
    amount: -2500,
    date: "2026-01-01",
    cleared: true,
    notes: "Settled transfer",
  });
  await call(root + `/transactions/${p}`, { cleared: true }, "PATCH");
  async function lock(account, balance) {
    const path = root + `/accounts/${account}/reconciliation`;
    const view = await call(path + `?statement_balance=${balance}`);
    await call(path, {
      action: "lock",
      statement_balance: balance,
      snapshot: view.data.snapshot,
      date: "2026-01-31",
    });
  }
  await lock(a, -2500);
  await lock(z, 2500);
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
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page
    .locator(".budget-list button")
    .filter({ hasText: "Paired protection" })
    .click();
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Settled transfer" })
    .filter({ has: page.getByRole("button", { name: /^Edit -/ }) })
    .getByRole("button", { name: /^Edit / })
    .click();
  const confirmation = page.getByLabel(
    "Confirm editing reconciled entries in this transfer",
  );
  await expect(confirmation).toBeVisible();
  await page.getByLabel("Amount", { exact: true }).fill("-27.00");
  await page
    .getByRole("button", { name: "Save transfer", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("Confirm");
  await confirmation.check();
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/paired-protection-desktop.png",
    fullPage: true,
  });
  const requests = [];
  let lose = true;
  await page.route("**/api/v1" + root + `/transactions/${t}`, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    const r = route.request();
    requests.push({
      body: r.postData(),
      key: r.headers()["idempotency-key"],
      clock: r.headers()["x-actual-edit-clock"],
      epoch: r.headers()["x-actual-budget-epoch"],
      owner: r.headers()["x-actual-history-owner"],
    });
    const target = new URL(r.url());
    const headers = { ...r.headers(), host: target.host };
    target.hostname = "127.0.0.1";
    const response = await route.fetch({ url: target.href, headers });
    if (lose) {
      lose = false;
      await route.abort();
    } else await route.fulfill({ response });
  });
  await page
    .getByRole("button", { name: "Save transfer", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  const peerProof = await call(root + `/transactions/${p}/splits`);
  await call(root + `/transactions/${p}/unlock`, {
    reconciliation_token: peerProof.reconciliation_token,
  });
  await lock(z, 2700);
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved transfer" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Retry saved transfer" }).click();
  await expect(
    page.getByRole("heading", { name: "Edit transfer", exact: true }),
  ).not.toBeVisible();
  expect(requests).toHaveLength(2);
  expect(requests[0].owner).toMatch(/^[0-9a-f-]{36}$/);
  expect(requests[1]).toEqual(requests[0]);
  const ownedHistory = await fetch(api + "/api/v1" + root + "/history", {
    headers: { "X-Actual-History-Owner": requests[0].owner },
  }).then((r) => r.json());
  expect(ownedHistory.data.undo).not.toBeNull();
  expect(JSON.parse(requests[0].body).transfer_protection_token).toMatch(
    /^[0-9a-f]{64}$/,
  );
  expect((await call(root + `/transactions/${t}`)).data).toMatchObject({
    amount: -2700,
    reconciled: true,
  });
  expect((await call(root + `/transactions/${p}`)).data).toMatchObject({
    amount: 2700,
    reconciled: true,
  });
  await page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Settled transfer" })
    .filter({ has: page.getByRole("button", { name: /^Edit -/ }) })
    .getByRole("button", { name: /^Edit / })
    .click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page
    .getByLabel("Confirm editing reconciled entries in this transfer")
    .focus();
  await expect(
    page.getByLabel("Confirm editing reconciled entries in this transfer"),
  ).toBeFocused();
  await page.screenshot({
    path: "test-results/paired-protection-mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  expect(errors).toEqual([]);
  await page.context().setOffline(true);
  await expect(
    page.getByRole("button", { name: "Save transfer", exact: true }),
  ).toBeDisabled();
  await page.context().setOffline(false);
  await page.getByRole("button", { name: "Close transfer form" }).click();
  const parent = randomUUID(),
    child = randomUUID(),
    sibling = randomUUID(),
    other = randomUUID();
  await call(root + "/transactions", {
    id: parent,
    account_id: a,
    amount: -1000,
    date: "2026-01-01",
    cleared: true,
    notes: "Settled split transfer",
  });
  const splitPath = root + `/transactions/${parent}/splits`;
  const v = await call(splitPath);
  await call(splitPath, {
    snapshot: v.snapshot,
    children: [
      {
        id: child,
        amount: -600,
        notes: "Transfer allocation",
        transfer_account_id: z,
        counterpart_id: other,
      },
      { id: sibling, amount: -400, notes: "Ordinary allocation" },
    ],
  });
  await call(root + `/transactions/${other}`, { cleared: true }, "PATCH");
  await lock(a, -3700);
  await lock(z, 3300);
  await page.setViewportSize({ width: 1280, height: 1000 });
  await page.reload();
  await page.getByRole("button", { name: "Edit splits", exact: true }).click();
  await page.getByLabel("Split 1 amount", { exact: true }).fill("-7.00");
  await page.getByLabel("Split 2 amount", { exact: true }).fill("-3.00");
  await page
    .getByLabel("Confirm editing this reconciled split", { exact: true })
    .check();
  await page.screenshot({
    path: "test-results/paired-protection-split.png",
    fullPage: true,
  });
  const splitRequests = [];
  let loseSplit = true;
  await page.route("**/api/v1" + splitPath, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    const r = route.request();
    splitRequests.push({
      body: r.postData(),
      key: r.headers()["idempotency-key"],
      clock: r.headers()["x-actual-edit-clock"],
      epoch: r.headers()["x-actual-budget-epoch"],
      owner: r.headers()["x-actual-history-owner"],
    });
    const u = new URL(r.url());
    const headers = { ...r.headers(), host: u.host };
    u.hostname = "127.0.0.1";
    const response = await route.fetch({ url: u.href, headers });
    if (loseSplit) {
      loseSplit = false;
      await route.abort();
    } else await route.fulfill({ response });
  });
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved split", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).not.toBeVisible();
  expect(splitRequests).toHaveLength(2);
  expect(splitRequests[1]).toEqual(splitRequests[0]);
  expect(JSON.parse(splitRequests[0].body).transfer_protection_token).toMatch(
    /^[0-9a-f]{64}$/,
  );
  expect((await call(splitPath)).data).toMatchObject({
    amount: -1000,
    reconciled: true,
  });
  expect((await call(root + `/transactions/${other}`)).data).toMatchObject({
    amount: 700,
    reconciled: true,
  });
  expect(errors).toEqual([]);
  console.log("paired protection browser passed");
} finally {
  await browser?.close();
  await server.stop();
}

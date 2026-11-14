import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
let browser;
async function call(path, body) {
  const r = await fetch(api + "/api/v1" + path, {
    method: body ? "POST" : "GET",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
async function forward(route) {
  const url = new URL(route.request().url());
  const headers = { ...route.request().headers(), host: url.host };
  url.hostname = "127.0.0.1";
  return route.fetch({ url: url.href, headers });
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
  await mkdir(new URL("../test-results/", import.meta.url), {
    recursive: true,
  });
  for (const kind of ["category", "category-group"]) {
    const budget = randomUUID(),
      account = randomUUID(),
      name = `Clock recovery ${kind}`;
    await call("/budgets", { id: budget, name });
    const base = `/budgets/${budget}`;
    await call(base + "/accounts", {
      id: account,
      name: "Checking",
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
    const journalKey = `actualBudget.categories.v1.${budget}`;
    const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    });
    await context.addInitScript(() => {
      const now = Date.now.bind(Date);
      Date.now = () =>
        now() + Number(sessionStorage.getItem("test-clock-offset") || "0");
    });
    const page = await context.newPage();
    await page.goto(server.url);
    await page
      .getByRole("button", { name: `${name} Open`, exact: true })
      .click();
    await page
      .getByRole("button", {
        name: "Budget",
        exact: true,
      })
      .click();
    await page
      .getByRole("button", {
        name: kind === "category" ? "Add category to Ungrouped" : "Add group",
        exact: true,
      })
      .first()
      .click();
    const field = () => page.getByLabel("Name", { exact: true });
    const save = () => page.getByRole("button", { name: "Save", exact: true });
    const retry = (p) =>
      p.getByRole("button", { name: "Retry unchanged request", exact: true });
    const cancel = () =>
      page.getByRole("button", {
        name: "Cancel pending request and keep fields",
        exact: true,
      });
    await field().fill(`Retained ${kind}`);
    await page.evaluate(() =>
      sessionStorage.setItem("test-clock-offset", "86400000"),
    );
    const attempts = [];
    let dropCancel = true;
    const resource = kind === "category" ? "categories" : "category-groups";
    await context.route(`**/api/v1${base}/${resource}`, async (route) => {
      const request = route.request();
      if (request.method() === "GET") return route.continue();
      attempts.push({
        method: request.method(),
        url: request.url(),
        body: request.postData(),
        key: request.headers()["idempotency-key"],
        clock: request.headers()["x-actual-edit-clock"],
        epoch: request.headers()["x-actual-budget-epoch"],
        cancel: request.headers()["x-actual-cancel-if-uncommitted"] === "true",
      });
      if (attempts.at(-1).cancel && dropCancel) {
        dropCancel = false;
        // Alternate lost request and lost acknowledgement, always using real API responses.
        if (kind === "category") await forward(route);
        return route.abort();
      }
      return route.continue();
    });
    await save().click();
    await expect(
      page
        .getByRole("region", { name: "Category and group editor" })
        .getByRole("alert"),
    ).toContainText("five minutes ahead");
    await expect(retry(page)).toBeVisible();
    const original = JSON.parse(
      await page.evaluate((key) => localStorage.getItem(key), journalKey),
    ).pending;
    expect(original).toBeTruthy();
    await page.evaluate(() => sessionStorage.setItem("test-clock-offset", "0"));
    const stale = await context.newPage();
    await stale.goto(server.url);
    await stale.getByRole("button", { name: "Budget", exact: true }).click();
    await expect(retry(stale)).toBeVisible();
    await page.screenshot({
      path: new URL(
        `../test-results/clock-${kind}-desktop.png`,
        import.meta.url,
      ).pathname,
      fullPage: true,
    });
    await page.setViewportSize({ width: 360, height: 800 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: new URL(`../test-results/clock-${kind}-mobile.png`, import.meta.url)
        .pathname,
      fullPage: true,
    });
    let releaseInfo;
    const infoHeld = new Promise((resolve) => {
      releaseInfo = resolve;
    });
    let infoStarted;
    const infoRequested = new Promise((resolve) => {
      infoStarted = resolve;
    });
    await page.route("**/api/v1/info", async (route) => {
      infoStarted();
      await infoHeld;
      await route.continue();
    });
    await cancel().click();
    await infoRequested;
    await expect(retry(page)).toBeDisabled();
    await expect(cancel()).toBeDisabled();
    releaseInfo();
    await expect(
      page
        .getByRole("region", { name: "Category and group editor" })
        .getByRole("alert"),
    ).toContainText("Cannot reach");
    await page.unroute("**/api/v1/info");
    expect(
      JSON.parse(
        await page.evaluate((key) => localStorage.getItem(key), journalKey),
      ).pending,
    ).toEqual(original);
    // The stale tab loaded before cancellation; retry must consult durable intent.
    await retry(stale).click();
    await expect(
      stale.getByRole("status").filter({ hasText: "Request canceled." }),
    ).toBeVisible();
    expect(attempts).toHaveLength(3);
    for (const attempt of attempts.slice(1))
      expect({ ...attempt, cancel: false }).toEqual(attempts[0]);
    expect(attempts.slice(1).every((a) => a.cancel)).toBe(true);
    const resolved = await page.evaluate(
      (key) => localStorage.getItem(key),
      journalKey,
    );
    await expect(retry(page)).toBeDisabled();
    expect(
      await page.evaluate((key) => localStorage.getItem(key), journalKey),
    ).toBe(resolved);
    expect(attempts).toHaveLength(3);
    await stale.close();
    await page.reload();
    await page.getByRole("button", { name: "Budget", exact: true }).click();
    // Correcting OS time alone leaves the captured local HLC ahead. No request may be sent.
    await save().click();
    await expect(
      page
        .getByRole("region", { name: "Category and group editor" })
        .getByRole("alert"),
    ).toContainText("five minutes ahead");
    expect(attempts).toHaveLength(3);
    await expect(field()).toHaveValue(`Retained ${kind}`);
    await expect(
      page.getByRole("button", {
        name: "Use corrected device time",
        exact: true,
      }),
    ).toBeVisible();
    await page.screenshot({
      path: new URL(
        `../test-results/clock-${kind}-reset-mobile.png`,
        import.meta.url,
      ).pathname,
      fullPage: true,
    });
    await page
      .getByRole("button", { name: "Use corrected device time", exact: true })
      .click();
    await save().click();
    await expect(retry(page)).toHaveCount(0);
    expect(attempts).toHaveLength(4);
    expect(attempts[3].key).not.toBe(attempts[0].key);
    expect(attempts[3].clock).not.toBe(attempts[0].clock);
    const rows = await call(base + "/" + resource);
    expect(rows.filter((row) => row.name === `Retained ${kind}`)).toHaveLength(
      1,
    );
    // A request already committed before its lost response must win over cancellation.
    await page
      .getByRole("button", {
        name: kind === "category" ? "Add category to Ungrouped" : "Add group",
        exact: true,
      })
      .first()
      .click();
    await field().fill(`Already committed ${kind}`);
    await context.unroute(`**/api/v1${base}/${resource}`);
    let dropOriginal = true;
    await context.route(`**/api/v1${base}/${resource}`, async (route) => {
      if (route.request().method() === "GET") return route.continue();
      if (dropOriginal) {
        dropOriginal = false;
        await forward(route);
        return route.abort();
      }
      return route.continue();
    });
    await save().click();
    await expect(retry(page)).toBeVisible();
    await cancel().click();
    await expect(retry(page)).toHaveCount(0);
    const committed = await call(base + "/" + resource);
    expect(
      committed.filter((row) => row.name === `Already committed ${kind}`),
    ).toHaveLength(1);
    {
      await page
        .getByRole("button", {
          name: kind === "category" ? "Add category to Ungrouped" : "Add group",
          exact: true,
        })
        .first()
        .click();
      await field().fill("   ");
      await save().click();
      await expect(
        page
          .getByRole("region", { name: "Category and group editor" })
          .getByRole("alert"),
      ).toContainText("name must contain");
      await cancel().click();
      await expect(retry(page)).toHaveCount(0);
      await expect(field()).toHaveValue("   ");
      await field().fill(`Corrected invalid ${kind}`);
      await save().click();
      await expect(retry(page)).toHaveCount(0);
      const corrected = await call(base + "/" + resource);
      expect(
        corrected.filter((row) => row.name === `Corrected invalid ${kind}`),
      ).toHaveLength(1);
    }
    await context.close();
  }
  console.log(
    "Metadata recovery passed: future rejection, corrected-time unsent recovery, lost cancellation/ack, stale tabs, immutable retries, original committed receipt, desktop/mobile.",
  );
} finally {
  await browser?.close();
  await server.stop();
}

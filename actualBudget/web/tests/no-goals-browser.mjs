import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function req(path, body, method) {
  const target = /^\/budgets\/([^/]+)/.exec(path)?.[1];
  const epoch =
    body && target
      ? (await (await fetch(`${api}/budgets/${target}/checkpoint`)).json()).data
          ?.epoch
      : undefined;
  return fetch(api + path, {
    method: method ?? (body === undefined ? "GET" : "POST"),
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
      ...(epoch ? { "X-Actual-Budget-Epoch": epoch } : {}),
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
}
async function call(path, body, method) {
  const r = await req(path, body, method);
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const bytes = await readFile(
    "../tests/fixtures/budget-goal-upgrade-source.zip",
  );
  const stage = await call("/imports/actual", bytes);
  expect(stage.status).toBe("ready");
  const current = await call(`/imports/${stage.id}/activate`, {});
  const root = `/budgets/${current.id}`;
  const financial = await call(root + "/transactions");
  const plan = await call(root + "/budget?month=2026-11");
  const prefs = await call(root + "/preferences");
  expect(prefs).not.toHaveProperty("experimental");
  expect(
    (
      await req(
        root + "/preferences",
        { experimental: { goalTemplatesEnabled: true } },
        "PATCH",
      )
    ).status,
  ).toBe(400);
  for (const [suffix, body] of [
    ["", undefined],
    ["/parse", { notes: "#template 100" }],
    ["/preview", {}],
    ["/projection", {}],
    ["/migration-preview", {}],
    ["/commands", { action: "apply", month: "2026-11" }],
  ])
    expect((await req(root + "/budget/templates" + suffix, body)).status).toBe(
      404,
    );
  expect(await call(root + "/transactions")).toEqual(financial);
  expect(await call(root + "/budget?month=2026-11")).toEqual(plan);
  const backup = await call(root + "/backups", {
    label: "Retired goals retained",
  });
  const archive = Buffer.from(
    await (
      await req(root + "/backups/" + backup.id + "/download")
    ).arrayBuffer(),
  );
  await call("/restores", archive);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  // An obsolete saved template draft must not reopen or lock the retired editor.
  await page.evaluate((id) => {
    localStorage.setItem(
      "actualBudget.selection",
      JSON.stringify({ budgetId: id }),
    );
    localStorage.setItem(
      `actualBudget.templates.v1.${id}`,
      '{"obsolete":true}',
    );
  }, current.id);
  await page.reload();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Goal templates", exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".budget-category").first()).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Next month", exact: true }),
  ).toBeEnabled();
  await mkdir("test-results/no-goals", { recursive: true });
  await page.screenshot({
    path: "test-results/no-goals/budget.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByText("Goal templates", { exact: true })).toHaveCount(
    0,
  );
  await expect(
    page.getByText("Budget automation", { exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/no-goals/mobile.png",
    fullPage: true,
  });
  expect(errors).toEqual([]);
  console.log(
    "PASS retired goal routes/settings/UI/draft; existing-goal import, unchanged finances, backup compatibility and mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
}

import { transactionAction } from "./register-ui.mjs";
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
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
  const result = await r.json();
  expect(r.ok, JSON.stringify(result)).toBe(true);
  return result;
}
try {
  const b = (
    await call("/budgets", { id: randomUUID(), name: "Split household" })
  ).data;
  const base = `/budgets/${b.id}`;
  const a = (
    await call(base + "/accounts", {
      id: randomUUID(),
      name: "Checking",
      opening_balance: 0,
      opening_date: "2026-01-01",
    })
  ).data;
  const childPayee = (
    await call(base + "/payees", { id: randomUUID(), name: "Child payee" })
  ).data;
  const childCategory = (
    await call(base + "/categories", {
      id: randomUUID(),
      name: "Child category",
      is_income: false,
    })
  ).data;
  await call(base + "/transactions", {
    id: randomUUID(),
    account_id: a.id,
    amount: -5001,
    date: "2026-01-02",
    notes: "Grocery purchase",
  });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 960 },
  });
  const page = await context.newPage();
  await page.goto(server.url);
  await page
    .locator(".budget-list button")
    .filter({ hasText: "Split household" })
    .click();
  await (
    await transactionAction(page, { name: "Split transaction", exact: true })
  ).click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toBeVisible();
  await page.getByLabel("Split 1 amount", { exact: true }).fill("-20.00");
  await page
    .getByRole("button", { name: "Distribute and save", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  const register = await call(base + "/transactions");
  expect(register.balances.total).toBe(-5001);
  expect(register.data[0].children.map((c) => c.amount)).toEqual([
    -2000, -3001,
  ]);
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  await page
    .getByLabel("Split 2 payee", { exact: true })
    .selectOption(childPayee.id);
  const attempts = [];
  await page.route("**/transactions/*/splits", async (route) => {
    const r = route.request();
    attempts.push({
      body: r.postData(),
      key: r.headers()["idempotency-key"],
      clock: r.headers()["x-actual-edit-clock"],
      epoch: r.headers()["x-actual-budget-epoch"],
      owner: r.headers()["x-actual-history-owner"],
    });
    if (attempts.length === 1) {
      const response = await fetch(
        r.url().replace("actualbudget.test", "127.0.0.1"),
        {
          method: r.method(),
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": r.headers()["idempotency-key"],
            "X-Actual-Edit-Clock": r.headers()["x-actual-edit-clock"],
            "X-Actual-Budget-Epoch": r.headers()["x-actual-budget-epoch"],
            "X-Actual-History-Owner": r.headers()["x-actual-history-owner"],
          },
          body: r.postData(),
        },
      );
      expect(response.ok, await response.text()).toBe(true);
      return route.abort();
    }
    return route.continue();
  });
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Retry saved split", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  await expect.poll(() => attempts.length).toBe(2);
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toHaveCount(0);
  expect(attempts[1]).toEqual(attempts[0]);
  await page.unroute("**/transactions/*/splits");
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  const peer = await page.context().newPage();
  await peer.goto(server.url);
  await expect(
    peer.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toBeVisible();
  await page
    .getByLabel("Split 2 category", { exact: true })
    .selectOption(childCategory.id);
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Split draft saved on this device." }),
  ).toBeVisible();
  await peer.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    peer
      .getByRole("alert")
      .filter({ hasText: "Another tab changed this split draft" }),
  ).toBeVisible();
  await peer
    .getByRole("button", { name: "Recover saved split", exact: true })
    .click();
  await expect(
    peer.getByLabel("Split 2 category", { exact: true }),
  ).toHaveValue(childCategory.id);
  await peer.close();
  await call(base + "/payees", {
    id: randomUUID(),
    name: "Unrelated metadata",
  });
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Refresh unchanged transaction",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Refresh unchanged transaction", exact: true })
    .click();
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  await expect(
    page.getByLabel("Split 2 category", { exact: true }),
  ).toHaveValue(childCategory.id);
  // Unsplitting must not silently discard edited children, for either action.
  let splitPosts = 0;
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith("/splits")) splitPosts++;
  });
  for (const action of ["Unsplit this child", "Unsplit all children"]) {
    for (const [field, value, original, select] of [
      ["amount", "-21.00", "-20.00", false],
      ["payee", childPayee.id, "", true],
      ["category", childCategory.id, "", true],
    ]) {
      const input = page.getByLabel(`Split 1 ${field}`, { exact: true });
      if (select) await input.selectOption(value);
      else await input.fill(value);
      await page
        .getByRole("button", { name: action, exact: true })
        .first()
        .click();
      await expect(
        page.getByRole("alert").filter({
          hasText:
            "Save split edits before unsplitting. Your draft is retained.",
        }),
      ).toBeVisible();
      await expect(input).toHaveValue(value);
      expect(splitPosts).toBe(0);
      if (field === "amount") {
        await mkdir("../../.scratch/ticket14-review", { recursive: true });
        const mobile = action === "Unsplit all children";
        await page.setViewportSize(
          mobile ? { width: 390, height: 844 } : { width: 1280, height: 960 },
        );
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= window.innerWidth,
          ),
        ).toBe(true);
        await page.screenshot({
          path: `../../.scratch/ticket14-review/unsplit-error-${mobile ? "mobile" : "desktop"}.png`,
          fullPage: true,
        });
        await page.setViewportSize({ width: 1280, height: 960 });
      }
      await page.reload();
      await expect(input).toHaveValue(value);
      if (select) await input.selectOption(original);
      else await input.fill(original);
    }
  }
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  await mkdir("../../.scratch/ticket14-visuals", { recursive: true });
  await page.getByLabel("Split 1 amount", { exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Split 1 payee", { exact: true })).toBeFocused();
  await page.screenshot({
    path: "../../.scratch/ticket14-visuals/splits-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "../../.scratch/ticket14-visuals/splits-mobile.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Recombine", exact: true }).click();
  await expect(
    await transactionAction(page, { name: "Split transaction", exact: true }),
  ).toBeVisible();
  // Integrated payee removal preserves historical references through split edits.
  const splitPath = `${base}/transactions/${register.data[0].id}/splits`;
  const current = await call(splitPath);
  await call(splitPath, {
    snapshot: current.snapshot,
    fields: { payee_id: childPayee.id },
    children: [
      { id: randomUUID(), amount: -2000, payee_id: childPayee.id },
      { id: randomUUID(), amount: -3001 },
    ],
  });
  await call(base + "/payees/batch", {
    action: "remove",
    ids: [childPayee.id],
  });
  await page.reload();
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  await expect(page.getByLabel("Split 1 payee", { exact: true })).toHaveValue(
    childPayee.id,
  );
  await expect(
    page
      .getByLabel("Split 1 payee", { exact: true })
      .getByRole("option", { name: "Removed payee", exact: true }),
  ).toHaveCount(1);
  await page
    .getByLabel("Split 1 category", { exact: true })
    .selectOption(childCategory.id);
  await expect(
    page.getByRole("status").filter({
      hasText: "Split draft saved on this device.",
    }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByLabel("Split 1 category", { exact: true }),
  ).toHaveValue(childCategory.id);
  await expect(page.getByLabel("Split 1 payee", { exact: true })).toHaveValue(
    childPayee.id,
  );
  for (const [label, width, height] of [
    ["desktop", 1280, 960],
    ["mobile", 390, 844],
  ]) {
    await page.setViewportSize({ width, height });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `../../.scratch/ticket14-visuals/removed-payee-${label}.png`,
      fullPage: true,
    });
  }
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("2 splits", { exact: true })).toBeVisible();
  const retained = (await call(splitPath)).data;
  expect(retained.retained_payee_id).toBe(childPayee.id);
  expect(retained.payee_id).toBeNull();
  expect(retained.children[0].retained_payee_id).toBe(childPayee.id);
  expect(retained.children[0].notes).toBe("");
  expect(retained.children[0].category_id).toBe(childCategory.id);
  await (
    await transactionAction(page, { name: "Edit splits", exact: true })
  ).click();
  await page
    .getByRole("button", { name: "Unsplit all children", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0);
  console.log("split browser checks passed");
} finally {
  await browser?.close();
  await server.stop();
}

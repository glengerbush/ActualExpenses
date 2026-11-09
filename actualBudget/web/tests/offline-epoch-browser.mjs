import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, epoch, method = "POST") {
  const r = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...(epoch ? { "X-Actual-Budget-Epoch": epoch } : {}),
    },
    body: JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
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
  const context = await browser.newContext(),
    page = await context.newPage();
  const budget = randomUUID(),
    account = randomUUID(),
    root = `/budgets/${budget}`;
  await write("/budgets", { id: budget, name: "Outbox generation" });
  await write(root + "/accounts", {
    id: account,
    name: "Spending",
    opening_balance: 10000,
    opening_date: "2026-01-01",
  });
  const point = await write(root + "/backups", {
    label: "Before pending expense",
  });
  await page.goto(server.url);
  await page.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget, account },
  );
  await page.reload();
  await expect(page.getByTestId("register-total")).toHaveText("100.00");
  await context.route("**/api/v1/**", (route) => route.abort());
  await page.evaluate(() => dispatchEvent(new Event("offline")));
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("3.21");
  await page
    .getByLabel("Notes", { exact: true })
    .fill("Old generation expense");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Old generation expense",
  );
  const restored = await write(root + `/backups/${point.id}/restore`, {
    confirm_name: "Outbox generation",
  });
  const attempts = [];
  await context.unroute("**/api/v1/**");
  await context.route(`**/api/v1${root}/transactions`, async (route) => {
    if (route.request().method() === "POST")
      attempts.push({
        body: route.request().postData(),
        key: route.request().headers()["idempotency-key"],
        clock: route.request().headers()["x-actual-edit-clock"],
        epoch: route.request().headers()["x-actual-budget-epoch"],
        cancel: route.request().headers()["x-actual-cancel-if-uncommitted"],
      });
    await route.continue();
  });
  await page.evaluate(() => dispatchEvent(new Event("online")));
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "This budget was restored",
  );
  expect(attempts[0].epoch).not.toBe(restored.budget.epoch);
  await page.reload();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "Old generation expense",
  );
  await page
    .getByRole("button", { name: "Retry pending expense", exact: true })
    .click();
  await expect.poll(() => attempts.length).toBe(2);
  expect(attempts[1]).toEqual(attempts[0]);
  await expect(
    page.getByRole("button", {
      name: "Cancel pending change safely",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Cancel pending change safely", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toHaveCount(0);
  expect(attempts.at(-1).cancel).toBe("true");
  expect(attempts.at(-1).epoch).toBe(attempts[0].epoch);
  const old = JSON.parse(attempts[0].body);
  let rows = (
    await (await fetch(api + "/api/v1" + root + "/transactions")).json()
  ).data;
  expect(rows.some((row) => row.id === old.id)).toBe(false);
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("3.21");
  await page.getByLabel("Notes", { exact: true }).fill("Reviewed replacement");
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("96.79");
  expect(attempts.at(-1).epoch).toBe(restored.budget.epoch);
  expect(attempts.at(-1).key).not.toBe(attempts[0].key);
  console.log(
    "PASS restored generation preserves old body/key/clock/epoch; explicit cancellation fences it; reviewed fresh expense uses new identity",
  );
  // A draft opened before restore also retains its original epoch until it is saved.
  await page
    .getByRole("button", { name: "Add transaction", exact: true })
    .click();
  await page.getByLabel("Amount", { exact: true }).fill("1");
  await page.getByLabel("Notes", { exact: true }).fill("Open before restore");
  await write(
    root + `/backups/${point.id}/restore`,
    { confirm_name: "Outbox generation" },
    restored.budget.epoch,
  );
  await page
    .getByRole("button", { name: "Save transaction", exact: true })
    .click();
  await expect(page.getByTestId("pending-expenses")).toContainText(
    "This budget was restored",
  );
  expect(attempts.at(-1).epoch).toBe(restored.budget.epoch);
  console.log(
    "PASS form-open generation captured before restore; submit never silently rebases old intent",
  );
  await context.close();
} finally {
  await browser?.close();
  await server.stop();
}

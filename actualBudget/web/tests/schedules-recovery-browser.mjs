import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
}
let browser;
try {
  const budget = randomUUID(),
    account = randomUUID(),
    schedule = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Foreground schedule recovery" });
  await call(root + "/accounts", {
    id: account,
    name: "Checking",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const due = (await call(root + "/schedules/due")).data;
  await call(root + "/schedules/commands", {
    action: "create",
    id: schedule,
    name: "Automatic bill",
    posts_transaction: true,
    snapshot: due.snapshot,
    conditions: [
      { field: "account", op: "is", value: account },
      { field: "amount", op: "is", value: -678 },
      { field: "date", op: "is", value: due.today },
    ],
  });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage();
  let mode = "lost";
  let captured = null,
    sends = 0,
    lookups = 0;
  await page.route(
    `**/api/v1/budgets/${budget}/schedules/commands`,
    async (route) => {
      const req = route.request(),
        headers = req.headers();
      if (headers["x-actual-receipt-only"] === "true") {
        lookups++;
        await route.continue();
        return;
      }
      if (mode === "pass") {
        await route.continue();
        return;
      }
      sends++;
      captured = {
        body: req.postDataJSON(),
        key: headers["idempotency-key"],
        clock: headers["x-actual-edit-clock"],
        epoch: headers["x-actual-budget-epoch"],
        owner: headers["x-actual-history-owner"],
      };
      if (mode === "before") {
        await route.abort("failed");
        return;
      }
      const target = new URL(req.url());
      const forwarded = { ...headers, host: target.host };
      target.hostname = "127.0.0.1";
      const response = await route.fetch({
        url: target.href,
        headers: forwarded,
      });
      expect(response.ok(), await response.text()).toBe(true);
      await route.abort("failed");
    },
  );
  await page.goto(`${server.url}/?budget=${budget}&view=transactions`);
  const key = `actualBudget.schedule-system.v1.${budget}`;
  await expect
    .poll(() =>
      page.evaluate(
        (key) => JSON.parse(localStorage.getItem(key) || "null"),
        key,
      ),
    )
    .not.toBeNull();
  await expect(
    page.getByRole("button", { name: "Review scheduled entries", exact: true }),
  ).toBeVisible();
  const saved = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)),
    key,
  );
  expect(saved.attempted).toBe(true);
  expect(saved.pending.body).toEqual(captured.body);
  expect(saved.pending.key).toBe(captured.key);
  expect(saved.pending.edit_clock).toBe(captured.clock);
  expect(saved.pending.budgetEpoch).toBe(captured.epoch);
  expect(saved.pending.historyOwner).toBe(captured.owner);
  const before = await call(root + "/transactions");
  expect(before.data).toHaveLength(1);
  expect(before.balances.total).toBe(-678);
  await page.reload();
  await expect
    .poll(() => page.evaluate((key) => localStorage.getItem(key), key))
    .toBeNull();
  expect(sends).toBe(1);
  expect(lookups).toBe(1);
  expect(await call(root + "/transactions")).toEqual(before);
  mode = "before";
  const secondSchedule = randomUUID();
  const current = (await call(root + "/schedules/due")).data;
  await call(root + "/schedules/commands", {
    action: "create",
    id: secondSchedule,
    name: "Restored bill",
    posts_transaction: true,
    snapshot: current.snapshot,
    conditions: [
      { field: "account", op: "is", value: account },
      { field: "amount", op: "is", value: -679 },
      { field: "date", op: "is", value: current.today },
    ],
  });
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Review scheduled entries", exact: true }),
  ).toBeVisible();
  const old = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)),
    key,
  );
  const backup = (await call(root + "/backups", { label: "Before restore" }))
    .data;
  await call(root + `/backups/${backup.id}/restore`, {
    confirm_name: "Foreground schedule recovery",
  });
  mode = "pass";
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Review scheduled entries", exact: true }),
  ).toBeVisible();
  const held = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)),
    key,
  );
  expect(held.pending).toEqual(old.pending);
  expect(held.review).toBeTruthy();
  expect((await call(root + "/transactions")).data).toHaveLength(1);
  await page
    .getByRole("button", { name: "Review scheduled entries", exact: true })
    .click();
  await expect(
    page.getByRole("heading", {
      name: "Automatic schedule work needs confirmation",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", {
      name: "Cancel automatic schedule request",
      exact: true,
    })
    .click();
  await expect
    .poll(() => page.evaluate((key) => localStorage.getItem(key), key))
    .toBeNull();
  expect((await call(root + "/transactions")).data).toHaveLength(1);
  const late = await fetch(api + old.pending.path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": old.pending.key,
      "X-Actual-Edit-Clock": old.pending.edit_clock,
      "X-Actual-Budget-Epoch": old.pending.budgetEpoch,
      "X-Actual-History-Owner": old.pending.historyOwner,
    },
    body: JSON.stringify(old.pending.body),
  });
  expect(late.status).toBe(409);
  expect((await late.json()).error.code).toBe("operation_canceled");
  await page.reload();
  await expect
    .poll(async () => (await call(root + "/transactions")).data.length)
    .toBe(2);
  expect((await call(root + "/transactions")).balances.total).toBe(-1357);
  console.log(
    "Foreground lost acknowledgment retains exact independent command and resolves original receipt once after reload: PASS",
  );
} finally {
  await browser?.close();
  await server.stop();
}

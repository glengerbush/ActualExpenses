import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, method = "POST", epoch) {
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
  const page = await browser.newPage();
  const budget = randomUUID();
  await write("/budgets", { id: budget, name: "Generation recovery" });
  const root = `/budgets/${budget}`;
  const point = await write(root + "/backups", { label: "Original" });
  await page.goto(server.url);
  await page.evaluate(
    (budget) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: null }),
      ),
    budget,
  );
  await page.reload();
  const firstEpoch = (
    await (await fetch(api + "/api/v1" + root + "/checkpoint")).json()
  ).data.epoch;
  await write(
    root + `/backups/${point.id}/restore`,
    { confirm_name: "Generation recovery" },
    "POST",
    firstEpoch,
  );
  async function delayedAccount() {
    return fetch(api + "/api/v1" + root + "/ledger", {
      method: "POST",
      headers: { "X-Actual-Budget-Epoch": firstEpoch },
    });
  }
  expect((await delayedAccount()).status).toBe(409);
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.locator("#number-format").selectOption("dot-comma");
  const settings = await page.evaluate(
    (budget) =>
      JSON.parse(
        localStorage.getItem("actualBudget.preferencesDraft.v1." + budget),
      ),
    budget,
  );
  expect(settings.budget_epoch).not.toBe(firstEpoch);
  await write(
    root + `/backups/${point.id}/restore`,
    { confirm_name: "Generation recovery" },
    "POST",
    settings.budget_epoch,
  );
  await page
    .getByRole("button", { name: "Save shared preferences", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Review current budget", exact: true })
    .click();
  await expect(page.locator(".restore-comparison")).toContainText("1,000.33");
  await page.screenshot({
    path: fileURLToPath(
      new URL(
        "../test-results/settings-generation-recovery.png",
        import.meta.url,
      ),
    ),
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Reapply as new settings", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Save shared preferences", exact: true })
    .click();
  await expect(
    page.getByText("Shared preferences saved.", { exact: true }),
  ).toBeVisible();
  expect((await delayedAccount()).status).toBe(409);
  const current = (
    await (await fetch(api + "/api/v1" + root + "/preferences")).json()
  ).data;
  expect(current.number_format).toBe("dot-comma");
  console.log(
    "Budget epoch browser: stale ledger preparation rejected; settings review/reapply passed",
  );
} finally {
  await browser?.close();
  await server.stop();
}

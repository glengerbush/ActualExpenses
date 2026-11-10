// Native exact-money boundary, public setup and real CSV download; no source-range claim.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
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
  const page = await browser.newPage();
  await page.clock.setFixedTime(new Date("2026-11-09T12:00:00Z"));
  await page.goto(server.url);
  const api = (path, body, method) =>
    page.evaluate(
      async ({ path, body, method, key }) => {
        const r = await fetch("/api/v1" + path, {
          method: method || (body === undefined ? "GET" : "POST"),
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
          },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        if (!r.ok) throw Error(await r.text());
        return (await r.json()).data;
      },
      { path, body, method, key: randomUUID() },
    );
  const budget = randomUUID(),
    category = randomUUID(),
    root = "/budgets/" + budget,
    name = "Exact analysis " + budget;
  await api("/budgets", { id: budget, name, budget_type: "tracking" });
  await api(root + "/accounts", {
    id: randomUUID(),
    name: "Cash",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  await api(root + "/categories", {
    id: category,
    name: "Exact allocation",
    is_income: false,
  });
  await api(
    root + "/preferences",
    { experimental: { budgetAnalysisReport: true } },
    "PATCH",
  );
  let current = await api(root + "/budget?month=2026-11");
  await api(root + "/budget", {
    action: "set",
    month: "2026-11",
    category,
    amount: 9007199254740991,
    snapshot: current.snapshot,
  });
  await page.reload();
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page
    .getByRole("button", { name: "New dashboard", exact: true })
    .click();
  await page.getByLabel("Dashboard name", { exact: true }).fill("Exact cents");
  await page
    .getByRole("button", { name: "Save dashboard", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Budget Analysis", exact: true })
    .click();
  await page.getByRole("button", { name: "New analysis", exact: true }).click();
  const editor = page.getByRole("form", { name: "Reporting editor" });
  await editor
    .getByRole("combobox", { name: "Date mode", exact: true })
    .selectOption("static");
  await editor.getByLabel("Start month", { exact: true }).fill("2026-11");
  await editor.getByLabel("End month", { exact: true }).fill("2026-11");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  const widget = page.getByRole("article", {
    name: "Budget Analysis",
    exact: true,
  });
  await expect(
    widget.getByRole("button", {
      name: "Inspect planned allocations 2026-11",
      exact: true,
    }),
  ).toHaveText("90,071,992,547,409.91");
  await expect(widget.locator("rect")).toHaveCount(3);
  const download = page.waitForEvent("download");
  await widget
    .getByRole("button", { name: "Export as CSV", exact: true })
    .click();
  expect(await readFile(await (await download).path(), "utf8")).toBe(
    "Month,Budgeted,Spent,Overspending Adjustment,Balance\n2026-11,90071992547409.91,0,0,90071992547409.91\n",
  );
  console.log(
    "PASS native MAX_MONEY public allocation → exact table and CSV cents",
  );
} finally {
  await browser?.close();
  await server.stop();
}

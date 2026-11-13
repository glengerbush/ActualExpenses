import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
let browser;
try {
  const base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  async function api(
    path,
    body,
    method = body === undefined ? "GET" : "POST",
    headers = {},
  ) {
    const response = await fetch(base + path, {
      method,
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
        ...headers,
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const result = await response.json();
    expect(response.ok, JSON.stringify(result)).toBe(true);
    return result.data;
  }
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/monte-carlo-baseline-source.json",
        import.meta.url,
      ),
    ),
  );
  const expected = source.cases.find((c) => c.name === "baseline-proportional");
  const budget = randomUUID(),
    root = `/budgets/${budget}`,
    dashboard = randomUUID(),
    widgetId = randomUUID();
  await api("/budgets", { id: budget, name: "Monte Carlo browser" });
  const checkpoint = await api(root + "/checkpoint");
  await api(
    root + "/reporting/commands",
    {
      writes: [
        {
          id: dashboard,
          kind: "page",
          revision: 0,
          tombstone: false,
          config: { name: "Planning" },
        },
        {
          id: widgetId,
          kind: "widget",
          revision: 0,
          tombstone: false,
          config: {
            type: "monte-carlo-card",
            dashboard_page_id: dashboard,
            x: 0,
            y: 0,
            width: 12,
            height: 6,
            meta: { ...expected.meta, name: "Retirement plan" },
          },
        },
      ],
    },
    "POST",
    { "X-Actual-Budget-Epoch": checkpoint.epoch },
  );
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    }),
    page = await context.newPage();
  const errors = [],
    runs = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith("/reports/monte-carlo"))
      runs.push(r.postDataJSON());
  });
  await page.goto(server.url);
  await page.getByRole("button", { name: /Monte Carlo browser.*Open/ }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  const widget = page.getByRole("article", {
    name: "Retirement plan",
    exact: true,
  });
  await expect(
    widget.getByText(
      "Enable Monte Carlo report in Settings to run this plan.",
      { exact: true },
    ),
  ).toBeVisible();
  expect(runs).toHaveLength(0);
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByText("Experimental features", { exact: true }).click();
  await page
    .getByRole("checkbox", { name: "Monte Carlo report", exact: true })
    .check();
  await page
    .getByRole("button", { name: "Save shared preferences", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await api(root + "/preferences")).experimental.monteCarloReport,
    )
    .toBe(true);
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Monte Carlo", exact: true }).click();
  expect(runs).toHaveLength(0);
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeEnabled();
  await widget
    .getByRole("button", { name: "Run simulation", exact: true })
    .focus();
  await page.keyboard.press("Enter");
  await expect(
    widget.getByLabel("Median ending balance", { exact: true }),
  ).toHaveText(
    new Intl.NumberFormat("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(expected.result.medianEndingBalance / 100),
    { timeout: 30000 },
  );
  expect(runs).toHaveLength(1);
  await expect(
    widget.getByRole("table", {
      name: "Balance percentiles by age",
      exact: true,
    }),
  ).toBeVisible();
  const money = (n) =>
    new Intl.NumberFormat("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(Math.round(n) / 100);
  expect(
    await widget
      .getByRole("table", { name: "Balance percentiles by age", exact: true })
      .locator("tbody tr")
      .evaluateAll((rows) =>
        rows.map((row) =>
          Array.from(row.cells).map((cell) => cell.textContent),
        ),
      ),
  ).toEqual(
    expected.result.percentileBands.map((row) => [
      String(60 + row.year),
      ...["p5", "p10", "p25", "p30", "p50", "p70", "p75", "p90"].map((key) =>
        money(row[key]),
      ),
    ]),
  );
  await widget
    .getByRole("button", { name: "Edit Retirement plan", exact: true })
    .click();
  const editor = page.getByRole("form", {
    name: "Reporting editor",
    exact: true,
  });
  await expect(editor.getByLabel("Current age", { exact: true })).toHaveValue(
    "60",
  );
  await editor.getByLabel("Target age", { exact: true }).fill("81");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  expect(
    (await api(root + "/reporting")).documents.find((d) => d.id === widgetId)
      .config.meta.targetAge,
  ).toBe(81);
  expect(runs).toHaveLength(1);
  await widget
    .getByRole("button", { name: "Run simulation", exact: true })
    .click();
  await expect(
    widget.getByLabel("Simulation years", { exact: true }),
  ).toHaveText("21", { timeout: 30000 });
  await mkdir(new URL("../test-results/monte-carlo/", import.meta.url), {
    recursive: true,
  });
  await widget.screenshot({
    path: new URL("../test-results/monte-carlo/desktop.png", import.meta.url)
      .pathname,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await widget.screenshot({
    path: new URL("../test-results/monte-carlo/mobile.png", import.meta.url)
      .pathname,
  });
  await widget
    .getByRole("button", { name: "Edit Retirement plan", exact: true })
    .click();
  await editor.screenshot({
    path: new URL("../test-results/monte-carlo/editor.png", import.meta.url)
      .pathname,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  // Invalid financial drafts survive reload and cannot be saved as defaults.
  await editor.getByLabel("Annual spending", { exact: true }).fill("invalid");
  await page.reload();
  await expect(
    editor.getByLabel("Annual spending", { exact: true }),
  ).toHaveValue("invalid");
  expect(
    await editor
      .getByLabel("Annual spending", { exact: true })
      .evaluate((e) => e.checkValidity()),
  ).toBe(false);
  await editor.getByLabel("Annual spending", { exact: true }).fill("21000.25");
  await editor
    .locator("summary")
    .filter({ hasText: /^Income/ })
    .click();
  await editor.getByRole("button", { name: "Add income", exact: true }).click();
  await editor.getByLabel("Income name", { exact: true }).fill("Pension");
  await editor
    .getByLabel("Gross annual income", { exact: true })
    .fill("12500.75");
  await editor.getByLabel("Income starts at age", { exact: true }).fill("67");
  await editor
    .locator("summary")
    .filter({ hasText: /^Contributions/ })
    .click();
  await editor
    .getByRole("button", { name: "Add contribution", exact: true })
    .click();
  await editor
    .getByLabel("Annual contribution", { exact: true })
    .fill("1200.50");
  await editor
    .getByRole("combobox", { name: "Contribution source", exact: true })
    .selectOption({ label: "Pension" });
  await editor
    .getByLabel("Deduct contribution before income tax", { exact: true })
    .check();
  await editor
    .getByRole("combobox", { name: "Tax model", exact: true })
    .selectOption("bands");
  await editor
    .locator("summary")
    .filter({ hasText: /^Tax bands/ })
    .click();
  await editor.getByLabel("Band tax rate (%)", { exact: true }).fill("12.5");
  await editor.getByLabel("Fixed annual fee", { exact: true }).fill("150.25");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  const edited = (await api(root + "/reporting")).documents.find(
    (d) => d.id === widgetId,
  ).config.meta;
  expect(edited.spendingPhases[0].annualWithdrawal).toBe(2100025);
  expect(edited.incomeStreams[0].annualAmount).toBe(1250075);
  expect(edited.incomeStreams[0].fromAge).toBe(67);
  expect(edited.contributions[0]).toMatchObject({
    annualAmount: 120050,
    sourceIncomeStreamId: edited.incomeStreams[0].id,
    beforeTax: true,
  });
  expect(edited.taxBands[0].rate).toBe(0.125);
  expect(edited.pots[0].annualFeeFixed).toBe(15025);
  expect(runs).toHaveLength(2);
  // A long run exposes real server cancellation, then a short run recovers.
  await widget
    .getByRole("button", { name: "Edit Retirement plan", exact: true })
    .click();
  await editor.getByLabel("Target age", { exact: true }).fill("160");
  await editor.getByLabel("Simulation runs", { exact: true }).fill("10000");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  await widget
    .getByRole("button", { name: "Run simulation", exact: true })
    .click();
  await expect.poll(() => runs.length).toBe(3);
  const runningId = runs.at(-1).request_id;
  await expect
    .poll(
      async () =>
        (await fetch(base + root + "/reports/monte-carlo/" + runningId)).status,
    )
    .toBe(200);
  await widget
    .getByRole("button", { name: "Cancel simulation", exact: true })
    .click();
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeEnabled();
  await expect
    .poll(
      async () =>
        (await fetch(base + root + "/reports/monte-carlo/" + runningId)).status,
    )
    .toBe(404);
  await expect(
    widget.getByRole("status").filter({ hasText: "Simulation cancelled." }),
  ).toBeVisible();
  await widget
    .getByRole("button", { name: "Edit Retirement plan", exact: true })
    .click();
  await editor.getByLabel("Target age", { exact: true }).fill("61");
  await editor.getByLabel("Simulation runs", { exact: true }).fill("1000");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  await widget
    .getByRole("button", { name: "Run simulation", exact: true })
    .click();
  await expect(
    widget.getByLabel("Simulation years", { exact: true }),
  ).toHaveText("1");
  const runCount = runs.length;
  await page.reload();
  await expect(
    widget.getByLabel("Simulation years", { exact: true }),
  ).toHaveText("1");
  expect(runs).toHaveLength(runCount);
  await context.setOffline(true);
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeDisabled();
  await expect(
    widget.getByLabel("Simulation years", { exact: true }),
  ).toHaveText("1");
  await context.setOffline(false);
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  const largeText = await page.addStyleTag({
    content: "html { font-size: 200% !important; }",
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await widget.screenshot({
    path: new URL("../test-results/monte-carlo/large-text.png", import.meta.url)
      .pathname,
  });
  await largeText.evaluate((style) => style.remove());
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.locator("#theme").selectOption("dark");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.keyboard.press("Control+k");
  await page
    .getByRole("searchbox", { name: "Search commands", exact: true })
    .fill("Monte Carlo");
  await page.keyboard.press("Enter");
  await expect(
    widget.getByLabel("Simulation years", { exact: true }),
  ).toHaveText("1");
  expect(runs).toHaveLength(runCount);
  await widget.screenshot({
    path: new URL("../test-results/monte-carlo/dark.png", import.meta.url)
      .pathname,
  });
  await page
    .getByRole("button", { name: "New Monte Carlo plan", exact: true })
    .click();
  await editor
    .getByLabel("Widget name", { exact: true })
    .fill("New plan defaults");
  await expect(
    editor.getByLabel("Keep surplus income", { exact: true }),
  ).toBeChecked();
  await expect(
    editor.getByLabel("Simulation runs", { exact: true }),
  ).toHaveValue("5000");
  await editor.getByLabel("Simulation runs", { exact: true }).fill("1000");
  await editor.getByLabel("Target age", { exact: true }).fill("61");
  await expect(
    editor.getByLabel("Inflation volatility (%)", { exact: true }),
  ).toHaveValue("2");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  const created = page.getByRole("article", {
    name: "New plan defaults",
    exact: true,
  });
  await created
    .getByRole("button", { name: "Run simulation", exact: true })
    .click();
  const defaults = source.cases.find(
    (c) => c.name === "empty-collections-defaults",
  );
  await expect(
    created.getByLabel("Median ending balance", { exact: true }),
  ).toHaveText(money(defaults.result.medianEndingBalance), { timeout: 30000 });
  await created
    .getByRole("button", { name: "Edit New plan defaults", exact: true })
    .click();
  const pot = editor.getByRole("group", {
    name: "Investment pots 2",
    exact: true,
  });
  await pot
    .getByRole("combobox", { name: "Asset allocation", exact: true })
    .selectOption("equity-80");
  await expect(
    pot.getByLabel("Expected annual return (%)", { exact: true }),
  ).toHaveValue("6.5");
  await pot
    .getByRole("combobox", { name: "Asset allocation", exact: true })
    .selectOption("custom-mix");
  await expect(pot.getByLabel("Stocks (%)", { exact: true })).toHaveValue("80");
  await expect(pot.getByLabel("Bonds (%)", { exact: true })).toHaveValue("20");
  await editor
    .getByRole("combobox", { name: "Return model", exact: true })
    .selectOption("historical-bootstrap");
  await pot.getByLabel("Stocks (%)", { exact: true }).fill("70");
  await expect(
    editor
      .getByRole("status")
      .filter({ hasText: "no complete historical asset mix" }),
  ).toBeVisible();
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  await created
    .getByRole("button", { name: "Run simulation", exact: true })
    .click();
  await expect(
    created
      .getByRole("status")
      .filter({ hasText: "no complete historical asset mix" }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  console.log(
    "PASS gated manual run, source summary/percentiles, rich input editing, retained invalid draft, cancellation/recovery, saved offline view, keyboard and responsive/large-text layouts",
  );
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { sourceRuntime } from "../../tools/reports-source-runtime.mjs";
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
    const value = await response.json();
    expect(response.ok, JSON.stringify(value)).toBe(true);
    return value.data;
  }
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/monte-carlo-strategies-source.json",
        import.meta.url,
      ),
    ),
  );
  const expected = source.cases.find((c) => c.name === "strategy-sequential");
  const reference = sourceRuntime();
  const pinned = reference.load("reports/monte-carlo/monteCarloSimulation");
  const ranked = Array.from(
    pinned.rankSimulationsWorstFirst(
      new Float64Array(expected.result.endingBalances),
      new Int32Array(expected.result.depletionYearBySimulation),
    ),
  );
  const budget = randomUUID(),
    root = `/budgets/${budget}`,
    dashboard = randomUUID(),
    widgetId = randomUUID(),
    account = randomUUID();
  await api("/budgets", { id: budget, name: "Run inspection" });
  await api(
    root + "/preferences",
    { experimental: { monteCarloReport: true } },
    "PATCH",
  );
  await api(root + "/accounts", {
    id: account,
    name: "Stocks",
    opening_balance: 35_000_000,
    opening_date: "2026-01-01",
  });
  const checkpoint = await api(root + "/checkpoint");
  const config = structuredClone(expected.meta);
  config.pots[0].accountId = account;
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
            meta: { ...config, name: "Scenario plan" },
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
  await page.getByRole("button", { name: /Run inspection.*Open/ }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  const widget = page.getByRole("article", {
    name: "Scenario plan",
    exact: true,
  });
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeEnabled();
  await widget
    .getByRole("button", { name: "Run simulation", exact: true })
    .click();
  const money = (n) =>
    new Intl.NumberFormat("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(Math.round(n) / 100);
  await expect(
    widget.getByLabel("Median ending balance", { exact: true }),
  ).toHaveText(money(expected.result.medianEndingBalance), { timeout: 30000 });
  await expect(
    widget.getByRole("img", { name: "Portfolio performance", exact: true }),
  ).toBeVisible();
  expect(runs[0].captureRunDetail).toBe(0);
  await widget.getByText("Individual simulation runs", { exact: true }).click();
  const list = widget.getByRole("table", {
    name: "Simulation runs",
    exact: true,
  });
  expect(
    await list
      .locator("tbody tr")
      .evaluateAll((rows) =>
        rows.map((row) => Number(row.cells[1].textContent)),
      ),
  ).toEqual(ranked.slice(0, 20).map((i) => i + 1));
  await widget.getByRole("button", { name: "Next runs", exact: true }).click();
  expect(
    await list
      .locator("tbody tr")
      .evaluateAll((rows) =>
        rows.map((row) => Number(row.cells[1].textContent)),
      ),
  ).toEqual(ranked.slice(20, 40).map((i) => i + 1));
  await widget
    .getByRole("combobox", { name: "Run order", exact: true })
    .selectOption("best");
  expect(
    await list
      .locator("tbody tr")
      .evaluateAll((rows) =>
        rows.map((row) => Number(row.cells[1].textContent)),
      ),
  ).toEqual(
    ranked
      .slice(-20)
      .reverse()
      .map((i) => i + 1),
  );
  expect(runs).toHaveLength(1);
  // A later account edit must not change the inputs of the inspected saved run.
  await api(root + "/transactions", {
    id: randomUUID(),
    account_id: account,
    amount: -1000000,
    date: "2026-01-02",
    cleared: true,
  });
  const afterExpense = await api(root + "/checkpoint");
  const inspect = async (n) => {
    await widget.getByLabel("Run number", { exact: true }).fill(String(n));
    await widget
      .getByRole("button", { name: "Inspect run", exact: true })
      .click();
    await expect(
      widget.getByRole("heading", { name: `Run ${n} cash flow`, exact: true }),
    ).toBeVisible({ timeout: 30000 });
  };
  await inspect(18);
  expect(runs).toHaveLength(2);
  expect(runs[1].captureRunDetail).toBe(17);
  expect(runs[1].config.pots[0].accountId).toBeNull();
  expect(runs[1].config.pots[0].startingBalance).toBe(35_000_000);
  const table = widget.getByRole("table", {
    name: "Annual cash flow",
    exact: true,
  });
  expect(
    await table
      .locator("tbody tr")
      .evaluateAll((rows) =>
        rows.map((row) =>
          Array.from(row.cells).map((cell) => cell.textContent),
        ),
      ),
  ).toEqual(
    expected.result.runDetail.map((row) => [
      String(60 + row.year - 1),
      money(row.income),
      money(row.withdrawal),
      money(row.plannedSpending),
      money(row.spent),
      money(row.incomeTax + row.taxPaid),
      money(row.contributions),
      money(row.surplusSaved),
      money(row.feesPaid),
      money(row.endBalance),
      row.afterDepletion
        ? "After depletion"
        : row.spent < row.plannedSpending
          ? "Spending shortfall"
          : "Funded year",
    ]),
  );
  await expect(
    widget.getByRole("img", { name: "Annual cash flow", exact: true }),
  ).toBeVisible();
  await inspect(1);
  await inspect(18);
  expect(runs).toHaveLength(2);
  expect(await api(root + "/checkpoint")).toEqual(afterExpense);
  await page.reload();
  await expect(
    widget.getByRole("heading", { name: "Run 18 cash flow", exact: true }),
  ).toBeVisible();
  expect(runs).toHaveLength(2);
  await widget.getByText("Individual simulation runs", { exact: true }).click();
  await context.setOffline(true);
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeDisabled();
  await inspect(18);
  expect(runs).toHaveLength(2);
  await widget.getByLabel("Run number", { exact: true }).fill("19");
  await expect(
    widget.getByRole("button", { name: "Inspect run", exact: true }),
  ).toBeDisabled();
  await context.setOffline(false);
  const view = widget.getByRole("combobox", {
    name: "Portfolio view",
    exact: true,
  });
  await view.selectOption("worst");
  await expect(
    widget
      .getByRole("img", { name: "Portfolio performance", exact: true })
      .locator("polyline"),
  ).toHaveCount(1);
  await widget.getByText("Single worst run values", { exact: true }).click();
  expect(
    await widget
      .getByRole("table", { name: "Single worst run values", exact: true })
      .locator("tbody tr td")
      .allTextContents(),
  ).toEqual(expected.result.worstRunPath.map(money));
  await view.selectOption("all");
  await mkdir(new URL("../test-results/monte-carlo/", import.meta.url), {
    recursive: true,
  });
  for (const title of ["Portfolio performance", "Annual cash flow"])
    await widget.getByRole("figure", { name: title, exact: true }).screenshot({
      path: new URL(
        `../test-results/monte-carlo/${title === "Portfolio performance" ? "portfolio" : "cashflow"}-desktop.png`,
        import.meta.url,
      ).pathname,
    });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await mkdir(new URL("../test-results/monte-carlo/", import.meta.url), {
    recursive: true,
  });
  await widget
    .getByRole("figure", { name: "Portfolio performance", exact: true })
    .screenshot({
      path: new URL(
        "../test-results/monte-carlo/portfolio-mobile.png",
        import.meta.url,
      ).pathname,
    });
  await widget
    .getByRole("figure", { name: "Annual cash flow", exact: true })
    .screenshot({
      path: new URL(
        "../test-results/monte-carlo/cashflow-mobile.png",
        import.meta.url,
      ).pathname,
    });
  const largeText = await page.addStyleTag({
    content: "html { font-size: 200% !important; }",
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  await widget
    .getByRole("figure", { name: "Annual cash flow", exact: true })
    .screenshot({
      path: new URL(
        "../test-results/monte-carlo/cashflow-large-text.png",
        import.meta.url,
      ).pathname,
    });
  await largeText.evaluate((style) => style.remove());
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.locator("#theme").selectOption("dark");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(
    widget.getByRole("heading", { name: "Run 18 cash flow", exact: true }),
  ).toBeVisible();
  await widget
    .getByRole("figure", { name: "Annual cash flow", exact: true })
    .screenshot({
      path: new URL(
        "../test-results/monte-carlo/cashflow-dark.png",
        import.meta.url,
      ).pathname,
    });
  expect(runs).toHaveLength(2);
  // Failure leaves the last saved scenario visible and permits an explicit retry.
  await page.reload();
  await widget.getByText("Individual simulation runs", { exact: true }).click();
  await expect(
    widget.getByRole("button", { name: "Run simulation", exact: true }),
  ).toBeEnabled();
  const route = "**/reports/monte-carlo";
  await page.route(route, (r) =>
    r.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({
        error: {
          code: "unavailable",
          message: "Temporary calculation failure. Retry.",
        },
      }),
    }),
  );
  await widget.getByLabel("Run number", { exact: true }).fill("19");
  await widget
    .getByRole("button", { name: "Inspect run", exact: true })
    .click();
  await expect(widget.getByRole("alert")).toHaveText(
    "Temporary calculation failure. Retry.",
  );
  await expect(
    widget.getByRole("heading", { name: "Run 18 cash flow", exact: true }),
  ).toBeVisible();
  await page.unroute(route);
  await inspect(19);

  const baseline = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/monte-carlo-baseline-source.json",
        import.meta.url,
      ),
    ),
  );
  const translate = (key, values = {}) =>
    key.replace(
      /\{\{\s*(\w+)\s*\}\}/g,
      (placeholder, key) => values[key] ?? placeholder,
    );
  const cashBuilder = reference.load(
    "graphs/util/monteCarloCashflowChart",
  ).buildMonteCarloCashflowChart;
  const storyBuilder = reference.load(
    "reports/monte-carlo/monteCarloYearStory",
  ).buildMonteCarloYearStory;
  for (const name of [
    "guardrails-locked-phases-minimum",
    "floor-ceiling-falling",
    "progressive-tax-before-tax-contributions",
    "bridge-depletes-before-access",
  ]) {
    const fixture = [...source.cases, ...baseline.cases].find(
      (c) => c.name === name,
    );
    const document = (await api(root + "/reporting")).documents.find(
      (d) => d.id === widgetId,
    );
    document.config.meta = { ...fixture.meta, name: "Scenario plan" };
    await api(root + "/reporting/commands", { writes: [document] }, "POST", {
      "X-Actual-Budget-Epoch": checkpoint.epoch,
    });
    await page.reload();
    await expect(
      widget.getByRole("button", { name: "Run simulation", exact: true }),
    ).toBeEnabled();
    await widget
      .getByRole("checkbox", {
        name: "Show results in today’s money",
        exact: true,
      })
      .setChecked(fixture.options.deflateToTodaysMoney ?? false);
    await widget
      .getByRole("button", { name: "Run simulation", exact: true })
      .click();
    await expect(
      widget.getByLabel("Median ending balance", { exact: true }),
    ).toHaveText(money(fixture.result.medianEndingBalance), { timeout: 30000 });
    if (fixture.options.captureRunDetail !== 0) {
      await widget
        .getByText("Individual simulation runs", { exact: true })
        .click();
      await inspect(fixture.options.captureRunDetail + 1);
    }
    const cash = cashBuilder({
      rows: fixture.result.runDetail,
      ...fixture.config,
      startAge: fixture.config.currentAge,
      translate,
    });
    expect(
      (
        await widget
          .getByRole("img", { name: "Annual cash flow", exact: true })
          .locator("rect title")
          .allTextContents()
      ).map((text) => text.replace(/\s+/g, " ").trim()),
    ).toEqual(
      cash.data.flatMap((point) =>
        [...cash.inflowSeries, ...cash.outflowSeries]
          .filter((s) => point.amounts[s.key] !== 0)
          .map(
            (s) =>
              `Age ${point.age} · ${s.label}: ${money(point.amounts[s.key])}`,
          ),
      ),
    );
    expect(await table.locator("tbody tr").count()).toBe(
      fixture.result.runDetail.length,
    );
    await widget.getByText("Year-by-year explanation", { exact: true }).click();
    const row =
      fixture.result.runDetail.find(
        (row) =>
          row.minimumApplied ||
          row.inaccessibleBalance ||
          row.ruleExplanation?.kind === "floor-ceiling",
      ) ?? fixture.result.runDetail[0];
    await widget
      .getByText(
        `Age ${fixture.config.currentAge + row.year - 1} · Spent: ${money(row.spent)}`,
        { exact: true },
      )
      .click();
    const sentences = storyBuilder({
      row,
      withdrawalRule: fixture.config.withdrawalRule,
      surplusPotName: "Surplus cash",
      hasSurplusPot: fixture.config.pots.some((p) => p.isSurplus),
      format: money,
      translate,
    });
    for (const sentence of sentences)
      await expect(widget.getByText(sentence, { exact: true })).toBeVisible();
    console.log("PASS source cash-flow series and year explanation", name);
  }
  expect(errors).toEqual([]);
  console.log(
    "PASS source ranking/detail, charts, frozen linked inputs, cached switching/reload/offline, mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
}

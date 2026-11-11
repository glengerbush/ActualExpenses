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
  const budget = randomUUID(),
    root = `/budgets/${budget}`,
    dashboard = randomUUID(),
    widgetId = randomUUID();
  await api("/budgets", { id: budget, name: "Withdrawal strategies" });
  await api(
    root + "/preferences",
    { experimental: { monteCarloReport: true } },
    "PATCH",
  );
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
            meta: { ...expected.meta, name: "Strategy plan" },
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
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (
      request.method() === "POST" &&
      request.url().endsWith("/reports/monte-carlo")
    )
      runs.push(request.postDataJSON());
  });
  await page.goto(server.url);
  await page
    .getByRole("button", { name: /Withdrawal strategies.*Open/ })
    .click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  const widget = page.getByRole("article", {
    name: "Strategy plan",
    exact: true,
  });
  const editor = page.getByRole("form", {
    name: "Reporting editor",
    exact: true,
  });
  const saved = async () =>
    (await api(root + "/reporting")).documents.find((d) => d.id === widgetId)
      .config.meta;
  const open = () =>
    widget
      .getByRole("button", { name: "Edit Strategy plan", exact: true })
      .click();
  const save = async () => {
    await editor
      .getByRole("button", { name: "Save widget", exact: true })
      .click();
    await expect(editor).toHaveCount(0);
  };
  await open();
  const strategy = editor.getByRole("combobox", {
    name: "Withdrawal strategy",
    exact: true,
  });
  await expect(strategy).toHaveValue("sequential");
  await expect(strategy.locator("option")).toHaveCount(4);
  await editor
    .getByRole("button", {
      name: "Move Investment pots 2 earlier",
      exact: true,
    })
    .click();
  await save();
  expect((await saved()).pots.map((p) => p.id)).toEqual(["bonds", "stocks"]);
  await open();
  await editor
    .getByRole("button", { name: "Move Investment pots 1 later", exact: true })
    .click();
  await save();
  expect((await saved()).pots.map((p) => p.id)).toEqual(["stocks", "bonds"]);

  const settings = [
    [
      "guardrails",
      "proportional",
      {
        "Capital preservation trigger (%)": ["preservationTriggerPct", 15],
        "Capital preservation cut (%)": ["preservationCutPct", 9],
        "Capital prosperity trigger (%)": ["prosperityTriggerPct", 12],
        "Capital prosperity raise (%)": ["prosperityIncreasePct", 7],
      },
    ],
    [
      "ratcheting",
      "sequential",
      {
        "Balance threshold multiple": ["balanceThresholdMultiple", 1.1],
        "Years in a row": ["consecutiveYears", 2],
        "Ratchet raise (%)": ["ratchetIncreasePct", 8],
      },
    ],
    [
      "floor-ceiling",
      "best-performer",
      {
        "Floor below planned amount (%)": ["floorPct", 18],
        "Ceiling above planned amount (%)": ["ceilingPct", 27],
      },
    ],
    [
      "boundaries",
      "target-mix",
      {
        "Upper withdrawal rate threshold (%)": ["upperRateThreshold", 5.5],
        "Cut when above the upper threshold (%)": ["upperCutPct", 13],
        "Lower withdrawal rate threshold (%)": ["lowerRateThreshold", 3.5],
        "Raise when below the lower threshold (%)": ["lowerIncreasePct", 6],
      },
    ],
  ];
  const retained = {};
  for (const [rule, mode, values] of settings) {
    await open();
    await strategy.selectOption(mode);
    await editor
      .getByRole("combobox", { name: "Withdrawal rule", exact: true })
      .selectOption(rule);
    for (const [label, [key, value]] of Object.entries(values)) {
      await editor.getByLabel(label, { exact: true }).fill(String(value));
      retained[key] = label.includes("(%)") ? value / 100 : value;
    }
    await save();
    const meta = await saved();
    expect(meta.withdrawalStrategy).toBe(mode);
    expect(meta.withdrawalRule).toMatchObject({ ...retained, type: rule });
  }
  expect(runs).toHaveLength(0);
  await page.reload();
  await open();
  await expect(strategy).toHaveValue("target-mix");
  await editor
    .getByRole("combobox", { name: "Withdrawal rule", exact: true })
    .selectOption("ratcheting");
  await editor.getByLabel("Years in a row", { exact: true }).fill("31");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toBeVisible();
  expect((await saved()).withdrawalRule.type).toBe("boundaries");
  await editor.getByLabel("Years in a row", { exact: true }).fill("2");
  await editor
    .getByRole("combobox", { name: "Withdrawal rule", exact: true })
    .selectOption("none");
  await strategy.selectOption("sequential");
  await save();
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
  expect(runs).toHaveLength(1);
  await open();
  await editor
    .getByRole("combobox", { name: "Withdrawal rule", exact: true })
    .selectOption("guardrails");
  await mkdir(new URL("../test-results/monte-carlo/", import.meta.url), {
    recursive: true,
  });
  await editor
    .getByRole("group", { name: "Withdrawals", exact: true })
    .screenshot({
      path: new URL(
        "../test-results/monte-carlo/strategies-desktop.png",
        import.meta.url,
      ).pathname,
    });
  await page.setViewportSize({ width: 390, height: 844 });
  await editor
    .getByRole("group", { name: "Withdrawals", exact: true })
    .screenshot({
      path: new URL(
        "../test-results/monte-carlo/strategies-mobile.png",
        import.meta.url,
      ).pathname,
    });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "PASS strategy choices, pot order, every rule parameter, persistence, limits, seeded output and mobile editor",
  );
} finally {
  await browser?.close();
  await server.stop();
}

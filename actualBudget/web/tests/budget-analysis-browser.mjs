// Public browser workflow; external supplied servers are honored.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
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
  const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    }),
    page = await context.newPage();
  page.on("pageerror", (error) => console.error("PAGEERROR", error.message));
  await page.clock.setFixedTime(new Date("2026-11-09T12:00:00Z"));
  await page.goto(server.url);
  const budget = randomUUID(),
    name = "Analysis browser " + budget;
  const api = (path, body, headers = {}) =>
    page.evaluate(
      async ({ path, body, headers, key }) => {
        const r = await fetch("/api/v1" + path, {
          method: body === undefined ? "GET" : "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key,
            ...headers,
          },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        if (!r.ok) throw Error(await r.text());
        return r.json();
      },
      { path, body, headers, key: randomUUID() },
    );
  const bytes = isolatedImportBytes(
    await readFile(
      new URL(
        "../../tests/fixtures/budget-analysis-envelope-source-26.10.zip",
        import.meta.url,
      ),
    ),
  ).toString("base64");
  const imported = await page.evaluate(
    async ({ bytes, key }) => {
      const r = await fetch("/api/v1/imports/actual", {
        method: "POST",
        headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
        body: Uint8Array.from(atob(bytes), (c) => c.charCodeAt(0)),
      });
      if (!r.ok) throw Error(await r.text());
      return r.json();
    },
    { bytes, key: randomUUID() },
  );
  expect(["ready", "activated"]).toContain(imported.data.status);
  if (imported.data.status === "ready")
    await api(`/imports/${imported.data.id}/activate`, {});
  const root = "/budgets/" + imported.data.budget_id,
    state = await api(root + "/reporting");
  await api(
    root + "/duplicate",
    { id: budget, name },
    { "X-Actual-Budget-Epoch": state.data.epoch },
  );
  await page.reload();
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Dashboard", exact: true })
    .selectOption({ label: "Household history" });
  const widget = page.getByRole("article", {
    name: "Saved budget analysis",
    exact: true,
  });
  await expect(
    widget.getByRole("region", { name: "Budget analysis result" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("table", { name: "Budget analysis values" }),
  ).toBeVisible();
  const oracle = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/budget-analysis-widget-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  ).cases.find((c) => c.method === "envelope");
  const money = (n) => (n / 100).toFixed(2),
    table = widget.getByRole("table", { name: "Budget analysis values" });
  for (const row of oracle.result.intervalData) {
    const cells = table.getByRole("row").filter({
      has: page.getByRole("rowheader", { name: row.date, exact: true }),
    });
    await expect(cells).toHaveText(
      row.date +
        money(row.budgeted) +
        money(row.spent) +
        money(row.overspendingAdjustment) +
        money(row.balance),
    );
  }
  const downloadPromise = page.waitForEvent("download");
  await widget
    .getByRole("button", { name: "Export as CSV", exact: true })
    .click();
  const download = await downloadPromise;
  expect(await readFile(await download.path(), "utf8")).toBe(oracle.csv);
  await widget
    .getByRole("button", {
      name: "Inspect planned allocations 2026-11",
      exact: true,
    })
    .click();
  const details = widget.getByRole("region", {
    name: "Budget analysis details",
    exact: true,
  });
  await expect(details).toContainText("Total: 17.00");
  await expect(details).toContainText("Planned category allocations");
  await widget
    .getByRole("button", {
      name: "Inspect recorded spending 2026-11",
      exact: true,
    })
    .click();
  await expect(details).toContainText("Total: -65.00");
  await expect(details).toContainText("Recorded leaf entries");
  await widget
    .getByRole("button", { name: "Close details", exact: true })
    .click();
  const before = (
    await api(`/budgets/${budget}/reporting`)
  ).data.documents.find((d) => d.id === oracle.widget.id);
  await widget
    .getByRole("button", { name: "Edit Saved budget analysis", exact: true })
    .click();
  const editor = page.getByRole("form", { name: "Reporting editor" });
  await expect(
    editor.getByRole("checkbox", { name: "Balance only", exact: true }),
  ).toBeChecked();
  await editor
    .getByRole("checkbox", { name: "Balance only", exact: true })
    .uncheck();
  await editor
    .getByRole("combobox", { name: "Chart type", exact: true })
    .selectOption("Bar");
  // Editing a source-name condition must preserve source operator/field semantics.
  await editor
    .getByRole("combobox", { name: "Report operator 1", exact: true })
    .selectOption("contains");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  await expect(table).toBeVisible();
  const saved = (await api(`/budgets/${budget}/reporting`)).data.documents.find(
    (d) => d.id === before.id,
  );
  expect(saved.config.meta.conditions).toEqual(before.config.meta.conditions);
  expect(saved.config.meta.timeFrame).toEqual(before.config.meta.timeFrame);
  expect(saved.config.meta.graphType).toBe("Bar");
  expect(saved.config.meta.balanceOnly).toBe(false);
  await context.route("**/reports/budget-analysis", (r) => r.abort());
  await widget
    .getByRole("button", { name: "Recalculate analysis", exact: true })
    .click();
  await expect(widget).toContainText("Cached analysis.");
  await expect(table).toBeVisible();
  await expect(
    widget.getByRole("button", {
      name: "Inspect recorded spending 2026-11",
      exact: true,
    }),
  ).toBeDisabled();
  await context.unroute("**/reports/budget-analysis");
  await widget
    .getByRole("button", { name: "Recalculate analysis", exact: true })
    .click();
  await expect(
    widget.getByText("Cached analysis.", { exact: false }),
  ).toHaveCount(0);
  await expect(table).toBeVisible();
  // Fault injection: optional view-cache writes fail, a fresh public calculation remains.
  await page.evaluate(() => {
    const original = IDBObjectStore.prototype.put;
    window.restoreAnalysisPut = () => (IDBObjectStore.prototype.put = original);
    IDBObjectStore.prototype.put = function (value, key) {
      if (typeof key === "string" && key.startsWith("view:budget-analysis:"))
        throw new DOMException("Injected analysis quota", "QuotaExceededError");
      return original.call(this, value, key);
    };
  });
  await widget
    .getByRole("button", { name: "Recalculate analysis", exact: true })
    .click();
  await expect(widget).toContainText(
    "Analysis calculated, but this browser could not save a cached copy.",
  );
  await expect(table).toBeVisible();
  await expect(
    widget.getByRole("button", {
      name: "Inspect recorded spending 2026-11",
      exact: true,
    }),
  ).toBeEnabled();
  await page.evaluate(() => window.restoreAnalysisPut());
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(widget).not.toContainText("-98.75");
  await expect(widget).toContainText("••••");
  await expect(widget.locator("svg")).toHaveCount(0);
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.keyboard.press("Control+k");
  await page
    .getByRole("searchbox", { name: "Search commands", exact: true })
    .fill("Budget Analysis");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("button", { name: "Budget Analysis", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "New analysis", exact: true }).click();
  await expect(
    editor.getByRole("combobox", { name: "Widget type", exact: true }),
  ).toHaveValue("budget-analysis-card");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  const created = page.getByRole("article", {
    name: "Budget Analysis",
    exact: true,
  });
  await expect(
    created.getByRole("table", { name: "Budget analysis values" }),
  ).toBeVisible();
  await created
    .getByRole("button", { name: "Remove Budget Analysis", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm remove", exact: true })
    .click();
  await expect(created).toHaveCount(0);
  const beforeOff = (await api(`/budgets/${budget}/reporting`)).data.documents;
  async function optIn(enabled) {
    await page
      .getByRole("button", { name: "Close reports", exact: true })
      .click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByText("Experimental features", { exact: true }).click();
    await page
      .getByRole("checkbox", { name: "Budget analysis", exact: true })
      .setChecked(enabled);
    await page
      .getByRole("button", { name: "Save shared preferences", exact: true })
      .click();
    await expect
      .poll(
        async () =>
          (await api(`/budgets/${budget}/preferences`)).data.experimental
            .budgetAnalysisReport,
      )
      .toBe(enabled);
    await page.getByRole("button", { name: "Done", exact: true }).click();
    await page
      .getByRole("button", { name: "Dashboards and reports", exact: true })
      .click();
  }
  await optIn(false);
  await expect(table).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "New analysis", exact: true }),
  ).toBeDisabled();
  expect((await api(`/budgets/${budget}/reporting`)).data.documents).toEqual(
    beforeOff,
  );
  await optIn(true);
  await expect(table).toBeVisible();
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await page.screenshot({
    path: new URL("../artifacts/analysis-desktop.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: new URL("../artifacts/analysis-mobile.png", import.meta.url).pathname,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Theme", exact: true })
    .selectOption("dark");
  await page.locator("#language").selectOption("de");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(
    widget.getByRole("columnheader", { name: "Budgetiert", exact: true }),
  ).toBeVisible();
  await expect(
    widget.getByRole("columnheader", { name: "Ausgegeben", exact: true }),
  ).toBeVisible();
  expect(
    (await api(`/budgets/${budget}/reporting`)).data.documents.find(
      (d) => d.id === saved.id,
    ),
  ).toEqual(saved);
  await widget
    .getByRole("button", {
      name: "Bearbeiten Saved budget analysis",
      exact: true,
    })
    .click();
  await expect(
    editor.getByRole("option", { name: "Balkendiagramm", exact: true }),
  ).toHaveCount(1);
  await page.screenshot({
    path: new URL("../artifacts/analysis-german-editor.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  await editor
    .getByRole("button", { name: "Discard local draft", exact: true })
    .click();
  await page.setViewportSize({ width: 640, height: 900 });
  await page.addStyleTag({ content: "html{font-size:200% !important}" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await widget
    .getByRole("button", { name: "Als CSV exportieren", exact: true })
    .focus();
  expect(await page.evaluate(() => document.activeElement?.tagName)).toBe(
    "BUTTON",
  );
  await page.screenshot({
    path: new URL("../artifacts/analysis-dark-large.png", import.meta.url)
      .pathname,
    fullPage: true,
  });
  console.log(
    "PASS source monthly/CSV, planned/recorded details, saved filter/chart settings, offline cache/reconnect, failed optional cache write, German invariance, mobile/dark/200%-root-text keyboard",
  );
} finally {
  await browser?.close();
  await server.stop();
}

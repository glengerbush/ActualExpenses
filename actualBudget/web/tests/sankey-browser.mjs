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
    name = "Sankey browser " + budget;
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
        "../../tests/fixtures/sankey-envelope-source-26.10.zip",
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
    name: "Saved money flow",
    exact: true,
  });
  await expect(
    widget.getByRole("region", { name: "Money flow result" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("table", { name: "Money flows" }),
  ).toBeVisible();
  const fixture = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/sankey-envelope-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const table = widget.getByRole("table", { name: "Money flows", exact: true });
  const money = (n) => (n / 100).toFixed(2);
  async function assertSource(name) {
    const result = fixture.cases.find((c) => c.name === name).result;
    const expected = result.links
      .filter(
        (l) =>
          l.source >= 0 &&
          l.target >= 0 &&
          !result.nodes[l.source].key.endsWith("__HIDDEN") &&
          !result.nodes[l.target].key.endsWith("__HIDDEN"),
      )
      .map((l) => [
        result.nodes[l.source].name,
        result.nodes[l.target].name,
        money(l.value),
      ]);
    await expect
      .poll(async () =>
        table.locator("tbody tr").evaluateAll((rows) =>
          rows.map((r) =>
            Array.from(r.cells)
              .slice(0, 3)
              .map((c) => c.textContent),
          ),
        ),
      )
      .toEqual(expected);
  }
  await assertSource("cross-month-peer");
  const before = (
    await api(`/budgets/${budget}/reporting`)
  ).data.documents.find((d) => d.id === fixture.widgets[0].id);
  const editor = page.getByRole("form", {
    name: "Reporting editor",
    exact: true,
  });
  async function edit() {
    await widget
      .getByRole("button", { name: "Edit Saved money flow", exact: true })
      .click();
  }
  async function save() {
    await editor
      .getByRole("button", { name: "Save widget", exact: true })
      .click();
    await expect(editor).toHaveCount(0);
  }
  await edit();
  await editor
    .getByRole("textbox", { name: "Start month", exact: true })
    .fill("2026-11");
  await editor
    .getByRole("textbox", { name: "End month", exact: true })
    .fill("2026-11");
  await editor
    .getByRole("spinbutton", { name: "Maximum nodes per layer", exact: true })
    .fill("1");
  await editor
    .getByRole("combobox", { name: "Sorting", exact: true })
    .selectOption("global");
  await save();
  await assertSource("small-global");
  const groupRow = table
    .locator("tbody tr")
    .filter({ hasText: "76.50" })
    .filter({
      has: page.getByRole("button", {
        name: "Inspect flow Other → Other",
        exact: true,
      }),
    })
    .first();
  await groupRow
    .getByRole("button", { name: "Inspect flow Other → Other", exact: true })
    .focus();
  await page.keyboard.press("Enter");
  const details = widget.getByRole("region", {
    name: "Flow records",
    exact: true,
  });
  await expect(details).toContainText(
    "Signed contributions to this flow: 76.50",
  );
  await expect(
    details.getByRole("table", { name: "Flow contributions", exact: true }),
  ).toBeVisible();
  await expect(details).toContainText("Food café ? %");
  await groupRow.getByText("Other breakdown", { exact: true }).click();
  await expect(groupRow).toContainText("Food café ? %: 48.00");
  await widget
    .getByRole("button", { name: "Close details", exact: true })
    .click();
  await edit();
  await editor
    .getByRole("combobox", { name: "View mode", exact: true })
    .selectOption("budgeted");
  await expect(
    editor.getByRole("combobox", { name: "From layer", exact: true }),
  ).toHaveValue("income_category");
  await editor
    .getByRole("spinbutton", { name: "Maximum nodes per layer", exact: true })
    .fill("15");
  await editor
    .getByRole("combobox", { name: "Sorting", exact: true })
    .selectOption("per-group");
  await save();
  await assertSource("budgeted");
  await table
    .getByRole("button", { name: /Inspect flow/ })
    .first()
    .click();
  await expect(details).toContainText("Budget context rows are derived");
  await widget
    .getByRole("button", { name: "Close details", exact: true })
    .click();
  const saved = (await api(`/budgets/${budget}/reporting`)).data.documents.find(
    (d) => d.id === before.id,
  );
  expect(saved.config.meta.showTransfers).toBe(true);
  expect(saved.config.meta.groupAccounts).toBe(
    before.config.meta.groupAccounts,
  );
  expect(saved.config.meta.conditions).toEqual(before.config.meta.conditions);
  expect(saved.config.meta.timeFrame).toEqual({
    ...before.config.meta.timeFrame,
    start: "2026-11",
    end: "2026-11",
  });
  await context.route("**/reports/sankey", (r) => r.abort());
  await widget
    .getByRole("button", { name: "Recalculate flow", exact: true })
    .click();
  await expect(widget).toContainText("Cached flow.");
  await expect(
    table.getByRole("button", { name: /Inspect flow/ }).first(),
  ).toBeDisabled();
  await context.unroute("**/reports/sankey");
  await widget
    .getByRole("button", { name: "Recalculate flow", exact: true })
    .click();
  await expect(widget.getByText("Cached flow.", { exact: false })).toHaveCount(
    0,
  );
  // Fault injection affects optional browser-cache writes only.
  await page.evaluate(() => {
    const original = IDBObjectStore.prototype.put;
    window.restoreSankeyPut = () => (IDBObjectStore.prototype.put = original);
    IDBObjectStore.prototype.put = function (value, key) {
      if (typeof key === "string" && key.startsWith("view:sankey:"))
        throw new DOMException("Injected Sankey quota", "QuotaExceededError");
      return original.call(this, value, key);
    };
  });
  await widget
    .getByRole("button", { name: "Recalculate flow", exact: true })
    .click();
  await expect(widget).toContainText(
    "Flow calculated, but this browser could not save a cached copy.",
  );
  await assertSource("budgeted");
  await expect(
    table.getByRole("button", { name: /Inspect flow/ }).first(),
  ).toBeEnabled();
  await page.evaluate(() => window.restoreSankeyPut());
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  await assertSource("budgeted");
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await widget.scrollIntoViewIfNeeded();
  await widget.screenshot({
    path: new URL("../artifacts/sankey-desktop.png", import.meta.url).pathname,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await widget.screenshot({
    path: new URL("../artifacts/sankey-mobile.png", import.meta.url).pathname,
  });
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.keyboard.press("Control+k");
  await page
    .getByRole("searchbox", { name: "Search commands", exact: true })
    .fill("Sankey");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("button", { name: "Sankey", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "New flow", exact: true }).click();
  await expect(
    editor.getByRole("combobox", { name: "Widget type", exact: true }),
  ).toHaveValue("sankey-card");
  // Save reaches the server but its response is lost; reload retries the same intent.
  let originalSave,
    deliveredSave = false;
  await context.route("**/reporting/commands", async (route) => {
    originalSave = {
      key: route.request().headers()["idempotency-key"],
      body: route.request().postDataJSON(),
      epoch: route.request().headers()["x-actual-budget-epoch"],
    };
    const forwarded = new URL(route.request().url());
    const headers = { ...route.request().headers(), host: forwarded.host };
    if (forwarded.hostname === "actualbudget.test")
      forwarded.hostname = "127.0.0.1";
    const delivered = await route.fetch({ url: forwarded.toString(), headers });
    expect(delivered.status()).toBe(200);
    await route.abort();
    deliveredSave = true;
  });
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Saved reporting change", exact: true }),
  ).toBeVisible();
  await expect.poll(() => deliveredSave).toBe(true);
  await context.unroute("**/reporting/commands");
  await page.reload();
  // The remembered reporting destination reopens automatically.
  await expect(
    page.getByRole("region", { name: "Saved reporting change", exact: true }),
  ).toBeVisible();
  let retried;
  await context.route("**/reporting/commands", async (route) => {
    retried = {
      key: route.request().headers()["idempotency-key"],
      body: route.request().postDataJSON(),
      epoch: route.request().headers()["x-actual-budget-epoch"],
    };
    await route.continue();
  });
  await page
    .getByRole("button", { name: "Retry saved reporting change", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Saved reporting change", exact: true }),
  ).toHaveCount(0);
  expect(retried).toEqual(originalSave);
  await context.unroute("**/reporting/commands");
  const created = page.getByRole("article", { name: "Sankey", exact: true });
  await expect(
    created.getByRole("region", { name: "Money flow result", exact: true }),
  ).toBeVisible();
  await created
    .getByRole("button", { name: "Remove Sankey", exact: true })
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
      .getByRole("checkbox", { name: "Sankey report", exact: true })
      .setChecked(enabled);
    await page
      .getByRole("button", { name: "Save shared preferences", exact: true })
      .click();
    await expect
      .poll(
        async () =>
          (await api(`/budgets/${budget}/preferences`)).data.experimental
            .sankeyReport,
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
    page.getByRole("button", { name: "New flow", exact: true }),
  ).toBeDisabled();
  expect((await api(`/budgets/${budget}/reporting`)).data.documents).toEqual(
    beforeOff,
  );
  await optIn(true);
  await expect(table).toBeVisible();
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Theme", exact: true })
    .selectOption("dark");
  await page.locator("#date-format").selectOption("dd.MM.yyyy");
  await page
    .getByRole("button", { name: "Save shared preferences", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await api(`/budgets/${budget}/preferences`)).data.date_format,
    )
    .toBe("dd.MM.yyyy");
  await page.locator("#language").selectOption("de");
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(
    widget.getByRole("columnheader", { name: "Von", exact: true }),
  ).toBeVisible();
  await expect(widget).toContainText("09.11.2026 · 01.11.2026–30.11.2026");
  expect(
    (await api(`/budgets/${budget}/reporting`)).data.documents.find(
      (d) => d.id === saved.id,
    ),
  ).toEqual(saved);
  await widget
    .getByRole("button", { name: "Bearbeiten Saved money flow", exact: true })
    .click();
  await expect(
    editor.getByRole("option", { name: "Alle sortieren", exact: true }),
  ).toHaveCount(1);
  await expect(
    editor.getByRole("option", { name: "Budgetiert", exact: true }),
  ).toHaveCount(1);
  await page.screenshot({
    path: new URL("../artifacts/sankey-german-editor.png", import.meta.url)
      .pathname,
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
    .getByRole("button", { name: "Recalculate flow", exact: true })
    .focus();
  expect(await page.evaluate(() => document.activeElement?.tagName)).toBe(
    "BUTTON",
  );
  await widget.screenshot({
    path: new URL("../artifacts/sankey-dark-large.png", import.meta.url)
      .pathname,
  });
  console.log(
    "PASS source-exact imported/editable Spent/Budgeted/grouped flows; record details, cache failure/fallback, navigation, opt-out, German config invariance and responsive keyboard renders",
  );
} finally {
  await browser?.close();
  await server.stop();
}

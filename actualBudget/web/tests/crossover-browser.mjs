// Public browser workflow; external supplied servers are honored.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { waitForReportingDraft } from "./reporting-draft.mjs";
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
    name = "Crossover browser " + budget;
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
        "../../tests/fixtures/crossover-source-26.10.zip",
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
    name: "Saved crossover café",
    exact: true,
  });
  await expect(
    widget.getByRole("region", { name: "Crossover result" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("table", { name: "Crossover values" }),
  ).toBeVisible();
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/crossover-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  await page
    .getByRole("button", { name: "Crossover Point", exact: true })
    .click();
  const table = widget.getByRole("table", {
    name: "Crossover values",
    exact: true,
  });
  const money = (n) =>
    new Intl.NumberFormat("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(n / 100);
  async function sourceRows(name) {
    const c = source.cases.find((c) => c.name === name);
    const expected = c.result.graphData.data
      .slice(0, 50)
      .map((row) => [
        row.x,
        row.isProjection ? "(projected)" : "Historical",
        money(row.investmentIncome),
        money(row.expenses),
        row.adjustedExpenses === undefined
          ? "Not applicable"
          : money(row.adjustedExpenses),
        money(row.nestEgg),
      ]);
    await expect
      .poll(async () =>
        table
          .locator("tbody tr")
          .evaluateAll((rows) =>
            rows.map((row) => Array.from(row.cells).map((c) => c.textContent)),
          ),
      )
      .toEqual(expected);
  }
  await sourceRows("saved-hampel");
  await widget
    .getByRole("button", { name: "Next months", exact: true })
    .click();
  await expect(table.getByRole("rowheader").first()).toHaveText(
    source.cases[0].result.graphData.data[50].x,
  );
  await widget
    .getByRole("button", { name: "Previous months", exact: true })
    .click();
  const before = (
    await api(`/budgets/${budget}/reporting`)
  ).data.documents.find((d) => d.id === source.widgets[0].id);
  const editor = page.getByRole("form", {
    name: "Reporting editor",
    exact: true,
  });
  const edit = () =>
    widget
      .getByRole("button", { name: "Edit Saved crossover café", exact: true })
      .click();
  async function save() {
    await editor
      .getByRole("button", { name: "Save widget", exact: true })
      .click();
    await expect(editor).toHaveCount(0);
  }
  // Fractional percentages remain editable and empty drafts survive a reload.
  await edit();
  const rate = editor.getByRole("spinbutton", {
    name: "Safe withdrawal rate (%)",
    exact: true,
  });
  const adjustment = editor.getByRole("spinbutton", {
    name: "Target Income (% of expenses)",
    exact: true,
  });
  const growth = editor.getByRole("spinbutton", {
    name: "Expected return (annual %)",
    exact: true,
  });
  await rate.fill("4.125");
  await adjustment.fill("110.25");
  await growth.fill("5.125");
  await save();
  await edit();
  await expect(rate).toHaveValue("4.125");
  await expect(adjustment).toHaveValue("110.25");
  await expect(growth).toHaveValue("5.125");
  await rate.fill("");
  await waitForReportingDraft(page, budget, { safeWithdrawalRate: "" });
  await page.reload();
  await expect(editor).toBeVisible();
  await expect(rate).toHaveValue("");
  expect(await rate.evaluate((input) => input.checkValidity())).toBe(false);
  await rate.fill(String(before.config.meta.safeWithdrawalRate * 100));
  await adjustment.fill("110");
  await growth.fill("5");
  await save();
  await sourceRows("saved-hampel");
  await edit();
  await expect(
    editor.getByRole("combobox", {
      name: "Expense Projection Type",
      exact: true,
    }),
  ).toHaveValue("hampel");
  await expect(
    editor.getByRole("textbox", {
      name: "Expected monthly contribution",
      exact: true,
    }),
  ).toHaveValue("100.00");
  await expect
    .poll(async () =>
      editor
        .getByRole("listbox", { name: "Expenses categories", exact: true })
        .evaluate((select) =>
          Array.from(select.selectedOptions)
            .map((option) => option.value)
            .sort(),
        ),
    )
    .toEqual([...source.widgets[0].meta.expenseCategoryIds].sort());
  await editor
    .getByRole("spinbutton", { name: "Safe withdrawal rate (%)", exact: true })
    .focus();
  await page.keyboard.press("Tab");
  await expect(
    editor.getByRole("combobox", {
      name: "Expense Projection Type",
      exact: true,
    }),
  ).toBeFocused();
  await editor
    .getByRole("combobox", { name: "Expense Projection Type", exact: true })
    .selectOption("mean");
  await save();
  await sourceRows("mean-contribution");
  let saved = (await api(`/budgets/${budget}/reporting`)).data.documents.find(
    (d) => d.id === before.id,
  );
  expect(saved.config.meta).toEqual({
    ...before.config.meta,
    projectionType: "mean",
  });
  await edit();
  await editor
    .getByRole("combobox", { name: "Expense Projection Type", exact: true })
    .selectOption("median");
  await editor
    .getByRole("spinbutton", {
      name: "Target Income (% of expenses)",
      exact: true,
    })
    .fill("100");
  await save();
  await sourceRows("median-no-adjustment");
  await edit();
  await editor
    .getByRole("combobox", { name: "Expense Projection Type", exact: true })
    .selectOption("hampel");
  await editor
    .getByRole("spinbutton", {
      name: "Target Income (% of expenses)",
      exact: true,
    })
    .fill("110");
  await editor
    .getByRole("checkbox", {
      name: "Use custom growth projections",
      exact: true,
    })
    .uncheck();
  await save();
  await sourceRows("historical-growth");
  await edit();
  await editor
    .getByRole("checkbox", {
      name: "Use custom growth projections",
      exact: true,
    })
    .check();
  await editor
    .getByRole("spinbutton", {
      name: "Expected return (annual %)",
      exact: true,
    })
    .fill("5");
  await editor
    .getByRole("textbox", {
      name: "Expected monthly contribution",
      exact: true,
    })
    .fill("100.00");
  await save();
  await sourceRows("saved-hampel");
  for (const mode of ["full", "sliding-window"]) {
    await edit();
    await editor
      .getByRole("combobox", { name: "Date mode", exact: true })
      .selectOption(mode);
    await save();
    await sourceRows("saved-hampel");
  }
  await edit();
  await editor
    .getByRole("combobox", { name: "Date mode", exact: true })
    .selectOption("static");
  await editor
    .getByRole("textbox", { name: "End month", exact: true })
    .fill("2027-03");
  await save();
  await sourceRows("saved-hampel");
  saved = (await api(`/budgets/${budget}/reporting`)).data.documents.find(
    (d) => d.id === before.id,
  );
  expect(saved.config.meta.timeFrame.end).toBe("2027-03");
  // Invalid financial drafts retain their entered text across reload and cannot be saved.
  await edit();
  await editor
    .getByRole("textbox", {
      name: "Expected monthly contribution",
      exact: true,
    })
    .fill("not money");
  await expect
    .poll(async () =>
      editor
        .getByRole("textbox", {
          name: "Expected monthly contribution",
          exact: true,
        })
        .evaluate((input) => input.validationMessage),
    )
    .not.toBe("");
  await waitForReportingDraft(page, budget, {
    expectedContribution: "not money",
  });
  await page.reload();
  await expect(editor).toBeVisible();
  await expect(
    editor.getByRole("textbox", {
      name: "Expected monthly contribution",
      exact: true,
    }),
  ).toHaveValue("not money");
  await editor
    .getByRole("textbox", {
      name: "Expected monthly contribution",
      exact: true,
    })
    .fill("100.00");
  await save();
  await sourceRows("saved-hampel");
  await context.route("**/reports/crossover", (route) => route.abort());
  await widget
    .getByRole("button", { name: "Recalculate crossover", exact: true })
    .click();
  await expect(widget).toContainText("Cached projection.");
  await expect(table).toBeVisible();
  await context.unroute("**/reports/crossover");
  await widget
    .getByRole("button", { name: "Recalculate crossover", exact: true })
    .click();
  await expect(
    widget.getByText("Cached projection.", { exact: false }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    const original = IDBObjectStore.prototype.put;
    window.restoreCrossoverPut = () =>
      (IDBObjectStore.prototype.put = original);
    IDBObjectStore.prototype.put = function (value, key) {
      if (typeof key === "string" && key.startsWith("view:crossover:"))
        throw new DOMException(
          "Injected Crossover quota",
          "QuotaExceededError",
        );
      return original.call(this, value, key);
    };
  });
  await widget
    .getByRole("button", { name: "Recalculate crossover", exact: true })
    .click();
  await expect(widget).toContainText(
    "Projection calculated, but this browser could not save a cached copy.",
  );
  await sourceRows("saved-hampel");
  await page.evaluate(() => window.restoreCrossoverPut());
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  await sourceRows("saved-hampel");
  await mkdir(new URL("../artifacts/", import.meta.url), { recursive: true });
  await widget
    .getByRole("checkbox", { name: "Show projected months only", exact: true })
    .check();
  await expect(widget).toContainText("Only projected months are drawn.");
  while (
    await widget
      .getByRole("button", { name: "Next months", exact: true })
      .isEnabled()
  )
    await widget
      .getByRole("button", { name: "Next months", exact: true })
      .click();
  await widget.screenshot({
    path: new URL("../artifacts/crossover-desktop.png", import.meta.url)
      .pathname,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  while (
    await widget
      .getByRole("button", { name: "Next months", exact: true })
      .isEnabled()
  )
    await widget
      .getByRole("button", { name: "Next months", exact: true })
      .click();
  await widget.screenshot({
    path: new URL("../artifacts/crossover-mobile.png", import.meta.url)
      .pathname,
  });
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.keyboard.press("Control+k");
  await page
    .getByRole("searchbox", { name: "Search commands", exact: true })
    .fill("Crossover Point");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("button", { name: "Crossover Point", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page
    .getByRole("button", { name: "New crossover", exact: true })
    .click();
  await expect(
    editor.getByRole("combobox", { name: "Widget type", exact: true }),
  ).toHaveValue("crossover-card");
  await expect(
    editor.getByRole("combobox", { name: "Date mode", exact: true }),
  ).toHaveValue("full");
  await save();
  const created = page.getByRole("article", {
    name: "Crossover Point",
    exact: true,
  });
  await expect(
    created.getByRole("table", { name: "Crossover values", exact: true }),
  ).toBeVisible();
  await created
    .getByRole("button", { name: "Remove Crossover Point", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirm remove", exact: true })
    .click();
  await expect(created).toHaveCount(0);
  saved = (await api(`/budgets/${budget}/reporting`)).data.documents.find(
    (d) => d.id === before.id,
  );
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
  await expect(widget).toContainText("09.11.2026 · 01.11.2025–31.10.2026");
  await expect(widget).toContainText("14,000.00");
  expect(
    (await api(`/budgets/${budget}/reporting`)).data.documents.find(
      (d) => d.id === saved.id,
    ),
  ).toEqual(saved);
  await widget
    .getByRole("button", {
      name: "Bearbeiten Saved crossover café",
      exact: true,
    })
    .click();
  await expect(
    editor.getByRole("option", { name: "Mittelwert", exact: true }),
  ).toHaveCount(1);
  await expect(
    editor.getByRole("option", { name: "Hampel-Filter-Median", exact: true }),
  ).toHaveCount(1);
  await editor.screenshot({
    path: new URL("../artifacts/crossover-german-editor.png", import.meta.url)
      .pathname,
  });
  await editor
    .getByRole("button", { name: "Discard local draft", exact: true })
    .click();
  await widget
    .getByRole("checkbox", { name: "Show projected months only", exact: true })
    .check();
  await page.setViewportSize({ width: 640, height: 900 });
  await page.addStyleTag({ content: "html{font-size:200% !important}" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  while (
    await widget
      .getByRole("button", { name: "Next months", exact: true })
      .isEnabled()
  )
    await widget
      .getByRole("button", { name: "Next months", exact: true })
      .click();
  await widget
    .getByRole("button", { name: "Recalculate crossover", exact: true })
    .focus();
  expect(await page.evaluate(() => document.activeElement?.tagName)).toBe(
    "BUTTON",
  );
  await widget.screenshot({
    path: new URL("../artifacts/crossover-dark-large.png", import.meta.url)
      .pathname,
  });
  console.log(
    "PASS whole-source saved crossover controls/financial rows, methods/growth/empty/range semantics, invalid draft reload, cache/quota/navigation/defaults, German/date invariance and responsive keyboard renders",
  );
} finally {
  await browser?.close();
  await server.stop();
}

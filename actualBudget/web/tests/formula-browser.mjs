import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
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
    const r = await fetch(base + path, {
      method,
      headers: {
        "Content-Type": Buffer.isBuffer(body)
          ? "application/zip"
          : "application/json",
        "Idempotency-Key": randomUUID(),
        ...headers,
      },
      body:
        body === undefined
          ? undefined
          : Buffer.isBuffer(body)
            ? body
            : JSON.stringify(body),
    });
    const value = await r.json();
    if (!r.ok) throw Error(JSON.stringify(value));
    return value.data;
  }
  const imported = await api(
    "/imports/actual",
    isolatedImportBytes(
      await readFile(
        new URL(
          "../../tests/fixtures/reports-source-26.10.zip",
          import.meta.url,
        ),
      ),
    ),
  );
  if (imported.status === "ready")
    await api(`/imports/${imported.id}/activate`, {});
  const state = await api(`/budgets/${imported.budget_id}/reporting`),
    budget = randomUUID(),
    name = "Formula browser " + budget;
  await api(
    `/budgets/${imported.budget_id}/duplicate`,
    { id: budget, name },
    "POST",
    { "X-Actual-Budget-Epoch": state.epoch },
  );
  const source = JSON.parse(
      await readFile(
        new URL("../../tests/fixtures/formula-source.json", import.meta.url),
      ),
    ),
    food = source.observations.find((o) => o.name === "query-filtered"),
    root = `/budgets/${budget}`;
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
    page = await context.newPage(),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.clock.setFixedTime(new Date("2026-11-09T12:00:00Z"));
  await page.goto(server.url);
  await page.getByRole("button", { name: new RegExp(name + ".*Open") }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByText("Experimental features", { exact: true }).click();
  await page
    .getByRole("checkbox", { name: "Formula mode", exact: true })
    .check();
  await page
    .getByRole("button", { name: "Save shared preferences", exact: true })
    .click();
  await expect
    .poll(
      async () => (await api(root + "/preferences")).experimental.formulaMode,
    )
    .toBe(true);
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.keyboard.press("Control+k");
  await page
    .getByRole("searchbox", { name: "Search commands", exact: true })
    .fill("Formula");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("button", { name: "Formula", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "New formula", exact: true }).click();
  const editor = page.getByRole("form", {
    name: "Reporting editor",
    exact: true,
  });
  await editor
    .getByRole("textbox", { name: "Widget name", exact: true })
    .fill("Food formula");
  await editor
    .getByRole("textbox", { name: "Formula", exact: true })
    .fill(food.formula);
  await editor
    .getByRole("textbox", { name: "Color formula", exact: true })
    .fill('=IF(RESULT<0,"#a00","#060")');
  await editor
    .getByRole("textbox", { name: "New query name", exact: true })
    .fill("Food");
  await editor.getByRole("button", { name: "Add Query", exact: true }).click();
  await editor
    .getByText("Import or export query configuration", { exact: true })
    .click();
  await editor
    .getByRole("textbox", { name: "Query configuration JSON", exact: true })
    .fill(JSON.stringify(food.queries.Food));
  await editor
    .getByRole("button", { name: "Import query configuration", exact: true })
    .click();
  await editor
    .getByRole("button", { name: "Calculate preview", exact: true })
    .click();
  const expected = Number(food.output.result).toFixed(2);
  await expect(
    editor.getByLabel("Calculated value", { exact: true }),
  ).toHaveText(expected);
  expect(
    await editor
      .getByLabel("Calculated value", { exact: true })
      .evaluate((e) => getComputedStyle(e).color),
  ).toBe("rgb(170, 0, 0)");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  const widget = page.getByRole("article", {
      name: "Food formula",
      exact: true,
    }),
    value = widget.getByLabel("Calculated value", { exact: true });
  await expect(value).toHaveText(expected);
  const saved = (await api(root + "/reporting")).documents.find(
    (d) => d.config.meta?.name === "Food formula",
  );
  expect(saved.config.meta.queries.Food).toEqual(food.queries.Food);
  const edit = () =>
    widget
      .getByRole("button", { name: "Edit Food formula", exact: true })
      .click();
  await edit();
  await editor
    .getByRole("textbox", { name: "Formula", exact: true })
    .fill("=NOT_A_FUNCTION(1)");
  await page.reload();
  await expect(
    editor.getByRole("textbox", { name: "Formula", exact: true }),
  ).toHaveValue("=NOT_A_FUNCTION(1)");
  await editor
    .getByRole("button", { name: "Calculate preview", exact: true })
    .click();
  await expect(editor.getByRole("alert")).toContainText("Formula error: NAME");
  await editor
    .getByRole("textbox", { name: "Formula", exact: true })
    .fill(food.formula);
  await editor
    .getByRole("combobox", { name: "Edit query", exact: true })
    .selectOption("Food");
  await editor
    .getByText("Import or export query configuration", { exact: true })
    .click();
  await editor
    .getByRole("button", { name: "Export query configuration", exact: true })
    .click();
  expect(
    JSON.parse(
      await editor
        .getByRole("textbox", { name: "Query configuration JSON", exact: true })
        .inputValue(),
    ),
  ).toEqual(food.queries.Food);
  await editor
    .getByRole("combobox", { name: "Font size mode", exact: true })
    .selectOption("static");
  await editor
    .getByRole("spinbutton", { name: "Font size", exact: true })
    .fill("36");
  await editor
    .getByRole("spinbutton", { name: "Font size", exact: true })
    .fill("");
  await page.reload();
  await expect(
    editor.getByRole("spinbutton", { name: "Font size", exact: true }),
  ).toHaveValue("");
  expect(
    await editor
      .getByRole("spinbutton", { name: "Font size", exact: true })
      .evaluate((input) => input.checkValidity()),
  ).toBe(false);
  await editor
    .getByRole("spinbutton", { name: "Font size", exact: true })
    .fill("36");
  await editor
    .getByRole("checkbox", { name: "Show title", exact: true })
    .uncheck();
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  await expect(value).toHaveText(expected);
  expect(
    await widget
      .getByRole("heading", { name: "Food formula", exact: true })
      .evaluate((e) => e.classList.contains("sr-only")),
  ).toBe(true);
  expect(await value.evaluate((e) => getComputedStyle(e).fontSize)).toBe(
    "36px",
  );
  await context.route("**/reports/formula", (route) => route.abort());
  await widget
    .getByRole("button", { name: "Recalculate formula", exact: true })
    .click();
  await expect(widget).toContainText("Cached formula.");
  await expect(value).toHaveText(expected);
  await context.unroute("**/reports/formula");
  await widget
    .getByRole("button", { name: "Recalculate formula", exact: true })
    .click();
  await expect(
    widget.getByText("Cached formula.", { exact: false }),
  ).toHaveCount(0);
  const checkpoint = await api(root + "/checkpoint");
  await context.route("**/checkpoint", (route) =>
    route.fulfill({
      json: { data: { ...checkpoint, snapshot: "c1" + "f".repeat(48) } },
    }),
  );
  await widget
    .getByRole("button", { name: "Recalculate formula", exact: true })
    .click();
  await expect(widget).toContainText("The budget version changed.");
  await expect(value).toHaveCount(0);
  await context.unroute("**/checkpoint");
  await widget
    .getByRole("button", { name: "Recalculate formula", exact: true })
    .click();
  await expect(value).toHaveText(expected);
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(value).toHaveText("••••");
  expect(await value.evaluate((e) => e.style.color)).toBe("");
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(value).toHaveText(expected);
  await expect(
    widget.getByRole("button", { name: "Recalculate formula", exact: true }),
  ).toBeEnabled();
  await mkdir(new URL("../test-results/formula/", import.meta.url), {
    recursive: true,
  });
  await widget.screenshot({
    path: new URL("../test-results/formula/desktop.png", import.meta.url)
      .pathname,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await edit();
  await editor
    .getByRole("combobox", { name: "Edit query", exact: true })
    .selectOption("Food");
  await expect(
    editor.getByRole("button", { name: "Add report condition", exact: true }),
  ).toBeEnabled();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await editor.screenshot({
    path: new URL("../test-results/formula/mobile-editor.png", import.meta.url)
      .pathname,
  });
  await editor
    .getByRole("button", { name: "Calculate preview", exact: true })
    .focus();
  await page.keyboard.press("Enter");
  await expect(
    editor.getByLabel("Calculated value", { exact: true }),
  ).toHaveText(expected);
  expect(errors).toEqual([]);
  console.log(
    "PASS formula opt-in/command, visual query import/export, exact source result/color, durable draft/error recovery, title/font, cache/privacy and responsive keyboard editor",
  );
} finally {
  await browser?.close();
  await server.stop();
}

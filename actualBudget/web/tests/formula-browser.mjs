import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { waitForReportingDraft } from "./reporting-draft.mjs";
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
    errors = [],
    httpFailures = [];
  const colorRequests = [];
  page.on("request", (request) => {
    if (request.url().endsWith("/reports/formula")) {
      const input = request.postDataJSON();
      if (input.named_expressions) colorRequests.push(input);
    }
  });
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("response", (response) => {
    if (response.status() >= 500)
      httpFailures.push(
        `${response.status()} ${new URL(response.url()).pathname}`,
      );
  });
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
  await waitForReportingDraft(page, budget, { formula: "=NOT_A_FUNCTION(1)" });
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
  await waitForReportingDraft(page, budget, { staticFontSize: "" });
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
  await page.evaluate(() => {
    globalThis.formulaOriginalPut = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (value, key) {
      if (typeof key === "string" && key.startsWith("view:formula:"))
        throw new DOMException("Formula cache is full", "QuotaExceededError");
      return globalThis.formulaOriginalPut.call(this, value, key);
    };
  });
  await widget
    .getByRole("button", { name: "Recalculate formula", exact: true })
    .click();
  await expect(widget).toContainText(
    "Calculated, but this browser could not save a cached copy.",
  );
  await expect(value).toHaveText(expected);
  await page.evaluate(() => {
    IDBObjectStore.prototype.put = globalThis.formulaOriginalPut;
    delete globalThis.formulaOriginalPut;
  });
  await mkdir(new URL("../test-results/formula/", import.meta.url), {
    recursive: true,
  });
  // Run the unchanged source hook against actual source CSS in an isolated
  // iframe; only imports/React useMemo are adapted for this browser witness.
  const reference = new URL(
    "../../reference/actual-26.10.0/packages/",
    import.meta.url,
  );
  const themeModule = await readFile(
    new URL("component-library/src/theme.ts", reference),
    "utf8",
  );
  const themeHook = await readFile(
    new URL("desktop-client/src/hooks/useThemeColors.ts", reference),
    "utf8",
  );
  const sourceCss = {};
  for (const mode of [
    "palette",
    "light",
    "dark",
    "midnight",
    "sidebar-redesign-light",
  ])
    sourceCss[mode] = await readFile(
      new URL(`component-library/src/themes/${mode}.css`, reference),
      "utf8",
    );
  async function sourceColors(mode, override = "") {
    return page.evaluate(
      ({ themeModule, themeHook, css }) => {
        const frame = document.createElement("iframe");
        frame.hidden = true;
        document.body.append(frame);
        try {
          const doc = frame.contentDocument,
            win = frame.contentWindow;
          const style = doc.createElement("style");
          style.textContent = css;
          doc.head.append(style);
          const theme = new Function(
            themeModule.replace("export const", "const") + "\nreturn theme;",
          )();
          const body = themeHook
            .replace(/import[\s\S]*?;\s*/g, "")
            .replace(
              "export function useThemeColors(): Record<string, string>",
              "function useThemeColors()",
            )
            .replace("const colors: Record<string, string>", "const colors")
            .replace("varString: string): string", "varString)");
          return new Function(
            "window",
            "document",
            "useMemo",
            "theme",
            body + "\nreturn useThemeColors();",
          )(win, doc, (fn) => fn(), theme);
        } finally {
          frame.remove();
        }
      },
      {
        themeModule,
        themeHook,
        css:
          sourceCss.palette +
          sourceCss[mode] +
          (mode === "light" ? sourceCss["sidebar-redesign-light"] : "") +
          override,
      },
    );
  }
  async function assertColors(mode, override = "") {
    const expectedColors = await sourceColors(mode, override);
    const actual = { ...colorRequests.at(-1).named_expressions };
    delete actual.RESULT;
    expect(actual).toEqual(
      Object.fromEntries(
        Object.entries(expectedColors).map(([key, color]) => [
          "theme_" + key,
          color,
        ]),
      ),
    );
  }
  for (const mode of ["light", "dark", "midnight"]) {
    await page
      .getByRole("button", { name: "Close reports", exact: true })
      .click();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.locator("#theme").selectOption(mode);
    const before = colorRequests.length;
    await page.getByRole("button", { name: "Done", exact: true }).click();
    await page
      .getByRole("button", { name: "Dashboards and reports", exact: true })
      .click();
    await expect.poll(() => colorRequests.length).toBeGreaterThan(before);
    await expect(
      widget.getByRole("button", { name: "Recalculate formula", exact: true }),
    ).toBeEnabled();
    await assertColors(mode);
    await widget.screenshot({
      path: new URL(
        `../test-results/formula/theme-${mode}.png`,
        import.meta.url,
      ).pathname,
    });
  }
  await page
    .getByRole("button", { name: "Close reports", exact: true })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.locator("#theme").selectOption("light");
  await page.getByRole("button", { name: "Open theme editor" }).click();
  await page.getByLabel("Theme slot").selectOption("override");
  const customCss =
    ":root { --formula-accent: #123456; --color-warningText: var(--formula-accent); --color-chartQual1: #654321; }";
  await page.getByLabel("Custom Theme CSS", { exact: true }).fill(customCss);
  await page.getByRole("button", { name: "Apply custom CSS" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Custom theme saved" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await expect(
    widget.getByRole("button", { name: "Recalculate formula", exact: true }),
  ).toBeEnabled();
  await widget
    .getByRole("button", { name: "Recalculate formula", exact: true })
    .click();
  await expect(
    widget.getByRole("button", { name: "Recalculate formula", exact: true }),
  ).toBeEnabled();
  await assertColors("light", customCss);
  await edit();
  await editor
    .getByRole("textbox", { name: "Color formula", exact: true })
    .fill("=theme_warningText");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  await expect(editor).toHaveCount(0);
  await expect(value).toHaveCSS("color", "rgb(18, 52, 86)");
  // Simulate the previous compiler cache; keep the actual stored source and
  // remove only its new compiled bindings. Reopening must upgrade on demand.
  await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("actualBudget.deviceThemes.v1", 1);
        open.onsuccess = () => {
          const db = open.result,
            tx = db.transaction("records", "readwrite"),
            records = tx.objectStore("records");
          const current = records.get("active");
          current.onsuccess = () => {
            const record = current.result;
            for (const slot of ["light", "dark", "override"]) {
              const theme = record[slot];
              if (!theme) continue;
              theme.version = 1;
              theme.id = "legacy-stable-id";
              delete theme.source_hash;
              for (const variables of [
                theme.variables,
                ...Object.values(theme.variables_by_base),
              ])
                for (const key of Object.keys(variables))
                  if (key.startsWith("--color-")) delete variables[key];
            }
            records.put(record, "active");
          };
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
        open.onerror = () => reject(open.error);
      }),
  );
  await page.reload();
  // Theme recovery starts a fresh calculation; wait for its financial result
  // before asserting the restored color (within the API's 15-second deadline).
  await expect(value).toHaveText(expected, { timeout: 15000 });
  await expect(value).toHaveCSS("color", "rgb(18, 52, 86)");
  await assertColors("light", customCss);
  const recoveredTheme = await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("actualBudget.deviceThemes.v1", 1);
        open.onsuccess = () => {
          const db = open.result,
            request = db
              .transaction("records", "readonly")
              .objectStore("records")
              .get("active");
          request.onsuccess = () => {
            db.close();
            resolve(request.result.override);
          };
          request.onerror = () => reject(request.error);
        };
      }),
  );
  expect(recoveredTheme.id).toBe("legacy-stable-id");
  expect(recoveredTheme.version).toBe(2);
  expect(recoveredTheme.source).toBe(customCss);
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
  await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await editor
    .getByRole("textbox", { name: "Formula", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL(
      "../test-results/formula/mobile-large-text.png",
      import.meta.url,
    ).pathname,
  });
  expect(errors).toEqual([]);
  expect(httpFailures).toEqual([]);
  console.log(
    "PASS formula opt-in/command, source result/color and all source theme bindings across light/dark/midnight/custom, legacy theme upgrade, durable drafts, cache/quota/privacy and responsive keyboard/large-text editor",
  );
} catch (error) {
  const failedPage = browser?.contexts()[0]?.pages()[0];
  if (failedPage)
    console.log(
      "Browser failure state:",
      (await failedPage.locator("body").innerText()).slice(-9000),
    );
  throw error;
} finally {
  await browser?.close();
  await server.stop();
}

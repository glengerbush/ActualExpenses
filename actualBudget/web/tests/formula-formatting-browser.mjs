import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
const server = await startServer();
let browser;
try {
  const base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  async function api(path, body) {
    const response = await fetch(base + path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        "Content-Type": Buffer.isBuffer(body)
          ? "application/zip"
          : "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body:
        body === undefined
          ? undefined
          : Buffer.isBuffer(body)
            ? body
            : JSON.stringify(body),
    });
    expect(response.ok, await response.clone().text()).toBe(true);
    return (await response.json()).data;
  }
  const stage = await api(
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
  if (stage.status === "ready") await api(`/imports/${stage.id}/activate`, {});
  const root = `/budgets/${stage.budget_id}`;
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/formula-formatting-source.json",
        import.meta.url,
      ),
    ),
  ).cases.find((c) => c.id === "absent");
  const observations = Object.fromEntries(
    source.observations
      .filter((o) => o.name.endsWith("-number"))
      .map((o) => [o.locale, o]),
  );
  const financialRows = await api(root + "/transactions");
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage();
  const errors = [],
    requests = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("response", (r) => {
    if (r.status() >= 500) errors.push(r.status() + " " + r.url());
  });
  page.on("request", (r) => {
    if (r.url().endsWith("/reports/formula")) requests.push(r.postDataJSON());
  });
  await page.goto(server.url);
  await page.evaluate(
    (id) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: id }),
      ),
    stage.budget_id,
  );
  await page.reload();
  const button = (name) => page.getByRole("button", { name, exact: true });
  const openReports = () => button("Dashboards and reports").click();
  await button("Settings").click();
  await page.getByText("Experimental features", { exact: true }).click();
  await page
    .getByRole("checkbox", { name: "Formula mode", exact: true })
    .check();
  await button("Save shared preferences").click();
  await expect
    .poll(
      async () => (await api(root + "/preferences")).experimental.formulaMode,
    )
    .toBe(true);
  await button("Done").click();
  await openReports();
  await button("Formula").click();
  await button("New formula").click();
  const editor = page.getByRole("form", {
    name: "Reporting editor",
    exact: true,
  });
  await editor
    .getByRole("textbox", { name: "Widget name", exact: true })
    .fill("Locale formula");
  await editor
    .getByRole("textbox", { name: "Formula", exact: true })
    .fill(observations.en.formula);
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  const widget = page.getByRole("article", {
    name: "Locale formula",
    exact: true,
  });
  const value = widget.getByLabel("Calculated value", { exact: true });
  await expect(value).toHaveText(observations.en.output.result);
  const saved = (await api(root + "/reporting")).documents.find(
    (d) => d.config.meta?.name === "Locale formula",
  );
  for (const locale of ["fr", "en"]) {
    await button("Close reports").click();
    await button(locale === "en" ? "Paramètres" : "Settings").click();
    await page.locator("#language").selectOption(locale);
    await expect(page.locator("html")).toHaveAttribute("lang", locale);
    await button("Done").click();
    await openReports();
    await expect(value).toHaveText(observations[locale].output.result);
    expect(
      requests.some(
        (r) =>
          r.locale === locale && r.formula === observations[locale].formula,
      ),
    ).toBe(true);
  }
  // Disabling hides results and stops evaluation without deleting saved work.
  await button("Close reports").click();
  await button("Settings").click();
  await page.getByText("Experimental features", { exact: true }).click();
  await page
    .getByRole("checkbox", { name: "Formula mode", exact: true })
    .uncheck();
  await button("Save shared preferences").click();
  await expect
    .poll(
      async () => (await api(root + "/preferences")).experimental.formulaMode,
    )
    .toBe(false);
  await button("Done").click();
  const disabledRequests = requests.length;
  await openReports();
  await expect(widget).toContainText("Enable Formula in Settings");
  await expect(
    widget.getByRole("button", { name: "Recalculate formula", exact: true }),
  ).toBeDisabled();
  await expect(value).toHaveCount(0);
  expect(requests).toHaveLength(disabledRequests);
  expect(
    (await api(root + "/reporting")).documents.find((d) => d.id === saved.id),
  ).toEqual(saved);
  expect(await api(root + "/transactions")).toEqual(financialRows);
  expect(errors).toEqual([]);
  console.log(
    "PASS source locale defaults after browser language changes, unrelated settings save, and disabled formula keeps saved work without evaluating",
  );
} finally {
  await browser?.close();
  await server.stop();
}

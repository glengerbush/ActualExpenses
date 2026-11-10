// Public browser workflow; external supplied servers are honored.
import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
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
  await page.addInitScript(() => {
    // Fault injection on disposable browser data; pending/draft stores are untouched.
    const put = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (value, key) {
      if (
        this.name === "cache" &&
        typeof key === "string" &&
        key.startsWith("view:forecast:")
      )
        throw new DOMException(
          "Injected cache quota exhaustion",
          "QuotaExceededError",
        );
      return put.call(this, value, key);
    };
  });
  page.on("pageerror", (error) => console.error("PAGEERROR", error.message));
  await page.clock.setFixedTime(new Date("2026-11-09T12:00:00Z"));
  await page.goto(server.url);
  const budget = randomUUID(),
    name = "Forecast browser " + budget;
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
        "../../tests/fixtures/forecast-source-26.10.zip",
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
    name: "Saved balance forecast",
    exact: true,
  });
  await expect(
    widget.getByRole("region", { name: "Balance forecast result" }),
  ).toBeVisible();

  await expect(
    widget.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await expect(widget).toContainText("104.00");
  await expect(widget.getByRole("status")).toContainText(
    "could not save a cached copy",
  );
  await expect(widget).not.toContainText("Cached estimate.");
  await widget
    .getByRole("button", { name: "Edit Saved balance forecast", exact: true })
    .click();
  const editor = page.getByRole("form", { name: "Reporting editor" });
  await editor.getByLabel("Widget name", { exact: true }).fill("");
  await editor
    .getByRole("button", { name: "Save widget", exact: true })
    .click();
  const unnamed = page.getByRole("article", {
    name: "Balance forecast",
    exact: true,
  });
  await expect(
    unnamed.getByRole("heading", { name: "Balance forecast", exact: true }),
  ).toBeVisible();
  await expect(
    unnamed.getByRole("table", { name: "Forecast balances" }),
  ).toBeVisible();
  await expect(unnamed).toContainText("104.00");
  await expect(unnamed.getByRole("status")).toContainText(
    "could not save a cached copy",
  );
  console.log(
    "PASS online forecast survives cache quota failure and blank saved names use the translated forecast title",
  );
} finally {
  await browser?.close();
  await server.stop();
}

import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../../tests/fixtures/formula-display-source.json",
      import.meta.url,
    ),
  ),
);
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
    expect(response.ok, await response.clone().text()).toBe(true);
    return (await response.json()).data;
  }
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const filter = process.argv.find((a) => a.startsWith("--case="))?.slice(7);
  for (const test of fixture.cases.filter((c) => !filter || c.id === filter)) {
    const stage = await api(
      "/imports/actual",
      isolatedImportBytes(
        await readFile(
          new URL(
            "../../tests/fixtures/" + test.archive.filename,
            import.meta.url,
          ),
        ),
      ),
    );
    if (stage.status === "ready")
      await api(`/imports/${stage.id}/activate`, {});
    const root = `/budgets/${stage.budget_id}`,
      state = await api(root + "/reporting"),
      headers = { "X-Actual-Budget-Epoch": state.epoch };
    await api(
      root + "/preferences",
      { ...(test.changes ?? {}), experimental: { formulaMode: true } },
      "PATCH",
      headers,
    );
    const dashboard = randomUUID();
    await api(
      root + "/reporting/commands",
      {
        writes: [
          {
            id: dashboard,
            kind: "page",
            revision: 0,
            tombstone: false,
            config: { name: "Source display" },
          },
          ...test.observations.map((o, i) => ({
            id: randomUUID(),
            kind: "widget",
            revision: 0,
            tombstone: false,
            config: {
              dashboard_page_id: dashboard,
              type: "formula-card",
              width: 6,
              height: 3,
              x: 0,
              y: i * 3,
              meta: {
                name: test.id + " " + i,
                formula: o.formula,
                queries: {},
                showTitle: true,
              },
            },
          })),
        ],
      },
      "POST",
      headers,
    );
    const before = await api(root + "/transactions"),
      checkpoint = await api(root + "/checkpoint");
    const context = await browser.newContext({
        locale: test.locale,
        viewport: { width: 390, height: 844 },
      }),
      page = await context.newPage(),
      errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("response", (r) => {
      if (r.status() >= 500) errors.push(r.status() + " " + r.url());
    });
    await page.goto(server.url);
    await page.evaluate(
      (b) =>
        localStorage.setItem(
          "actualBudget.selection",
          JSON.stringify({ budgetId: b }),
        ),
      stage.budget_id,
    );
    await page.reload();
    await page
      .getByRole("button", { name: "Dashboards and reports", exact: true })
      .click();
    await page
      .getByRole("combobox", {
        name: test.locale === "fr" ? "Tableau de bord" : "Dashboard",
        exact: true,
      })
      .selectOption(dashboard);
    for (const [i, o] of test.observations.entries()) {
      const card = page.getByRole("article", {
        name: test.id + " " + i,
        exact: true,
      });
      if (o.error) {
        await expect(card.getByRole("alert")).toHaveText(
          "Calculated value is outside the supported display range. Adjust the formula.",
        );
        await expect(
          card.getByLabel("Calculated value", { exact: true }),
        ).toHaveCount(0);
      } else
        await expect(
          card.getByLabel("Calculated value", { exact: true }),
        ).toHaveText(o.display);
    }
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    expect(await api(root + "/transactions")).toEqual(before);
    expect(await api(root + "/checkpoint")).toEqual(checkpoint);
    expect(errors).toEqual([]);
    if (test.id === "absent-en" && process.argv.includes("--screenshots")) {
      const directory = new URL(
        "../test-results/formula-display/",
        import.meta.url,
      );
      await mkdir(directory, { recursive: true });
      for (const [index, name] of [
        [2, "negative-zero"],
        [4, "range-error"],
      ])
        await page
          .getByRole("article", { name: test.id + " " + index, exact: true })
          .screenshot({ path: new URL(name + ".png", directory).pathname });
    }
    await context.close();
    console.log(
      "PASS source numeric display " +
        test.id +
        "; unchanged financial rows/checkpoint and390px layout",
    );
  }
} finally {
  await browser?.close();
  await server.stop();
}

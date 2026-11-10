import { cloneImportedTemplate } from "./import-template.mjs";
import { chromium, expect } from "@playwright/test";
import { readFile, mkdir } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "./server.mjs";
const fixture = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/transaction-format-flows.json",
        import.meta.url,
      ),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
}
function financial(rows, refs, cats, item, source) {
  return rows
    .map((r) => ({
      id: r.id === item.ids.manual ? r.id : "<new>",
      amount: r.amount,
      date: r.date,
      payee:
        refs.find((p) => p.id === r[source ? "payee" : "payee_id"])?.name ??
        null,
      category:
        cats.find((c) => c.id === r[source ? "category" : "category_id"])
          ?.name ?? null,
      notes: r.notes ?? "",
      cleared: !!r.cleared,
      reconciled: !!r.reconciled,
      imported_id: r.imported_id ?? null,
      imported_payee: r.imported_payee ?? null,
    }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
let browser, page;
try {
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  for (const item of fixture.cases) {
    const archive = await readFile(
      new URL(
        "../../tests/fixtures/transaction-format-flows/" +
          item.archive.filename,
        import.meta.url,
      ),
    );
    expect(createHash("sha256").update(archive).digest("hex")).toBe(
      item.archive.sha256,
    );
    const response = await fetch(api + "/imports/actual", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: archive,
    });
    expect(response.status).toBe(201);
    const stage = (await response.json()).data;
    await call(`/imports/${stage.id}/activate`, {});
    stage.budget_id = await cloneImportedTemplate(api, stage.budget_id);
    const root = `/budgets/${stage.budget_id}`,
      context = await browser.newContext({
        viewport: { width: 1280, height: 1000 },
      });
    page = await context.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(server.url);
    await page.evaluate(
      ({ budget, account }) =>
        localStorage.setItem(
          "actualBudget.selection",
          JSON.stringify({ budgetId: budget, accountId: account }),
        ),
      { budget: stage.budget_id, account: item.ids.account },
    );
    await page.reload();
    await page
      .getByRole("button", { name: "Transaction files", exact: true })
      .click();
    for (let round = 0; round < 2; round++) {
      await page.getByLabel("Transaction file").setInputFiles({
        name: item.filename,
        mimeType: "application/octet-stream",
        buffer: Buffer.from(item.bytes_base64, "base64"),
      });
      await page
        .getByRole("button", { name: "Read file", exact: true })
        .click();
      await expect(
        page.getByRole("button", { name: "Preview selected import" }),
      ).toBeVisible();
      await page
        .getByLabel("Date order", { exact: false })
        .selectOption(item.frontend.parseDateFormat ?? "yyyy mm dd");
      await page
        .getByLabel("Import notes", { exact: true })
        .setChecked(item.options.importNotes ?? false);
      await page
        .getByLabel("Use memo when payee is missing")
        .setChecked(item.options.fallbackMissingPayeeToMemo ?? false);
      await page
        .getByLabel("Swap payee and memo")
        .setChecked(item.options.swapPayeeAndMemo ?? false);
      await page
        .getByRole("button", { name: "Preview selected import" })
        .click();
      await expect(
        page.getByRole("heading", { name: "Review proposed import" }),
      ).toBeVisible();
      await expect(page.getByRole("alert")).toHaveCount(0);
      if (round === 0) {
        await mkdir(
          new URL("../test-results/transaction-import/", import.meta.url),
          { recursive: true },
        );
        await page.screenshot({
          path: new URL(
            `../test-results/transaction-import/${item.id}-review.png`,
            import.meta.url,
          ).pathname,
          fullPage: true,
        });
      }
      await page
        .getByRole("button", { name: "Import reviewed records" })
        .click();
      await expect(
        page.getByText("Import saved:", { exact: false }),
      ).toBeVisible();
      const current = await call(root + "/transactions"),
        payees = (await call(root + "/payees")).data,
        categories = (await call(root + "/categories")).data,
        expected = round === 0 ? item.after : item.repeated;
      expect(financial(current.data, payees, categories, item, false)).toEqual(
        financial(
          expected.transactions,
          expected.payees,
          expected.categories,
          item,
          true,
        ),
      );
    }
    expect(errors).toEqual([]);
    console.log(
      "PASS " + item.id + " browser preview/commit/repeated financial source",
    );
    await context.close();
  }
} catch (e) {
  if (page && !page.isClosed())
    console.error((await page.locator("body").innerText()).slice(-12000));
  throw e;
} finally {
  await browser?.close();
  await server.stop();
}

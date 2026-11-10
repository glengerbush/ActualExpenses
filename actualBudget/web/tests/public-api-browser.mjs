// Existing register observes an independent external Python caller's writes.
import assert from "node:assert/strict";
import { chromium, expect } from "@playwright/test";
import { readFile, writeFile, mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { randomUUID, createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { startServer } from "./server.mjs";
import { cloneImportedTemplate } from "./import-template.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  source = JSON.parse(
    await readFile(
      new URL("../../tests/fixtures/public-batch-source.json", import.meta.url),
    ),
  ),
  directory = await mkdtemp(tmpdir() + "/public-api-browser-"),
  artifacts = fileURLToPath(
    new URL("../test-results/public-api/", import.meta.url),
  ),
  tool = fileURLToPath(
    new URL("../../tools/public-api-client.py", import.meta.url),
  );
let browser, page;
async function call(path, body, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(r.status, status, (await r.clone().text()).slice(0, 800));
  return r.json();
}
async function python(args) {
  return new Promise((resolve, reject) =>
    execFile(
      "python3",
      [tool, ...args],
      { maxBuffer: 8 * 1024 * 1024 },
      (error, stdout, stderr) => {
        try {
          assert.equal(
            error?.code ?? 0,
            0,
            stderr + " " + stdout.slice(0, 1000),
          );
          resolve(JSON.parse(stdout));
        } catch (e) {
          reject(e);
        }
      },
    ),
  );
}
try {
  const bytes = await readFile(
    new URL("../../tests/fixtures/" + source.archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    source.archive.sha256,
  );
  const upload = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  assert.equal(upload.status, 201);
  const stage = (await upload.json()).data,
    template = (await call("/imports/" + stage.id + "/activate", {}, 201)).data,
    budget = await cloneImportedTemplate(api, template.id),
    root = "/budgets/" + budget,
    account = source.operations[0].transaction.account_id;
  assert.equal(
    source.sums.find((s) => s.account === account).total,
    -1434,
    "Pinned source displayed account total",
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
    viewport: { width: 1440, height: 1000 },
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
    { budget, account },
  );
  await page.reload();
  await expect(page.getByTestId("register-total")).toHaveText("-10.35");
  await expect(
    page.locator(".register-table tbody tr").filter({ hasText: "=literal" }),
  ).toHaveCount(1);
  const body = directory + "/reviewed.json",
    outbox = directory + "/pending.json";
  await writeFile(body, JSON.stringify({ operations: source.operations }));
  await python([
    "--server",
    server.url.replace("actualbudget.test", "127.0.0.1"),
    "--budget",
    budget,
    "prepare",
    "--path",
    "/transactions/batch",
    "--body",
    body,
    "--outbox",
    outbox,
  ]);
  await python(["send", "--outbox", outbox]);
  await page
    .getByRole("button", { name: "Refresh transactions", exact: true })
    .click();
  await expect(page.getByTestId("register-total")).toHaveText("-14.34");
  const corrected = page
      .locator(".register-table tbody tr")
      .filter({ hasText: "External batch correction" }),
    added = page
      .locator(".register-table tbody tr")
      .filter({ hasText: "External batch purchase" });
  await expect(corrected).toContainText("13.00");
  await expect(added).toContainText("3.33");
  await expect(
    page.locator(".register-table tbody tr").filter({ hasText: "=literal" }),
  ).toHaveCount(0);
  const financial = (
    await call(root + "/query", {
      version: 1,
      table: "transactions",
      splits: "all",
      select: source.fields,
      order_by: ["id"],
    })
  ).data;
  assert.deepEqual(financial, source.after);
  await mkdir(artifacts, { recursive: true });
  await page.screenshot({ path: artifacts + "/desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(corrected).toBeVisible();
  await expect(page.getByTestId("register-total")).toHaveText("-14.34");
  await page.screenshot({ path: artifacts + "/mobile.png", fullPage: true });
  await corrected.scrollIntoViewIfNeeded();
  await page.screenshot({ path: artifacts + "/mobile-register-viewport.png" });
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(page.getByTestId("register-total")).toHaveText("••••");
  await expect(corrected.locator('td[data-label="Payment"]')).toHaveText(
    "••••",
  );
  await page.emulateMedia({ colorScheme: "dark" });
  await page.addStyleTag({ content: ":root{font-size:200%}" });
  await page.screenshot({
    path: artifacts + "/dark-large-private.png",
    fullPage: true,
  });
  await corrected.scrollIntoViewIfNeeded();
  await page.screenshot({
    path: artifacts + "/dark-large-private-viewport.png",
  });
  await expect(
    page.getByRole("alert").filter({ hasText: "Unable to complete operation" }),
  ).toHaveCount(0);
  assert.deepEqual(
    (
      await call(root + "/query", {
        version: 1,
        table: "transactions",
        splits: "all",
        select: source.fields,
        order_by: ["id"],
      })
    ).data,
    source.after,
    "Privacy/rendering never modifies financial results",
  );
  assert.deepEqual(errors, []);
  console.log(
    "PASS existing desktop/mobile/private register observes real external API batch, exact source money/IDs/rows and no browser errors",
  );
} catch (error) {
  if (page) {
    await mkdir(artifacts, { recursive: true });
    await writeFile(artifacts + "/failure.html", await page.content());
    await page
      .screenshot({ path: artifacts + "/failure.png", fullPage: true })
      .catch(() => {});
  }
  throw error;
} finally {
  await browser?.close();
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}

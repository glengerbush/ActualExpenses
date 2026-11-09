import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
let browser, diagnosticPage;
const attempts = [],
  responses = [],
  pageErrors = [];
async function call(path, body, headers = {}) {
  const response = await fetch(api + path, {
    method: body ? "POST" : "GET",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const result = await response.json();
  expect(response.ok, JSON.stringify(result)).toBe(true);
  return result;
}
try {
  const upload = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: await readFile(
      new URL("../../tests/fixtures/reconciliation-large.zip", import.meta.url),
    ),
  });
  expect(upload.ok).toBe(true);
  const imported = (await upload.json()).data;
  // Shared-server imports deduplicate; keep their activated budget pristine.
  expect(["ready", "activated"]).toContain(imported.status);
  const template = (await call(`/imports/${imported.id}/activate`, {})).data;
  const source = (await call(`/budgets/${template.id}`)).data;
  const templateBefore = (await call(`/budgets/${template.id}/transactions`))
    .data;
  const copyName = "Large split workflow " + randomUUID();
  const copy = (
    await call(
      `/budgets/${template.id}/duplicate`,
      { id: randomUUID(), name: copyName },
      { "X-Actual-Budget-Epoch": source.epoch },
    )
  ).data;
  const fixture = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/reconciliation-large.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const large = fixture.accounts[1],
    base = `/budgets/${copy.id}`,
    path = `${base}/transactions/${large.parent}/splits`;
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1280, height: 960 },
  });
  diagnosticPage = page;
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("response", async (response) => {
    if (
      !response.url().endsWith(path) ||
      response.request().method() !== "POST"
    )
      return;
    const body = await response.text().catch((error) => String(error));
    responses.push({
      phase: "browser",
      status: response.status(),
      body: response.ok() ? `success ${body.length} bytes` : body,
    });
  });
  await page.goto(server.url);
  await page
    .locator(".budget-list button")
    .filter({ hasText: copyName })
    .click();
  await page
    .getByRole("button", { name: "1001 allocations", exact: true })
    .click();
  await page.getByRole("button", { name: "Edit splits", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toBeVisible({ timeout: 30000 });
  await expect(page.getByLabel(/^Split \d+ amount$/)).toHaveCount(20);
  await page.getByLabel("Go to split", { exact: true }).fill("1001");
  await page.getByRole("button", { name: "Show split", exact: true }).click();
  await expect(
    page.getByText("Splits 1001–1001 of 1001", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Split 1001 notes", { exact: true }),
  ).toHaveValue("Allocation 1001");
  await page
    .getByLabel("Split 1001 notes", { exact: true })
    .fill("Last imported allocation edited");
  await page
    .getByRole("button", { name: "Previous splits", exact: true })
    .click();
  await expect(
    page.getByLabel("Split 981 amount", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next splits", exact: true }).click();
  await expect(
    page.getByLabel("Split 1001 notes", { exact: true }),
  ).toHaveValue("Last imported allocation edited");
  await page.route("**/transactions/*/splits", async (route) => {
    const request = route.request();
    if (request.method() !== "POST") return route.continue();
    attempts.push({
      body: request.postData(),
      key: request.headers()["idempotency-key"],
      clock: request.headers()["x-actual-edit-clock"],
      epoch: request.headers()["x-actual-budget-epoch"],
      owner: request.headers()["x-actual-history-owner"],
    });
    if (attempts.length === 1) {
      const response = await fetch(
        request.url().replace("actualbudget.test", "127.0.0.1"),
        {
          method: "POST",
          headers: Object.fromEntries(
            Object.entries(request.headers()).filter(([key]) =>
              [
                "content-type",
                "idempotency-key",
                "x-actual-edit-clock",
                "x-actual-budget-epoch",
                "x-actual-history-owner",
              ].includes(key),
            ),
          ),
          body: request.postData(),
        },
      );
      const body = await response.text();
      responses.push({
        phase: "manual-commit",
        status: response.status,
        body: response.ok ? `success ${body.length} bytes` : body,
      });
      expect(response.ok, body).toBe(true);
      return route.abort();
    }
    return route.continue();
  });
  await page.getByRole("button", { name: "Save splits", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Retry saved split", exact: true }),
  ).toBeVisible();
  await page.reload();
  await page
    .getByRole("button", { name: "Retry saved split", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0, { timeout: 30000 });
  expect(attempts).toHaveLength(2);
  expect(attempts[0]).toEqual(attempts[1]);
  expect(attempts[0].body.length).toBeLessThan(1024);
  expect(JSON.parse(attempts[0].body).changes.upsert).toHaveLength(1);
  let current = await call(path);
  expect(current.data.children[1000].notes).toBe(
    "Last imported allocation edited",
  );
  expect(current.data.children[0].notes).toBe("Allocation 1");
  await page.unroute("**/transactions/*/splits");
  await page.getByRole("button", { name: "Edit splits", exact: true }).click();
  await page.getByLabel("Split 1 amount", { exact: true }).fill("0");
  await page.getByRole("button", { name: "Add split", exact: true }).click();
  await expect(
    page.getByLabel("Split 1002 amount", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Distribute and save", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Split transaction", exact: true }),
  ).toHaveCount(0, { timeout: 30000 });
  current = await call(path);
  expect(current.data.children).toHaveLength(1002);
  expect(current.data.children[0].amount).toBe(0);
  expect(current.data.children[1001].amount).toBe(-1);
  expect(current.data.amount).toBe(-1001);
  await page.getByRole("button", { name: "Edit splits", exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByLabel("Go to split", { exact: true }).fill("1002");
  await page
    .getByRole("button", { name: "Show split", exact: true })
    .press("Enter");
  await expect(
    page.getByLabel("Split 1002 amount", { exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await mkdir("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/splits-large-mobile.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 960 });
  await page.screenshot({
    path: "test-results/splits-large-desktop.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Discard split draft", exact: true })
    .click();
  expect((await call(`/budgets/${template.id}/transactions`)).data).toEqual(
    templateBefore,
  );
  console.log(
    "PASS large split browser: 1001 imported allocations, bounded rendering, last-child edit, sparse immutable retry/reload, add/distribute, keyboard and mobile",
  );
} catch (error) {
  if (diagnosticPage) {
    await mkdir("test-results", { recursive: true });
    const diagnostics = {
      attempts,
      responses,
      pageErrors,
      alerts: await diagnosticPage.getByRole("alert").allTextContents(),
    };
    await writeFile(
      "test-results/splits-large-failure.html",
      await diagnosticPage.content(),
    );
    await writeFile(
      "test-results/splits-large-failure.json",
      JSON.stringify(diagnostics, null, 2),
    );
    await diagnosticPage.screenshot({
      path: "test-results/splits-large-failure.png",
      fullPage: true,
    });
    console.error("Large split failure:", JSON.stringify(diagnostics));
  }
  throw error;
} finally {
  await browser?.close();
  await server.stop();
}

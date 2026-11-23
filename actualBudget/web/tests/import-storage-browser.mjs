import { chromium, webkit, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createHash, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const server = await startServer();
const useWebKit = process.env.IMPORT_BROWSER === "webkit";
let browser;
try {
  browser = await (useWebKit ? webkit : chromium).launch(
    useWebKit
      ? {}
      : {
          executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
          args: ["--no-sandbox"],
        },
  );
  const context = await browser.newContext();
  // WebKit ephemeral sessions reject IDB Blob/File values. Exercise the same
  // storage boundary on the standard Chromium CI runner too; bytes must work.
  if (!useWebKit)
    await context.addInitScript(() => {
      const put = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function (value, ...args) {
        if (this.name === "session" && window.failImportStorage)
          throw new DOMException(
            "Storage is disabled",
            window.failImportStorage,
          );
        if (this.name === "session" && value?.file instanceof Blob)
          throw new DOMException(
            "BlobURLs are not yet supported",
            "DataCloneError",
          );
        return put.call(this, value, ...args);
      };
    });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const url = server.url.replace("actualbudget.test", "127.0.0.1");
  const original = await readFile(
    fileURLToPath(
      new URL("../../tests/fixtures/import-26.10.0.zip", import.meta.url),
    ),
  );
  // A real folder-wrapped export, unique per invocation on the shared test server.
  // Database and metadata bytes remain untouched; existing imports stay intact.
  const archive = execFileSync(
    "python3",
    [
      "-c",
      `
import io,sys,zipfile
source=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read()))
output=io.BytesIO()
with zipfile.ZipFile(output,"w",zipfile.ZIP_DEFLATED) as target:
 for name in source.namelist(): target.writestr(sys.argv[1]+"/"+name,source.read(name))
sys.stdout.buffer.write(output.getvalue())
`,
      randomUUID(),
    ],
    { input: original },
  );
  const expectedHash = createHash("sha256").update(archive).digest("hex");
  const uploads = [];
  await context.addInitScript(() => {
    const originalFetch = window.fetch;
    window.fetch = async (...args) => {
      const response = await originalFetch(...args);
      if (
        String(args[0]) === "/api/v1/imports/actual" &&
        response.ok &&
        !sessionStorage.getItem("lost-import-ack")
      ) {
        sessionStorage.setItem("lost-import-ack", "yes");
        throw new TypeError(
          "Simulated connection loss after the server committed",
        );
      }
      return response;
    };
  });
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/api/v1/imports/actual")
      uploads.push({ key: request.headers()["idempotency-key"] });
  });
  async function open(page) {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page
      .getByRole("navigation", { name: "Settings sections" })
      .getByRole("button", { name: "Import", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Import", exact: true }),
    ).toBeVisible();
  }
  await page.goto(url);
  await open(page);
  await page.getByLabel("Actual ZIP export").setInputFiles({
    name: "Desktop budget.zip",
    mimeType: "application/zip",
    buffer: archive,
  });
  if (!useWebKit) {
    await page.evaluate(() => {
      window.failImportStorage = "SecurityError";
    });
    await page
      .getByRole("button", { name: "Review archive", exact: true })
      .click();
    await expect(page.getByRole("alert")).toContainText(
      "Check that browser storage is enabled",
    );
    await expect(page.getByRole("alert")).not.toContainText("Free storage");
    expect(uploads).toHaveLength(0);
    await page.evaluate(() => {
      delete window.failImportStorage;
    });
  }
  await page
    .getByRole("button", { name: "Review archive", exact: true })
    .click();
  await expect
    .poll(() => uploads.length)
    .toBe(1)
    .catch(async (error) => {
      console.error(await page.getByRole("alert").allTextContents());
      throw error;
    });
  await expect(page.getByRole("alert")).toContainText(
    "Upload outcome is unconfirmed",
  );
  await page.reload();
  await open(page);
  await page
    .getByRole("button", { name: "Retry saved upload", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Import reviewed archive", exact: true }),
  ).toBeEnabled();
  expect(uploads).toHaveLength(2);
  const stages = await (
    await context.request.get(url + "/api/v1/imports")
  ).json();
  expect(
    stages.data.filter((stage) => stage.report.archive_sha256 === expectedHash),
  ).toHaveLength(1);
  expect(uploads[1]).toEqual(uploads[0]);
  await expect(
    page.locator(".balance-row").filter({ hasText: "Spending" }),
  ).toBeVisible();
  await expect(
    page.locator(".balance-row").filter({ hasText: "Reserve" }),
  ).toBeVisible();
  await page.reload();
  await open(page);
  await page
    .getByRole("button", { name: "Import reviewed archive", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transactions", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  expect(errors).toEqual([]);
  console.log(
    `PASS ${useWebKit ? "WebKit" : "Chromium with Blob storage restriction"}: desktop archive, durable retry/reload, unchanged bytes and activation`,
  );
} finally {
  await browser?.close();
  await server.stop();
}

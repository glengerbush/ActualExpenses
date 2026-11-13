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
  async function open() {
    await page
      .getByRole("button", { name: "Import Actual budget", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Import Actual budget", exact: true }),
    ).toBeVisible();
  }
  await page.goto(url);
  await open();
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
  await open();
  await page
    .getByRole("button", { name: "Retry saved upload", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Activate imported budget", exact: true }),
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
  await open();
  await page
    .getByRole("button", { name: "Activate imported budget", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Transaction history", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveCount(0);
  if (!useWebKit) {
    // Upgrade a pending upload written by v0.2.3, before the byte representation.
    const legacy = await browser.newContext();
    const oldPage = await legacy.newPage();
    await oldPage.goto(url);
    const legacyKey = randomUUID();
    await oldPage.evaluate(
      async ({ bytes, key, activationKey }) => {
        const db = await new Promise((resolve, reject) => {
          const request = indexedDB.open("actualBudget.imports.v1", 1);
          request.onupgradeneeded = () =>
            request.result.createObjectStore("session");
          request.onsuccess = () => resolve(request.result);
          request.onerror = () => reject(request.error);
        });
        await new Promise((resolve, reject) => {
          const tx = db.transaction("session", "readwrite");
          tx.objectStore("session").put(
            {
              version: 1,
              sourceKind: "actual",
              key,
              activationKey,
              filename: "Old desktop export.zip",
              file: new Blob([new Uint8Array(bytes)]),
              stage: null,
              pendingUpload: true,
              pendingActivation: false,
            },
            "current",
          );
          tx.oncomplete = resolve;
          tx.onabort = () => reject(tx.error);
        });
        db.close();
      },
      { bytes: [...archive], key: legacyKey, activationKey: randomUUID() },
    );
    await oldPage
      .getByRole("button", { name: "Import Actual budget", exact: true })
      .click();
    await oldPage
      .getByRole("button", { name: "Retry saved upload", exact: true })
      .click();
    await expect(
      oldPage.getByRole("button", {
        name: "Open imported budget",
        exact: true,
      }),
    ).toBeEnabled();
    const upgraded = await oldPage.evaluate(async () => {
      const db = await new Promise((resolve) => {
        const r = indexedDB.open("actualBudget.imports.v1", 1);
        r.onsuccess = () => resolve(r.result);
      });
      const value = await new Promise((resolve) => {
        const r = db
          .transaction("session")
          .objectStore("session")
          .get("current");
        r.onsuccess = () => resolve(r.result);
      });
      db.close();
      return {
        version: value.version,
        key: value.key,
        bytes: value.file instanceof ArrayBuffer,
        hash: value.stage.report.archive_sha256,
      };
    });
    expect(upgraded).toEqual({
      version: 2,
      key: legacyKey,
      bytes: true,
      hash: expectedHash,
    });
    await legacy.close();
  }
  expect(errors).toEqual([]);
  console.log(
    `PASS ${useWebKit ? "WebKit" : "Chromium with Blob storage restriction"}: desktop archive, durable retry/reload, unchanged bytes and activation`,
  );
} finally {
  await browser?.close();
  await server.stop();
}

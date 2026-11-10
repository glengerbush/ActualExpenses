import { chromium, expect } from "@playwright/test";
import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  artifacts = new URL("../test-results/ynab/", import.meta.url);
let browser;
async function read(path) {
  const r = await fetch(api + path);
  assert.equal(r.status, 200);
  return (await r.json()).data;
}
async function forward(route) {
  const url = new URL(route.request().url());
  const headers = { ...route.request().headers(), host: url.host };
  if (url.hostname === "actualbudget.test") url.hostname = "127.0.0.1";
  return route.fetch({ url: url.href, headers });
}
const errors = [];
try {
  await mkdir(artifacts, { recursive: true });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  for (const kind of ["nynab", "ynab4"]) {
    const context = await browser.newContext({
        viewport: { width: 1360, height: 1000 },
      }),
      page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(server.url);
    await page
      .getByRole("button", { name: "Import YNAB budget", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Import YNAB budget", exact: true }),
    ).toBeVisible();
    await page.getByLabel("Budget file format").selectOption(kind);
    const name = kind === "nynab" ? "nynab-demo.json" : "ynab4-demo.zip",
      bytes = isolatedImportBytes(
        await readFile(
          new URL("../../tests/fixtures/ynab/" + name, import.meta.url),
        ),
        kind === "nynab" ? "json" : "zip",
      );
    await page
      .getByLabel(kind === "nynab" ? "nYNAB JSON export" : "YNAB4 ZIP export", {
        exact: true,
      })
      .setInputFiles({
        name,
        mimeType: kind === "nynab" ? "application/json" : "application/zip",
        buffer: bytes,
      });
    const attempts = [];
    let deliveredStage;
    await page.route("**/api/v1/imports/" + kind + "?*", async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      const request = route.request();
      attempts.push({
        url: new URL(request.url()).pathname + new URL(request.url()).search,
        key: request.headers()["idempotency-key"],
        type: request.headers()["content-type"],
        sha: createHash("sha256")
          .update(request.postDataBuffer())
          .digest("hex"),
      });
      if (attempts.length === 1) {
        const response = await forward(route);
        assert.equal(
          response.status(),
          201,
          "Lose only an acknowledgement after actual server commit",
        );
        deliveredStage = (await response.json()).data;
        await route.abort("failed");
      } else await route.continue();
    });
    if (kind === "nynab") {
      await page.evaluate(() => {
        window.savedPut = IDBObjectStore.prototype.put;
        IDBObjectStore.prototype.put = function (...args) {
          if (this.name === "session")
            throw new DOMException("Full", "QuotaExceededError");
          return window.savedPut.apply(this, args);
        };
      });
      await page
        .getByRole("button", { name: "Review archive", exact: true })
        .click();
      await expect(page.getByRole("alert")).toContainText(/storage/i);
      assert.equal(
        attempts.length,
        0,
        "Quota failure cannot send an unsaved original",
      );
      await page.evaluate(() => {
        IDBObjectStore.prototype.put = window.savedPut;
      });
    }
    await page
      .getByRole("button", { name: "Review archive", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Retry saved upload", exact: true }),
    ).toBeEnabled();
    assert.equal(deliveredStage.report.source_format, kind);
    assert.equal(
      (await read("/imports/" + deliveredStage.id)).status === "activated",
      false,
      "First forward committed a stage, not an active budget",
    );
    assert.equal(attempts.length, 1);
    await page.reload();
    await page
      .getByRole("button", { name: "Import YNAB budget", exact: true })
      .click();
    await expect(page.getByText(name, { exact: true })).toBeVisible();
    await page
      .getByRole("button", { name: "Retry saved upload", exact: true })
      .click();
    await expect(
      page.getByRole("heading", {
        name: "Review migration differences",
        exact: true,
      }),
    ).toBeVisible();
    assert.equal(attempts.length, 2);
    assert.deepEqual(
      attempts[1],
      attempts[0],
      "Reload keeps original format/path/query/bytes/key",
    );
    assert.equal(
      attempts[0].sha,
      createHash("sha256").update(bytes).digest("hex"),
    );
    await expect(
      page.getByText(/edit logs|edit history|edit log/).first(),
    ).toBeVisible();
    const originalLink = page.getByRole("link", {
        name: "Download original archive",
        exact: true,
      }),
      download = await fetch(
        api + (await originalLink.getAttribute("href")).replace("/api/v1", ""),
      );
    assert.deepEqual(Buffer.from(await download.arrayBuffer()), bytes);
    await page.screenshot({
      path: new URL(kind + "-desktop.png", artifacts).pathname,
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({
      path: new URL(kind + "-mobile.png", artifacts).pathname,
      fullPage: true,
    });
    if (kind === "nynab") {
      await page.emulateMedia({ colorScheme: "dark" });
      await page.evaluate(
        () => (document.documentElement.style.fontSize = "32px"),
      );
      assert.ok(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      );
      await page.screenshot({
        path: new URL("nynab-dark-large.png", artifacts).pathname,
        fullPage: true,
      });
      await page.evaluate(() => (document.documentElement.style.fontSize = ""));
      await page.emulateMedia({ colorScheme: "light" });
    }
    let activationFirst;
    const activations = [];
    await page.route(
      "**/api/v1/imports/" + deliveredStage.id + "/activate",
      async (route) => {
        const r = route.request();
        activations.push({
          key: r.headers()["idempotency-key"],
          body: r.postData(),
        });
        if (activations.length === 1) {
          const forwarded = await forward(route);
          assert.equal(forwarded.status(), 201);
          activationFirst = (await forwarded.json()).data;
          await route.abort("failed");
        } else await route.continue();
      },
    );
    await page
      .getByRole("button", { name: "Activate imported budget", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Retry saved activation", exact: true }),
    ).toBeEnabled();
    assert.ok(
      (await read("/budgets")).some((b) => b.id === activationFirst.id),
      "Activation committed before ACK loss",
    );
    const committed = await read(
      "/budgets/" + activationFirst.id + "/transactions?limit=1000",
    );
    await page.reload();
    await page
      .getByRole("button", { name: "Import YNAB budget", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Retry saved activation", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Import YNAB budget", exact: true }),
    ).not.toBeVisible();
    assert.deepEqual(activations[1], activations[0]);
    assert.deepEqual(
      await read("/budgets/" + activationFirst.id + "/transactions?limit=1000"),
      committed,
      "Retry activation never reinstalls historical money",
    );
    await context.close();
    console.log(
      "PASS " +
        kind +
        " UI quota/original upload+activation ACK recovery, differences and responsive review",
    );
  }
  assert.deepEqual(errors, []);
} finally {
  await browser?.close();
  await server.stop();
}

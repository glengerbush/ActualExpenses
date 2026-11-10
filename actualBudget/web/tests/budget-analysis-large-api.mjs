// Public HTTP tracer against unchanged pinned frontend/budget/CSV expectations.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
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
  const page = await browser.newPage();
  await page.goto(server.url);
  const api = (path, body, headers = {}, method) =>
    page.evaluate(
      async ({ path, body, headers, method }) => {
        const response = await fetch("/api/v1" + path, {
          method: method || (body === undefined ? "GET" : "POST"),
          headers: { "Content-Type": "application/json", ...headers },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        return { status: response.status, ...(await response.json()) };
      },
      { path, body, headers, method },
    );
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/budget-analysis-large-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const bytes = isolatedImportBytes(
    await readFile(
      new URL(
        "../../tests/fixtures/budget-analysis-large-source-26.10.zip",
        import.meta.url,
      ),
    ),
  ).toString("base64");
  const stage = await page.evaluate(
    async ({ bytes, key }) => {
      const r = await fetch("/api/v1/imports/actual", {
        method: "POST",
        headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
        body: Uint8Array.from(atob(bytes), (c) => c.charCodeAt(0)),
      });
      return { status: r.status, ...(await r.json()) };
    },
    { bytes, key: randomUUID() },
  );
  assert.equal(stage.status, 201, JSON.stringify(stage));
  assert.ok(
    ["ready", "activated"].includes(stage.data.status),
    JSON.stringify(stage),
  );
  if (stage.data.status === "ready")
    assert.equal(
      (
        await api(
          "/imports/" + stage.data.id + "/activate",
          {},
          { "Idempotency-Key": randomUUID() },
        )
      ).status,
      201,
    );
  const original = "/budgets/" + stage.data.budget_id,
    state = await api(original + "/reporting"),
    id = randomUUID();
  assert.equal(
    (
      await api(
        original + "/duplicate",
        { id, name: "Large analysis proof " + id },
        {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": state.data.epoch,
        },
      )
    ).status,
    201,
  );
  const root = "/budgets/" + id;
  const normal = await api(root + "/budget?month=2026-10");
  assert.equal(normal.status, 200, JSON.stringify(normal));
  const before = await api(
      root + "/companion?account_id=" + source.ids.account,
    ),
    query = {
      version: 1,
      as_of: source.clock.slice(0, 10),
      options: source.options,
    };
  const value = await api(root + "/reports/budget-analysis", query);
  assert.equal(value.status, 200, JSON.stringify(value));
  assert.deepEqual(value.data.result, source.expected);
  assert.deepEqual(
    await api(root + "/companion?account_id=" + source.ids.account),
    before,
  );
  assert.equal(
    (await api(root + "/reports/budget-analysis", query)).data.cache.hit,
    true,
  );
  console.log(
    "PASS genuine funded history has valid ordinary budget and exact late-period analysis without unused summary overflow",
  );
} finally {
  await browser?.close();
  await server.stop();
}

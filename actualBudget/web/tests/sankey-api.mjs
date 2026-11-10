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
        return {
          status: response.status,
          ...(await response.json().catch(() => ({}))),
        };
      },
      { path, body, headers, method },
    );
  for (const method of ["envelope", "tracking"]) {
    const source = JSON.parse(
        await readFile(
          new URL(
            "../../tests/fixtures/sankey-" + method + "-source-26.10.json",
            import.meta.url,
          ),
          "utf8",
        ),
      ),
      bytes = isolatedImportBytes(
        await readFile(
          new URL(
            "../../tests/fixtures/sankey-" + method + "-source-26.10.zip",
            import.meta.url,
          ),
        ),
      ).toString("base64");
    const stage = await page.evaluate(
      async ({ bytes, key }) => {
        const r = await fetch("/api/v1/imports/actual", {
          method: "POST",
          headers: {
            "Content-Type": "application/zip",
            "Idempotency-Key": key,
          },
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
      registry = await api(original + "/reporting"),
      id = randomUUID();
    assert.equal(
      (
        await api(
          original + "/duplicate",
          { id, name: "Sankey proof " + id },
          {
            "Idempotency-Key": randomUUID(),
            "X-Actual-Budget-Epoch": registry.data.epoch,
          },
        )
      ).status,
      201,
    );
    const root = "/budgets/" + id;
    const state = await api(root + "/reporting");
    assert.deepEqual(
      state.data.documents.find((d) => d.id === source.widgets[0].id).config
        .meta,
      source.widgets[0].meta,
    );
    assert.equal(
      (await api(root + "/preferences")).data.experimental.sankeyReport,
      true,
    );
    const companion = await api(
      root + "/companion?account_id=" + source.ids.checking,
    );
    for (const oracle of source.cases) {
      const response = await api(root + "/reports/sankey", {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: oracle.options,
      });
      assert.equal(
        response.status,
        200,
        JSON.stringify({ case: oracle.name, ...response }),
      );
      assert.deepEqual(
        response.data.base_graph,
        oracle.baseGraph,
        oracle.name + " ordered base graph",
      );
      assert.deepEqual(
        response.data.result,
        oracle.result,
        oracle.name + " full source nodes/links/Other/percentages",
      );
    }
    assert.deepEqual(
      await api(root + "/companion?account_id=" + source.ids.checking),
      companion,
    );
    assert.deepEqual((await api(root + "/reporting")).data, state.data);
    console.log(
      "PASS source Sankey " +
        method +
        " " +
        source.cases.length +
        " base/final graphs, preserved settings and readonly companion",
    );
  }
} finally {
  await browser?.close();
  await server.stop();
}

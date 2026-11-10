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
  for (const method of ["envelope", "tracking"]) {
    const source = JSON.parse(
        await readFile(
          new URL(
            "../../tests/fixtures/budget-analysis-" +
              method +
              "-source-26.10.json",
            import.meta.url,
          ),
          "utf8",
        ),
      ),
      bytes = isolatedImportBytes(
        await readFile(
          new URL(
            "../../tests/fixtures/budget-analysis-" +
              method +
              "-source-26.10.zip",
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
          { id, name: "Analysis proof " + id },
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
      (await api(root + "/preferences")).data.experimental.budgetAnalysisReport,
      true,
    );
    const companion = await api(
        root + "/companion?account_id=" + source.ids.checking,
      ),
      request = {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: source.cases[0].options,
      };
    const result = await api(root + "/reports/budget-analysis", request);
    assert.equal(result.status, 200, JSON.stringify(result));
    assert.deepEqual(result.data.result, source.cases[0].expected);
    assert.deepEqual(
      await api(root + "/companion?account_id=" + source.ids.checking),
      companion,
    );
    assert.deepEqual((await api(root + "/reporting")).data, state.data);
    for (const oracle of source.cases) {
      const result = await api(root + "/reports/budget-analysis", {
        ...request,
        options: oracle.options,
      });
      assert.equal(result.status, 200, JSON.stringify(result));
      assert.deepEqual(
        result.data.result,
        oracle.expected,
        JSON.stringify(oracle.options),
      );
      const repeat = await api(root + "/reports/budget-analysis", {
        ...request,
        options: oracle.options,
      });
      assert.equal(repeat.data.cache.hit, true);
      assert.deepEqual(repeat.data.result, oracle.expected);
    }
    const first = await api(root + "/reports/budget-analysis", request),
      month = source.details.month,
      expectedMonth = source.cases[0].expected.intervalData.find(
        (row) => row.date === month,
      );
    const seen = [];
    for (let offset = 0; offset < source.details.rows.length; offset += 2) {
      const detail = await api(root + "/reports/budget-analysis/drilldown", {
        query: request,
        snapshot: first.data.snapshot,
        month,
        metric: "spent",
        offset,
        limit: 2,
      });
      assert.equal(detail.status, 200, JSON.stringify(detail));
      assert.equal(detail.data.basis, "recorded_entries");
      assert.equal(detail.data.total_count, source.details.rows.length);
      assert.equal(detail.data.total_amount, expectedMonth.spent);
      assert.equal(
        detail.data.has_more,
        offset + detail.data.rows.length < source.details.rows.length,
      );
      seen.push(...detail.data.rows);
    }
    const identity = (row) => ({
      id: row.id,
      date: row.date,
      amount: row.amount,
      account: row.account_id,
      category: row.category_id,
      payee: row.payee_id,
      notes: row.notes || null,
    });
    assert.deepEqual(
      seen.map(identity).sort((a, b) => a.id.localeCompare(b.id)),
      source.details.rows,
      "Paged recorded detail membership matches genuine source AQL",
    );
    const plans = await api(root + "/reports/budget-analysis/drilldown", {
      query: request,
      snapshot: first.data.snapshot,
      month,
      metric: "budgeted",
      offset: 0,
      limit: 50,
    });
    assert.equal(plans.status, 200, JSON.stringify(plans));
    assert.equal(plans.data.basis, "budget_allocations");
    assert.equal(plans.data.total_amount, expectedMonth.budgeted);
    const baseIds = source.details.query.filterExpressions[0].category.$oneof;
    const planned = baseIds
      .flatMap((id) => {
        const cell = source.dto.months[month].find((cell) =>
          cell.name.endsWith("!budget-" + id),
        );
        return cell?.value ? [{ category_id: id, amount: cell.value }] : [];
      })
      .sort((a, b) => a.category_id.localeCompare(b.category_id));
    assert.deepEqual(
      plans.data.rows
        .map((row) => ({ category_id: row.category_id, amount: row.amount }))
        .sort((a, b) => a.category_id.localeCompare(b.category_id)),
      planned,
    );
    assert.deepEqual(
      await api(root + "/companion?account_id=" + source.ids.checking),
      companion,
    );
    assert.deepEqual((await api(root + "/reporting")).data, state.data);
    const headers = () => ({
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": state.data.epoch,
    });
    const changedWidget = structuredClone(
      state.data.documents.find((d) => d.id === source.widgets[0].id),
    );
    changedWidget.config.meta.name = "Same finance, new presentation";
    changedWidget.config.meta.graphType = "Bar";
    assert.equal(
      (
        await api(
          root + "/reporting/commands",
          { writes: [changedWidget] },
          headers(),
        )
      ).status,
      200,
    );
    const renamed = await api(root + "/reports/budget-analysis", request);
    assert.equal(renamed.data.cache.hit, true);
    assert.equal(
      renamed.data.cache.input_revision,
      first.data.cache.input_revision,
    );
    assert.notEqual(renamed.data.snapshot, first.data.snapshot);
    assert.deepEqual(renamed.data.result, first.data.result);
    assert.equal(
      (
        await api(root + "/reports/budget-analysis/drilldown", {
          query: request,
          snapshot: first.data.snapshot,
          month,
          metric: "spent",
        })
      ).status,
      409,
    );
    assert.equal(
      (
        await api(root + "/reports/budget-analysis/drilldown", {
          query: request,
          snapshot: renamed.data.snapshot,
          month,
          metric: "spent",
        })
      ).status,
      200,
    );
    assert.equal(
      (
        await api(
          root + "/preferences",
          { experimental: { budgetAnalysisReport: false } },
          headers(),
          "PATCH",
        )
      ).status,
      200,
    );
    const off = await api(root + "/reports/budget-analysis", request);
    assert.equal(off.status, 409);
    assert.equal(off.error.code, "feature_disabled");
    assert.equal(
      (
        await api(
          root + "/preferences",
          { experimental: { budgetAnalysisReport: true } },
          headers(),
          "PATCH",
        )
      ).status,
      200,
    );
    const enabled = await api(root + "/reports/budget-analysis", request);
    assert.equal(enabled.data.cache.hit, true);
    assert.equal(
      enabled.data.cache.input_revision,
      first.data.cache.input_revision,
    );
    assert.equal(
      (
        await api(
          root + "/transactions",
          {
            id: randomUUID(),
            account_id: source.ids.checking,
            category_id: source.ids.food,
            date: "2026-11-29",
            amount: -1,
            notes: "Confirmed analysis invalidation",
          },
          headers(),
        )
      ).status,
      201,
    );
    const posted = await api(root + "/reports/budget-analysis", request);
    assert.equal(posted.data.cache.hit, false);
    assert.notEqual(
      posted.data.cache.input_revision,
      first.data.cache.input_revision,
    );
    assert.equal(
      posted.data.result.intervalData.find((row) => row.date === month).spent,
      expectedMonth.spent - 1,
    );
    const budget = (await api(root + "/budget?month=" + month)).data;
    assert.equal(
      (
        await api(
          root + "/budget",
          {
            action: "set",
            month,
            category: source.ids.food,
            amount: 1001,
            snapshot: budget.snapshot,
          },
          headers(),
        )
      ).status,
      200,
    );
    const allocated = await api(root + "/reports/budget-analysis", request);
    assert.equal(allocated.data.cache.hit, false);
    assert.notEqual(
      allocated.data.cache.input_revision,
      posted.data.cache.input_revision,
    );
    assert.equal(
      allocated.data.result.intervalData.find((row) => row.date === month)
        .budgeted,
      expectedMonth.budgeted + 1,
    );
    for (const bad of [
      { ...request, version: 2 },
      { ...request, as_of: "2026-02-30" },
      { ...request, options: { ...request.options, endDate: "2020-01-01" } },
      {
        ...request,
        options: {
          ...request.options,
          startDate: "0001-01-01",
          endDate: "9999-12-31",
        },
      },
      { ...request, options: { ...request.options, conditionsOp: "xor" } },
      {
        ...request,
        options: {
          ...request.options,
          conditions: Array.from({ length: 21 }, () => ({
            field: "category",
            op: "is",
            value: null,
          })),
        },
      },
    ])
      assert.equal(
        (await api(root + "/reports/budget-analysis", bad)).status,
        400,
        JSON.stringify(bad),
      );
    console.log(
      "PASS " +
        method +
        " source detail paging, planned basis, stale snapshot, presentation/opt-in cache reuse, posted/plan invalidation and typed work/input bounds",
    );
    console.log(
      "PASS " +
        method +
        " " +
        source.cases.length +
        " source analyses, opt-in/config, cache reuse and readonly companion/reporting",
    );
  }
} finally {
  await browser?.close();
  await server.stop();
}

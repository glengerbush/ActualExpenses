// Observable readonly forecasting against whole pinned26.10 handler expectations.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
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
  const expressions = process.argv.includes("--expressions"),
    extended = expressions || process.argv.includes("--extended"),
    stem = expressions
      ? "forecast-expressions-source-26.10"
      : extended
        ? "forecast-scenarios-source-26.10"
        : "forecast-source-26.10";
  const source = JSON.parse(
      await readFile(
        new URL("../../tests/fixtures/" + stem + ".json", import.meta.url),
        "utf8",
      ),
    ),
    bytes = (
      await readFile(
        new URL("../../tests/fixtures/" + stem + ".zip", import.meta.url),
      )
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
          `/imports/${stage.data.id}/activate`,
          {},
          { "Idempotency-Key": randomUUID() },
        )
      ).status,
      201,
    );
  const original = stage.data.budget_id,
    current = await api(`/budgets/${original}/reporting`),
    budget = randomUUID();
  assert.equal(
    (
      await api(
        `/budgets/${original}/duplicate`,
        { id: budget, name: "Forecast proof " + budget },
        {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": current.data.epoch,
        },
      )
    ).status,
    201,
  );
  const root = `/budgets/${budget}`;
  assert.equal(
    (await api(root + "/preferences")).data.experimental.balanceForecastReport,
    true,
  );
  const registry = await api(root + "/reporting"),
    saved = registry.data.documents.find((d) => d.id === source.widgets[0].id);
  assert.deepEqual(saved.config.meta, source.widgets[0].meta);
  const account = source.cases[0].options.accountIds[0],
    companionBefore = await api(root + "/companion?account_id=" + account),
    schedulesBefore = await api(root + "/schedules");
  for (const oracle of source.cases) {
    const query = {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: {
          ...oracle.options,
          granularity:
            oracle.options.source === "tracking-budget" ? "Monthly" : "Daily",
        },
      },
      projection = await api(root + "/reports/forecast", query);
    assert.equal(projection.status, 200, JSON.stringify(projection));
    assert.deepEqual(projection.data.result, oracle.expected);
    assert.equal(projection.data.recorded, false);
    const repeated = await api(root + "/reports/forecast", query);
    assert.equal(
      repeated.data.cache.hit,
      true,
      "Same input revision reuses forecast work",
    );
    assert.deepEqual(repeated.data.result, oracle.expected);
    assert.equal(repeated.data.snapshot, projection.data.snapshot);
  }
  let supplemental;
  if (extended && !expressions) {
    supplemental = JSON.parse(
      await readFile(
        new URL(
          "../../tests/fixtures/forecast-view-source-26.10.json",
          import.meta.url,
        ),
        "utf8",
      ),
    );
    assert.equal(supplemental.base_sha256, source.archive_sha256);
    for (const granularity of ["Daily", "Monthly"]) {
      const result = await api(root + "/reports/forecast", {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: { ...supplemental.view.options, granularity },
      });
      assert.equal(result.status, 200, JSON.stringify(result));
      assert.deepEqual(result.data.result, supplemental.view.result);
      for (const field of [
        "chart",
        "scheduled_occurrence_count",
        "lowest_displayed",
      ])
        assert.deepEqual(
          result.data[field],
          supplemental.view.views[granularity][field],
        );
    }
  }
  assert.deepEqual(
    await api(root + "/companion?account_id=" + account),
    companionBefore,
    "Forecasts do not alter companion money or ledger cursors",
  );
  assert.deepEqual(
    await api(root + "/schedules"),
    schedulesBefore,
    "Forecasts do not advance schedules",
  );
  if (supplemental) {
    assert.equal(
      (
        await api(
          root + "/accounts/" + supplemental.account_mutation.id,
          { name: supplemental.account_mutation.name },
          {
            "Idempotency-Key": randomUUID(),
            "X-Actual-Budget-Epoch": registry.data.epoch,
          },
          "PATCH",
        )
      ).status,
      200,
    );
    assert.equal(
      (
        await api(
          root + "/rules",
          {
            id: randomUUID(),
            snapshot: (await api(root + "/rules")).snapshot,
            ...supplemental.rule_mutation,
          },
          {
            "Idempotency-Key": randomUUID(),
            "X-Actual-Budget-Epoch": registry.data.epoch,
          },
        )
      ).status,
      201,
    );
  }
  if (supplemental)
    for (const item of supplemental.account_cases) {
      const actual = await api(root + "/reports/forecast", {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: { ...item.options, granularity: "Daily" },
      });
      assert.equal(actual.status, 200, JSON.stringify(actual));
      // Account rename can reorder the source's account presentation; compare every
      // financial point and entry using stable date/account identity.
      const ordered = (value) => ({
        ...value,
        dataPoints: [...value.dataPoints].sort((a, b) =>
          (a.date + a.accountId).localeCompare(b.date + b.accountId),
        ),
      });
      assert.deepEqual(
        ordered(actual.data.result),
        ordered(item.expected),
        JSON.stringify(item.options),
      );
    }
  if (supplemental)
    assert.equal(
      (
        await api(
          root + "/accounts/" + supplemental.account_mutation.id,
          { name: "checking" },
          {
            "Idempotency-Key": randomUUID(),
            "X-Actual-Budget-Epoch": registry.data.epoch,
          },
          "PATCH",
        )
      ).status,
      200,
    );
  if (supplemental) {
    const plan = supplemental.group_plan,
      prior = (await api(root + "/budget?month=" + plan.mutation.month)).data;
    const allocated = await api(
      root + "/budget",
      { action: "set", ...plan.mutation, snapshot: prior.snapshot },
      {
        "Idempotency-Key": randomUUID(),
        "X-Actual-Budget-Epoch": registry.data.epoch,
      },
    );
    assert.equal(allocated.status, 200, JSON.stringify(allocated));
    const projection = await api(root + "/reports/forecast", {
      version: 1,
      as_of: source.clock.slice(0, 10),
      options: { ...plan.options, granularity: "Monthly" },
    });
    assert.equal(projection.status, 200, JSON.stringify(projection));
    assert.deepEqual(
      projection.data.result,
      plan.expected,
      "Tracking forecasts preserve source group membership semantics",
    );
  }
  const basic = {
      version: 1,
      as_of: source.clock.slice(0, 10),
      options: { ...source.cases[0].options, granularity: "Daily" },
    },
    beforeChange = await api(root + "/reports/forecast", basic),
    headers = () => ({
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": registry.data.epoch,
    });
  const added = await api(
    root + "/transactions",
    {
      id: randomUUID(),
      account_id: account,
      amount: -1,
      date: "2026-11-10",
      notes: "Confirmed cache invalidation witness",
    },
    headers(),
  );
  assert.equal(added.status, 201, JSON.stringify(added));
  const changed = await api(root + "/reports/forecast", basic);
  assert.equal(changed.status, 200);
  assert.equal(changed.data.cache.hit, false);
  assert.notEqual(changed.data.snapshot, beforeChange.data.snapshot);
  assert.equal(changed.data.result.lowestBalance.balance, 10299);
  const disabled = await api(
    root + "/preferences",
    { experimental: { balanceForecastReport: false } },
    headers(),
    "PATCH",
  );
  assert.equal(disabled.status, 200);
  const off = await api(root + "/reports/forecast", basic);
  assert.equal(off.status, 409);
  assert.equal(off.error.code, "feature_disabled");
  assert.equal(
    (
      await api(
        root + "/preferences",
        { experimental: { balanceForecastReport: true } },
        headers(),
        "PATCH",
      )
    ).status,
    200,
  );
  const enabled = await api(root + "/reports/forecast", basic);
  assert.equal(enabled.status, 200);
  assert.equal(enabled.data.cache.hit, false);
  assert.equal(enabled.data.result.lowestBalance.balance, 10299);
  assert.deepEqual(
    (await api(root + "/reporting")).data.documents,
    registry.data.documents,
  );
  const rejects = [
    { ...basic, version: 2 },
    {
      ...basic,
      options: {
        ...basic.options,
        startDate: "1900-01-01",
        endDate: "2030-12-31",
      },
    },
    { ...basic, options: { ...basic.options, source: "unknown" } },
    {
      ...basic,
      options: {
        ...basic.options,
        conditions: [{ field: "notes", op: "matches", value: "(" }],
      },
    },
    {
      ...basic,
      options: {
        ...basic.options,
        source: "tracking-budget",
        granularity: "Daily",
      },
    },
  ];
  if (extended)
    rejects.push({
      ...basic,
      options: {
        ...basic.options,
        source: "tracking-budget",
        granularity: "Monthly",
        endDate: "2027-12-31",
      },
    });
  else
    rejects.push({
      ...basic,
      options: {
        ...basic.options,
        source: "tracking-budget",
        granularity: "Monthly",
      },
    });
  for (const request of rejects) {
    const rejected = await api(root + "/reports/forecast", request);
    assert.equal(rejected.status, 400, JSON.stringify(rejected));
  }
  console.log(
    "PASS revision cache reuse/invalidation, default-preserving opt-out/re-enable, readonly companion/schedules and typed bounds/errors",
  );
  console.log(
    "PASS source opted-in widget and whole-source recurring forecast through running public API",
  );
} finally {
  await browser?.close();
  await server.stop();
}

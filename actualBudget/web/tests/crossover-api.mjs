// Public HTTP tracer; genuine pinned whole-handler expectations, isolated imports.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
const server = await startServer();
try {
  const origin = server.url.replace("actualbudget.test", "127.0.0.1");
  async function api(path, body, headers = {}, method) {
    const response = await fetch(origin + "/api/v1" + path, {
      method: method || (body === undefined ? "GET" : "POST"),
      headers: { "Content-Type": "application/json", ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    return {
      status: response.status,
      ...(await response.json().catch(() => ({}))),
    };
  }
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/crossover-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const imported = await fetch(origin + "/api/v1/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: isolatedImportBytes(
      await readFile(
        new URL(
          "../../tests/fixtures/crossover-source-26.10.zip",
          import.meta.url,
        ),
      ),
    ),
  });
  assert.equal(imported.status, 201);
  const stage = await imported.json();
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
  const original = "/budgets/" + stage.data.budget_id,
    importedState = await api(original + "/reporting"),
    budget = randomUUID(),
    root = "/budgets/" + budget;
  assert.equal(
    (
      await api(
        original + "/duplicate",
        { id: budget, name: "Crossover proof " + budget },
        {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": importedState.data.epoch,
        },
      )
    ).status,
    201,
  );
  const before = await api(root + "/reporting"),
    widget = before.data.documents.find((d) => d.id === source.widgets[0].id);
  assert.deepEqual(widget.config.meta, source.widgets[0].meta);
  const response = await api(root + "/reports/crossover", {
    version: 1,
    as_of: source.clock.slice(0, 10),
    options: source.cases[0].options,
  });
  assert.equal(response.status, 200, JSON.stringify(response));
  assert.deepEqual(response.data.result, source.cases[0].result);
  // Whole-source fixture comparison; every observed integer output is exact.
  for (const test of source.cases) {
    const query = {
      version: 1,
      as_of: source.clock.slice(0, 10),
      options: test.options,
    };
    const result = await api(root + "/reports/crossover", query);
    if (test.nonfinite_paths.length) {
      assert.equal(result.status, 400, JSON.stringify(result));
      continue;
    }
    assert.equal(result.status, 200, JSON.stringify(result));
    assert.deepEqual(result.data.result, test.result, test.name);
    assert.equal(result.data.months.length, test.result.graphData.data.length);
    assert.equal(result.data.recorded, false);
    assert.equal(result.data.epoch, before.data.epoch);
    assert.ok(!JSON.stringify(result).includes("NaN"));
  }
  assert.deepEqual(await api(root + "/reporting"), before);
  const query = {
    version: 1,
    as_of: source.clock.slice(0, 10),
    options: source.cases[0].options,
  };
  await api(root + "/reports/crossover", query); // ten cases exceed the eight-entry cache
  assert.equal(
    (await api(root + "/reports/crossover", query)).data.cache.hit,
    true,
  );
  const renamed = {
    ...widget,
    config: {
      ...widget.config,
      meta: { ...widget.config.meta, name: "Crossover renamed" },
    },
  };
  assert.equal(
    (
      await api(
        root + "/reporting/commands",
        { writes: [renamed] },
        {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": before.data.epoch,
        },
      )
    ).status,
    200,
  );
  assert.equal(
    (await api(root + "/reports/crossover", query)).data.cache.hit,
    true,
  );
  const invalid = [
    { ...query, version: 2 },
    { ...query, options: { ...query.options, start: "2026-13" } },
    { ...query, options: { ...query.options, projectionType: "unknown" } },
    { ...query, options: { ...query.options, estimatedReturn: -1.01 } },
    { ...query, options: { ...query.options, expenseAdjustmentFactor: -1 } },
    {
      ...query,
      options: {
        ...query.options,
        incomeAccountIds: Array(1001).fill(source.ids.investment),
      },
    },
  ];
  for (const body of invalid)
    assert.equal((await api(root + "/reports/crossover", body)).status, 400);
  const empty = await api(root + "/reports/crossover", {
    ...query,
    options: {
      ...query.options,
      start: "1000-01",
      end: "9999-12",
      incomeAccountIds: [],
    },
  });
  assert.equal(empty.status, 200, JSON.stringify(empty));
  assert.deepEqual(empty.data.result.graphData.data, []);
  const changed = await api(
    root + "/transactions/38380000-0000-4000-8000-000000000012",
    { amount: -15000 },
    {
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": before.data.epoch,
    },
    "PATCH",
  );
  assert.equal(changed.status, 200, JSON.stringify(changed));
  const recalculated = await api(root + "/reports/crossover", query);
  assert.equal(recalculated.status, 200, JSON.stringify(recalculated));
  assert.equal(recalculated.data.cache.hit, false);
  assert.equal(recalculated.data.result.lastKnownMonthlyExpenses, 15000);
  assert.equal(
    (await api(root + "/reports/crossover", query)).data.cache.hit,
    true,
  );
  console.log(
    "PASS all10 whole-source Crossover results (nine exact finite graphs +explicit nonfinite refusal), readonly/imported configuration, cosmetic reuse/financial invalidation, empty fast path and typed bounds",
  );
} finally {
  await server.stop();
}

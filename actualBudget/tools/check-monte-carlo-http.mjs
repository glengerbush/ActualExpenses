// Public planning API versus independent pinned engine observations.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const source = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/monte-carlo-baseline-source.json",
      import.meta.url,
    ),
  ),
);
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(
  path,
  body,
  status = 200,
  method = body === undefined ? "GET" : "POST",
) {
  const response = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30000),
  });
  const text = await response.text();
  assert.equal(response.status, status, `${path}: ${text}`);
  return JSON.parse(text);
}
function compare(actual, expected, path) {
  if (typeof expected === "number") {
    assert.equal(typeof actual, "number", path);
    const tolerance = Number.isInteger(expected)
      ? 0
      : Math.min(0.01, Math.max(1e-6, Math.abs(expected) * 1e-12));
    assert.ok(
      Number.isFinite(actual) && Math.abs(actual - expected) <= tolerance,
      `${path}: ${actual} != ${expected}`,
    );
  } else if (expected !== null && typeof expected === "object") {
    assert.deepEqual(
      Object.keys(actual).sort(),
      Object.keys(expected).sort(),
      path,
    );
    for (const key of Object.keys(expected))
      compare(actual[key], expected[key], path + "." + key);
  } else assert.equal(actual, expected, path);
}
try {
  const id = randomUUID(),
    root = `/budgets/${id}`;
  await call("/budgets", { id, name: "Monte Carlo source comparison" }, 201);
  await call(
    root + "/preferences",
    { experimental: { monteCarloReport: true } },
    200,
    "PATCH",
  );
  const before = await call(root + "/transactions"),
    checkpoint = await call(root + "/checkpoint");
  const cases = process.argv.includes("--all")
    ? source.cases
    : source.cases.filter((test) => test.name === "baseline-proportional");
  for (const test of cases) {
    const request = {
      version: 1,
      request_id: randomUUID(),
      config: test.meta,
      ...test.options,
    };
    const response = (await call(root + "/reports/monte-carlo", request)).data;
    compare(response.config, test.config, test.name + ".config");
    compare(response.result, test.result, test.name + ".result");
    assert.equal(response.recorded, false);
    assert.equal(response.snapshot, checkpoint.data.snapshot);
    const again = (
      await call(root + "/reports/monte-carlo", {
        ...request,
        request_id: randomUUID(),
      })
    ).data;
    assert.equal(again.cache.hit, true);
    assert.deepEqual(again.result, response.result);
    console.log("PASS", test.name);
  }
  assert.deepEqual(await call(root + "/transactions"), before);
  assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
} finally {
  await server.stop();
}

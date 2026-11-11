import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function request(
  path,
  body,
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
  return { status: response.status, body: await response.json() };
}
async function call(path, body, status = 200, method) {
  const result = await request(path, body, method);
  assert.equal(result.status, status, JSON.stringify(result.body));
  return result.body;
}
try {
  const budget = randomUUID(),
    root = `/budgets/${budget}`,
    route = root + "/reports/monte-carlo";
  await call("/budgets", { id: budget, name: "Monte Carlo lifecycle" }, 201);
  const plan = {
    version: 1,
    request_id: randomUUID(),
    config: { simulationCount: 1000, currentAge: 60, targetAge: 61 },
  };
  assert.equal((await call(route, plan, 409)).error.code, "feature_disabled");
  await call(
    root + "/preferences",
    { experimental: { monteCarloReport: true } },
    200,
    "PATCH",
  );
  await call(route, { ...plan, config: { unknown: 1 } }, 400);
  await call(
    route,
    { ...plan, config: { pots: Array(257).fill({ id: "pot" }) } },
    400,
  );
  await call(
    route,
    { ...plan, config: { pots: [{ startingBalance: "5000" }] } },
    400,
  );
  const before = await call(root + "/checkpoint");
  const id = randomUUID();
  const pending = request(route, {
    ...plan,
    request_id: id,
    config: { simulationCount: 10000, targetAge: 160 },
  });
  let running = false;
  for (let i = 0; i < 100; i++) {
    const result = await request(route + "/" + id);
    if (result.status === 200) {
      running = true;
      break;
    }
    assert.equal(result.status, 404);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.ok(running, "Simulation must become observable while running");
  // Let this deliberately long plan enter computation before interrupting it.
  await new Promise((resolve) => setTimeout(resolve, 150));
  assert.equal(
    (await call(route, { ...plan, request_id: id }, 409)).error.code,
    "conflict",
  );
  assert.equal((await call(route, plan, 429)).error.code, "simulation_busy");
  await call(
    `/budgets/${randomUUID()}/reports/monte-carlo/${id}`,
    undefined,
    404,
  );
  // A live simulation must not monopolize SQLite or delay ordinary writes.
  const account = randomUUID(),
    started = performance.now();
  await call(
    root + "/accounts",
    {
      id: account,
      name: "Linked investments",
      opening_balance: 5_000_000,
      opening_date: "2026-01-01",
    },
    201,
  );
  // The companion's ordinary cleared-expense route must remain responsive too.
  await call(
    root + "/transactions",
    {
      id: randomUUID(),
      account_id: account,
      amount: -1234,
      date: "2026-01-02",
      notes: "Expense during simulation",
      cleared: true,
    },
    201,
  );
  assert.ok(
    performance.now() - started < 2000,
    "Ordinary write blocked behind simulation",
  );
  assert.equal(
    (await call(route + "/" + id, undefined, 200, "DELETE")).data
      .cancellation_requested,
    true,
  );
  const stopped = await pending;
  assert.equal(stopped.status, 409);
  assert.equal(stopped.body.error.code, "simulation_cancelled");
  await call(route + "/" + id, undefined, 404);
  const checkpoint = await call(root + "/checkpoint"),
    ledger = await call(root + "/transactions");
  assert.notEqual(checkpoint.data.snapshot, before.data.snapshot);
  const linked = {
    ...plan,
    config: {
      ...plan.config,
      pots: [{ id: "linked", accountId: account, startingBalance: 1 }],
    },
  };
  const first = (await call(route, linked)).data;
  assert.equal(first.config.pots[0].startingBalance, 4_998_766);
  assert.equal(first.cache.hit, false);
  assert.equal(
    (await call(route, { ...linked, request_id: randomUUID() })).data.cache.hit,
    true,
  );
  assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
  assert.deepEqual(await call(root + "/transactions"), ledger);
  await call(
    root + "/transactions",
    {
      id: randomUUID(),
      account_id: account,
      amount: 10000,
      date: "2026-01-02",
      notes: "Contribution",
      cleared: true,
    },
    201,
  );
  const changed = (await call(route, { ...linked, request_id: randomUUID() }))
    .data;
  assert.equal(changed.config.pots[0].startingBalance, 5_008_766);
  assert.equal(changed.cache.hit, false);
  assert.notEqual(changed.cache.input_revision, first.cache.input_revision);
  const fallback = (
    await call(route, {
      ...linked,
      request_id: randomUUID(),
      config: {
        ...linked.config,
        pots: [{ id: "gone", accountId: randomUUID(), startingBalance: 12345 }],
      },
    })
  ).data;
  assert.equal(fallback.config.pots[0].startingBalance, 12345);
  assert.equal(fallback.warnings[0].code, "account_unavailable");
  if (process.argv.includes("--limits")) {
    const checkpoint = await call(root + "/checkpoint"),
      ledger = await call(root + "/transactions");
    const started = performance.now();
    const rejected = await call(
      route,
      {
        ...plan,
        request_id: randomUUID(),
        config: {
          simulationCount: 10000,
          currentAge: 60,
          targetAge: 160,
          pots: Array.from({ length: 128 }, (_, index) => ({
            id: `pot-${index}`,
            startingBalance: 50_000_000,
          })),
        },
      },
      400,
    );
    assert.match(rejected.error.message, /limit|memory|interrupt/i);
    assert.ok(
      performance.now() - started < 25000,
      "Runtime failed to interrupt within its bounded allowance",
    );
    assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
    assert.deepEqual(await call(root + "/transactions"), ledger);
    const recovered = (
      await call(route, { ...plan, request_id: randomUUID(), seed: 50 })
    ).data;
    assert.equal(recovered.result.simulationCount, 1000);
    console.log(
      "PASS runtime resource refusal, unchanged ledger/checkpoint and subsequent fresh calculation",
    );
  }
  await call(
    root + "/preferences",
    { experimental: { monteCarloReport: false } },
    200,
    "PATCH",
  );
  assert.equal((await call(route, linked, 409)).error.code, "feature_disabled");
  console.log(
    "PASS opt-in, strict validation, cancellation/admission, concurrent writes, linked balances, fallback, cache invalidation and unchanged financial state",
  );
} finally {
  await server.stop();
}

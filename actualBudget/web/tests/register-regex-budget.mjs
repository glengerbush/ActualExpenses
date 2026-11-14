import { expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(15000),
  });
  return { status: response.status, ...(await response.json()) };
}
try {
  const budget = randomUUID(),
    account = randomUUID();
  expect(
    (await call("/budgets", { id: budget, name: "Bounded ECMAScript" })).status,
  ).toBe(201);
  expect(
    (
      await call(`/budgets/${budget}/accounts`, {
        id: account,
        name: "Spending",
        opening_balance: 0,
        opening_date: "2026-01-01",
      })
    ).status,
  ).toBe(201);
  const longBudget = randomUUID(),
    longAccount = randomUUID();
  await call("/budgets", { id: longBudget, name: "Long prefix work" });
  await call(`/budgets/${longBudget}/accounts`, {
    id: longAccount,
    name: "Spending",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const longPayee = randomUUID();
  await call(`/budgets/${longBudget}/payees`, {
    id: longPayee,
    name: "a".repeat(120),
  });
  for (let n = 0; n < 10; n++)
    await call(`/budgets/${longBudget}/transactions`, {
      id: randomUUID(),
      account_id: longAccount,
      date: "2026-01-02",
      amount: -1,
      payee_id: longPayee,
    });
  const prefixStarted = performance.now();
  const prefixBounded = await call(
    `/budgets/${longBudget}/transactions?limit=1&query=` +
      encodeURIComponent(
        JSON.stringify({
          filter: {
            conditions: [
              {
                field: "payee_name",
                op: "matches",
                value: "a".repeat(118) + "b",
              },
            ],
          },
        }),
      ),
  );
  // Names are capped at 120 characters; this prefix scan fits the bounded work.
  expect(prefixBounded.status).toBe(200);
  expect(prefixBounded.filtered_count).toBe(0);
  expect(prefixBounded.data).toEqual([]);
  expect(performance.now() - prefixStarted).toBeLessThan(3000);
  const prefixRecovery = await call(
    `/budgets/${longBudget}/transactions?limit=1&query=` +
      encodeURIComponent(
        JSON.stringify({
          filter: {
            conditions: [{ field: "payee_name", op: "matches", value: "a" }],
          },
        }),
      ),
  );
  expect(prefixRecovery.status).toBe(200);
  expect(prefixRecovery.filtered_count).toBe(10);
  const root = `/budgets/${budget}/transactions`;
  const pathologicalPayee = randomUUID(),
    repeatedPayee = randomUUID();
  await call(`/budgets/${budget}/payees`, {
    id: pathologicalPayee,
    name: "a".repeat(64) + "!",
  });
  await call(`/budgets/${budget}/payees`, {
    id: repeatedPayee,
    name: "x".repeat(120),
  });
  expect(
    (
      await call(root, {
        id: randomUUID(),
        account_id: account,
        date: "2026-01-02",
        amount: -100,
        payee_id: pathologicalPayee,
      })
    ).status,
  ).toBe(201);
  const query = (conditions) =>
    root +
    "?limit=1&query=" +
    encodeURIComponent(JSON.stringify({ filter: { conditions } }));
  const pattern = (value) => ({ field: "payee_name", op: "matches", value });
  let started = performance.now();
  const exhausted = await call(query([pattern("^(a+)+$")]));
  expect(exhausted.status).toBe(400);
  expect(exhausted.error.code).toBe("regex_work_limit");
  expect(exhausted.data).toBeUndefined();
  expect(exhausted.filtered_balances).toBeUndefined();
  expect(performance.now() - started).toBeLessThan(10000);
  expect((await call(query([pattern("^a")]))).filtered_balances.total).toBe(
    -100,
  );
  // No optimizer unrolling: enormous counted repeats compile in bounded space,
  // and excessive structural nesting is rejected before the parser runs.
  for (const value of [
    "(".repeat(65) + "a" + ")".repeat(65),
    "a?".repeat(257),
  ]) {
    const rejected = await call(query([pattern(value)]));
    expect(rejected.status).toBe(400);
    expect(rejected.error.code).toBe("regex_complexity_limit");
  }
  const giantRepeat = await call(query([pattern("^a{999999999}$")]));
  expect(giantRepeat.status).toBe(200);
  expect(giantRepeat.filtered_count).toBe(0);
  for (let n = 0; n < 3000; n++) {
    expect(
      (
        await call(root, {
          id: randomUUID(),
          account_id: account,
          date: "2026-01-03",
          amount: -1,
          payee_id: repeatedPayee,
        })
      ).status,
    ).toBe(201);
  }
  // Each successful ^x match is too short to trigger an engine poll. The
  // request-wide precharge must still stop repeated cheap matches across rows.
  started = performance.now();
  const many = await call(
    query(Array.from({ length: 20 }, () => pattern("^x"))),
  );
  expect(many.status).toBe(400);
  expect(many.error.code).toBe("regex_work_limit");
  expect(many.data).toBeUndefined();
  expect(performance.now() - started).toBeLessThan(10000);
  const after = await call(query([pattern("^x")]));
  expect(after.status).toBe(200);
  expect(after.filtered_count).toBe(3000);
  expect(after.filtered_balances.total).toBe(-3000);
  expect(after.data).toHaveLength(1);
  console.log(
    "PASS pathological match cancellation, compile bounds, cheap-match request budget, no partial totals, and subsequent valid request",
  );
} finally {
  await server.stop();
}

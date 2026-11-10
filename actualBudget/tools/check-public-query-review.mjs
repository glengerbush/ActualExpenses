// Review regressions: result cardinality, expression placement, global output budget.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  selected = process.argv.find((v) => v.startsWith("--case="))?.slice(7);
async function call(path, body, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(
    r.status,
    status,
    `${path}: ${(await r.clone().text()).slice(0, 700)}`,
  );
  return r.json();
}
try {
  const b = (
      await call(
        "/budgets",
        { id: randomUUID(), name: "Query output bounds" },
        201,
      )
    ).data,
    root = "/budgets/" + b.id,
    a = (
      await call(
        root + "/accounts",
        {
          id: randomUUID(),
          name: "Checking",
          opening_balance: 0,
          opening_date: "2026-11-09",
        },
        201,
      )
    ).data;
  for (let i = 0; i < 2; i++) {
    const id = randomUUID();
    await call(
      root + "/transactions",
      {
        id,
        account_id: a.id,
        amount: -20,
        date: "2026-11-09",
        notes: "Long parent",
      },
      201,
    );
    await call(root + "/transactions/" + id + "/splits", {
      snapshot: (await call(root + "/checkpoint")).data.snapshot,
      children: Array.from({ length: 20 }, () => ({
        id: randomUUID(),
        amount: -1,
        notes: "🪙".repeat(2500),
      })),
    });
  }
  const cp = (await call(root + "/checkpoint")).data,
    ledger = await call(root + "/transactions?limit=1000"),
    base = {
      version: 1,
      table: "transactions",
      snapshot: cp.snapshot,
      limit: 1,
    };
  if (!selected || selected === "aggregate") {
    const result = await call(root + "/query", {
      ...base,
      select: [{ total: { $sum: "$amount" } }],
    });
    assert.deepEqual(result.data, [{ total: -40 }]);
    assert.equal(result.matched_count, 1);
    assert.equal(
      result.next_offset,
      null,
      "A sole aggregate result has no next page",
    );
    console.log("PASS aggregate result cardinality");
  }
  if (!selected || selected === "placement") {
    for (const fn of ["$sum", "$count", "$sumOver"])
      for (const part of ["group", "filter"]) {
        const query = {
          ...base,
          select: ["id"],
          ...(part === "group"
            ? { group_by: [{ [fn]: "$amount" }] }
            : { filter: [{ amount: { $transform: fn, $gt: 0 } }] }),
        };
        const result = await call(root + "/query", query, 400);
        assert.equal(result.error.code, "invalid_query");
      }
    console.log("PASS aggregate/window placement returns typed400");
  }
  if (!selected || selected === "output") {
    const result = await call(
      root + "/query",
      {
        ...base,
        limit: 1000,
        splits: "grouped",
        select: [
          "id",
          ...Array.from({ length: 32 - 1 }, (_, i) => ({
            ["notes" + i]: "$notes",
          })),
        ],
      },
      400,
    );
    assert.equal(result.error.code, "invalid_query");
    assert.match(result.error.message, /output|response|emitted/i);
    console.log("PASS cumulative grouped output refusal");
  }
  if (!selected || selected === "text-work") {
    const result = await call(
      root + "/query",
      {
        ...base,
        select: ["id"],
        filter: [
          {
            $and: Array.from({ length: 50 }, () => ({
              notes: { $transform: "$lower", $ne: "skip" },
            })),
          },
        ],
      },
      400,
    );
    assert.equal(result.error.code, "invalid_query");
    assert.match(result.error.message, /text work/i);
    console.log("PASS shared scalar text allowance across count and page");
  }
  assert.deepEqual((await call(root + "/checkpoint")).data, cp);
  assert.deepEqual(
    await call(root + "/transactions?limit=1000"),
    ledger,
    "All query failures preserve financial graph",
  );
} finally {
  await server.stop();
}

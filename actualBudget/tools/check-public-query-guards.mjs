// Public validation/resource guards and no-write proof, not a private compiler test.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function request(path, body, status = 200, headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(r.status, status, `${path}: ${await r.clone().text()}`);
  return r.json();
}
try {
  const budget = randomUUID(),
    root = "/budgets/" + budget;
  await request("/budgets", { id: budget, name: "Bounded query fixture" }, 201);
  for (let i = 0; i < 2; i++)
    await request(
      root + "/accounts",
      {
        id: randomUUID(),
        name: "Asset " + i,
        opening_balance: i === 0 ? 9007199254740991 : -9007199254740991,
        opening_date: "2026-11-09",
      },
      201,
    );
  const checkpoint = (await request(root + "/checkpoint")).data,
    ledger = await request(root + "/transactions?limit=1000"),
    body = {
      version: 1,
      table: "transactions",
      select: ["id"],
      filter: [],
      group_by: [],
      order_by: ["id"],
      snapshot: checkpoint.snapshot,
    };
  const cases = [
    { ...body, table: "operations" },
    { ...body, select: ["private_key"] },
    { ...body, select: [{ x: { $evil: "DROP TABLE transactions" } }] },
    { ...body, select: [{ first: "$id", ignored: "$amount" }] },
    { ...body, limit: 0 },
    { ...body, limit: 1001 },
    { ...body, offset: 100001 },
    { ...body, snapshot: null, order_by: Array(9).fill("id") },
    { ...body, select: Array(33).fill("id") },
    { ...body, filter: [{ id: { $oneof: Array(501).fill(randomUUID()) } }] },
    { ...body, calculation: true, select: ["id", "amount"] },
    {
      ...body,
      select: [{ total: { $sum: { $abs: "$amount" } } }],
      calculation: true,
    },
  ];
  let nested = { id: randomUUID() };
  for (let i = 0; i < 10; i++) nested = { $and: [nested] };
  cases.push({ ...body, filter: [nested] });
  for (const invalid of cases) {
    const response = await request(root + "/query", invalid, 400);
    assert.equal(typeof response.error.code, "string");
    assert.equal(typeof response.error.message, "string");
  }
  for (const header of [
    "X-Actual-Receipt-Only",
    "X-Actual-Cancel-If-Uncommitted",
  ])
    await request(root + "/query", body, 400, { [header]: "true" });
  const literal = "'; DROP TABLE transactions; -- € Café",
    literalResult = await request(root + "/query", {
      ...body,
      select: [{ value: { literal } }],
    });
  assert.deepEqual(
    literalResult.data,
    [{ value: literal }, { value: literal }],
    "Literal is bound data, never executable SQL",
  );
  const alias = 'x"; DROP TABLE transactions; --',
    aliased = await request(root + "/query", {
      ...body,
      select: [{ [alias]: "$id" }],
    });
  assert.equal(aliased.data.length, 2);
  assert.ok(aliased.data.every((row) => typeof row[alias] === "string"));
  const owner = randomUUID(),
    history = await request(root + "/history", undefined, 200, {
      "X-Actual-History-Owner": owner,
      "X-Actual-Budget-Epoch": checkpoint.epoch,
    });
  assert.equal(history.data.undo, null);
  assert.equal(history.data.redo, null);
  assert.deepEqual(await request(root + "/transactions?limit=1000"), ledger);
  assert.deepEqual(
    (await request(root + "/checkpoint")).data,
    checkpoint,
    "Invalid/literal/read queries produce no field clocks, feed events",
  );
  const first = await request(root + "/query", { ...body, limit: 1 });
  assert.equal(first.data.length, 1);
  assert.equal(first.next_offset, 1);
  const second = await request(root + "/query", {
    ...body,
    limit: 1,
    offset: first.next_offset,
    snapshot: first.snapshot,
  });
  assert.equal(second.data.length, 1);
  assert.notEqual(first.data[0].id, second.data[0].id);
  await request(
    root + "/accounts",
    {
      id: randomUUID(),
      name: "Remote edit",
      opening_balance: 0,
      opening_date: "2026-11-09",
    },
    201,
  );
  const stale = await request(root + "/query", { ...body, offset: 1 }, 409);
  assert.equal(stale.error.code, "snapshot_changed");
  console.log(
    "PASS bounded query validation/literal/overflow/read-only/history/consistent pages and stale continuation",
  );
} finally {
  await server.stop();
}

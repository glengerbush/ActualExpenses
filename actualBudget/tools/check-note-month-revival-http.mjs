// A financial month may revive without reviving an explicitly undone annotation.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200, headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
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
  const budget = (
    await call(
      "/budgets",
      {
        id: randomUUID(),
        name: "Month allocation lifecycle",
        budget_type: "tracking",
      },
      201,
    )
  ).data;
  const root = "/budgets/" + budget.id,
    month = "2026-11",
    id = "budget-" + month,
    category = randomUUID();
  const headers = {
    "X-Actual-Budget-Epoch": (await call(root)).data.epoch,
    "X-Actual-History-Owner": randomUUID(),
  };
  await call(
    root + "/categories",
    { id: category, name: "Food", is_income: false },
    201,
  );
  for (let i = 0; i < 2; i++) {
    const cp = (await call(root + "/checkpoint")).data,
      history = (await call(root + "/history", undefined, 200, headers)).data;
    await call(
      root + "/notes/commands",
      { id, note: "Retired annotation", snapshot: cp.snapshot },
      400,
      headers,
    );
    assert.deepEqual((await call(root + "/checkpoint")).data, cp);
    assert.deepEqual(
      (await call(root + "/history", undefined, 200, headers)).data,
      history,
    );
    const result = await call(
      root + "/budget",
      { action: "set", month, category, amount: 100, snapshot: cp.snapshot },
      200,
      headers,
    );
    assert.equal(
      result.data.months[month].categories.find((row) => row.id === category)
        .budgeted,
      100,
    );
    assert.equal((await call(root + "/notes/read", { id })).data, null);
    const undo = (await call(root + "/history", undefined, 200, headers)).data
      .undo;
    await call(root + "/history/undo", { action_id: undo.id }, 200, headers);
    assert.equal(
      (await call(root + "/budget?month=" + month)).data.months[
        month
      ].categories.find((row) => row.id === category).budgeted,
      0,
    );
    assert.equal((await call(root + "/notes/read", { id })).data, null);
  }
  console.log(
    "PASS allocation revival and inverse preserve financial values without reviving retired month annotations",
  );
} finally {
  await server.stop();
}

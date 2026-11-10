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
  const b = (
      await call(
        "/budgets",
        {
          id: randomUUID(),
          name: "Month annotation lifecycle",
          budget_type: "envelope",
        },
        201,
      )
    ).data,
    root = "/budgets/" + b.id,
    month = "2026-11",
    id = "budget-" + month,
    headers = {
      "X-Actual-Budget-Epoch": (await call(root)).data.epoch,
      "X-Actual-History-Owner": randomUUID(),
    };
  await call(
    root + "/notes/commands",
    {
      id,
      note: "Undo this annotation",
      snapshot: (await call(root + "/checkpoint")).data.snapshot,
    },
    200,
    headers,
  );
  const action = (await call(root + "/history", undefined, 200, headers)).data
    .undo;
  await call(root + "/history/undo", { action_id: action.id }, 200, headers);
  assert.equal((await call(root + "/notes/read", { id })).data, null);
  for (let i = 0; i < 2; i++) {
    const cursor = (await call(root + "/checkpoint")).data.snapshot,
      result = await call(
        root + "/budget",
        { action: "reset_hold", month, snapshot: cursor },
        200,
        headers,
      );
    assert.equal(
      result.data.months[month].notes,
      "",
      "Financial month revival cannot resurrect undone note text",
    );
    assert.equal(result.data.months[month].summary.buffered, 0);
    assert.equal((await call(root + "/notes/read", { id })).data, null);
    const changes = (
      await call(root + "/changes?cursor=" + encodeURIComponent(cursor))
    ).data;
    assert.equal(
      changes.find((c) => c.entity === "budget_months").data.notes,
      "",
    );
    if (i === 0) {
      const hold = (await call(root + "/history", undefined, 200, headers)).data
        .undo;
      await call(root + "/history/undo", { action_id: hold.id }, 200, headers);
    }
  }
  console.log(
    "PASS financial month revive/undo/revive preserves absent annotation, buffered values and feed projection",
  );
} finally {
  await server.stop();
}

// Replacement explicit-ID guarantee; SDK creates strip caller IDs, so no source parity claim.
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
    `${path}: ${(await r.clone().text()).slice(0, 600)}`,
  );
  return r.json();
}
try {
  const budget = (
      await call(
        "/budgets",
        { id: randomUUID(), name: "Explicit note identities" },
        201,
      )
    ).data,
    root = "/budgets/" + budget.id,
    epoch = (await call(root)).data.epoch,
    headers = { "X-Actual-Budget-Epoch": epoch };
  for (const kind of ["accounts", "categories", "category-groups"]) {
    const id = randomUUID(),
      note = "Preserve arbitrary prose for " + kind;
    await call(
      root + "/notes/commands",
      { id, note, snapshot: (await call(root + "/checkpoint")).data.snapshot },
      200,
      headers,
    );
    const cp = (await call(root + "/checkpoint")).data,
      before = await call(root + "/" + kind),
      body = {
        id,
        name: "Explicit " + kind,
        ...(kind === "accounts"
          ? { opening_balance: 0, opening_date: "2026-11-09" }
          : { is_income: false }),
      };
    const refused = await call(root + "/" + kind, body, 409);
    assert.equal(refused.error.code, "note_identity_conflict");
    assert.deepEqual((await call(root + "/notes/read", { id })).data, {
      id,
      note,
    });
    assert.deepEqual((await call(root + "/checkpoint")).data, cp);
    assert.deepEqual(await call(root + "/" + kind), before);
    await call(
      root + "/notes/commands",
      { id, note: "", snapshot: cp.snapshot },
      200,
      headers,
    );
    await call(root + "/" + kind, body, 201);
    assert.equal(
      (await call(root + "/notes/read", { id })).data.note,
      "",
      "Explicit clear resolves the collision without hidden prose",
    );
  }
  console.log(
    "PASS replacement explicit domain IDs refuse retained standalone prose; explicit clear permits creation",
  );
} finally {
  await server.stop();
}

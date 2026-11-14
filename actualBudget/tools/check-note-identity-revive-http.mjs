// Replacement identity guarantee: erased standalone note, new reference, fresh intent.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "This multi-fixture regression requires isolated local servers",
);
let server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200, headers = {}, method) {
  const r = await fetch(api + path, {
    method: method ?? (body === undefined ? "GET" : "POST"),
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
  for (const route of ["notes", "metadata"]) {
    await server.stop();
    server = await startServer();
    api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    const b = (
        await call(
          "/budgets",
          { id: randomUUID(), name: "Erased note " + route },
          201,
        )
      ).data,
      root = "/budgets/" + b.id,
      epoch = (await call(root)).data.epoch,
      id = randomUUID(),
      headers = {
        "X-Actual-Budget-Epoch": epoch,
        "X-Actual-History-Owner": randomUUID(),
        "Idempotency-Key": randomUUID(),
      },
      body = {
        id,
        note: "Original erased annotation",
        snapshot: (await call(root + "/checkpoint")).data.snapshot,
      },
      original = await call(root + "/notes/commands", body, 200, headers),
      action = (await call(root + "/history", undefined, 200, headers)).data
        .undo;
    await call(root + "/history/undo", { action_id: action.id }, 200, {
      ...headers,
      "Idempotency-Key": randomUUID(),
    });
    assert.equal((await call(root + "/notes/read", { id })).data, null);
    await call(
      root + "/accounts",
      {
        id,
        name: "New account",
        opening_balance: 0,
        opening_date: "2026-11-09",
      },
      201,
    );
    assert.equal((await call(root + "/notes/read", { id })).data, null);
    if (route === "notes")
      await call(
        root + "/notes/commands",
        {
          id,
          note: "Fresh reviewed annotation",
          snapshot: (await call(root + "/checkpoint")).data.snapshot,
        },
        200,
        { "X-Actual-Budget-Epoch": epoch },
      );
    else
      await call(
        root + "/accounts/" + id,
        { notes: "Fresh reviewed annotation" },
        200,
        {},
        "PATCH",
      );
    assert.equal(
      (await call(root + "/notes/read", { id })).data.note,
      "Fresh reviewed annotation",
    );
    const backup = (
        await call(root + "/backups", { label: "Revived note identity" }, 201)
      ).data,
      bytes = await (
        await fetch(api + root + "/backups/" + backup.id + "/download")
      ).arrayBuffer(),
      upload = await fetch(api + "/restores", {
        method: "POST",
        headers: {
          "Content-Type": "application/zip",
          "Idempotency-Key": randomUUID(),
        },
        body: bytes,
      });
    assert.equal(upload.status, 201, await upload.clone().text());
    const stage = (await upload.json()).data,
      budget = (await call(root)).data;
    await call(
      root + "/restores/" + stage.id + "/activate",
      { confirm_name: budget.name },
      200,
      { "X-Actual-Budget-Epoch": epoch },
    );
    assert.equal(
      (await call(root + "/notes/read", { id })).data.note,
      "Fresh reviewed annotation",
    );
    assert.deepEqual(
      await call(root + "/notes/commands", body, 200, headers),
      original,
      "Original old-epoch erased-note receipt remains exact",
    );
    assert.equal(
      (await call(root + "/notes/read", { id })).data.note,
      "Fresh reviewed annotation",
    );
    console.log(
      "PASS erased→reference→fresh " +
        route +
        " annotation and portable/exact receipt recovery",
    );
  }
} finally {
  await server.stop();
}

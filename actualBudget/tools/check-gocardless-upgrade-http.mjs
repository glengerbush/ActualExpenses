// Genuine24 → current database/native archive preservation through public HTTP.
import assert from "node:assert/strict";
import { copyFile, mkdtemp, readFile, rm } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { startServer } from "../web/tests/server.mjs";
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "Upgrade proof owns disposable local state",
);
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/gocardless-schema24.json", import.meta.url),
    "utf8",
  ),
);
const files = {};
for (const extension of [".sqlite3", ".zip"]) {
  files[extension] = await readFile(
    new URL(
      "../tests/fixtures/gocardless-schema24" + extension,
      import.meta.url,
    ),
  );
  assert.equal(
    createHash("sha256").update(files[extension]).digest("hex"),
    fixture.artifacts[extension].sha256,
  );
}
for (const mode of ["database", "archive"]) {
  const directory = await mkdtemp(tmpdir() + "/gocardless-upgrade-");
  if (mode === "database")
    await copyFile(
      new URL("../tests/fixtures/gocardless-schema24.sqlite3", import.meta.url),
      directory + "/actualbudget.sqlite3",
    );
  const server = await startServer({ dataDirectory: directory }),
    api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  const root = "/budgets/" + fixture.budget_id;
  async function call(path, body, headers = {}, status = 200, method) {
    const response = await fetch(api + path, {
      method: method ?? (body === undefined ? "GET" : "POST"),
      headers: { "Content-Type": "application/json", ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const data = await response.json();
    assert.equal(
      response.status,
      status,
      mode + " " + path + ": " + JSON.stringify(data),
    );
    return data;
  }
  try {
    if (mode === "archive") {
      const response = await fetch(api + "/restores", {
        method: "POST",
        headers: {
          "Content-Type": "application/zip",
          "Idempotency-Key": randomUUID(),
        },
        body: files[".zip"],
      });
      assert.equal(response.status, 201, await response.clone().text());
      const stage = (await response.json()).data;
      assert.equal(stage.report.schema_version, 25);
      await call(
        `${root}/restores/${stage.id}/activate`,
        { confirm_name: fixture.snapshot.budget.data.name },
        { "Idempotency-Key": randomUUID() },
      );
    }
    const before = (await call(root + "/checkpoint")).data;
    for (const [name, suffix] of [
      ["accounts", "/accounts"],
      ["transactions", "/transactions"],
      ["payees", "/payees"],
      ["categories", "/categories"],
    ]) {
      const actual = await call(root + suffix),
        expected = structuredClone(fixture.snapshot[name]);
      if (mode === "archive") {
        delete actual.snapshot;
        delete expected.snapshot;
      }
      assert.deepEqual(
        actual,
        expected,
        mode +
          " exact original " +
          name +
          " including financial fields/reference IDs/HLCs",
      );
    }
    assert.deepEqual(
      await call(root + "/notes/read", { id: fixture.note_id }),
      fixture.snapshot.note,
      mode + " original notes24 identity/prose",
    );
    const bank = (await call(root + "/banking")).data,
      old = fixture.snapshot.banking.data;
    assert.deepEqual(
      bank.accounts,
      old.accounts,
      mode + " original remote identities/balances/generations",
    );
    assert.deepEqual(
      bank.links,
      old.links,
      mode + " original links/settings revisions/opening/window",
    );
    if (mode === "database") {
      assert.deepEqual(
        bank,
        old,
        "DDL migration preserves all public banking clocks/cursor/metadata",
      );
      assert.deepEqual(await call(root), fixture.snapshot.budget);
      assert.deepEqual(
        await call(root + "/history", undefined, {
          "X-Actual-History-Owner": fixture.owner,
        }),
        fixture.snapshot.history,
      );
      assert.deepEqual(before, fixture.snapshot.checkpoint.data);
    } else {
      assert.notEqual(before.epoch, fixture.snapshot.checkpoint.data.epoch);
      for (const connection of bank.connections) {
        const original = old.connections.find((c) => c.id === connection.id);
        assert.ok(original);
        assert.notEqual(connection.generation, original.generation);
        assert.deepEqual(connection, {
          ...original,
          generation: connection.generation,
          state: "credentials_missing",
          error_code: null,
          message: null,
        });
        await call(
          root + `/banking/connections/${connection.id}/commands`,
          { action: "discover", generation: connection.generation },
          {
            "Idempotency-Key": randomUUID(),
            "X-Actual-Budget-Epoch": before.epoch,
          },
          409,
        );
      }
    }
    for (const receipt of fixture.receipts) {
      assert.deepEqual(
        await call(
          receipt.path,
          receipt.body,
          {
            ...receipt.headers,
            "Idempotency-Key": receipt.key,
            "X-Actual-Receipt-Only": "true",
          },
          receipt.status ?? 200,
          receipt.method,
        ),
        receipt.response,
        mode + " original exact receipt",
      );
    }
    assert.deepEqual(
      (await call(root + "/checkpoint")).data,
      before,
      "receipt probes/read-only checks leave current checkpoint intact",
    );
    const pending = fixture.captured_request;
    if (mode === "database") {
      const changed = await call(
        pending.path,
        pending.body,
        {
          ...pending.headers,
          "Idempotency-Key": pending.key,
          "X-Actual-History-Owner": randomUUID(),
          "X-Actual-Cancel-If-Uncommitted": "true",
        },
        409,
      );
      assert.notEqual(
        changed.error.code,
        "operation_canceled",
        "Genuine old capture must retain its full owner/clock/body identity after DDL migration",
      );
      assert.deepEqual((await call(root + "/checkpoint")).data, before);
    }
    await call(
      pending.path,
      pending.body,
      {
        ...pending.headers,
        "Idempotency-Key": pending.key,
        "X-Actual-Receipt-Only": "true",
      },
      404,
    );
    const canceled = await call(
      pending.path,
      pending.body,
      {
        ...pending.headers,
        "Idempotency-Key": pending.key,
        "X-Actual-Cancel-If-Uncommitted": "true",
      },
      409,
    );
    assert.equal(canceled.error.code, "operation_canceled");
    assert.deepEqual(
      await call(
        pending.path,
        pending.body,
        { ...pending.headers, "Idempotency-Key": pending.key },
        409,
      ),
      canceled,
    );
    assert.deepEqual(
      (await call(root + "/transactions")).data,
      fixture.snapshot.transactions.data,
      "canceling retained old request neither imports nor resurrects a row",
    );
    const headers = {
      "X-Actual-Budget-Epoch": before.epoch,
      "Idempotency-Key": randomUUID(),
    };
    const added = (
      await call(
        root + "/banking/connections",
        {
          id: randomUUID(),
          provider: "gocardless",
          name: "After genuine24 upgrade",
          secret_id: "SYNTHETIC-ID",
          secret_key: "SYNTHETIC-KEY",
        },
        headers,
        201,
      )
    ).data;
    assert.equal(added.provider, "gocardless");
    assert.deepEqual(
      (await call(root + "/banking")).data.connections.filter(
        (c) => c.id !== added.id,
      ),
      bank.connections,
      "new provider leaves every old connection unchanged",
    );
    assert.deepEqual(
      await call(root + "/notes/read", { id: fixture.note_id }),
      fixture.snapshot.note,
    );
    console.log(
      "PASS genuine24 " +
        mode +
        ": exact financial/reference/HLC/history/receipt/notes preservation, retained capture cancellation and new GoCardless custody",
    );
  } finally {
    await server.stop();
    await rm(directory, { recursive: true, force: true });
  }
}

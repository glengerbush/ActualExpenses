// Public retirement contract; immutable genuine24 financial state stays usable.
import assert from "node:assert/strict";
import { copyFile, mkdtemp, readFile, rm } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { startServer } from "../web/tests/server.mjs";
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "Genuine upgrade proof owns disposable copied state",
);
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/gocardless-schema24.json", import.meta.url),
    "utf8",
  ),
);
const files = {};
for (const ext of [".sqlite3", ".zip"]) {
  files[ext] = await readFile(
    new URL("../tests/fixtures/gocardless-schema24" + ext, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(files[ext]).digest("hex"),
    fixture.artifacts[ext].sha256,
  );
}
for (const mode of ["database", "archive"]) {
  const directory = await mkdtemp(tmpdir() + "/bank-retirement-");
  if (mode === "database")
    await copyFile(
      new URL("../tests/fixtures/gocardless-schema24.sqlite3", import.meta.url),
      directory + "/actualbudget.sqlite3",
    );
  const server = await startServer({ dataDirectory: directory });
  const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  async function call(path, body, status = 200, headers = {}) {
    const res = await fetch(api + path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
        ...headers,
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    assert.equal(res.status, status, mode + " " + path + ": " + text);
    return text ? JSON.parse(text) : null;
  }
  try {
    const info = await call("/info");
    assert.ok(
      !info.data.capabilities.some((c) =>
        /banking|simplefin|gocardless|bank_sync/i.test(c),
      ),
      "No operational external-bank capability",
    );
    const contract = await call("/openapi.json");
    assert.ok(
      !Object.keys(contract.paths).some((p) => /\/banking(?:\/|$)/.test(p)),
      "No external-bank contract routes",
    );
    const root = "/budgets/" + fixture.budget_id;
    if (mode === "archive") {
      const res = await fetch(api + "/restores", {
        method: "POST",
        headers: {
          "Content-Type": "application/zip",
          "Idempotency-Key": randomUUID(),
        },
        body: files[".zip"],
      });
      assert.equal(res.status, 201, await res.clone().text());
      const stage = (await res.json()).data;
      assert.equal(stage.report.schema_version, 27);
      assert.ok(
        stage.report.retired_external_connections > 0,
        "Legacy native conversion reports removed external configuration",
      );
      await call(root + "/restores/" + stage.id + "/activate", {
        confirm_name: fixture.snapshot.budget.data.name,
      });
    }
    for (const [name, path] of [
      ["accounts", "/accounts"],
      ["transactions", "/transactions"],
      ["payees", "/payees"],
      ["categories", "/categories"],
    ]) {
      const actual = await call(root + path);
      assert.deepEqual(
        actual.data,
        fixture.snapshot[name].data,
        "Original financial/reference/status/imported fields remain exact: " +
          name,
      );
    }
    const before = await call(root + "/checkpoint");
    const preferences = (await call(root + "/preferences")).data.experimental;
    assert.ok(
      !("enableBanking" in preferences) && !("akahuBankSync" in preferences),
    );
    const connection =
      fixture.snapshot.banking?.data?.connections?.[0]?.id ?? randomUUID();
    for (const path of [
      "/banking",
      "/banking/connections/" +
        connection +
        "/gocardless/institutions?country=GB",
      "/accounts/" + fixture.snapshot.accounts.data[0].id + "/banking",
    ])
      await call(root + path, undefined, 404);
    for (const path of [
      "/banking/connections",
      "/banking/connections/" + connection + "/commands",
    ])
      await call(
        root + path,
        {
          provider: "simplefin",
          setup_token: "SYNTHETIC-RETIRED-SECRET",
          action: "discover",
        },
        404,
      );
    assert.deepEqual(
      await call(root + "/checkpoint"),
      before,
      "Retired requests cannot change any budget state",
    );
    if (mode === "database") {
      const owned = {
        "X-Actual-History-Owner": fixture.owner,
        "X-Actual-Budget-Epoch": before.data.epoch,
      };
      assert.equal(
        (await call(root + "/history", undefined, 200, owned)).data.undo.id,
        fixture.snapshot.history.data.undo.id,
      );
      let undos = 0;
      for (; undos < 20; undos++) {
        const next = (await call(root + "/history", undefined, 200, owned)).data
          .undo;
        if (!next) break;
        await call(root + "/history/undo", { action_id: next.id }, 200, owned);
      }
      assert.ok(
        undos > 0 && undos < 20,
        "Preserved history drains through old financial imports",
      );
      for (let n = 0; n < undos; n++) {
        const next = (await call(root + "/history", undefined, 200, owned)).data
          .redo;
        assert.ok(next);
        await call(root + "/history/redo", { action_id: next.id }, 200, owned);
      }
      // Owned inverses mint fresh field clocks; initial upgrade comparison above
      // still requires every original clock unchanged before user mutations.
      const financial = (rows) => rows.map(({ field_versions, ...row }) => row);
      assert.deepEqual(
        financial((await call(root + "/transactions")).data),
        financial(fixture.snapshot.transactions.data),
        "Undo/redo restores every original financial/import field and identity",
      );
    }
    console.log(
      "PASS external-bank retirement genuine24 " +
        mode +
        " financial identity + removed routes/capability",
    );
  } finally {
    await server.stop();
    await rm(directory, { recursive: true, force: true });
  }
}

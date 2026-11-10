// Genuine linked Actual archive -> manual account -> portable native round trip.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/bank-linked-source-26.10.json", import.meta.url),
  ),
).cases[0];
const bytes = await readFile(
  new URL(
    "../tests/fixtures/bank-sync-source/" + fixture.archive.filename,
    import.meta.url,
  ),
);
assert.equal(
  createHash("sha256").update(bytes).digest("hex"),
  fixture.archive.sha256,
);
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200, method) {
  const response = await fetch(api + path, {
    method: method ?? (body === undefined ? "GET" : "POST"),
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: Buffer.isBuffer(body)
      ? body
      : body === undefined
        ? undefined
        : JSON.stringify(body),
  });
  assert.equal(
    response.status,
    status,
    path + ": " + (await response.clone().text()),
  );
  return response.headers.get("content-type")?.includes("application/json")
    ? response.json()
    : Buffer.from(await response.arrayBuffer());
}
function financial(rows, source = false) {
  return rows
    .map((t) => ({
      id: t.id,
      account: t[source ? "account" : "account_id"],
      category: t[source ? "category" : "category_id"] ?? null,
      payee: t[source ? "payee" : "payee_id"] ?? null,
      amount: t.amount,
      date: t.date,
      notes: t.notes ?? "",
      cleared: !!t.cleared,
      reconciled: !!t.reconciled,
      starting: !!t[source ? "starting_balance_flag" : "starting_balance"],
      imported_id: t.imported_id ?? null,
      imported_payee: t.imported_payee ?? null,
      raw_synced_data: t.raw_synced_data ?? null,
      parent: t.parent_id ?? null,
      transfer: t.transfer_id ?? null,
    }))
    .sort((a, b) => a.id.localeCompare(b.id));
}
function noFlags(value) {
  assert.ok(!("enableBanking" in value.experimental));
  assert.ok(!("akahuBankSync" in value.experimental));
}
try {
  const stage = (await call("/imports/actual", bytes, 201)).data;
  assert.ok(
    stage.report.requirements.some(
      (r) =>
        r.feature === "external_bank_connections_retired" &&
        r.count > 0 &&
        !r.blocking,
    ),
  );
  assert.deepEqual(await call("/imports/" + stage.id + "/original.zip"), bytes);
  await call("/imports/" + stage.id + "/activate", {}, 201);
  const root = "/budgets/" + stage.budget_id;
  const original = (await call(root + "/transactions")).data;
  assert.deepEqual(
    financial(original),
    financial(fixture.snapshot.transactions, true),
  );
  const accounts = (await call(root + "/accounts")).data;
  assert.deepEqual(
    accounts.map((a) => [a.id, a.name, a.off_budget, a.closed]),
    fixture.snapshot.accounts.map((a) => [
      a.id,
      a.name,
      !!a.offbudget,
      !!a.closed,
    ]),
  );
  noFlags((await call(root + "/preferences")).data);
  for (const name of ["enableBanking", "akahuBankSync"]) {
    await call(
      root + "/preferences",
      { experimental: { [name]: true } },
      400,
      "PATCH",
    );
  }
  const converted = (
    await call("/preferences/convert-actual", {
      synced: { "flags.enableBanking": true, "flags.akahuBankSync": true },
    })
  ).data;
  noFlags(converted.shared);
  assert.equal(
    converted.warnings.filter((w) =>
      /External bank connection feature removed/.test(w.reason),
    ).length,
    2,
  );
  const backup = (
    await call(root + "/backups", { label: "Manual-only round trip" }, 201)
  ).data;
  const archive = await call(root + "/backups/" + backup.id + "/download");
  const portable = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        "import sys,io,zipfile,json; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); print(json.dumps({'manifest':json.loads(z.read('manifest.json')),'state':json.loads(z.read('budget.json'))}))",
      ],
      { input: archive, encoding: "utf8" },
    ),
  );
  assert.equal(portable.manifest.schema_version, 27);
  assert.ok(
    !Object.keys(portable.state.tables).some((name) =>
      name.startsWith("bank_"),
    ),
  );
  const versions = portable.state.tables.field_versions;
  const entity = versions.columns.indexOf("entity");
  assert.ok(!versions.rows.some((row) => row[entity].startsWith("bank_")));
  const prefs = portable.state.tables.budget_preferences;
  for (const row of prefs.rows)
    noFlags(JSON.parse(row[prefs.columns.indexOf("settings")]));
  const restored = (await call("/restores", archive, 201)).data;
  await call(root + "/restores/" + restored.id + "/activate", {
    confirm_name: portable.state.budget.name,
  });
  assert.deepEqual(
    financial((await call(root + "/transactions")).data),
    financial(original),
  );
  noFlags((await call(root + "/preferences")).data);
  console.log(
    "PASS genuine linked source becomes manual; exact financial/import data, original retained, retired flags refused and native archive round trip",
  );
} finally {
  await server.stop();
}

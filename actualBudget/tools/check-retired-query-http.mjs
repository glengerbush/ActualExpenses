// Genuine source deletion, imported saved filters, and portable native recovery.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "This multi-fixture regression requires isolated local servers",
);
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/retired-query-saved-source.json",
      import.meta.url,
    ),
  ),
);
let server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function freshServer() {
  await server.stop();
  server = await startServer();
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
}
async function call(path, body, status = 200, headers = {}) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(
    response.status,
    status,
    `${path}: ${(await response.clone().text()).slice(0, 800)}`,
  );
  return response.json();
}
async function upload(path, bytes) {
  const response = await fetch(api + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  assert.equal(response.status, 201, await response.clone().text());
  return (await response.json()).data;
}
const ids = (rows) => rows.map((r) => r.id).sort();
const conditionOp = { is: "eq", oneOf: "in" };
try {
  for (const mode of fixture.cases) {
    await freshServer();
    const bytes = await readFile(
      new URL("../tests/fixtures/" + mode.archive.filename, import.meta.url),
    );
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      mode.archive.sha256,
    );
    const staged = await upload("/imports/actual", bytes),
      activated = (await call("/imports/" + staged.id + "/activate", {}, 201))
        .data,
      id = activated.id,
      root = "/budgets/" + id;
    const settings = (await call(root + "/register-settings")).data.settings;
    async function verify(label) {
      const checkpoint = (await call(root + "/checkpoint")).data,
        ledger = await call(root + "/transactions?limit=1000");
      const flat = ledger.data
        .flatMap((r) => [r, ...(r.children ?? [])])
        .map((r) => ({
          id: r.id,
          amount: r.amount,
          category: r.category_id ?? null,
          parent_id: r.parent_id ?? null,
          transfer_id: r.transfer_id ?? null,
        }))
        .sort((a, b) => a.id.localeCompare(b.id));
      assert.deepEqual(
        flat,
        mode.after,
        mode.id + " complete source financial graph",
      );
      for (const item of mode.cases) {
        const saved = settings.saved_filters.find((f) => f.name === item.id);
        assert.ok(saved, "Imported source saved filter " + item.id);
        const expected = {
          ...item.condition,
          op: conditionOp[item.condition.op],
        };
        const { options, ...condition } = saved.query.filter.conditions[0];
        assert.deepEqual(
          condition,
          expected,
          "Saved category ID remains literal",
        );
        const result = await call(
          root +
            "/transactions?limit=1000&query=" +
            encodeURIComponent(JSON.stringify(saved.query)),
        );
        assert.deepEqual(
          ids(result.data),
          ids(
            item.grouped.filter(
              (r) => !mode.after.find((s) => s.id === r.id)?.parent_id,
            ),
          ),
          `${mode.id}/${item.id} register ${label}`,
        );
        const queried = await call(root + "/query", {
          version: 1,
          table: "transactions",
          select: ["id", "amount", "category"],
          filter: [item.filter],
          order_by: ["id"],
          limit: 1000,
          snapshot: checkpoint.snapshot,
        });
        assert.deepEqual(
          queried.data,
          item.data,
          `${mode.id}/${item.id} literal ActualQL output`,
        );
        const report = (
          await call(root + "/reports/query", {
            version: 1,
            kind: "custom",
            as_of: "2026-11-09",
            options: item.options,
          })
        ).data.result;
        delete report.legend;
        assert.deepEqual(
          report,
          item.report,
          `${mode.id}/${item.id} complete custom spreadsheet ${label}`,
        );
      }
      assert.deepEqual(
        (await call(root + "/checkpoint")).data,
        checkpoint,
        "All projections read-only",
      );
      assert.deepEqual(await call(root + "/transactions?limit=1000"), ledger);
    }
    await verify("import");
    const backup = (
        await call(
          root + "/backups",
          { label: "Literal saved category filters" },
          201,
        )
      ).data,
      archive = Buffer.from(
        await (
          await fetch(api + root + "/backups/" + backup.id + "/download")
        ).arrayBuffer(),
      ),
      restore = await upload("/restores", archive),
      budget = (await call(root)).data;
    await call(
      root + "/restores/" + restore.id + "/activate",
      { confirm_name: budget.name },
      200,
      { "X-Actual-Budget-Epoch": budget.epoch },
    );
    assert.deepEqual(
      (await call(root + "/register-settings")).data.settings,
      settings,
      "Native restore preserves complete saved filter definitions",
    );
    await verify("native restore");
    console.log(
      "PASS " +
        mode.id +
        " literal category IDs, exact report, imported saved filters/native roundtrip",
    );
  }
} finally {
  await server.stop();
}

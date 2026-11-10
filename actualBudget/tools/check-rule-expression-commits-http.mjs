// Persist/retry/refusal through the public API against real rule-apply-actions.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/rule-expression-commits-source.json",
      import.meta.url,
    ),
  ),
);
const archive = await readFile(
  new URL("../tests/fixtures/" + fixture.archive.filename, import.meta.url),
);
assert.equal(
  createHash("sha256").update(archive).digest("hex"),
  fixture.archive.sha256,
);
function project(rows, subject) {
  return rows
    .flatMap((r) => [r, ...(r.children ?? [])])
    .filter((r) => r.id === subject || r.parent_id === subject)
    .map((r) => ({
      id: r.id === subject ? subject : "generated-child",
      parent: !!r.parent_id,
      amount: r.amount,
      notes: r.notes ?? "",
      date: r.date,
      account: r.account_id,
      category: r.category_id,
      payee: r.payee_id,
      cleared: r.cleared,
      is_parent: !!r.is_parent,
      is_child: !!r.parent_id,
    }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
for (const test of fixture.cases) {
  const server = await startServer();
  const base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  async function call(path, body, key = randomUUID()) {
    const response = await fetch(base + path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        "Content-Type": Buffer.isBuffer(body)
          ? "application/zip"
          : "application/json",
        "Idempotency-Key": key,
      },
      body:
        body === undefined
          ? undefined
          : Buffer.isBuffer(body)
            ? body
            : JSON.stringify(body),
    });
    return { status: response.status, ...(await response.json()) };
  }
  try {
    const stage = await call("/imports/actual", archive);
    assert.equal(stage.status, 201, JSON.stringify(stage));
    assert.equal(stage.data.status, "ready", JSON.stringify(stage));
    assert.equal(
      (await call(`/imports/${stage.data.id}/activate`, {})).status,
      201,
    );
    const root = `/budgets/${stage.data.budget_id}`;
    const imported = await call(root + "/rules");
    // Source serializes inferred type metadata; native derives it from fields.
    for (const rule of fixture.rules)
      assert.deepEqual(
        imported.data.find((r) => r.id === rule.id),
        {
          ...rule,
          conditions: rule.conditions.map(({ type, ...value }) => value),
          actions: rule.actions.map(({ type, ...value }) => value),
        },
      );
    const checkpoint = await call(root + "/checkpoint"),
      id = randomUUID();
    const rule = {
      id,
      stage: null,
      conditionsOp: "and",
      conditions: fixture.rules[0].conditions,
      actions: test.actions,
    };
    const created = await call(root + "/rules", {
      snapshot: checkpoint.data.snapshot,
      ...rule,
    });
    assert.equal(created.status, 201, JSON.stringify(created));
    const before = await call(root + "/transactions"),
      stamp = await call(root + "/checkpoint");
    assert.deepEqual(
      project(before.data, fixture.subject),
      test.before,
      test.name + " initial ledger",
    );
    const command = {
        snapshot: stamp.data.snapshot,
        ids: [fixture.subject],
        rule_ids: [id],
      },
      key = randomUUID();
    const applied = await call(root + "/rules/apply", command, key);
    if (test.error) {
      assert.ok(
        [400, 422].includes(applied.status),
        test.name + JSON.stringify(applied),
      );
      assert.deepEqual(
        await call(root + "/transactions"),
        before,
        test.name + " atomic refusal",
      );
      assert.deepEqual(
        await call(root + "/checkpoint"),
        stamp,
        test.name + " checkpoint unchanged",
      );
    } else {
      assert.equal(applied.status, 200, test.name + JSON.stringify(applied));
      const after = await call(root + "/transactions"),
        saved = await call(root + "/checkpoint");
      const expected = [...test.after].sort((a, b) =>
        JSON.stringify(a).localeCompare(JSON.stringify(b)),
      );
      // Register parents display shared child category/payee summaries. Compare
      // source persisted rows through the public query seam, without summaries.
      const financial = await call(root + "/query", {
        version: 1,
        table: "transactions",
        splits: "all",
        snapshot: saved.data.snapshot,
        select: [
          "id",
          "account",
          "category",
          "payee",
          "amount",
          "notes",
          "date",
          "cleared",
          "is_parent",
          "parent_id",
        ],
        limit: 1000,
      });
      assert.equal(financial.status, 200, JSON.stringify(financial));
      assert.deepEqual(
        project(
          financial.data.map(({ account, category, payee, ...r }) => ({
            ...r,
            account_id: account,
            category_id: category,
            payee_id: payee,
          })),
          fixture.subject,
        ),
        expected,
        test.name,
      );
      // Running balances are derived and must change when an earlier amount
      // changes. Every persisted field/version on unrelated rows stays exact.
      const unrelated = (rows) =>
        rows
          .filter(
            (r) => r.id !== fixture.subject && r.parent_id !== fixture.subject,
          )
          .map(({ running_balance, ...r }) => r);
      assert.deepEqual(
        unrelated(after.data),
        unrelated(before.data),
        test.name + " unrelated rows",
      );
      assert.deepEqual(
        await call(root + "/rules/apply", command, key),
        applied,
        test.name + " retry receipt",
      );
      assert.deepEqual(
        await call(root + "/checkpoint"),
        saved,
        test.name + " retry writes nothing",
      );
    }
  } finally {
    await server.stop();
  }
}
console.log(
  `PASS ${fixture.cases.length} source rule writes/refusals, import and idempotent retries`,
);

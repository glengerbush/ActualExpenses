// Public preview seam against the independent running-Actual observations.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL(
      process.argv.includes("--nonfinite")
        ? "../tests/fixtures/rule-expression-nonfinite-source.json"
        : process.argv.includes("--execution")
          ? "../tests/fixtures/rule-expression-execution-source.json"
          : process.argv.includes("--chains")
            ? "../tests/fixtures/rule-expression-chain-source.json"
            : process.argv.includes("--context")
              ? "../tests/fixtures/rule-expression-context-source.json"
              : "../tests/fixtures/rule-expressions-source.json",
      import.meta.url,
    ),
  ),
);
const server = await startServer(),
  base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const response = await fetch(base + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
  const value = await response.json();
  return { status: response.status, ...value };
}
function native(row) {
  return Object.fromEntries(
    Object.entries(row).map(([key, value]) => [
      { account: "account_id", category: "category_id", payee: "payee_id" }[
        key
      ] ?? key,
      value,
    ]),
  );
}
function projection(row, source = false, payeeName = null) {
  const mapped = source ? native(row) : row;
  const value = Object.fromEntries(
    [
      "amount",
      "notes",
      "date",
      "cleared",
      "account_id",
      "category_id",
      "payee_id",
    ].map((k) => [k, mapped[k] ?? null]),
  );
  if (payeeName) value.payee_id = "generated-payee";
  value.rule_errors = (source ? row._ruleErrors : row.rule_errors) ?? [];
  value.split_error =
    row.error?.type === "SplitTransactionError" ? row.error : null;
  value.children =
    (source ? row.subtransactions : row.children)?.map((c) =>
      projection(c, source),
    ) ?? [];
  // Native previews use the persisted ledger shape. Source rules-run leaves
  // empty child notes undefined and computes a display-only parent category;
  // the independent rule-apply-actions fixture proves empty text/null on save.
  value.notes ??= "";
  if (value.children.length) value.category_id = null;
  return value;
}
try {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + fixture.seed.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    fixture.seed.sha256,
  );
  const stage = await call("/imports/actual", bytes);
  assert.equal(stage.status, 201);
  if (stage.data.status === "ready")
    assert.equal(
      (await call(`/imports/${stage.data.id}/activate`, {})).status,
      201,
    );
  const root = `/budgets/${stage.data.budget_id}`,
    before = await call(root + "/transactions"),
    checkpoint = await call(root + "/checkpoint");
  const filter = process.argv.find((a) => a.startsWith("--case="))?.slice(7);
  let count = 0;
  for (const test of fixture.cases.filter(
    (c) => !filter || c.name === filter,
  )) {
    let stamp = checkpoint;
    if (test.rules) {
      for (const rule of test.rules) {
        stamp = await call(root + "/checkpoint");
        const created = await call(root + "/rules", {
          ...rule,
          snapshot: stamp.data.snapshot,
        });
        assert.equal(created.status, 201, JSON.stringify(created));
      }
    }
    stamp = await call(root + "/checkpoint");
    const response = await call(root + "/rules/preview", {
      snapshot: stamp.data.snapshot,
      ...(test.stored
        ? { ids: [test.transaction.id] }
        : { transaction: native(test.transaction) }),
      ...(test.rules
        ? { rule_ids: test.rules.map((r) => r.id) }
        : { rule: test.rule }),
    });
    assert.ok(
      !JSON.stringify(response).includes("$actualRuleNumber"),
      test.name,
    );
    if (test.error) {
      assert.ok([400, 422].includes(response.status), test.name);
      assert.match(response.error.message, /template|handlebars/i);
    } else {
      assert.equal(
        response.status,
        200,
        test.name + " " + JSON.stringify(response),
      );
      const actual = response.data.transactions[0].after,
        expected = projection(test.output, true, test.payee_name);
      // This one non-financial template renders the server's current UTC date.
      // All source dates are frozen; explicit transaction-date results stay exact.
      if (test.name === "template transaction fields")
        expected.notes = expected.notes.replace(
          /\|2026-11-09$/,
          "|" + new Date().toISOString().slice(0, 10),
        );
      assert.deepEqual(
        projection(actual, false, test.payee_name),
        expected,
        test.name,
      );
      if (test.payee_name) assert.equal(actual.payee_name, test.payee_name);
      if (test.output._deleted) assert.equal(actual.deleted, true);
    }
    assert.deepEqual(
      await call(root + "/checkpoint"),
      stamp,
      test.name + " read-only checkpoint",
    );
    count++;
  }
  if (process.argv.includes("--execution") && !filter) {
    const subject = before.data.find(
      (row) =>
        row.amount < -1 &&
        !row.is_parent &&
        !row.parent_id &&
        !row.starting_balance,
    );
    assert.ok(subject);
    const rule = {
      ...fixture.cases.find((c) => c.name === "unbalanced calculated split")
        .rule,
      id: randomUUID(),
      conditions: [{ field: "account", op: "is", value: subject.account_id }],
    };
    const stamp = await call(root + "/checkpoint");
    assert.equal(
      (await call(root + "/rules", { ...rule, snapshot: stamp.data.snapshot }))
        .status,
      201,
    );
    const ready = await call(root + "/checkpoint");
    const refused = await call(root + "/rules/apply", {
      snapshot: ready.data.snapshot,
      ids: [subject.id],
      rule_ids: [rule.id],
    });
    assert.equal(refused.status, 400);
    assert.match(refused.error.message, /split allocations do not equal/);
    assert.deepEqual(await call(root + "/checkpoint"), ready);
  }
  const after = await call(root + "/transactions");
  const changesDefinitions =
    process.argv.includes("--execution") ||
    fixture.cases.some((test) => test.rules);
  if (changesDefinitions) {
    const { snapshot: beforeSnapshot, ...beforeRows } = before;
    const { snapshot: afterSnapshot, ...afterRows } = after;
    assert.deepEqual(afterRows, beforeRows);
  } else assert.deepEqual(after, before);
  if (!changesDefinitions)
    assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
  console.log(
    `PASS ${count} source rule-expression previews and unchanged financial state`,
  );
} finally {
  await server.stop();
}

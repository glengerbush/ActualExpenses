// Shared calculated-work allowance at the real template projection API.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "../web/tests/server.mjs";

const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, status = 200, headers = {}) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
    signal: AbortSignal.timeout(25000),
  });
  const value = await response.json();
  assert.equal(response.status, status, JSON.stringify(value));
  return value;
}
try {
  const fixture = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/budget-template-expressions.json",
        import.meta.url,
      ),
    ),
  );
  const test = fixture.cases[0];
  const bytes = await readFile(
    new URL(
      "../tests/fixtures/budget-template-expressions/" + test.archive.filename,
      import.meta.url,
    ),
  );
  const imported = await call("/imports/actual", bytes, 201);
  assert.equal(imported.data.status, "ready");
  await call(`/imports/${imported.data.id}/activate`, {}, 201);
  const root = `/budgets/${imported.data.budget_id}`;
  const before = (await call(root + "/checkpoint")).data;
  await call(
    root + "/schedules/commands",
    {
      action: "update",
      id: test.ids.Rent,
      snapshot: before.snapshot,
      actions: [
        { op: "link-schedule", value: test.ids.Rent },
        ...Array.from({ length: 7 }, () => ({
          op: "set",
          field: "notes",
          value: "",
          options: { formula: '=CONCAT("bounded", " calculation")' },
        })),
      ],
    },
    200,
    { "X-Actual-Budget-Epoch": before.epoch },
  );
  const checkpoint = await call(root + "/checkpoint");
  const ledger = await call(root + "/transactions");
  const body = {
    month: test.month,
    snapshot: checkpoint.data.snapshot,
    category_id: test.ids.A,
    templates: Array(16).fill(test.templates.A[0]),
  };
  const started = performance.now();
  const refused = await call(root + "/budget/templates/projection", body);
  const elapsed = performance.now() - started;
  assert.match(
    JSON.stringify(refused.data.errors),
    /Calculated rule work limit reached/,
  );
  assert.equal(refused.data.budgeted, 0);
  assert.ok(refused.data.per_template.every((value) => value === 0));
  assert.ok(elapsed < 15000, `Cumulative execution took ${elapsed}ms`);
  assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
  assert.deepEqual(await call(root + "/transactions"), ledger);
  const recovered = await call(root + "/budget/templates/projection", {
    ...body,
    templates: test.templates.A,
  });
  assert.deepEqual(recovered.data.errors, []);
  assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
  console.log(
    `PASS: template calculations share a request limit, preserve state and recover (${Math.round(elapsed)}ms)`,
  );
} finally {
  await server.stop();
}

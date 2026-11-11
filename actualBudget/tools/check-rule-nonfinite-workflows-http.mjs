// Invalid calculated amounts cannot crash file matching or reach financial writes.
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
  });
  const result = await response.json();
  assert.equal(response.status, status, path + ": " + JSON.stringify(result));
  return result;
}
try {
  for (const mode of ["amount", "split", "deleted"]) {
    const split = mode === "split",
      deleted = mode === "deleted";
    const budget = randomUUID(),
      account = randomUUID(),
      root = `/budgets/${budget}`;
    await call(
      "/budgets",
      { id: budget, name: `Nonfinite ${split ? "allocation" : "amount"}` },
      201,
    );
    await call(
      root + "/accounts",
      {
        id: account,
        name: "Spending",
        opening_balance: 0,
        opening_date: "2026-01-01",
      },
      201,
    );
    let cp = (await call(root + "/checkpoint")).data;
    await call(
      root + "/rules",
      {
        id: randomUUID(),
        snapshot: cp.snapshot,
        stage: null,
        conditionsOp: "and",
        conditions: [
          { field: "account", op: "is", value: account },
          { field: "notes", op: "is", value: "invalid" },
        ],
        actions: split
          ? [
              {
                op: "set-split-amount",
                value: null,
                options: {
                  splitIndex: 1,
                  method: "formula",
                  formula: '="Infinity"',
                },
              },
              {
                op: "set-split-amount",
                value: null,
                options: { splitIndex: 2, method: "remainder" },
              },
            ]
          : [
              {
                op: "set",
                field: "amount",
                value: 0,
                options: { template: "Infinity" },
              },
              ...(deleted ? [{ op: "delete-transaction" }] : []),
            ],
      },
      201,
    );
    const upload = await fetch(
      `${api}${root}/accounts/${account}/transaction-imports?filename=expenses.csv`,
      {
        method: "POST",
        headers: { "Idempotency-Key": randomUUID() },
        body: "Date,Amount,Memo\n2026-11-09,-1.00,invalid\n2026-11-09,-2.00,valid\n",
      },
    );
    assert.equal(upload.status, 201);
    const path = root + "/transaction-imports/" + (await upload.json()).data.id;
    cp = (await call(root + "/checkpoint")).data;
    const before = await call(root + "/transactions");
    const body = {
      snapshot: cp.snapshot,
      mapping: { date: "Date", amount: "Amount", notes: "Memo" },
      options: { has_header_row: true, reconcile: true },
    };
    const preview = (await call(path + "/preview", body)).data;
    assert.equal(preview.errors.length, deleted ? 0 : 1);
    if (deleted) {
      assert.equal(preview.rows[0].transaction.deleted, true);
      assert.deepEqual(
        preview.rows.map((row) => row.row),
        [0, 1],
      );
    } else {
      assert.equal(preview.errors[0].row, 0);
      assert.match(preview.errors[0].message, /exact|minor|amount/i);
      assert.deepEqual(
        preview.rows.map((row) => row.row),
        [1],
      );
    }
    assert.deepEqual((await call(root + "/checkpoint")).data, cp);
    assert.deepEqual(await call(root + "/transactions"), before);
    const headers = { "X-Actual-Budget-Epoch": cp.epoch };
    if (!deleted)
      await call(
        path + "/commit",
        {
          ...body,
          rows: [
            { row: 0, action: "add" },
            { row: 1, action: "add" },
          ],
        },
        400,
        headers,
      );
    assert.deepEqual((await call(root + "/checkpoint")).data, cp);
    assert.deepEqual(await call(root + "/transactions"), before);
    const command = {
      ...body,
      rows: [
        ...(deleted ? [{ row: 0, action: "add" }] : []),
        { row: 1, action: "add" },
      ],
    };
    const key = { ...headers, "Idempotency-Key": randomUUID() };
    const saved = await call(path + "/commit", command, 200, key);
    const ledger = (await call(root + "/transactions")).data;
    assert.equal(ledger.length, 1);
    assert.equal(ledger[0].amount, -200);
    assert.equal(ledger[0].notes, "valid");
    assert.deepEqual(await call(path + "/commit", command, 200, key), saved);
  }
  console.log(
    "PASS: nonfinite file amounts/allocations report row errors, refuse atomically and allow valid rows with exact retry",
  );
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
  await call(`/imports/${imported.data.id}/activate`, {}, 201);
  const root = `/budgets/${imported.data.budget_id}`;
  for (const split of [false, true]) {
    const cp = (await call(root + "/checkpoint")).data;
    await call(
      root + "/schedules/commands",
      {
        action: "update",
        id: test.ids.Rent,
        snapshot: cp.snapshot,
        actions: [
          { op: "link-schedule", value: test.ids.Rent },
          ...(split
            ? [
                {
                  op: "set-split-amount",
                  value: null,
                  options: {
                    splitIndex: 1,
                    method: "formula",
                    formula: '= "Infinity"',
                  },
                },
                {
                  op: "set-split-amount",
                  value: null,
                  options: { splitIndex: 2, method: "remainder" },
                },
              ]
            : [
                {
                  op: "set",
                  field: "amount",
                  value: 0,
                  options: { template: "Infinity" },
                },
              ]),
        ],
      },
      200,
      { "X-Actual-Budget-Epoch": cp.epoch },
    );
    const checkpoint = await call(root + "/checkpoint"),
      ledger = await call(root + "/transactions");
    const result = await call(root + "/budget/templates/projection", {
      month: test.month,
      snapshot: checkpoint.data.snapshot,
      category_id: test.ids.A,
      templates: test.templates.A,
    });
    assert.match(JSON.stringify(result.data.errors), /finite|amount/i);
    assert.equal(result.data.budgeted, 0);
    assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
    assert.deepEqual(await call(root + "/transactions"), ledger);
  }
  console.log(
    "PASS: nonfinite schedule template amounts report errors without substituting an amount or mutating the budget",
  );
} finally {
  await server.stop();
}

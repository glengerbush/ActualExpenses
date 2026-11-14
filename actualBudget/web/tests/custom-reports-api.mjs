// Public custom financial/query surface; independent pinned source expectations.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
let server = await startServer();
let browser;
try {
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage();
  await page.goto(server.url);
  const api = (path, body, headers = {}, method) =>
    page.evaluate(
      async ({ path, body, headers, method }) => {
        const r = await fetch("/api/v1" + path, {
          method: method || (body === undefined ? "GET" : "POST"),
          headers: { "Content-Type": "application/json", ...headers },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        return { status: r.status, ...(await r.json()) };
      },
      { path, body, headers, method },
    );
  const source = JSON.parse(
      await readFile(
        new URL(
          "../../tests/fixtures/custom-reports-visible-source-26.10.json",
          import.meta.url,
        ),
        "utf8",
      ),
    ),
    bytes = (
      await readFile(
        new URL(
          "../../tests/fixtures/reports-source-26.10.zip",
          import.meta.url,
        ),
      )
    ).toString("base64");
  const staged = await page.evaluate(
    async ({ bytes, key }) => {
      const r = await fetch("/api/v1/imports/actual", {
        method: "POST",
        headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
        body: Uint8Array.from(atob(bytes), (c) => c.charCodeAt(0)),
      });
      return { status: r.status, ...(await r.json()) };
    },
    { bytes, key: randomUUID() },
  );
  assert.equal(staged.status, 201);
  assert.ok(["ready", "activated"].includes(staged.data.status));
  const activated = await api(
    `/imports/${staged.data.id}/activate`,
    {},
    { "Idempotency-Key": randomUUID() },
  );
  assert.equal(activated.status, 201);
  const b = activated.data.id;
  const dateShapes = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/custom-report-dates-visible-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const groupedShapes = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/custom-report-groups-visible-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  for (const [index, oracle] of [
    ...source.cases,
    ...dateShapes.cases,
    ...groupedShapes.cases,
  ].entries()) {
    const response = await api(`/budgets/${b}/reports/query`, {
      version: 1,
      kind: "custom",
      as_of: "2026-11-09",
      options: oracle.options,
    });
    assert.equal(
      response.status,
      200,
      `case${index} ${JSON.stringify(oracle.options)} ${JSON.stringify(response)}`,
    );
    delete response.data.result.legend;
    assert.deepEqual(
      response.data.result,
      oracle.expected,
      `custom source case${index} ${JSON.stringify(oracle.options)}`,
    );
  }

  for (const options of [
    { startDate: "2010-01-01", endDate: "2026-11-09", interval: "Daily" },
    { startDate: "2026-11", endDate: "2026-11", mode: "unsupported" },
  ]) {
    const rejected = await api(`/budgets/${b}/reports/query`, {
      version: 1,
      kind: "custom",
      as_of: "2026-11-09",
      options,
    });
    assert.equal(rejected.status, 400);
  }
  const query = async (options) => {
    const response = await api(`/budgets/${b}/reports/query`, {
      version: 1,
      kind: "custom",
      as_of: "2026-11-09",
      options,
    });
    assert.equal(response.status, 200, JSON.stringify(response));
    return response.data;
  };
  const details = async (projection, options, cell, offset = 0) => {
    const response = await api(`/budgets/${b}/reports/drilldown`, {
      query: { version: 1, kind: "custom", as_of: "2026-11-09", options },
      snapshot: projection.snapshot,
      cell_id: cell.id,
      offset,
      limit: 2,
    });
    assert.equal(response.status, 200, JSON.stringify(response));
    return response.data;
  };
  const payment = source.cases.find(
      (c) =>
        c.options.balanceTypeOp === "totalDebts" &&
        c.options.groupBy === "Category" &&
        c.options.showOffBudget === false &&
        c.options.showHiddenCategories === true,
    ).options,
    paymentResult = await query(payment),
    food = source.categories.list.find((c) => c.name === "food");
  const foodTotal = paymentResult.cells.find(
      (c) => c.group === food.id && c.label === "food total",
    ),
    foodRows = await details(paymentResult, payment, foodTotal);
  assert.equal(foodRows.total_amount, -6000);
  assert.equal(foodRows.total_count, 5);
  assert.equal(foodRows.rows.length, 2);
  assert.equal(foodRows.has_more, true);
  assert.equal(foodRows.record_kind, "transactions");
  const nextFood = await details(paymentResult, payment, foodTotal, 2);
  assert.ok(
    nextFood.rows.every(
      (row) => !foodRows.rows.some((previous) => previous.id === row.id),
    ),
  );
  const november = await details(
    paymentResult,
    payment,
    paymentResult.cells.find(
      (c) => c.group === food.id && c.label === "food / 2026-11",
    ),
  );
  assert.equal(november.total_amount, -5000);
  assert.equal(november.total_count, 4);
  const deposit = { ...payment, balanceTypeOp: "totalAssets" },
    depositResult = await query(deposit);
  assert.equal(
    (
      await details(
        depositResult,
        deposit,
        depositResult.cells.find(
          (c) => c.group === food.id && c.label === "food total",
        ),
      )
    ).total_amount,
    200,
  );
  const payee = { ...payment, groupBy: "Payee" },
    payeeResult = await query(payee);
  assert.equal(
    (await details(payeeResult, payee, payeeResult.cells[0])).total_amount,
    -10100,
  );
  const budgeted = { ...payment, balanceTypeOp: "totalBudgeted" },
    budgetedResult = await query(budgeted),
    allocations = await details(
      budgetedResult,
      budgeted,
      budgetedResult.cells.find(
        (c) => c.group === food.id && c.label === "food total",
      ),
    );
  assert.equal(budgetedResult.recorded, false);
  assert.equal(budgetedResult.basis, "budget_allocations");
  assert.equal(allocations.record_kind, "budget_allocations");
  assert.equal(allocations.total_amount, 5000);
  assert.equal(allocations.total_count, 2);
  assert.ok(
    allocations.rows.every(
      (row) => row.account_id === null && row.id.startsWith("tracking:"),
    ),
  );
  const nested = { ...payment, groupBy: "CategoryGroup", nested: true },
    nestedResult = await query(nested),
    leaf = nestedResult.cells.find(
      (c) => c.group === "leaf:" + food.id && c.label.endsWith("food total"),
    );
  assert.ok(leaf);
  assert.equal((await details(nestedResult, nested, leaf)).total_amount, -6000);
  const incomeGroup = source.categories.grouped.find((g) => g.is_income).id;
  assert.equal(
    (
      await query({
        ...payment,
        balanceTypeOp: "totalTotals",
        conditions: [{ field: "category_group", op: "is", value: incomeGroup }],
      })
    ).result.totalTotals,
    17000,
  );
  // Pinned source keeps retired category predicates literal; mutation replacements
  // must not silently retarget a saved report to a different category.
  const retirement = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/custom-report-retirement-source-26.10.json",
        import.meta.url,
      ),
    ),
  );
  const current = await api(`/budgets/${b}/reporting`),
    epochHeaders = {
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": current.data.epoch,
    };
  const replacement = source.categories.list.find((c) => c.name === "empty");
  const retired = await api(
    `/budgets/${b}/categories/${food.id}`,
    { replacement_id: replacement.id },
    epochHeaders,
    "DELETE",
  );
  assert.equal(retired.status, 200, JSON.stringify(retired));
  assert.equal(food.id, retirement.food);
  assert.equal(replacement.id, retirement.replacement);
  for (const [category, expected] of [
    [food.id, retirement.retired],
    [replacement.id, retirement.target],
  ]) {
    assert.equal(
      (
        await query({
          ...payment,
          conditions: [{ field: "category", op: "is", value: category }],
        })
      ).result.totalDebts,
      expected,
    );
  }
  const report = current.data.documents.find((d) => d.kind === "custom_report"),
    edit = {
      writes: [
        {
          ...report,
          config: {
            ...report.config,
            name: "Custom CAS source settings",
            metadata: { kept: ["unknown", "future"] },
          },
        },
      ],
    },
    key = randomUUID(),
    headers = {
      "Idempotency-Key": key,
      "X-Actual-Budget-Epoch": current.data.epoch,
    };
  const saved = await api(`/budgets/${b}/reporting/commands`, edit, headers);
  assert.equal(saved.status, 200);
  assert.deepEqual(
    await api(`/budgets/${b}/reporting/commands`, edit, headers),
    saved,
  );
  assert.equal(
    (
      await api(`/budgets/${b}/reporting/commands`, edit, {
        ...headers,
        "Idempotency-Key": randomUUID(),
      })
    ).status,
    409,
  );
  assert.deepEqual(
    saved.data.documents.find((d) => d.id === report.id).config.metadata,
    { kept: ["unknown", "future"] },
  );
  // Supported inline categories can be ungrouped; their planned money must survive reports.
  await server.stop();
  server = await startServer();
  await page.goto(server.url);
  const legacyBudget = (await api("/budget")).data.id,
    legacyAccount = randomUUID(),
    legacyCategory = randomUUID(),
    legacyRoot = `/budgets/${legacyBudget}`;
  const legacyEpoch = (await api(legacyRoot + "/reporting")).data.epoch;
  const legacyWrite = () => ({
    "Idempotency-Key": randomUUID(),
    "X-Actual-Budget-Epoch": legacyEpoch,
  });
  assert.equal(
    (
      await api(
        legacyRoot + "/accounts",
        {
          id: legacyAccount,
          name: "Legacy cash",
          opening_balance: 0,
          opening_date: "2026-11-01",
        },
        legacyWrite(),
      )
    ).status,
    201,
  );
  assert.equal(
    (
      await api(
        legacyRoot + "/transactions",
        {
          id: randomUUID(),
          account_id: legacyAccount,
          amount: -100,
          date: "2026-11-02",
          new_category: { id: legacyCategory, name: "Legacy expense" },
        },
        legacyWrite(),
      )
    ).status,
    201,
  );
  const budgetBefore = (await api(legacyRoot + "/budget?month=2026-11")).data;
  const budgetAfter = await api(
    legacyRoot + "/budget",
    {
      action: "set",
      month: "2026-11",
      category: legacyCategory,
      amount: 5000,
      snapshot: budgetBefore.snapshot,
    },
    legacyWrite(),
  );
  assert.equal(budgetAfter.status, 200, JSON.stringify(budgetAfter));
  assert.equal(budgetAfter.data.months["2026-11"].summary.total_budgeted, 5000);
  for (const groupBy of ["Category", "CategoryGroup"]) {
    const legacyQuery = {
      version: 1,
      kind: "custom",
      as_of: "2026-11-09",
      options: {
        startDate: "2026-11",
        endDate: "2026-11",
        interval: "Monthly",
        balanceTypeOp: "totalBudgeted",
        groupBy,
        nested: groupBy === "CategoryGroup",
        showHiddenCategories: false,
      },
    };
    const report = await api(legacyRoot + "/reports/query", legacyQuery);
    assert.equal(report.status, 200, JSON.stringify(report));
    assert.equal(
      report.data.result.totalBudgeted,
      5000,
      "Ungrouped planned allocation remains visible",
    );
    const cell = report.data.cells.find(
      (c) =>
        c.group ===
          (groupBy === "Category"
            ? legacyCategory
            : "leaf:" + legacyCategory) &&
        c.label.endsWith("Legacy expense total"),
    );
    assert.ok(cell, JSON.stringify(report.data));
    const rows = await api(legacyRoot + "/reports/drilldown", {
      query: legacyQuery,
      snapshot: report.data.snapshot,
      cell_id: cell.id,
      offset: 0,
      limit: 2,
    });
    assert.equal(rows.status, 200);
    assert.equal(rows.data.total_count, 1);
    assert.equal(rows.data.total_amount, 5000);
    assert.equal(rows.data.rows[0].category, "Legacy expense");
    assert.equal(rows.data.record_kind, "budget_allocations");
  }
  console.log(
    "PASS public inline ungrouped category allocations in category/nested totals and details",
  );
  console.log(
    "PASS retired category mapping and source report CAS/immutable replay/future metadata preservation",
  );
  console.log(
    "PASS custom split/refund/payee/period/nested detail membership and explicit budget allocation basis",
  );
  console.log(
    `PASS ${source.cases.length + dateShapes.cases.length + groupedShapes.cases.length} pinned custom report financial cases (correct frontend category DTOs)`,
  );
} finally {
  await browser?.close();
  await server.stop();
}

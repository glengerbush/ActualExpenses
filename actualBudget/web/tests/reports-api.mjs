// Public reporting persistence contract. No implementation/DB access.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
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
  const api = (path, body, headers = {}) =>
    page.evaluate(
      async ({ path, body, headers }) => {
        const r = await fetch("/api/v1" + path, {
          method: body === undefined ? "GET" : "POST",
          headers: { "Content-Type": "application/json", ...headers },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        return { status: r.status, ...(await r.json()) };
      },
      { path, body, headers },
    );
  const budget = randomUUID();
  assert.equal(
    (
      await api(
        "/budgets",
        {
          id: budget,
          name: "Reporting API " + budget,
          budget_type: "tracking",
        },
        { "Idempotency-Key": randomUUID() },
      )
    ).status,
    201,
  );
  const path = `/budgets/${budget}/reporting`,
    initial = await api(path);
  assert.equal(initial.status, 200);
  assert.deepEqual(initial.data.documents, []);
  const id = randomUUID(),
    command = {
      writes: [
        {
          id,
          kind: "page",
          revision: 0,
          config: { name: "Household" },
          tombstone: false,
        },
      ],
    },
    headers = {
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": initial.data.epoch,
    };
  const saved = await api(path + "/commands", command, headers);
  assert.equal(saved.status, 200);
  assert.equal(saved.data.documents[0].revision, 1);
  assert.deepEqual(await api(path + "/commands", command, headers), saved);
  const conflict = await api(
    path + "/commands",
    { writes: [{ ...command.writes[0], config: { name: "Stale" } }] },
    { ...headers, "Idempotency-Key": randomUUID() },
  );
  assert.equal(conflict.status, 409);
  assert.equal(conflict.error.code, "reporting_changed");
  const cancelHeaders = { ...headers, "Idempotency-Key": randomUUID() },
    rename = {
      writes: [
        { ...command.writes[0], revision: 1, config: { name: "Cancelled" } },
      ],
    };
  const staleClock = await api(path + "/commands", rename, {
    ...headers,
    "Idempotency-Key": randomUUID(),
    "X-Actual-Edit-Clock": saved.edit_clock,
  });
  assert.equal(staleClock.status, 409);
  assert.equal(staleClock.error.code, "reporting_changed");
  assert.equal(
    (
      await api(path + "/commands", rename, {
        ...cancelHeaders,
        "X-Actual-Cancel-If-Uncommitted": "true",
      })
    ).status,
    409,
  );
  assert.equal(
    (await api(path + "/commands", rename, cancelHeaders)).status,
    409,
  );
  assert.equal((await api(path)).data.documents[0].config.name, "Household");
  console.log(
    "PASS reporting public API: entity CAS, immutable replay, cancellation fence",
  );
  const invalidBody = {
      writes: [
        {
          id: randomUUID(),
          kind: "page",
          revision: 0,
          config: { name: "" },
          tombstone: false,
        },
      ],
    },
    invalidHeaders = { ...headers, "Idempotency-Key": randomUUID() };
  assert.equal(
    (await api(path + "/commands", invalidBody, invalidHeaders)).status,
    400,
  );
  assert.equal(
    (
      await api(path + "/commands", invalidBody, {
        ...invalidHeaders,
        "X-Actual-Cancel-If-Uncommitted": "true",
      })
    ).status,
    409,
  );
  assert.equal(
    (await api(path + "/commands", invalidBody, invalidHeaders)).status,
    409,
  );
  // Fresh server connection: no register read has installed SQLite predicates.
  const textAccount = randomUUID();
  assert.equal(
    (
      await api(
        `/budgets/${budget}/accounts`,
        {
          id: textAccount,
          name: "Text filters",
          opening_balance: 0,
          opening_date: "2026-11-01",
        },
        { "Idempotency-Key": randomUUID() },
      )
    ).status,
    201,
  );
  assert.equal(
    (
      await api(
        `/budgets/${budget}/transactions`,
        {
          id: randomUUID(),
          account_id: textAccount,
          amount: -100,
          date: "2026-11-02",
          new_payee: { id: randomUUID(), name: "Case-fold café" },
        },
        { "Idempotency-Key": randomUUID() },
      )
    ).status,
    201,
  );
  const textQuery = {
    version: 1,
    kind: "summary",
    as_of: "2026-11-09",
    options: {
      start: "2026-11",
      end: "2026-11",
      summary: { type: "sum" },
      conditions: [{ field: "payee_name", op: "contains", value: "CAFÉ" }],
    },
  };
  let textResult = await api(`/budgets/${budget}/reports/query`, textQuery);
  assert.equal(textResult.status, 200, JSON.stringify(textResult));
  assert.equal(textResult.data.result.total, -100);
  const regexQuery = {
    ...textQuery,
    options: {
      ...textQuery.options,
      conditions: [
        { field: "payee_name", op: "matches", value: "^Case.*café$" },
      ],
    },
  };
  textResult = await api(`/budgets/${budget}/reports/query`, regexQuery);
  assert.equal(textResult.status, 200, JSON.stringify(textResult));
  assert.equal(textResult.data.result.total, -100);
  const regexRows = await api(`/budgets/${budget}/reports/drilldown`, {
    query: regexQuery,
    snapshot: textResult.data.snapshot,
    cell_id: textResult.data.cells[0].id,
    offset: 0,
    limit: 1,
  });
  assert.equal(regexRows.status, 200, JSON.stringify(regexRows));
  assert.equal(regexRows.data.total_amount, -100);
  const invalidRegex = await api(`/budgets/${budget}/reports/query`, {
    ...regexQuery,
    options: {
      ...regexQuery.options,
      conditions: [{ field: "payee_name", op: "matches", value: "(" }],
    },
  });
  assert.equal(invalidRegex.status, 400);
  assert.equal(invalidRegex.error.code, "invalid_regex");
  assert.equal(
    (await api(`/budgets/${budget}/reports/query`, regexQuery)).data.result
      .total,
    -100,
  );
  assert.equal(
    (
      await api(
        `/budgets/${budget}/transactions`,
        {
          id: randomUUID(),
          account_id: textAccount,
          amount: -1,
          date: "2026-11-03",
          new_payee: { id: randomUUID(), name: "a".repeat(64) + "!" },
        },
        { "Idempotency-Key": randomUUID() },
      )
    ).status,
    201,
  );
  const hardRegex = {
      ...regexQuery,
      options: {
        ...regexQuery.options,
        conditions: [{ field: "payee_name", op: "matches", value: "^(a+)+$" }],
      },
    },
    easyRegex = {
      ...regexQuery,
      options: {
        ...regexQuery.options,
        conditions: [{ field: "payee_name", op: "matches", value: "^a" }],
      },
    };
  const exhausted = await api(`/budgets/${budget}/reports/query`, hardRegex);
  assert.equal(exhausted.status, 400);
  assert.equal(exhausted.error.code, "regex_work_limit");
  const recovered = await api(`/budgets/${budget}/reports/query`, easyRegex);
  assert.equal(recovered.status, 200);
  assert.equal(recovered.data.result.total, -1);
  const inspectHard = await api(`/budgets/${budget}/reports/drilldown`, {
    query: hardRegex,
    snapshot: recovered.data.snapshot,
    cell_id: "0",
    offset: 0,
    limit: 1,
  });
  assert.equal(inspectHard.status, 400);
  assert.equal(inspectHard.error.code, "regex_work_limit");
  assert.equal(
    (
      await api(`/budgets/${budget}/reports/drilldown`, {
        query: easyRegex,
        snapshot: recovered.data.snapshot,
        cell_id: "0",
        offset: 0,
        limit: 1,
      })
    ).data.total_amount,
    -1,
  );
  console.log(
    "PASS fresh scoped reporting text/regex predicates, typed regex failure and subsequent reset",
  );
  const fixture = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/reports-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const bytes = (
    await readFile(
      new URL("../../tests/fixtures/reports-source-26.10.zip", import.meta.url),
    )
  ).toString("base64");
  const staged = await page.evaluate(
    async ({ bytes, key }) => {
      const response = await fetch("/api/v1/imports/actual", {
        method: "POST",
        headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
        body: Uint8Array.from(atob(bytes), (x) => x.charCodeAt(0)),
      });
      return { status: response.status, ...(await response.json()) };
    },
    { bytes, key: randomUUID() },
  );
  assert.equal(staged.status, 201);
  assert.equal(
    staged.data.status,
    "ready",
    JSON.stringify(staged.data.report.requirements),
  );
  assert.equal(
    (
      await api(
        `/imports/${staged.data.id}/activate`,
        {},
        { "Idempotency-Key": randomUUID() },
      )
    ).status,
    201,
  );
  const imported = (await api(`/budgets/${staged.data.budget_id}/reporting`))
    .data;
  for (const [table, kind] of [
    ["dashboard_pages", "page"],
    ["dashboard", "widget"],
    ["custom_reports", "custom_report"],
  ])
    for (const source of fixture.source[table]) {
      const document = imported.documents.find((d) => d.id === source.id);
      assert.ok(document, `Preserve ${kind} ${source.id}`);
      assert.equal(document.kind, kind);
      for (const [field, value] of Object.entries(source))
        if (!["id", "tombstone"].includes(field))
          assert.deepEqual(
            document.config[field],
            value,
            `Preserve ${kind}.${field}`,
          );
    }
  for (const oracle of fixture.cases.filter((c) =>
    ["cash-flow", "net-worth", "summary", "calendar", "spending"].includes(
      c.kind,
    ),
  )) {
    const response = await api(
      `/budgets/${staged.data.budget_id}/reports/query`,
      {
        version: 1,
        kind: oracle.kind,
        as_of: "2026-11-09",
        options: oracle.options,
      },
    );
    assert.equal(response.status, 200, JSON.stringify(response));
    assert.deepEqual(response.data.result, oracle.expected);
  }
  console.log(
    "PASS pinned source dashboards/reports and historical projections public API",
  );
  for (const oracle of fixture.live) {
    const response = await api(
      `/budgets/${staged.data.budget_id}/reports/query`,
      {
        version: 1,
        kind: "range",
        as_of: "2026-11-09",
        options: {
          range: oracle.range,
          includeCurrent: oracle.includeCurrent,
          first_day_of_week: "1",
        },
      },
    );
    assert.equal(response.status, 200, JSON.stringify(response));
    assert.deepEqual(response.data.result, oracle.expected);
  }
  const widgetDates = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/report-dates-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  for (const oracle of widgetDates.cases) {
    const response = await api(
      `/budgets/${staged.data.budget_id}/reports/query`,
      {
        version: 1,
        kind: "widget-range",
        as_of: "2026-11-09",
        options: oracle.options,
      },
    );
    assert.equal(response.status, 200, JSON.stringify(response));
    assert.deepEqual(response.data.result, oracle.expected);
  }
  const calendarQuery = {
    version: 1,
    kind: "calendar",
    as_of: "2026-11-09",
    options: { start: "2026-11", end: "2026-11" },
  };
  const calendar = (
    await api(`/budgets/${staged.data.budget_id}/reports/query`, calendarQuery)
  ).data;
  const cell = calendar.cells.find((c) => c.label === "2026-11-07 expense");
  assert.ok(cell);
  const inspected = await api(
    `/budgets/${staged.data.budget_id}/reports/drilldown`,
    {
      query: calendarQuery,
      snapshot: calendar.snapshot,
      cell_id: cell.id,
      offset: 0,
      limit: 1,
    },
  );
  assert.equal(inspected.status, 200, JSON.stringify(inspected));
  assert.equal(inspected.data.total_count, 2);
  assert.equal(inspected.data.total_amount, -1500);
  assert.equal(inspected.data.rows.length, 1);
  assert.equal(
    (
      await api(`/budgets/${staged.data.budget_id}/reports/drilldown`, {
        query: calendarQuery,
        snapshot: calendar.snapshot,
        cell_id: cell.id,
        offset: 0,
        limit: 201,
      })
    ).status,
    400,
  );
  const changed = {
    writes: [
      {
        id: fixture.ids.dashboard,
        kind: "page",
        revision: 1,
        config: { name: "New layout" },
        tombstone: false,
      },
    ],
  };
  assert.equal(
    (
      await api(
        `/budgets/${staged.data.budget_id}/reporting/commands`,
        changed,
        {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": imported.epoch,
        },
      )
    ).status,
    200,
  );
  assert.equal(
    (
      await api(`/budgets/${staged.data.budget_id}/reports/drilldown`, {
        query: calendarQuery,
        snapshot: calendar.snapshot,
        cell_id: cell.id,
        offset: 1,
        limit: 1,
      })
    ).status,
    409,
  );
  console.log("PASS live ranges +bounded split drilldown +snapshot fence");
  const horizon = JSON.parse(
      await readFile(
        new URL(
          "../../tests/fixtures/reports-horizon-source-26.10.json",
          import.meta.url,
        ),
        "utf8",
      ),
    ),
    horizonBytes = (
      await readFile(
        new URL(
          "../../tests/fixtures/reports-horizon-source-26.10.zip",
          import.meta.url,
        ),
      )
    ).toString("base64");
  const horizonStage = await page.evaluate(
    async ({ bytes, key }) => {
      const r = await fetch("/api/v1/imports/actual", {
        method: "POST",
        headers: { "Content-Type": "application/zip", "Idempotency-Key": key },
        body: Uint8Array.from(atob(bytes), (x) => x.charCodeAt(0)),
      });
      return { status: r.status, ...(await r.json()) };
    },
    { bytes: horizonBytes, key: randomUUID() },
  );
  assert.equal(horizonStage.status, 201);
  assert.equal(horizonStage.data.status, "ready");
  assert.equal(
    (
      await api(
        `/imports/${horizonStage.data.id}/activate`,
        {},
        { "Idempotency-Key": randomUUID() },
      )
    ).status,
    201,
  );
  for (const oracle of horizon.cases) {
    const response = await api(
      `/budgets/${horizonStage.data.budget_id}/reports/query`,
      {
        version: 1,
        kind: oracle.kind,
        as_of: "2026-11-09",
        options: oracle.options,
      },
    );
    assert.equal(response.status, 200, JSON.stringify(response));
    const actual = response.data.result;
    if (oracle.kind === "spending")
      actual.intervalData = actual.intervalData.map(({ months, ...day }) => ({
        ...day,
        month_count: Object.keys(months).length,
      }));
    assert.deepEqual(actual, oracle.expected);
  }
  const tooManyDays = await api(
    `/budgets/${horizonStage.data.budget_id}/reports/query`,
    {
      version: 1,
      kind: "net-worth",
      as_of: "2026-11-09",
      options: {
        start: "2011-01",
        end: "2026-11",
        interval: "Daily",
        accounts: [horizon.account],
      },
    },
  );
  assert.equal(tooManyDays.status, 400);
  console.log(
    "PASS sparse15-year monthly/yearly net worth, full monthly cash flow/yearly summary/all-time average, bounded daily rejection",
  );
} finally {
  await browser?.close();
  await server.stop();
}

import { expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
let base = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
const extraServers = [],
  budgetRoots = new Map();
let bootstrapped = false;
async function call(path, body, method = "POST", headers = {}) {
  if (path === "/budgets" && body) {
    if (bootstrapped) {
      const next = await startServer();
      extraServers.push(next);
      base = next.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    }
    bootstrapped = true;
    budgetRoots.set(body.id, base);
  }
  const id = /^\/budgets\/([^/?]+)/.exec(path)?.[1];
  const target = budgetRoots.get(id) ?? base;
  const r = await fetch(target + path, {
    method: body === undefined ? "GET" : method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: r.status, ...(await r.json()) };
}
try {
  // Valid current totals must remain readable when chronological cross-account
  // running balances temporarily exceed the exact JSON money range.
  const boundaryBudget = randomUUID();
  await call("/budgets", { id: boundaryBudget, name: "Intermediate boundary" });
  for (const [amount, date] of [
    [-9007199254740991, "2026-03-01"],
    [9007199254740991, "2026-01-01"],
    [9007199254740991, "2026-01-02"],
  ]) {
    const account = randomUUID();
    await call(`/budgets/${boundaryBudget}/accounts`, {
      id: account,
      name: date,
      opening_balance: 0,
      opening_date: "2025-01-01",
    });
    expect(
      (
        await call(`/budgets/${boundaryBudget}/transactions`, {
          id: randomUUID(),
          account_id: account,
          date,
          amount,
          notes: "Boundary",
        })
      ).status,
    ).toBe(201);
  }
  for (const query of [{}, { search: "2026" }]) {
    const result = await call(
      `/budgets/${boundaryBudget}/transactions?query=${encodeURIComponent(JSON.stringify(query))}`,
    );
    expect(result.status).toBe(200);
    expect(result.balances.total).toBe(9007199254740991);
    const outOfRange = result.data.find((row) => row.date === "2026-01-02");
    expect(outOfRange.running_balance).toBe(null);
    expect(outOfRange.running_balance_error).toBe("exact_money_range");
  }
  const b = randomUUID(),
    a = randomUUID();
  expect(
    (await call("/budgets", { id: b, name: "Find transactions" })).status,
  ).toBe(201);
  await call(`/budgets/${b}/accounts`, {
    id: a,
    name: "Spending",
    opening_balance: 10000,
    opening_date: "2026-01-01",
  });
  for (const [amount, notes, cleared] of [
    [-1250, "Coffee", true],
    [-375, "Coffee beans", false],
    [2400, "Refund", true],
  ])
    await call(`/budgets/${b}/transactions`, {
      id: randomUUID(),
      account_id: a,
      date: "2026-01-02",
      amount,
      new_payee: { id: randomUUID(), name: notes },
      notes,
      cleared,
    });
  const query = {
    search: "Coffee",
    sort: "amount",
    direction: "asc",
    filter: {
      op: "and",
      conditions: [{ field: "amount", op: "lt", value: 0 }],
    },
  };
  const page = await call(
    `/budgets/${b}/transactions?account_id=${a}&limit=1&query=${encodeURIComponent(JSON.stringify(query))}`,
  );
  expect(page.status).toBe(200);
  expect(page.data[0].amount).toBe(-1250);
  expect(page.filtered_balances).toEqual({
    total: -1625,
    cleared: -1625,
    uncleared: 0,
  });
  expect(page.filtered_count).toBe(2);
  expect(page.balances.total).toBe(10775);
  expect(page.next_offset).toBe(1);
  const next = await call(
    `/budgets/${b}/transactions?account_id=${a}&limit=1&offset=1&snapshot=${page.snapshot}&query=${encodeURIComponent(JSON.stringify(query))}`,
  );
  expect(next.data[0].amount).toBe(-375);
  expect(next.next_offset).toBe(null);
  const initial = (await call(`/budgets/${b}/register-settings`)).data;
  const key = randomUUID(),
    settings = {
      views: {
        [a]: { query, columns: ["date", "amount"], as_of: "2026-01-02" },
      },
      saved_filters: [{ id: randomUUID(), name: "Coffee", query }],
    };
  const headers = {
    "Idempotency-Key": key,
    "X-Actual-Budget-Epoch": initial.epoch,
  };
  const saved = await call(
    `/budgets/${b}/register-settings`,
    { revision: 0, settings },
    "PATCH",
    headers,
  );
  expect(saved.status).toBe(200);
  expect(saved.data.revision).toBe(1);
  expect(
    await call(
      `/budgets/${b}/register-settings`,
      { revision: 0, settings },
      "PATCH",
      headers,
    ),
  ).toEqual(saved);
  expect(
    (
      await call(
        `/budgets/${b}/register-settings`,
        { revision: 0, settings },
        "PATCH",
        { "X-Actual-Budget-Epoch": initial.epoch },
      )
    ).status,
  ).toBe(409);
  expect(
    (
      await call(
        `/budgets/${b}/register-settings`,
        { revision: 1, settings },
        "PATCH",
        { "X-Actual-Budget-Epoch": "0".repeat(32) },
      )
    ).status,
  ).toBe(409);
  expect(
    (await call(`/budgets/${b}/transactions?limit=1&snapshot=${page.snapshot}`))
      .status,
  ).toBe(409);
  const fixture = JSON.parse(
    await readFile(
      new URL("../../tests/fixtures/register-query.json", import.meta.url),
      "utf8",
    ),
  );
  const oracleBudget = randomUUID(),
    oracleAccount = randomUUID();
  await call("/budgets", { id: oracleBudget, name: "Pinned filter oracle" });
  await call(`/budgets/${oracleBudget}/accounts`, {
    id: oracleAccount,
    name: "Spending",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const row of fixture.input)
    expect(
      (
        await call(`/budgets/${oracleBudget}/transactions`, {
          ...row,
          account_id: oracleAccount,
        })
      ).status,
    ).toBe(201);
  for (const test of fixture.cases) {
    const result = await call(
      `/budgets/${oracleBudget}/transactions?limit=1&query=${encodeURIComponent(JSON.stringify(test.query))}`,
    );
    if (
      test.query.filter.conditions.some(
        (condition) => condition.field === "notes",
      )
    ) {
      // The pinned text/tag cases now verify retirement; regex on live names is
      // exercised separately by register-regex-budget.
      expect(result.status, test.name).toBe(400);
      expect(result.error.code, test.name).toBe("invalid_request");
      continue;
    }
    const expectedIds = test.query.filter.conditions.some(
      (condition) => condition.field === "cleared",
    )
      ? fixture.input.filter((row) => row.amount > 0).map((row) => row.id)
      : test.ids;
    const total = fixture.input
      .filter((row) => expectedIds.includes(row.id))
      .reduce((sum, row) => sum + row.amount, 0);
    expect(result.status, test.name).toBe(200);
    expect(result.filtered_balances, test.name).toEqual({
      total,
      cleared: total,
      uncleared: 0,
    });
    expect(result.filtered_count, test.name).toBe(expectedIds.length);
    const ids = [];
    let offset = 0;
    do {
      const next = await call(
        `/budgets/${oracleBudget}/transactions?limit=1&offset=${offset}&snapshot=${result.snapshot}&query=${encodeURIComponent(JSON.stringify(test.query))}`,
      );
      ids.push(...next.data.map((r) => r.id));
      offset = next.next_offset;
    } while (offset !== null);
    expect(ids.sort(), test.name).toEqual([...expectedIds].sort());
  }
  for (const [value, expected] of [
    [{ frequency: "monthly", start: "2026-01-02" }, [2400, -1250, -1300]],
    [
      {
        frequency: "monthly",
        start: "2026-01-02",
        patterns: [{ type: "day", value: 3 }],
      },
      [-375],
    ],
    [{ frequency: "yearly", start: "2026-01-02" }, [-1300, -1250]],
  ]) {
    const result = await call(
      `/budgets/${oracleBudget}/transactions?query=${encodeURIComponent(JSON.stringify({ filter: { conditions: [{ field: "date", op: "eq", value }] } }))}`,
    );
    expect(result.status).toBe(200);
    expect(result.data.map((r) => r.amount).sort((a, b) => a - b)).toEqual(
      expected.sort((a, b) => a - b),
    );
  }
  const archive = await readFile(
    new URL(
      "../../tests/fixtures/register-settings-26.10.zip",
      import.meta.url,
    ),
  );
  const importServer = await startServer();
  extraServers.push(importServer);
  base = importServer.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  const upload = await fetch(base + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: archive,
  });
  expect(upload.status).toBe(201);
  const staged = (await upload.json()).data;
  expect(staged.status, JSON.stringify(staged.report)).toBe("ready");
  expect(
    staged.report.requirements.some(
      (item) =>
        item.feature.startsWith("register:retired-saved-filter:") &&
        item.blocking === false,
    ),
  ).toBe(true);
  const activated = await call(`/imports/${staged.id}/activate`, {});
  expect(activated.status).toBe(201);
  const imported = activated.data.id;
  budgetRoots.set(imported, base);
  const importedSettings = (
    await call(`/budgets/${imported}/register-settings`)
  ).data;
  expect(
    importedSettings.settings.saved_filters.map((f) => f.name).sort(),
  ).toEqual(["Identity", "Monthly", "Payments"]);
  const importedAccount = "0165d0a7-99e2-4719-a102-c007b7a8daaa";
  expect(
    importedSettings.settings.views[importedAccount].query.hide_reconciled,
  ).toBe(true);
  expect(importedSettings.settings.views[importedAccount].columns).toEqual([
    "date",
    "account",
    "payee",
    "category",
    "payment",
    "deposit",
    "balance",
  ]);
  const backup = await call(
    `/budgets/${imported}/backups`,
    { label: "Register settings round trip" },
    "POST",
    { "X-Actual-Budget-Epoch": importedSettings.epoch },
  );
  expect(backup.status, JSON.stringify(backup)).toBe(201);
  const changed = structuredClone(importedSettings.settings);
  changed.saved_filters = [];
  expect(
    (
      await call(
        `/budgets/${imported}/register-settings`,
        { revision: 0, settings: changed },
        "PATCH",
        { "X-Actual-Budget-Epoch": importedSettings.epoch },
      )
    ).status,
  ).toBe(200);
  const restored = await call(
    `/budgets/${imported}/backups/${backup.data.id}/restore`,
    { confirm_name: activated.data.name },
    "POST",
    { "X-Actual-Budget-Epoch": importedSettings.epoch },
  );
  expect(restored.status, JSON.stringify(restored)).toBe(200);
  const afterRestore = (await call(`/budgets/${imported}/register-settings`))
    .data;
  expect(afterRestore.settings).toEqual(importedSettings.settings);
  expect(afterRestore.epoch).not.toBe(importedSettings.epoch);
  expect(
    (
      await call(
        `/budgets/${imported}/register-settings`,
        { revision: 0, settings: changed },
        "PATCH",
        { "X-Actual-Budget-Epoch": importedSettings.epoch },
      )
    ).status,
  ).toBe(409);
  for (const query of [
    {
      filter: {
        conditions: Array.from({ length: 21 }, () => ({
          field: "amount",
          op: "eq",
          value: 0,
        })),
      },
    },
    {
      filter: {
        conditions: [
          {
            field: "date",
            op: "eq",
            value: {
              frequency: "monthly",
              start: "2026-01-01",
              patterns: [{ type: "day", value: -2147483648 }],
            },
          },
        ],
      },
    },
    {
      filter: {
        conditions: [
          { field: "notes; DROP TABLE budgets", op: "eq", value: "x" },
        ],
      },
    },
  ])
    expect(
      (
        await call(
          `/budgets/${b}/transactions?query=${encodeURIComponent(JSON.stringify(query))}`,
        )
      ).status,
    ).toBe(400);
  const amountSearch = await call(
    `/budgets/${oracleBudget}/transactions?query=${encodeURIComponent(JSON.stringify({ search: "12.50" }))}`,
  );
  expect(amountSearch.data.map((r) => r.amount)).toEqual([-1250]);
  const importedVisible = await call(
    `/budgets/${imported}/transactions?query=${encodeURIComponent(JSON.stringify({ hide_reconciled: true }))}`,
  );
  expect(importedVisible.data.every((row) => !row.reconciled)).toBe(true);
  // The response remains bounded when thousands of rows match; aggregates cover all.
  const large = randomUUID();
  await call(`/budgets/${b}/accounts`, {
    id: large,
    name: "Large",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  const parityPayees = [randomUUID(), randomUUID()];
  for (const [index, name] of ["even", "odd"].entries())
    await call(`/budgets/${b}/payees`, { id: parityPayees[index], name });
  for (let i = 0; i < 1200; i++)
    await call(`/budgets/${b}/transactions`, {
      id: randomUUID(),
      account_id: large,
      date: "2026-01-01",
      amount: -1,
      payee_id: parityPayees[i % 2],
    });
  const started = performance.now();
  const bounded = await call(
    `/budgets/${b}/transactions?account_id=${large}&limit=7&query=${encodeURIComponent(JSON.stringify({ search: "even" }))}`,
  );
  expect(bounded.data).toHaveLength(7);
  expect(bounded.filtered_count).toBe(600);
  expect(bounded.filtered_balances.total).toBe(-600);
  expect(JSON.stringify(bounded).length).toBeLessThan(20000);
  console.log(
    `1200-row filtered page: ${(performance.now() - started).toFixed(1)}ms, ${JSON.stringify(bounded).length} bytes`,
  );
  console.log(
    "PASS register filters, pinned parity, stable pages, exact totals, settings CAS/replay/epoch and bounded retrieval",
  );
} finally {
  await Promise.all(extraServers.map((value) => value.stop()));
  await server.stop();
}

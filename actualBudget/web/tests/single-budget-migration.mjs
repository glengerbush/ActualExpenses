import { expect } from "@playwright/test";
import { randomUUID as uuid } from "node:crypto";
import { mkdtemp, rm, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const dir = await mkdtemp(tmpdir() + "/singleton-migration-");
let server = await startServer({ dataDirectory: dir });
const api = () =>
  server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body) {
  const r = await fetch(api() + path, {
    method: body ? "POST" : "GET",
    headers: { "Content-Type": "application/json", "Idempotency-Key": uuid() },
    body: body ? JSON.stringify(body) : undefined,
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
try {
  const b = uuid(),
    other = uuid(),
    a = uuid(),
    g = uuid(),
    c = uuid(),
    p = uuid(),
    t = uuid(),
    root = "/budgets/" + b;
  await call("/budgets", { id: b, name: "First" });
  await call(root + "/accounts", {
    id: a,
    name: "Ledger",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  await call(root + "/category-groups", {
    id: g,
    name: "Living",
    color: "#8a3e89",
  });
  await call(root + "/categories", {
    id: c,
    name: "Food",
    group_id: g,
    color: "#aa99ff",
  });
  await call(root + "/payees", { id: p, name: "Shop" });
  await call(root + "/transactions", {
    id: t,
    account_id: a,
    category_id: c,
    payee_id: p,
    date: "2026-11-05",
    amount: -1200,
    recurrence: {
      enabled: true,
      config: {
        start: "2026-11-05",
        frequency: "monthly",
        interval: 1,
        endMode: "never",
      },
    },
  });
  let v = await call(root + "/budget?month=2026-11");
  await call(root + "/budget", {
    snapshot: v.snapshot,
    month: "2026-11",
    action: "set",
    category: c,
    amount: 5000,
  });
  const epoch = (await call("/budget")).epoch;
  await server.stop();
  execFileSync("python3", [
    "tests/historical-schema.py",
    dir + "/actualbudget.sqlite3",
    "36",
  ]);
  execFileSync("python3", [
    "-c",
    `
import sqlite3,sys,json
c=sqlite3.connect(sys.argv[1]); source,target=sys.argv[2:]
c.execute("INSERT INTO tags(budget_id,id,tag) VALUES(?,?,'food')",(source,'11000000-0000-4000-8000-000000000001'))
account=c.execute("SELECT id FROM accounts WHERE budget_id=? LIMIT 1",(source,)).fetchone()[0]
c.execute("INSERT INTO transaction_import_settings(budget_id,id,account_id,option_key,value) VALUES(?,?,?,'csv-delimiter',?)",(source,'csv-delimiter-'+account,account,json.dumps(';')))
triggers=c.execute("SELECT name,sql FROM sqlite_master WHERE type='trigger'").fetchall()
for name,sql in triggers:c.execute('DROP TRIGGER "'+name+'"')
c.execute("INSERT INTO budgets SELECT ?,'Second',budget_type,deleted FROM budgets WHERE id=?",(target,source))
for (table,) in c.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchall():
 cols=[r[1] for r in c.execute('PRAGMA table_info("'+table+'")')]
 if 'budget_id' not in cols or table in ['budget_backups','native_restore_stages','staged_imports','imported_sources','operation_budget_links']:continue
 rows=c.execute('SELECT * FROM "'+table+'" WHERE budget_id=?',(source,)).fetchall()
 for row in rows:
  values=list(row);values[cols.index('budget_id')]=target
  c.execute('INSERT INTO "'+table+'" VALUES('+','.join('?' for _ in cols)+')',values)
for name,sql in triggers:c.execute(sql)
c.commit()
`,
    dir + "/actualbudget.sqlite3",
    b,
    other,
  ]);
  server = await startServer({ dataDirectory: dir });
  const single = await call("/budget");
  expect(single.id).toBe(b);
  expect(single.epoch).not.toBe(epoch);
  expect(single.empty).toBe(false);
  expect((await call("/budgets")).length).toBe(1);
  const rows = await call(root + "/transactions");
  expect(rows.length).toBe(2);
  expect(new Set(rows.map((r) => r.id)).size).toBe(2);
  expect(rows.reduce((s, r) => s + r.amount, 0)).toBe(-2400);
  expect(
    (await call(root + "/categories")).filter((x) => x.name === "Food"),
  ).toHaveLength(2);
  v = await call(root + "/budget?month=2026-11");
  expect(v.months["2026-11"].summary.total_budgeted).toBe(10000);
  expect(v.months["2026-11"].summary.total_spent).toBe(-2400);
  expect((await call(root + "/schedules")).schedules).toHaveLength(2);
  for (const row of rows) {
    expect(row.recurrence.enabled).toBe(true);
    expect(row.payee_name).toBe("Shop");
    expect(row.category_name).toBe("Food");
  }
  expect(
    (await call(root + "/backups")).filter((p) =>
      p.label.startsWith("Before merging"),
    ),
  ).toHaveLength(2);
  expect(
    (await readdir(dir + "/backups")).some((n) => n.startsWith("before-v36-")),
  ).toBe(true);
  expect(
    (
      await fetch(api() + "/budgets", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": uuid(),
        },
        body: JSON.stringify({ id: uuid(), name: "Forbidden" }),
      })
    ).status,
  ).toBe(409);
  expect(
    (await fetch(api() + "/budgets/" + other + "/transactions")).status,
  ).toBe(410);
  const remembered = rows.map((r) => r.id).sort();
  await server.stop();
  server = await startServer({ dataDirectory: dir });
  expect((await call(root + "/transactions")).map((r) => r.id).sort()).toEqual(
    remembered,
  );
  const points = await call(root + "/backups");
  const sourcePoint = points.find((p) => p.label === "Before merging Second");
  const current = await call("/budget");
  const restored = await fetch(
    api() + root + "/backups/" + sourcePoint.id + "/restore",
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": uuid(),
        "X-Actual-Budget-Epoch": current.epoch,
      },
      body: JSON.stringify({ confirm_name: current.name }),
    },
  );
  expect(restored.ok, await restored.clone().text()).toBe(true);
  expect((await call("/budget")).id).toBe(b);
  expect(await call(root + "/transactions")).toHaveLength(1);
  expect((await call(root + "/schedules")).schedules).toHaveLength(1);
  expect(await call("/budgets")).toHaveLength(1);
  console.log(
    "PASS single-budget migration: backups, collision-safe records, plans, recurrence, stale guards, restart",
  );
} finally {
  await server.stop();
  await rm(dir, { recursive: true, force: true });
}

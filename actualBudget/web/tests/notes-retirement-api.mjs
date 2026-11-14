import { expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, rm, readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { startServer } from "./server.mjs";
const directory = await mkdtemp(tmpdir() + "/actualbudget-notes-");
let server = await startServer({ dataDirectory: directory });
const b = randomUUID(),
  a = randomUUID(),
  g = randomUUID(),
  c = randomUUID(),
  t = randomUUID(),
  root = `/budgets/${b}`;
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const bytes = Buffer.isBuffer(body);
  const r = await fetch(
    server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1" + path,
    {
      method,
      headers: {
        "Content-Type": bytes ? "application/zip" : "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body:
        body === undefined ? undefined : bytes ? body : JSON.stringify(body),
    },
  );
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
try {
  await call("/budgets", { id: b, name: "Notes migration" });
  await call(root + "/accounts", {
    id: a,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  await call(root + "/category-groups", { id: g, name: "Living" });
  await call(root + "/categories", { id: c, name: "Food", group_id: g });
  await call(root + "/transactions", {
    id: t,
    account_id: a,
    category_id: c,
    amount: -1234,
    date: "2026-11-01",
  });
  await server.stop();
  // Rebuild the actual schema31 DDL before adding legacy notes.
  execFileSync("python3", [
    "tests/historical-schema.py",
    directory + "/actualbudget.sqlite3",
    "31",
  ]);
  execFileSync("python3", [
    "-c",
    `import sqlite3,sys
c=sqlite3.connect(sys.argv[1]+'/actualbudget.sqlite3');b,g,k,t=sys.argv[2:]
for table,id in [('categories',k),('category_groups',g),('transactions',t)]: c.execute('UPDATE '+table+" SET notes='obsolete' WHERE budget_id=? AND id=?",(b,id))
for id in [b,k,g,'budget-2026-11']: c.execute("INSERT OR REPLACE INTO notes(budget_id,id,note,tombstone) VALUES(?,?,?,0)",(b,id,'obsolete'))
c.commit()`,
    directory,
    b,
    g,
    c,
    t,
  ]);
  server = await startServer({ dataDirectory: directory });
  expect((await call(root + `/transactions/${t}`)).amount).toBe(-1234);
  expect((await call(root + `/transactions/${t}`)).notes).toBe("");
  expect(await call(root + `/categories/${c}`)).not.toHaveProperty("notes");
  expect(await call(root + `/category-groups/${g}`)).not.toHaveProperty(
    "notes",
  );
  const result = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `import sqlite3,sys,json
c=sqlite3.connect(sys.argv[1]+'/actualbudget.sqlite3')
print(json.dumps({'version':c.execute('PRAGMA user_version').fetchone()[0], 'notes':sum(c.execute('SELECT count(*) FROM '+t+" WHERE notes<>''").fetchone()[0] for t in ['categories','category_groups','transactions','budget_months']), 'generic':c.execute("SELECT count(*) FROM notes WHERE note<>'' AND tombstone=0").fetchone()[0]}))`,
        directory,
      ],
      { encoding: "utf8" },
    ),
  );
  expect(result.notes).toBe(0);
  expect(result.generic).toBe(0);
  expect(result.version).toBeGreaterThanOrEqual(32);
  // Actual archives and portable archives must still decode legacy columns.
  const staged = await call(
    "/imports/actual",
    await readFile("../tests/fixtures/budgeting-envelope-allocated.zip"),
  );
  expect(staged.status).toBe("ready");
  await call(`/imports/${staged.id}/activate`, {});
  const imported = await call(
    `/budgets/${staged.budget_id}/categories?limit=1000`,
  );
  expect(imported.every((row) => !Object.hasOwn(row, "notes"))).toBe(true);
  await call(
    "/restores",
    await readFile("../tests/fixtures/budgeting-schema15.zip"),
  );
  console.log(
    "PASS schema31 migration, money preservation, Actual import and portable restore",
  );
} finally {
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}

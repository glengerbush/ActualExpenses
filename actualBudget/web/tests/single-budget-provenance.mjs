import { expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { startServer } from "./server.mjs";

const fixture = await readFile(
  new URL("../../tests/fixtures/import-26.10.0.zip", import.meta.url),
);
const historical = fileURLToPath(
  new URL("./historical-schema.py", import.meta.url),
);

for (const mode of ["activate", "merge", "shared-merge"]) {
  const directory = await mkdtemp(`${tmpdir()}/single-budget-provenance-`);
  let server = await startServer({ dataDirectory: directory });
  const api = () =>
    server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  async function call(path, body, extra = {}) {
    const response = await fetch(api() + path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
        ...extra,
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    expect(response.ok, await response.clone().text()).toBe(true);
    return (await response.json()).data;
  }
  async function stage() {
    const response = await fetch(api() + "/imports/actual", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: fixture,
    });
    expect(response.ok, await response.clone().text()).toBe(true);
    return (await response.json()).data;
  }
  async function repeatImport() {
    const current = await call("/budget");
    const staged = await stage();
    const before = await call(`/budgets/${current.id}/transactions?limit=1000`);
    const result = await call(`/imports/${staged.id}/merge`, {
      budget_id: current.id,
      epoch: current.epoch,
    });
    expect(result.import_result.added).toBe(0);
    expect(
      await call(`/budgets/${current.id}/transactions?limit=1000`),
    ).toEqual(before);
  }
  try {
    const original = await call("/budget");
    const staged = await stage();
    if (mode === "activate") await call(`/imports/${staged.id}/activate`, {});
    else
      await call(`/imports/${staged.id}/merge`, {
        budget_id: original.id,
        epoch: original.epoch,
      });
    const source = await call("/budget");
    const originals = await call(
      `/budgets/${source.id}/transactions?limit=1000`,
    );
    expect(originals.length).toBeGreaterThan(0);
    const other = randomUUID();
    await server.stop();
    const originalMapping =
      mode === "shared-merge"
        ? JSON.parse(
            execFileSync(
              "python3",
              [
                "-c",
                `
import sqlite3,sys
c=sqlite3.connect(sys.argv[1]); print(c.execute("SELECT metadata FROM budget_provenance WHERE budget_id=? AND kind='archive-merge'",(sys.argv[2],)).fetchone()[0])
`,
                directory + "/actualbudget.sqlite3",
                source.id,
              ],
              { encoding: "utf8" },
            ),
          )
        : null;
    execFileSync("python3", [
      historical,
      directory + "/actualbudget.sqlite3",
      "36",
    ]);
    execFileSync("python3", [
      "-c",
      `
import sqlite3,sys
c=sqlite3.connect(sys.argv[1]); source,other,mode=sys.argv[2:]
if mode=='shared-merge':
    triggers=c.execute("SELECT name,sql FROM sqlite_master WHERE type='trigger'").fetchall()
    for name,sql in triggers:c.execute('DROP TRIGGER "'+name+'"')
    c.execute("INSERT INTO budgets SELECT ?,'Secondary',budget_type,deleted FROM budgets WHERE id=?",(other,source))
    for (table,) in c.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchall():
        cols=[r[1] for r in c.execute('PRAGMA table_info("'+table+'")')]
        if 'budget_id' not in cols or table in ['budget_backups','native_restore_stages','staged_imports','imported_sources','operation_budget_links']:continue
        for row in c.execute('SELECT * FROM "'+table+'" WHERE budget_id=?',(source,)).fetchall():
            values=list(row);values[cols.index('budget_id')]=other
            c.execute('INSERT INTO "'+table+'" VALUES('+','.join('?' for _ in cols)+')',values)
    for name,sql in triggers:c.execute(sql)
else:
    c.execute("INSERT INTO budgets(rowid,id,name,budget_type,deleted) VALUES(0,?,'Primary','tracking',0)",(other,))
c.commit()
`,
      directory + "/actualbudget.sqlite3",
      source.id,
      other,
      mode,
    ]);
    server = await startServer({ dataDirectory: directory });
    const current = await call("/budget");
    const rows = await call(`/budgets/${current.id}/transactions?limit=1000`);
    expect(rows).toHaveLength(
      originals.length * (mode === "shared-merge" ? 2 : 1),
    );
    await repeatImport();

    if (mode === "shared-merge") {
      // The primary copy wins deduplication; the secondary financial copy still exists.
      const metadata = JSON.parse(
        execFileSync(
          "python3",
          [
            "-c",
            `
import sqlite3,sys,json
c=sqlite3.connect(sys.argv[1]); print(c.execute("SELECT metadata FROM budget_provenance WHERE budget_id=? AND kind='archive-merge'",(sys.argv[2],)).fetchone()[0])
`,
            directory + "/actualbudget.sqlite3",
            current.id,
          ],
          { encoding: "utf8" },
        ),
      );
      expect(metadata).toEqual(originalMapping);
    } else {
      // Restoring a former source into a different ID must retain repeat-import safety.
      const points = await call(`/budgets/${current.id}/backups`);
      const point = points.find(
        (point) => point.label === `Before merging ${source.name}`,
      );
      expect(point).toBeTruthy();
      await call(
        `/budgets/${current.id}/backups/${point.id}/restore`,
        { confirm_name: current.name },
        { "X-Actual-Budget-Epoch": current.epoch },
      );
      expect((await call("/budget")).id).toBe(current.id);
      await repeatImport();
    }
    console.log(`PASS single-budget provenance: ${mode}`);
  } finally {
    await server.stop();
    await rm(directory, { recursive: true, force: true });
  }
}

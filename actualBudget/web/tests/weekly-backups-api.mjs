import { expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const directory = await mkdtemp(tmpdir() + "/actualbudget-weekly-");
let server = await startServer({ dataDirectory: directory });
const b = randomUUID();
const sql = (query) =>
  execFileSync("python3", [
    "-c",
    "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute(sys.argv[2]); c.commit()",
    directory + "/actualbudget.sqlite3",
    query,
  ]);
async function call(path, body, method) {
  const url =
    server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1" + path;
  const r = await fetch(url, {
    method: method ?? (body ? "POST" : "GET"),
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return r.json();
}
try {
  expect((await call("/backup-policy")).data).toEqual({
    interval_seconds: 604800,
    retention_count: 10,
    format: "complete",
  });
  await call("/budgets", { id: b, name: "Weekly backup" });
  const before = (await call("/budgets/" + b)).data;
  expect(before.auto_due).toBeGreaterThan(Date.now() / 1000 + 604790);
  const a = randomUUID();
  await call(`/budgets/${b}/accounts`, {
    id: a,
    name: "History",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  expect((await call("/budgets/" + b)).data.auto_due).toBe(before.auto_due);
  sql(
    `UPDATE budget_lifecycle SET auto_due=unixepoch('now')-1 WHERE budget_id='${b}'`,
  );
  await call("/backup-policy", { interval_seconds: 604800 }, "PATCH");
  await expect
    .poll(async () => (await call("/budgets/" + b)).data.auto_due)
    .toBeNull();
  let backups = (await call(`/budgets/${b}/backups`)).data;
  expect(backups.filter((x) => x.kind === "automatic")).toHaveLength(1);
  const download = await fetch(
    server.url.replace("actualbudget.test", "127.0.0.1") +
      `/api/v1/budgets/${b}/backups/${backups.find((x) => x.kind === "automatic").id}/download`,
  );
  expect(download.ok).toBe(true);
  const bytes = Buffer.from(await download.arrayBuffer());
  const files = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        "import zipfile,io,sys,json; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); print(json.dumps(z.namelist()))",
      ],
      { input: bytes },
    ),
  );
  expect(files).toContain("budget.json");
  expect(files).toContain("manifest.json");
  await call("/backup-policy", { interval_seconds: 0 }, "PATCH");
  await call("/backup-policy", { interval_seconds: 604800 }, "PATCH");
  expect((await call("/budgets/" + b)).data.auto_due).toBeNull();
  await call("/backup-policy", { interval_seconds: 0 }, "PATCH");
  await call(`/budgets/${b}/transactions`, {
    id: randomUUID(),
    account_id: a,
    date: "2026-11-01",
    amount: -500,
  });
  expect((await call("/budgets/" + b)).data.auto_due).toBeNull();
  await call("/backup-policy", { interval_seconds: 604800 }, "PATCH");
  expect((await call("/budgets/" + b)).data.auto_due).toBeGreaterThan(
    Date.now() / 1000 + 604790,
  );
  await server.stop();
  server = await startServer({ dataDirectory: directory });
  expect((await call("/backup-policy")).data.interval_seconds).toBe(604800);
  expect(
    (await call(`/budgets/${b}/backups`)).data.filter(
      (x) => x.kind === "automatic",
    ),
  ).toHaveLength(1);
  console.log(
    "PASS weekly dirty-only deadlines, complete archive, disabled changes and restart persistence",
  );
} finally {
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}

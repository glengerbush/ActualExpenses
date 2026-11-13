import { expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, cp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const dir = await mkdtemp(tmpdir() + "/actualbudget-retirement-");
await cp(
  new URL(
    "../../tests/fixtures/rule-expressions-schema26.sqlite3",
    import.meta.url,
  ),
  dir + "/actualbudget.sqlite3",
);
const financial = () =>
  JSON.parse(
    execFileSync("python3", [
      "-c",
      `import sqlite3,json,sys
c=sqlite3.connect(sys.argv[1]);print(json.dumps(c.execute('SELECT budget_id,id,account_id,date,amount,tombstone FROM transactions ORDER BY budget_id,id').fetchall()))`,
      dir + "/actualbudget.sqlite3",
    ]),
  );
const before = financial();
let server;
try {
  server = await startServer({ dataDirectory: dir });
  const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  async function call(path, body) {
    const r = await fetch(api + path, {
      method: body ? "POST" : "GET",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    expect(r.ok, await r.clone().text()).toBe(true);
    return r.json();
  }
  expect(financial()).toEqual(before);
  for (const b of (await call("/budgets")).data) {
    expect(
      (await call(`/budgets/${b.id}/preferences`)).data,
    ).not.toHaveProperty("experimental");
    for (const rule of (await call(`/budgets/${b.id}/rules`)).data)
      for (const a of rule.actions) {
        expect(a.options?.formula).toBeUndefined();
        expect(a.options?.template).toBeUndefined();
      }
  }
  const contract = await call("/openapi.json");
  expect(
    Object.keys(contract.paths).some((p) =>
      /\/themes\/|\/reports\/(formula|forecast|sankey|budget-analysis|monte-carlo)/.test(
        p,
      ),
    ),
  ).toBe(false);
  for (const file of [
    "formula-saved-26.10.zip",
    "monte-carlo-plans-26.10.zip",
    "rules-template-requirement-26.10.0.zip",
  ]) {
    const r = await fetch(api + "/imports/actual", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: await readFile(
        new URL("../../tests/fixtures/" + file, import.meta.url),
      ),
    });
    expect(r.ok, await r.clone().text()).toBe(true);
    const stage = (await r.json()).data;
    expect(stage.status, JSON.stringify(stage.report.requirements)).toBe(
      "ready",
    );
    const b = (await call(`/imports/${stage.id}/activate`, {})).data;
    expect(
      (await call(`/budgets/${b.id}/preferences`)).data,
    ).not.toHaveProperty("experimental");
    for (const d of (await call(`/budgets/${b.id}/reporting`)).data.documents)
      if (
        /^(formula|monte-carlo|sankey|budget-analysis|balance-forecast)-card$/.test(
          d.config.type,
        )
      )
        expect(d.tombstone).toBe(true);
    const point = (
      await call(`/budgets/${b.id}/backups`, {
        label: "Retirement compatibility",
      })
    ).data;
    await call(`/budgets/${b.id}/backups/${point.id}/restore`, {
      confirm_name: b.name,
    });
  }
  const fixed = randomUUID();
  await call("/budgets", { id: fixed, name: "Fixed rules" });
  const account = (await call(`/budgets/${fixed}/ledger`, {})).data.id;
  const category = randomUUID();
  await call(`/budgets/${fixed}/categories`, {
    id: category,
    name: "Food",
    is_income: false,
  });
  const checkpoint = async () =>
    (await call(`/budgets/${fixed}/checkpoint`)).data.snapshot;
  await call(`/budgets/${fixed}/rules`, {
    id: randomUUID(),
    stage: null,
    conditionsOp: "and",
    conditions: [{ field: "amount", op: "lt", value: 0 }],
    actions: [{ op: "set", field: "category", value: category }],
    snapshot: await checkpoint(),
  });
  const saved = (
    await call(`/budgets/${fixed}/rules/transactions`, {
      id: randomUUID(),
      account_id: account,
      amount: -123,
      date: "2026-11-13",
      snapshot: await checkpoint(),
    })
  ).data;
  expect(saved.category_id).toBe(category);
  expect(saved.amount).toBe(-123);
  console.log(
    "PASS experimental retirement: unchanged financial rows on upgrade, retired routes, legacy source imports and native restore",
  );
} finally {
  await server?.stop();
  await rm(dir, { recursive: true, force: true });
}

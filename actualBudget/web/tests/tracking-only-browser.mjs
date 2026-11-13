import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdtemp, readFile, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { startServer } from "./server.mjs";
const directory = await mkdtemp(tmpdir() + "/tracking-only-");
let server = await startServer({ dataDirectory: directory }),
  browser;
const api = () =>
  server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function req(path, body, method) {
  return fetch(api() + path, {
    method: method ?? (body === undefined ? "GET" : "POST"),
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
}
async function call(path, body, method) {
  const r = await req(path, body, method);
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
try {
  const budget = randomUUID(),
    account = randomUUID(),
    category = randomUUID(),
    group = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Tracking only" });
  await call(root + "/accounts", {
    id: account,
    name: "Ledger",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  await call(root + "/category-groups", { id: group, name: "Everyday" });
  await call(root + "/categories", {
    id: category,
    name: "Groceries",
    group_id: group,
  });
  await call(root + "/transactions", {
    id: randomUUID(),
    account_id: account,
    category_id: category,
    amount: -2500,
    date: "2026-11-05",
    notes: "Keep transaction notes",
  });
  const original = await call(root + "/transactions");
  const oldEpoch = (await call(root + "/budget?month=2026-11")).epoch;
  await server.stop();
  execFileSync("python3", [
    "-c",
    `import sqlite3,sys,json
c=sqlite3.connect(sys.argv[1]+'/actualbudget.sqlite3');b,k=sys.argv[2:]
c.execute('DROP TRIGGER budgeting_method')
c.execute("UPDATE budget_settings SET method='envelope' WHERE budget_id=?",(b,))
c.execute("UPDATE budgets SET budget_type='envelope' WHERE id=?",(b,))
for kind,amount in [('envelope',12000),('tracking',999)]:
 c.execute('INSERT INTO budget_cells(budget_id,id,method,month,category_id,amount,carryover) VALUES(?,?,?,?,?,?,1)',(b,kind+':2026-11:'+k,kind,'2026-11',k,amount))
c.execute("INSERT INTO budget_months(budget_id,id,buffered,notes) VALUES(?,'2026-11',500,'Old budget note')",(b,))
c.execute("INSERT INTO notes(budget_id,id,note) VALUES(?,'budget-2026-11','Old budget note')",(b,))
c.execute("CREATE TRIGGER budgeting_method AFTER UPDATE OF method ON budget_settings BEGIN UPDATE budgets SET budget_type=NEW.method WHERE id=NEW.budget_id; END;")
payload=json.dumps({'id':b,'name':'Tracking only','budget_type':'envelope'},separators=(',',':'))
c.execute("INSERT INTO operations(scope,operation_id,payload,response) VALUES('POST /budgets','legacy-envelope-create',?,?)",(payload,payload))
c.execute('PRAGMA user_version=30');c.commit()`,
    directory,
    budget,
    category,
  ]);
  server = await startServer({ dataDirectory: directory });
  const replay = await fetch(api() + "/budgets", {method:"POST", headers:{"Content-Type":"application/json", "Idempotency-Key":"legacy-envelope-create"},body:JSON.stringify({id:budget,name:"Tracking only",budget_type:"envelope"})});
  expect(replay.status).toBe(201);
  expect((await replay.json()).data.budget_type).toBe("envelope"); // Exact historical receipt, not current state.
  let view = await call(root + "/budget?month=2026-11&count=2");
  expect(view.method).toBe("tracking");
  expect(view.epoch).not.toBe(oldEpoch);
  expect(view.months["2026-11"]).not.toHaveProperty("notes");
  expect(view.months["2026-11"].summary).not.toHaveProperty("to_budget");
  expect(view.months["2026-11"].summary.total_budgeted).toBe(12000);
  expect(view.months["2026-11"].summary.total_spent).toBe(-2500);
  expect(
    view.months["2026-12"].categories.find((c) => c.id === category).balance,
  ).toBe(9500);
  // Compare transaction data rather than cursor/epoch metadata, which intentionally rotate.
  expect(await call(root + "/transactions")).toEqual(original);
  expect(
    (
      await req("/budgets", {
        id: randomUUID(),
        name: "Removed method",
        budget_type: "envelope",
      })
    ).status,
  ).toBe(400);
  for (const action of [
    "method",
    "zero",
    "notes",
    "hold",
    "reset_hold",
    "move",
    "cover",
    "cover_overbudgeted",
  ]) {
    expect(
      (
        await req(root + "/budget", {
          snapshot: view.snapshot,
          month: "2026-11",
          action,
        })
      ).status,
    ).toBe(400);
  }
  expect(
    (
      await req(root + "/notes/commands", {
        snapshot: view.snapshot,
        id: "budget-2026-11",
        note: "Cannot return",
      })
    ).status,
  ).toBe(400);
  expect(
    (
      await req(root + "/notes/commands", {
        snapshot: view.snapshot,
        id: budget,
        note: "Cannot return",
      })
    ).status,
  ).toBe(400);
  // Current and old portable archives still stage successfully.
  const backup = await call(root + "/backups", { label: "Tracking only" });
  await call(
    "/restores",
    Buffer.from(
      await (
        await req(root + "/backups/" + backup.id + "/download")
      ).arrayBuffer(),
    ),
  );
  await call(
    "/restores",
    await readFile("../tests/fixtures/budgeting-schema15.zip"),
  );
  // A real Actual envelope export is accepted and its active allocations survive conversion.
  const stage = await call(
    "/imports/actual",
    await readFile("../tests/fixtures/budgeting-envelope-allocated.zip"),
  );
  expect(stage.status).toBe("ready");
  const activated = await call(`/imports/${stage.id}/activate`, {});
  expect(activated.budget_type).toBe("tracking");
  const imported = await call(
    `/budgets/${stage.budget_id}/budget?month=2026-11&count=4`,
  );
  const expected = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `import sqlite3,zipfile,tempfile,json,os
with zipfile.ZipFile('../tests/fixtures/budgeting-envelope-allocated.zip') as z:
 data=z.read(next(n for n in z.namelist() if n.endswith('db.sqlite')))
f=tempfile.NamedTemporaryFile(delete=False);f.write(data);f.close()
c=sqlite3.connect(f.name)
print(json.dumps(c.execute('SELECT month,category,amount FROM zero_budgets WHERE amount<>0').fetchall()))
c.close();os.unlink(f.name)`,
      ],
      { encoding: "utf8" },
    ),
  );
  expect(expected.length).toBeGreaterThan(0);
  for (const [month, cat, amount] of expected) {
    const m = String(month).slice(0, 4) + "-" + String(month).slice(4);
    if (imported.months[m])
      expect(
        imported.months[m].categories.find((c) => c.id === cat)?.budgeted,
      ).toBe(amount);
  }
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
      viewport: { width: 1440, height: 1000 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.getByText("Tracking only", { exact: true }).click();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  for (const label of [
    "Change budgeting method",
    "Clear allocations",
    "Edit month notes",
    "Hold income",
    "Move money",
  ])
    await expect(
      page.getByRole("button", { name: label, exact: true }),
    ).toHaveCount(0);
  await expect(page.locator(".budget-month")).toHaveCount(3);
  const row = page
    .locator(".budget-month")
    .first()
    .locator(".budget-category")
    .filter({ hasText: "Groceries" });
  await row.click();
  await page.getByLabel("Amount", { exact: true }).fill("130");
  await page.getByRole("button", { name: "Save plan", exact: true }).click();
  await expect(page.locator(".inline-plan-row")).toHaveCount(0);
  await mkdir("test-results/tracking-only", { recursive: true });
  await page.screenshot({
    path: "test-results/tracking-only/desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/tracking-only/mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByRole("button", { name: "All budgets", exact: true }).click();
  await page.getByRole("button", { name: "New budget", exact: true }).click();
  await expect(
    page.getByLabel("Budgeting method", { exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Budget name", { exact: true }).fill("Just tracking");
  await page
    .getByRole("button", { name: "Create budget", exact: true })
    .click();
  await expect(page.getByText("Just tracking", { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
  console.log(
    "PASS tracking-only migration, allocation carry, preserved transactions, retired APIs, Actual/native imports, row editing and desktop/mobile UI",
  );
} finally {
  if (browser) await browser.close();
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}

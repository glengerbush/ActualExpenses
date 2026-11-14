// Installation-wide migration qualification at HTTP and portable archive seams.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { startServer } from "../web/tests/server.mjs";
import {
  sourceArchive,
  nativeArchive,
  tableRows,
} from "./migration-archive.mjs";
const root = new URL("../tests/fixtures/complete-migration/", import.meta.url);
const manifest = JSON.parse(await readFile(new URL("manifest.json", root)));
const directory = await mkdtemp(tmpdir() + "/complete-migration-http-");
let server, api, currentDirectory = directory;
const epochs = new Map();
async function start() {
  server = await startServer({ dataDirectory: currentDirectory });
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
}
async function call(path, body, status = 200, key = randomUUID()) {
  const budget = /^\/budgets\/([^/]+)/.exec(path)?.[1];
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": key,
      ...(epochs.has(budget)
        ? { "X-Actual-Budget-Epoch": epochs.get(budget) }
        : {}),
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
  assert.equal(
    response.status,
    status,
    path + ": " + (await response.clone().text()).slice(0, 1500),
  );
  return response.headers.get("content-type")?.includes("application/json")
    ? response.json()
    : Buffer.from(await response.arrayBuffer());
}
const sorted = (rows) => rows.toSorted((a, b) => a.id.localeCompare(b.id));
const parse = (value) =>
  typeof value === "string" ? JSON.parse(value) : value;
const fields = (row, names) =>
  Object.fromEntries(names.map((name) => [name, row[name] ?? null]));
function compareRows(actual, expected, label) {
  assert.deepEqual(sorted(actual), sorted(expected), label);
}
function resolve(rows, id, field) {
  const seen = new Set();
  while (id != null) {
    const next = rows.find((row) => row.id === id)?.[field];
    if (next == null || next === id) break;
    assert.ok(!seen.has(id), "fixture alias cycle");
    seen.add(id);
    id = next;
  }
  return id ?? null;
}
function reconcile(source, archive) {
  const s = source.tables,
    n = (table) => tableRows(archive, table);
  const map = (
    name,
    columns,
    convert = (row) => fields(row, columns),
    target = name,
  ) =>
    compareRows(
      n(target).map((row) => fields(row, columns)),
      s[name].map(convert),
      name,
    );
  map("account_groups", ["id", "name", "tombstone"]);
  map(
    "accounts",
    ["id", "name", "off_budget", "closed", "group_id", "tombstone"],
    (r) => ({
      id: r.id,
      name: r.name,
      off_budget: r.offbudget ?? 0,
      closed: r.closed ?? 0,
      group_id: r.account_group_id ?? null,
      tombstone: r.tombstone,
    }),
  );
  map("category_groups", ["id", "name", "is_income", "hidden", "tombstone"], (r) => ({...fields(r, ["id", "name", "is_income", "tombstone"]), hidden: 0}));
  map(
    "categories",
    ["id", "name", "is_income", "group_id", "hidden", "tombstone"],
    (r) => ({
      id: r.id,
      name: r.name,
      is_income: r.is_income,
      group_id: r.cat_group,
      hidden: 0,
      tombstone: r.tombstone,
    }),
  );
  map("tags", ["id", "tag", "color", "description", "hidden", "tombstone"]);
  map(
    "cleanup_groups",
    ["id", "name", "tombstone"],
    undefined,
    "budget_cleanup_groups",
  );
  map("payee_locations", [
    "id",
    "payee_id",
    "latitude",
    "longitude",
    "tombstone",
  ]);
  map(
    "payees",
    [
      "id",
      "name",
      "favorite",
      "tombstone",
      "transfer_account_id",
      "learn_categories",
    ],
    (r) => ({
      id: r.id,
      name: r.transfer_acct
        ? s.accounts.find((a) => a.id === r.transfer_acct).name
        : r.name,
      favorite: r.favorite ?? 0,
      tombstone: r.tombstone,
      transfer_account_id: r.transfer_acct,
      learn_categories: r.learn_categories ?? 1,
    }),
  );
  for (const row of s.payee_mapping.filter((row) => row.id !== row.targetId)) {
    const payee = n("payees").find((p) => p.id === row.id);
    if (payee)
      assert.equal(
        payee.merged_into,
        resolve(s.payee_mapping, row.id, "targetId"),
      );
  }
  const transactionFields = [
    "id",
    "account_id",
    "amount",
    "date",
    "payee_id",
    "category_id",
    "notes",
    "cleared",
    "reconciled",
    "starting_balance",
    "is_parent",
    "parent_id",
    "transfer_id",
    "tombstone",
  ];
  map("transactions", transactionFields, (r) => ({
    id: r.id,
    account_id: r.acct,
    amount: r.amount,
    date: String(r.date).replace(/^(\d{4})(\d{2})(\d{2})$/, "$1-$2-$3"),
    payee_id: resolve(s.payee_mapping, r.description, "targetId"),
    category_id: resolve(s.category_mapping, r.category, "transferId"),
    notes: "", // Transaction notes were explicitly retired.
    cleared: 1,
    reconciled: 0,
    starting_balance: r.starting_balance_flag ?? 0,
    is_parent: r.isParent ?? 0,
    parent_id: r.parent_id,
    transfer_id: r.transferred_id,
    tombstone: r.tombstone,
  }));
  const ruleFields = [
    "id",
    "stage",
    "conditions_op",
    "conditions",
    "actions",
    "tombstone",
  ];
  const normalizedRule = (row) => ({
    ...fields(row, ruleFields),
    conditions: parse(row.conditions),
    actions: parse(row.actions),
  });
  const aliases = {
    acct: "account",
    description: "payee",
    imported_description: "imported_payee",
  };
  compareRows(
    n("rules").map(normalizedRule),
    s.rules.map((row) => {
      const r = normalizedRule(row);
      for (const item of [...r.conditions, ...r.actions])
        if (item.field in aliases) item.field = aliases[item.field];
      return r;
    }),
    "rules and schedule-rule relationships",
  );
  map(
    "schedules",
    ["id", "rule_id", "name", "posts_transaction", "completed", "tombstone"],
    (r) => ({
      id: r.id,
      rule_id: r.rule,
      name: r.name,
      posts_transaction: r.posts_transaction ?? 0,
      completed: r.completed ?? 0,
      tombstone: r.tombstone,
    }),
  );
  for (const row of s.schedules_next_date) {
    const next =
      row.local_next_date_ts === row.base_next_date_ts
        ? row.local_next_date
        : row.base_next_date;
    assert.equal(
      n("schedules").find((r) => r.id === row.schedule_id).next_date,
      String(next).replace(/^(\d{4})(\d{2})(\d{2})$/, "$1-$2-$3"),
    );
  }
  const reports = [];
  for (const [table, kind] of [
    ["dashboard_pages", "page"],
    ["dashboard", "widget"],
    ["custom_reports", "custom_report"],
  ])
    for (const row of s[table]) {
      const { id, tombstone, ...config } = row;
      for (const key of [
        "meta",
        "conditions",
        "metadata",
        "selected_categories",
      ])
        if (typeof config[key] === "string" && config[key] !== "")
          config[key] = parse(config[key]);
      reports.push({ id, kind, tombstone: kind === "widget" && ["formula-card", "monte-carlo-card", "sankey-card", "budget-analysis-card", "balance-forecast-card"].includes(config.type) ? 1 : tombstone, config });
    }
  compareRows(
    n("reporting_documents").map((r) => ({
      id: r.id,
      kind: r.kind,
      tombstone: r.tombstone,
      config: parse(r.config),
    })),
    reports,
    "saved report metadata",
  );
  const cells = [];
  for (const [table, method] of [
    ["zero_budgets", "envelope"],
    ["reflect_budgets", "tracking"],
  ])
    for (const row of s[table]) {
      const month = String(row.month).replace(/^(\d{4})(\d{2})$/, "$1-$2");
      cells.push({
        id: `${method}:${month}:${row.category}`,
        method,
        month,
        category_id: row.category,
        amount: row.amount ?? 0,
        carryover: row.carryover ?? 0,
        goal: row.goal,
        long_goal: row.long_goal,
      });
    }
  const sourceMethod = s.preferences?.find((row) => row.id === "budgetType")?.value ?? "envelope";
  if (sourceMethod === "envelope") {
    // The formerly active envelope plan becomes tracking. Preserve its exact
    // allocations/carryover and the historical envelope rows in the archive.
    const active = cells.filter((row) => row.method === "envelope");
    cells.splice(0, cells.length, ...active, ...active.map((row) => ({
      ...row, id: row.id.replace(/^envelope:/, "tracking:"), method: "tracking",
    })));
  }
  compareRows(
    n("budget_cells").map((r) => ({
      ...fields(r, [
        "id",
        "method",
        "month",
        "category_id",
        "amount",
        "carryover",
      ]),
      goal: parse(r.goal),
      long_goal: parse(r.long_goal),
    })),
    cells,
    "allocations and goal values",
  );
  compareRows(
    n("budget_automations").map((r) => ({
      id: r.id,
      goal_def: parse(r.goal_def),
      cleanup_def: parse(r.cleanup_def),
      template_settings: parse(r.template_settings),
    })),
    s.categories.map((r) => ({
      id: r.id,
      goal_def: parse(r.goal_def),
      cleanup_def: parse(r.cleanup_def),
      template_settings: parse(r.template_settings) ?? { source: "notes" },
    })),
    "automation definitions",
  );
  for (const row of s.notes) {
    const category = n("categories").find((c) => c.id === row.id);
    assert.ok(category, "composite notes belong to categories");
    assert.equal(category.notes, "");
  }
  const settings = parse(n("register_settings")[0].settings);
  compareRows(
    settings.saved_filters.map((r) => ({ id: r.id, name: r.name })),
    s.transaction_filters
      .filter((r) => !r.tombstone)
      .map((r) => fields(r, ["id", "name"])),
    "saved filter identity",
  );
  assert.equal(n("imported_sources").length, 1);
  assert.deepEqual(
    parse(n("imported_sources")[0].source_metadata),
    source.metadata,
  );
}
try {
  for (const fixture of manifest.cases) {
    currentDirectory = directory + '/' + fixture.name;
    await start();
    const bytes = await readFile(new URL(fixture.name + ".zip", root));
    assert.equal(createHash("sha256").update(bytes).digest("hex"),fixture.archive_sha256);
    const stage = (await call("/imports/actual", bytes, 201)).data;
    assert.equal(stage.status,"ready",JSON.stringify(stage.report.requirements));
    assert.deepEqual(stage.report.counts,fixture.source_counts);
    assert.deepEqual(await call(`/imports/${stage.id}/original.zip`),bytes);
    const source=sourceArchive(bytes), activationKey=randomUUID();
    await server.stop(); await start();
    assert.deepEqual((await call(`/imports/${stage.id}`)).data,stage);
    const activation=await call(`/imports/${stage.id}/activate`,{},201,activationKey);
    await server.stop(); await start();
    assert.equal((await call("/budgets")).data.length,1);
    const path = `/budgets/${stage.budget_id}`;
    assert.deepEqual(
      await call(`/imports/${stage.id}/activate`, {}, 201, activationKey),
      activation,
    );
    if (fixture.name !== "empty") {
      const prefs = (await call(path + "/preferences")).data;
      assert.equal(prefs.date_format, "dd/MM/yyyy");
      assert.equal(prefs.hide_fraction, false);
      assert.equal(prefs.experimental, undefined);
    }
    for (const expected of fixture.balances) {
      const actual = (await call(`${path}/accounts/${expected.id}`)).data;
      assert.equal(
        actual.balance,
        expected.balance,
        fixture.name + " account balance",
      );
    }
    const backup = (
      await call(
        path + "/backups",
        { label: "Complete migration qualification" },
        201,
      )
    ).data;
    const zip = await call(`${path}/backups/${backup.id}/download`),
      archive = nativeArchive(zip);
    reconcile(source, archive);
    const restore = (await call("/restores", zip, 201)).data;
    await call(`${path}/restores/${restore.id}/activate`, {
      confirm_name: archive.state.budget.name,
    });
    epochs.set(stage.budget_id, (await call(path)).data.epoch);
    const after = (
      await call(
        path + "/backups",
        { label: "Restored migration qualification" },
        201,
      )
    ).data;
    reconcile(
      source,
      nativeArchive(await call(`${path}/backups/${after.id}/download`)),
    );
    assert.deepEqual(await call(`/imports/${stage.id}/original.zip`), bytes);
    assert.deepEqual(
      (await call(`/imports/${stage.id}`)).data.report,
      stage.report,
    );
    console.log(
      `PASS ${fixture.name}: ${source.tables.transactions.length} source rows, all feature records, balances, activation replay, backup/restore and original audit`,
    );
    await server.stop();
  }
  await start();
  const before = (await call("/budgets")).data;
  await call("/imports/actual", Buffer.from("corrupt archive"), 422);
  assert.deepEqual((await call("/budgets")).data, before);
} finally {
  await server?.stop();
  await rm(directory, { recursive: true, force: true });
}

// Resource queries execute the published engine; no replacement projections/calculators.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
process.env.TZ = "UTC";
const NativeDate = Date,
  clock = "2026-11-01T12:00:00Z";
globalThis.Date = class extends NativeDate {
  constructor(...args) {
    super(...(args.length ? args : [clock]));
  }
  static now() {
    return NativeDate.parse(clock);
  }
};
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/query-resources-source-"),
  check = process.argv.includes("--check"),
  path = new URL(
    "../tests/fixtures/query-resources-source.json",
    import.meta.url,
  ),
  hash = (v) => createHash("sha256").update(v).digest("hex"),
  plain = (v) => JSON.parse(JSON.stringify(v));
assert.equal(
  JSON.parse(
    await readFile(
      require
        .resolve("@actual-app/api")
        .replace(/dist\/index\.js$/, "package.json"),
    ),
  ).version,
  "26.10.0",
);
const sets = [
  [
    "transaction-metadata",
    "transaction-merge-bank-id-kept-26.10.zip",
    [
      [
        "transaction-import-metadata",
        api
          .q("transactions")
          .select(["id", "imported_id", "raw_synced_data", "schedule"])
          .orderBy("id"),
      ],
    ],
  ],
  [
    "envelope-cells",
    "budget-template-financial/envelope-copy-history.zip",
    [
      [
        "envelope-budget-cells",
        api
          .q("zero_budgets")
          .select([
            "month",
            "category",
            "amount",
            "carryover",
            "goal",
            "long_goal",
          ])
          .orderBy(["month", "category"]),
      ],
    ],
  ],
  [
    "tracking-cells",
    "budget-template-financial/tracking-copy-history.zip",
    [
      [
        "tracking-budget-cells",
        api
          .q("reflect_budgets")
          .select([
            "month",
            "category",
            "amount",
            "carryover",
            "goal",
            "long_goal",
          ])
          .orderBy(["month", "category"]),
      ],
    ],
  ],

  [
    "references",
    "transaction-export-base-26.10.zip",
    [
      [
        "numeric-like",
        api
          .q("transactions")
          .filter({ amount: { $like: "1234" } })
          .select(["id"])
          .orderBy("id"),
      ],
      [
        "numeric-lower",
        api
          .q("transactions")
          .select(["id", { text: { $lower: "$amount" } }])
          .orderBy("id"),
      ],
      [
        "accounts",
        api
          .q("accounts")
          .select(["id", "name", "offbudget", "closed", "account_group_id"])
          .orderBy("id"),
      ],
      [
        "account-groups",
        api.q("account_groups").select(["id", "name"]).orderBy("id"),
      ],
      [
        "payees",
        api
          .q("payees")
          .select([
            "id",
            "name",
            "transfer_acct",
            "favorite",
            "learn_categories",
          ])
          .orderBy("id"),
      ],
      [
        "payee-account-join",
        api
          .q("payees")
          .select(["id", "transfer_acct.name", "transfer_acct.offbudget"])
          .orderBy("id"),
      ],
      [
        "categories",
        api
          .q("categories")
          .select(["id", "name", "is_income", "hidden", "group"])
          .orderBy("id"),
      ],
      [
        "category-groups",
        api
          .q("category_groups")
          .select(["id", "name", "is_income", "hidden"])
          .orderBy("id"),
      ],
      [
        "category-join",
        api
          .q("categories")
          .select(["id", "name", "group.name", "group.hidden"])
          .orderBy("id"),
      ],
      ["category-count", api.q("categories").calculate({ $count: "*" })],
      [
        "category-groups-count",
        api
          .q("categories")
          .select(["group", { total: { $count: "*" } }])
          .groupBy("group")
          .orderBy("group"),
      ],
      [
        "joined-category-filter",
        api
          .q("categories")
          .filter({ "group.is_income": false })
          .select(["id", "name"])
          .orderBy("id"),
      ],
    ],
  ],
  [
    "rules",
    "transaction-import-rules-before-match-26.10.zip",
    [
      [
        "rules",
        api
          .q("rules")
          .select(["id", "stage", "conditions_op", "conditions", "actions"])
          .orderBy("id"),
      ],
      ["rule-count", api.q("rules").calculate({ $count: "*" })],
    ],
  ],
  [
    "schedules",
    "schedules-26.10.zip",
    [
      [
        "schedules",
        api
          .q("schedules")
          .select([
            "id",
            "name",
            "rule",
            "next_date",
            "posts_transaction",
            "completed",
            "custom_upcoming_length",
          ])
          .orderBy("id"),
      ],
      [
        "schedule-rule-join",
        api
          .q("schedules")
          .select([
            "id",
            "rule.stage",
            "rule.conditions_op",
            "rule.conditions",
            "rule.actions",
          ])
          .orderBy("id"),
      ],
      ["schedule-count", api.q("schedules").calculate({ $count: "*" })],
    ],
  ],
  [
    "automations",
    "budget-templates-simple-base-26.10.zip",
    [
      [
        "category-automations",
        api
          .q("categories")
          .select(["id", "goal_def", "cleanup_def", "template_settings"])
          .orderBy("id"),
      ],
    ],
  ],
];
try {
  await api.init({ dataDir: directory });
  const records = [];
  for (const [id, filename, queries] of sets) {
    const bytes = await readFile(
      new URL("../tests/fixtures/" + filename, import.meta.url),
    );
    await api.importBudget(bytes, { filename: "query-" + id + ".zip" });
    const financial = () =>
      api.aqlQuery(
        api
          .q("transactions")
          .options({ splits: "all" })
          .select("*")
          .orderBy("id"),
      );
    const before = plain((await financial()).data),
      cases = [];
    for (const [name, q] of queries) {
      try {
        cases.push({
          id: name,
          query: q.serialize(),
          data: plain((await api.aqlQuery(q)).data),
        });
      } catch (e) {
        cases.push({
          id: name,
          query: q.serialize(),
          error: { type: e.type ?? e.name, message: e.message },
        });
      }
    }
    assert.deepEqual(
      plain((await financial()).data),
      before,
      "Read queries leave source financial graph unchanged",
    );
    records.push({
      id,
      archive: { filename, sha256: hash(bytes) },
      financial: before,
      cases,
    });
  }
  const output = {
    oracle:
      "@actual-app/api26.10 actual q/aqlQuery over reviewed genuine archives",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    engine_entry_sha256: hash(
      await readFile(require.resolve("@actual-app/api")),
    ),
    sets: records,
  };
  if (check) assert.deepEqual(output, JSON.parse(await readFile(path)));
  else if (process.argv.includes("--extend")) {
    const old = JSON.parse(await readFile(path));
    for (const set of old.sets)
      for (const c of set.cases)
        assert.deepEqual(
          output.sets
            .find((s) => s.id === set.id)
            .cases.find((n) => n.id === c.id),
          c,
        );
    await writeFile(path, JSON.stringify(output, null, 2) + "\n");
  } else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS21 real source resource/relationship/JSON/aggregate query cases",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

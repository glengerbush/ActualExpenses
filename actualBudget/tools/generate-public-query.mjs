// Independent ActualQL execution through published @actual-app/api26.10.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
process.env.TZ = "UTC";
const OriginalDate = Date,
  clock = "2026-11-09T12:00:00.000Z";
globalThis.Date = class extends OriginalDate {
  constructor(...args) {
    super(...(args.length ? args : [clock]));
  }
  static now() {
    return OriginalDate.parse(clock);
  }
};
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api");
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
const directory = await mkdtemp(tmpdir() + "/public-query-source-"),
  check = process.argv.includes("--check"),
  extend = process.argv.includes("--extend"),
  path = new URL("../tests/fixtures/public-query-source.json", import.meta.url),
  hash = (v) => createHash("sha256").update(v).digest("hex"),
  plain = (v) => JSON.parse(JSON.stringify(v));
const archive = await readFile(
    new URL(
      "../tests/fixtures/transaction-export-base-26.10.zip",
      import.meta.url,
    ),
  ),
  seed = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-export-source.json",
        import.meta.url,
      ),
    ),
  );
assert.equal(hash(archive), seed.archive.sha256);
try {
  await api.init({ dataDir: directory });
  await api.importBudget(archive, {
    type: "actual",
    filename: "public-query-seed.zip",
  });
  const ids = seed.ids,
    q = () => api.q("transactions"),
    fields = [
      "id",
      "account",
      "payee",
      "category",
      "date",
      "amount",
      "notes",
      "is_parent",
      "is_child",
      "parent_id",
      "transfer_id",
      "cleared",
      "reconciled",
    ];
  const cases = [];
  const run = async (id, query) => {
    let result;
    try {
      result = { data: plain((await api.aqlQuery(query)).data) };
    } catch (error) {
      result = {
        error: {
          type: error?.type ?? error?.name ?? null,
          message: error?.message ?? String(error),
        },
      };
    }
    cases.push({ id, query: query.serialize(), ...result });
  };
  const before = plain(
    (
      await api.aqlQuery(
        q().select("*").options({ splits: "all" }).orderBy("id"),
      )
    ).data,
  );
  for (const mode of ["inline", "grouped", "all", "none"]) {
    await run(
      "split-" + mode,
      q().options({ splits: mode }).select(fields).orderBy("id"),
    );
    await run(
      "sum-" + mode,
      q().options({ splits: mode }).calculate({ $sum: "$amount" }),
    );
    await run(
      "count-" + mode,
      q().options({ splits: mode }).calculate({ $count: "*" }),
    );
  }
  await run(
    "grouped-child-match",
    q()
      .options({ splits: "grouped" })
      .filter({ category: ids.food })
      .select(fields)
      .orderBy("id"),
  );
  await run(
    "join-three-levels",
    q()
      .select([
        "id",
        "account.name",
        "payee.name",
        "category.name",
        "category.group.name",
        "category.is_income",
      ])
      .orderBy("id"),
  );
  await run(
    "joined-filter",
    q()
      .filter({ "category.group.name": "-Expenses" })
      .select(["id", "amount", "category.name"])
      .orderBy("id"),
  );
  await run(
    "sum-by-payee",
    q()
      .groupBy("payee.name")
      .select([
        "payee.name",
        { total: { $sum: "$amount" } },
        { count: { $count: "*" } },
      ])
      .orderBy("payee.name"),
  );
  await run(
    "sum-by-month",
    q()
      .groupBy({ $month: "$date" })
      .select([{ month: { $month: "$date" } }, { total: { $sum: "$amount" } }])
      .orderBy({ $month: "$date" }),
  );
  await run(
    "named-sum",
    q()
      .filter({ amount: { $lt: 0 } })
      .select({ total: { $sum: "$amount" } }),
  );
  await run(
    "empty-sum",
    q()
      .filter({ id: "47000000-0000-4000-8000-000000000999" })
      .calculate({ $sum: "$amount" }),
  );
  await run(
    "empty-count",
    q()
      .filter({ id: "47000000-0000-4000-8000-000000000999" })
      .calculate({ $count: "*" }),
  );
  const ordered = () =>
    q()
      .select(["id", "date", "amount"])
      .orderBy([{ amount: "asc" }, { id: "asc" }]);
  await run("page-all", ordered());
  await run("page-first", ordered().limit(2));
  await run("page-second", ordered().limit(2).offset(2));
  await run(
    "nested-and-or",
    q()
      .filter({
        $and: [
          { date: [{ $gte: "2026-11-01" }, { $lte: "2026-11-30" }] },
          { $or: [{ cleared: true }, { amount: { $gt: 0 } }] },
        ],
      })
      .select(["id", "amount", "cleared"])
      .orderBy("id"),
  );
  await run(
    "null-category",
    q().filter({ category: null }).select(["id", "category"]).orderBy("id"),
  );
  await run(
    "not-category-includes-null",
    q()
      .filter({ category: { $ne: ids.food } })
      .select(["id", "category"])
      .orderBy("id"),
  );
  await run(
    "membership",
    q()
      .filter({ id: { $oneof: [ids.first, ids.parent, ids.first] } })
      .select(["id", "amount"])
      .options({ splits: "all" })
      .orderBy("id"),
  );
  await run(
    "empty-membership",
    q()
      .filter({ id: { $oneof: [] } })
      .select("id")
      .orderBy("id"),
  );
  await run(
    "like-notlike",
    q()
      .filter({ notes: { $notlike: "%line%" } })
      .select(["id", "notes"])
      .orderBy("id"),
  );
  await run(
    "regexp",
    q()
      .filter({ notes: { $regexp: "line|SUM" } })
      .select(["id", "notes"])
      .orderBy("id"),
  );
  await run(
    "documented-regex-spelling",
    q()
      .filter({ notes: { $regex: "line" } })
      .select("id"),
  );
  await run(
    "month-transform",
    q()
      .filter({ date: { $transform: "$month", $eq: "2026-11" } })
      .select(["id", "date"])
      .orderBy("id"),
  );
  await run(
    "scalar-functions",
    q()
      .select([
        "id",
        {
          absolute: { $abs: "$amount" },
          negative: { $neg: "$amount" },
          half: { $idiv: ["$amount", 2] },
          lower: { $lower: "$notes" },
          prefix: { $substr: ["$notes", 1, 4] },
          year: { $year: "$date" },
          month: { $month: "$date" },
          day: { $day: "$date" },
          literal: { $literal: "$amount" },
          identity: { $id: "$id" },
        },
      ])
      .orderBy("id"),
  );
  for (const [name, expression] of Object.entries({
    negative: { $neg: "$amount" },
    half: { $idiv: ["$amount", 2] },
    lower: { $lower: "$notes" },
    prefix: { $substr: ["$notes", 1, 4] },
    year: { $year: "$date" },
    month: { $month: "$date" },
    day: { $day: "$date" },
    identity: { $id: "$id" },
    condition: { $condition: { amount: { $lt: 0 } } },
    nocase: { $nocase: "$notes" },
    literal_number: { $literal: 123 },
    divide_zero: { $idiv: ["$amount", 0] },
  }))
    await run(
      "function-" + name,
      q()
        .select(["id", { value: expression }])
        .orderBy("id"),
    );
  await run(
    "window-sum",
    q()
      .select(["id", "amount", { remaining: { $sumOver: "$amount" } }])
      .orderBy([{ date: "asc" }, { id: "asc" }]),
  );
  await run(
    "conditional-expression",
    q()
      .select(["id", { expense: { $condition: { amount: { $lt: 0 } } } }])
      .orderBy("id"),
  );
  await run(
    "literal-unicode",
    q()
      .select([{ value: { $literal: "Café $$ value" } }])
      .limit(1),
  );
  await run("invalid-table", api.q("operations").select("*"));
  await run("invalid-field", q().select("password"));
  await run("invalid-function", q().calculate({ $evil: "$amount" }));
  await run(
    "invalid-split-option",
    q().options({ splits: "bogus" }).select("id"),
  );
  const after = plain(
    (
      await api.aqlQuery(
        q().select("*").options({ splits: "all" }).orderBy("id"),
      )
    ).data,
  );
  assert.deepEqual(
    after,
    before,
    "Every source read/error leaves financial rows unchanged",
  );
  assert.ok(cases.some((c) => c.error));
  assert.ok(cases.some((c) => c.data !== undefined));
  const source_files = {};
  for (const file of [
    "packages/api/app/query.ts",
    "packages/loot-core/src/server/aql/compiler.ts",
    "packages/loot-core/src/server/aql/schema/index.ts",
    "packages/loot-core/src/server/aql/schema/executors.ts",
  ])
    source_files[file] = hash(
      await readFile(
        new URL("../reference/actual-26.10.0/" + file, import.meta.url),
      ),
    );
  const output = plain({
    oracle: "@actual-app/api@26.10.0 actual q/aqlQuery",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    source_files,
    archive: seed.archive,
    provenance:
      "Uses reviewed genuine transaction-export seed archive; no replacement query/compiler/formulas. All source IDs and source query serialization/results preserved. JSON wire normalizes negative zero. Source failures retain type/message, no process-specific stack.",
    before,
    cases,
  });
  if (check) assert.deepEqual(output, JSON.parse(await readFile(path)));
  else if (extend) {
    const old = JSON.parse(await readFile(path));
    for (const record of old.cases)
      assert.deepEqual(
        output.cases.find((c) => c.id === record.id),
        record,
        "Existing source observation stays unchanged: " + record.id,
      );
    assert.deepEqual(output.before, old.before);
    assert.deepEqual(output.source_files, old.source_files);
    await writeFile(path, JSON.stringify(output, null, 2) + "\n");
  } else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS " +
      cases.length +
      " independently executed query observations, " +
      cases.filter((c) => c.error).length +
      " source errors",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

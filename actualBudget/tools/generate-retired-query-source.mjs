// Real source category deletion + ActualQL and unchanged custom-report spreadsheet.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { sourceRuntime } from "./reports-source-runtime.mjs";
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
  api = require("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/retired-query-source-"),
  check = process.argv.includes("--check"),
  extend = process.argv.includes("--extend"),
  saved = process.argv.includes("--saved"),
  suffix = saved ? "-saved" : "",
  path = new URL(
    "../tests/fixtures/retired-query" + suffix + "-source.json",
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
const seed = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-export-source.json",
        import.meta.url,
      ),
    ),
  ),
  input = await readFile(
    new URL("../tests/fixtures/" + seed.archive.filename, import.meta.url),
  );
assert.equal(hash(input), seed.archive.sha256);
const records = [];
try {
  const engine = await api.init({ dataDir: directory });
  for (const mode of ["deleted-no-replacement", "deleted-with-replacement"]) {
    await api.importBudget(input, { filename: "query-category-source.zip" });
    const ids = seed.ids,
      before = plain(
        (
          await api.aqlQuery(
            api
              .q("transactions")
              .options({ splits: "all" })
              .select(["id", "amount", "category", "parent_id", "transfer_id"])
              .orderBy("id"),
          )
        ).data,
      );
    assert.ok((await api.getCategories()).some((c) => c.id === ids.food));
    const conditions = [
      [
        "old-equality",
        { category: ids.food },
        { field: "category", op: "is", value: ids.food },
      ],
      [
        "old-membership",
        { category: { $oneof: [ids.food] } },
        { field: "category", op: "oneOf", value: [ids.food] },
      ],
      [
        "explicit-null",
        { category: null },
        { field: "category", op: "is", value: null },
      ],
      [
        "target-equality",
        { category: ids.other },
        { field: "category", op: "is", value: ids.other },
      ],
      [
        "old-and-target-membership",
        { category: { $oneof: [ids.food, ids.other] } },
        { field: "category", op: "oneOf", value: [ids.food, ids.other] },
      ],
    ];
    if (saved)
      for (const [id, , condition] of conditions)
        await engine.send("filter-create", {
          state: { name: id, conditions: [condition], conditionsOp: "and" },
          filters: [],
        });
    await api.deleteCategory(
      ids.food,
      mode === "deleted-with-replacement" ? ids.other : undefined,
    );
    const after = plain(
        (
          await api.aqlQuery(
            api
              .q("transactions")
              .options({ splits: "all" })
              .select(["id", "amount", "category", "parent_id", "transfer_id"])
              .orderBy("id"),
          )
        ).data,
      ),
      categories = await engine.send("get-categories"),
      accounts = await api.getAccounts(),
      payees = await api.getPayees(),
      runtime = sourceRuntime(api, engine),
      cases = [];

    for (const [id, filter, condition] of conditions) {
      const query = api
          .q("transactions")
          .filter(filter)
          .select(["id", "amount", "category"])
          .orderBy("id"),
        data = plain((await api.aqlQuery(query)).data),
        total = (
          await api.aqlQuery(
            api.q("transactions").filter(filter).calculate({ $sum: "$amount" }),
          )
        ).data;
      const options = {
        startDate: "2026-11",
        endDate: "2026-11",
        interval: "Monthly",
        conditions: [condition],
        conditionsOp: "and",
        showEmpty: false,
        showOffBudget: true,
        showHiddenCategories: true,
        showUncategorized: true,
        trimIntervals: false,
        groupBy: "Account",
        balanceTypeOp: "totalTotals",
        sortByOp: "asc",
        graphType: "BarGraph",
        firstDayOfWeekIdx: "1",
      };
      let report;
      await runtime
        .load("spreadsheets/custom-spreadsheet")
        .createCustomSpreadsheet({ ...options, categories, accounts, payees })(
        {},
        (value) => (report = plain(value)),
      );
      assert.ok(report);
      delete report.legend;
      const grouped = plain(
        (
          await api.aqlQuery(
            api
              .q("transactions")
              .options({ splits: "grouped" })
              .filter(filter)
              .select(["id", "amount", "category"])
              .orderBy("id"),
          )
        ).data,
      );
      cases.push({
        id,
        filter,
        condition,
        query: query.serialize(),
        data,
        total,
        options,
        report,
        grouped,
      });
    }
    const original =
      check || extend
        ? await readFile(
            new URL(
              `../tests/fixtures/retired-query${suffix}-${mode}-26.10.zip`,
              import.meta.url,
            ),
          )
        : await api.exportBudget();
    if (!check && !extend)
      await writeFile(
        new URL(
          `../tests/fixtures/retired-query${suffix}-${mode}-26.10.zip`,
          import.meta.url,
        ),
        original,
        { flag: "wx" },
      );
    assert.deepEqual(
      plain(
        (
          await api.aqlQuery(
            api
              .q("transactions")
              .options({ splits: "all" })
              .select(["id", "amount", "category", "parent_id", "transfer_id"])
              .orderBy("id"),
          )
        ).data,
      ),
      after,
    );
    records.push({
      id: mode,
      input: seed.archive,
      ids,
      before,
      after,
      archive: {
        filename: `retired-query${suffix}-${mode}-26.10.zip`,
        sha256: hash(original),
      },
      source_files: runtime.hashes,
      ...(saved
        ? {
            saved_filters: plain(
              (
                await api.aqlQuery(
                  api.q("transaction_filters").select("*").orderBy("name"),
                )
              ).data,
            ),
          }
        : {}),
      cases,
    });
  }
  const output = plain({
    oracle:
      "Actual26.10 public deleteCategory/aqlQuery +unchanged custom spreadsheet using real AQL",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    provenance:
      "Original genuine seed reimported independently for each actual deletion mode. Input IDs, all monetary/category/graph rows, saved predicate shapes, source report results and genuine post-delete exports retained. Presentation-only legend omitted as prior report source fixture.",
    cases: records,
  });
  const stable = (value) => {
    const ids = new Map(
      value.cases.flatMap((c) =>
        (c.saved_filters ?? []).map((f) => [f.id, "filter:" + f.name]),
      ),
    );
    const walk = (v) =>
      typeof v === "string"
        ? (ids.get(v) ?? v)
        : Array.isArray(v)
          ? v.map(walk)
          : v && typeof v === "object"
            ? Object.fromEntries(
                Object.entries(v).map(([k, x]) => [k, walk(x)]),
              )
            : v;
    return walk(value);
  };
  if (check)
    assert.deepEqual(stable(output), stable(JSON.parse(await readFile(path))));
  else if (extend) {
    const old = JSON.parse(await readFile(path));
    for (let i = 0; i < old.cases.length; i++) {
      const fresh = {
        ...output.cases[i],
        cases: output.cases[i].cases.map(({ grouped, ...c }) => c),
      };
      assert.deepEqual(
        stable({ cases: [fresh] }),
        stable({ cases: [old.cases[i]] }),
        "Old source observations remain exact",
      );
      old.cases[i].cases = old.cases[i].cases.map((c, j) => ({
        ...c,
        grouped: output.cases[i].cases[j].grouped,
      }));
    }
    await writeFile(path, JSON.stringify(old, null, 2) + "\n");
  } else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS source retired category equality/membership/null/replacement and real custom report projections",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

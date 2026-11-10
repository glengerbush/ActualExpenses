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
  directory = await mkdtemp(tmpdir() + "/query-types-source-"),
  check = process.argv.includes("--check"),
  path = new URL("../tests/fixtures/query-types-source.json", import.meta.url),
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
const tx = () => api.q("transactions").select(["id"]).orderBy("id");
const sets = [
  [
    "types",
    "transaction-export-base-26.10.zip",
    [
      [
        "month-literal",
        api
          .q("transactions")
          .select([{ month: { $month: "2026-11-05" } }])
          .limit(1),
      ],
      [
        "year-literal",
        api
          .q("transactions")
          .select([{ year: { $year: "2026" } }])
          .limit(1),
      ],
      [
        "month-notes",
        api
          .q("transactions")
          .select([{ month: { $month: "$notes" } }])
          .limit(1),
      ],
      [
        "month-invalid-literal",
        api
          .q("transactions")
          .select([{ month: { $month: "wrong" } }])
          .limit(1),
      ],
      ["date-year", tx().filter({ date: "2026" })],
      ["date-month", tx().filter({ date: "2026-11" })],
      ["date-day", tx().filter({ date: "2026-11-05" })],
      ["date-month-gte", tx().filter({ date: { $gte: "2026-11" } })],
      ["date-invalid", tx().filter({ date: "2026-13-40" })],
      ["date-number", tx().filter({ date: 20261105 })],
      ["boolean-null", tx().filter({ cleared: null })],
      ["boolean-ne-null", tx().filter({ cleared: { $ne: null } })],
      ["boolean-zero", tx().filter({ cleared: 0 })],
      ["boolean-text", tx().filter({ cleared: "false" })],
      ["id-number", tx().filter({ id: 1 })],
      ["amount-string", tx().filter({ amount: "1234" })],
      [
        "lower-id",
        api
          .q("transactions")
          .select([{ text: { $lower: "$id" } }])
          .orderBy("id"),
      ],
      [
        "lower-date",
        api
          .q("transactions")
          .select([{ text: { $lower: "$date" } }])
          .orderBy("id"),
      ],
      [
        "substr-number",
        api
          .q("transactions")
          .select([{ text: { $substr: ["$amount", 1, 2] } }])
          .orderBy("id"),
      ],
      [
        "scalar-aggregate",
        api.q("transactions").calculate({ $abs: { $sum: "$amount" } }),
      ],
      [
        "group-order-aggregate",
        api
          .q("transactions")
          .select(["account", { total: { $sum: "$amount" } }])
          .groupBy("account")
          .orderBy({ $sum: "$amount" }),
      ],
      [
        "date-membership",
        tx().filter({ date: { $oneof: ["2026-11", "2026-11-05"] } }),
      ],
      [
        "boolean-membership",
        tx().filter({ cleared: { $oneof: [null, true] } }),
      ],
      ["boolean-like", tx().filter({ cleared: { $like: "true" } })],
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
  console.log("PASS24 real source type/calendar/expression cases");
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

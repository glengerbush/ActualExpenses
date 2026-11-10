// Executed AQL normalization; expected matching never computed by replacement code.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/query-unicode-source-"),
  path = new URL(
    "../tests/fixtures/query-unicode-source.json",
    import.meta.url,
  ),
  check = process.argv.includes("--check"),
  seed = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-export-source.json",
        import.meta.url,
      ),
    ),
  );
const hash = (v) => createHash("sha256").update(v).digest("hex");
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
const sourceFiles = Object.fromEntries(
  await Promise.all(
    [
      "packages/loot-core/src/shared/normalisation.ts",
      "packages/loot-core/src/server/aql/compiler.ts",
    ].map(async (p) => [
      p,
      hash(
        await readFile(
          new URL("../reference/actual-26.10.0/" + p, import.meta.url),
        ),
      ),
    ]),
  ),
);
const inputs = [
  ["composed", "Café", "cafe"],
  ["decomposed", "Cafe\u0301", "CAFÉ"],
  ["expansions", "ŁØßŒ", "lossoe"],
  ["greek", "Άλφα", "αλφα"],
  ["greek-final", "ΟΣ", "ος"],
  ["non-diacritic-mark", "a\u034fb", "ab"],
  ["non-diacritic-preserved", "a\u034fb", "a\u034fb"],
  ["diacritic-modifier", "a\u02b0b", "ab"],
  ["wildcard-percent", "Crème brûlée", "creme%brulee"],
  ["wildcard-question", "café", "caf?"],
  ["underscore-literal", "café", "caf_"],
  ["escaped-percent", "100% café", "100\\% cafe"],
  ["negative", "café", "tea"],
  ["empty", "café", ""],
  ["empty-text", "", "%"],
  ["empty-both", "", ""],
  ["only-diacritic", "\u0301", "%"],
];
try {
  await api.init({ dataDir: directory });
  const archive = await readFile(
    new URL("../tests/fixtures/" + seed.archive.filename, import.meta.url),
  );
  assert.equal(hash(archive), seed.archive.sha256);
  await api.importBudget(archive, { filename: "unicode-query.zip" });
  const cases = [];
  for (const [id, notes, pattern] of inputs) {
    await api.updateTransaction(seed.ids.first, { notes });
    let observed;
    for (let attempt = 0; attempt < 100; attempt++) {
      observed = (
        await api.aqlQuery(
          api
            .q("transactions")
            .filter({ id: seed.ids.first })
            .select(["id", "notes"]),
        )
      ).data;
      if (observed[0]?.notes === notes) break;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    assert.deepEqual(
      observed,
      [{ id: seed.ids.first, notes }],
      "Source mutation must be observable before matching",
    );
    for (const op of ["$like", "$notlike", "$regexp"]) {
      const query = api
        .q("transactions")
        .filter({ id: seed.ids.first, notes: { [op]: pattern } })
        .select(["id", "notes"])
        .orderBy("id");
      let result;
      try {
        result = { data: (await api.aqlQuery(query)).data };
      } catch (e) {
        result = { error: { type: e.type ?? e.name, message: e.message } };
      }
      cases.push({
        id: id + "-" + op,
        notes,
        pattern,
        op,
        query: query.serialize(),
        ...result,
      });
    }
  }
  const output = {
    oracle:
      "@actual-app/api26.10 updateTransaction + real AQL LIKE/NOTLIKE/regexp",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    input: seed.archive,
    source_files: sourceFiles,
    engine_entry_sha256: hash(
      await readFile(require.resolve("@actual-app/api")),
    ),
    id: seed.ids.first,
    cases,
  };
  if (check) assert.deepEqual(output, JSON.parse(await readFile(path)));
  else if (process.argv.includes("--extend")) {
    const old = JSON.parse(await readFile(path));
    for (const c of old.cases)
      assert.deepEqual(
        output.cases.find((n) => n.id === c.id),
        c,
        "Old Unicode observations unchanged",
      );
    await writeFile(path, JSON.stringify(output, null, 2) + "\n");
  } else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS" + cases.length + " genuine Unicode/filter source observations",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

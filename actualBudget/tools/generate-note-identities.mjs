// Real notes row presence: missing differs from an explicitly cleared known ID.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  prefix = process.argv.includes("--prefix"),
  suffix = prefix ? "-prefix" : "",
  directory = await mkdtemp(tmpdir() + "/note-identities-source-"),
  path = new URL(
    "../tests/fixtures/note-identities" + suffix + "-source.json",
    import.meta.url,
  ),
  zip = new URL(
    "../tests/fixtures/note-identities" + suffix + "-cleared-26.10.zip",
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
  ),
  hash = (v) => createHash("sha256").update(v).digest("hex");
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
try {
  await api.init({ dataDir: directory });
  const original = await readFile(
    new URL("../tests/fixtures/" + seed.archive.filename, import.meta.url),
  );
  assert.equal(hash(original), seed.archive.sha256);
  await api.importBudget(original, { filename: "note-identities.zip" });
  const identities = prefix
      ? [
          ["nonmonth-prefix", "budget-2026-13"],
          ["arbitrary-prefix", "budget-automation"],
        ]
      : [
          ["account", seed.ids.account],
          ["category", seed.ids.food],
          ["group", seed.ids.group],
          ["month", "budget-2026-11"],
          ["standalone", "caller:cleared-note"],
        ],
    cases = [];
  const snapshot = async () => ({
    reads: await Promise.all(
      identities.map(async ([kind, id]) => ({
        kind,
        id,
        value: (await api.getNote(id)) ?? null,
      })),
    ),
    rows: (
      await api.aqlQuery(
        api
          .q("notes")
          .filter({ id: { $oneof: identities.map((x) => x[1]) } })
          .select(["id", "note"])
          .orderBy("id"),
      )
    ).data,
  });
  cases.push({ id: "initial", state: await snapshot() });
  for (const [kind, id] of identities) {
    const note = "Original " + kind + " prose";
    await api.updateNote(id, note);
    assert.deepEqual(await api.getNote(id), { id, note });
    cases.push({
      id: "write-" + kind,
      change: { id, note },
      state: await snapshot(),
    });
    await api.updateNote(id, "");
    assert.deepEqual(await api.getNote(id), { id, note: "" });
    cases.push({
      id: "clear-" + kind,
      change: { id, note: "" },
      state: await snapshot(),
    });
  }
  const bytes = check ? await readFile(zip) : await api.exportBudget();
  if (!check) await writeFile(zip, bytes, { flag: "wx" });
  const result = {
    oracle:
      "@actual-app/api26.10 getNote/updateNote and actual AQL notes table",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    input: seed.archive,
    archive: {
      filename: "note-identities" + suffix + "-cleared-26.10.zip",
      sha256: hash(bytes),
    },
    identities,
    cases,
  };
  if (check) assert.deepEqual(result, JSON.parse(await readFile(path)));
  else
    await writeFile(path, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS" +
      cases.length +
      " genuine missing/written/cleared note identity states",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

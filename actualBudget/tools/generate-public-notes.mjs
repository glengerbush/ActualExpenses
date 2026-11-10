// Independent published SDK arbitrary-ID notes; no replacement persistence.
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
  hash = (v) => createHash("sha256").update(v).digest("hex"),
  check = process.argv.includes("--check"),
  path = new URL("../tests/fixtures/public-notes-source.json", import.meta.url),
  archivePath = new URL(
    "../tests/fixtures/public-notes-base-26.10.zip",
    import.meta.url,
  ),
  directory = await mkdtemp(tmpdir() + "/public-notes-source-");
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
  const input = await readFile(
    new URL(
      "../tests/fixtures/transaction-export-base-26.10.zip",
      import.meta.url,
    ),
  );
  await api.importBudget(input, { filename: "public-notes-seed.zip" });
  const before = (
    await api.aqlQuery(
      api
        .q("transactions")
        .options({ splits: "all" })
        .select("*")
        .orderBy("id"),
    )
  ).data;
  const id = "automation:47/arbitrary-note",
    missing = await api.getNote(id);
  await api.updateNote(
    id,
    "Original prose\n# literal template text\nUnicode café 🌿",
  );
  const first = await api.getNote(id);
  const raw = check ? await readFile(archivePath) : await api.exportBudget();
  if (!check) await writeFile(archivePath, raw, { flag: "wx" });
  await api.updateNote(id, "Updated caller prose");
  const updated = await api.getNote(id);
  await api.updateNote(id, "");
  const cleared = await api.getNote(id);
  assert.deepEqual(
    (
      await api.aqlQuery(
        api
          .q("transactions")
          .options({ splits: "all" })
          .select("*")
          .orderBy("id"),
      )
    ).data,
    before,
  );
  const result = {
    oracle: "@actual-app/api26.10 getNote/updateNote + genuine original export",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    input: {
      filename: "transaction-export-base-26.10.zip",
      sha256: hash(input),
    },
    archive: { filename: "public-notes-base-26.10.zip", sha256: hash(raw) },
    id,
    missing: missing ?? null,
    first,
    updated,
    cleared,
    financial: before,
  };
  if (check) assert.deepEqual(result, JSON.parse(await readFile(path)));
  else
    await writeFile(path, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS arbitrary-ID original notes SDK/read/update/clear with unchanged financial rows",
  );
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

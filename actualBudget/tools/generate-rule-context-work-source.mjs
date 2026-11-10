// Large history created and exported through the pinned public Actual API.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
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
const directory = await mkdtemp(tmpdir() + "/rule-context-source-");
const base = new URL("../tests/fixtures/", import.meta.url);
const count = 60000;
try {
  await api.init({ dataDir: directory });
  let account;
  const check = process.argv.includes("--check");
  if (check) {
    const fixture = JSON.parse(
      await readFile(new URL("rule-context-work-source.json", base)),
    );
    const archive = await readFile(new URL(fixture.archive.filename, base));
    assert.equal(
      createHash("sha256").update(archive).digest("hex"),
      fixture.archive.sha256,
    );
    await api.importBudget(archive, { filename: fixture.archive.filename });
    account = fixture.account;
    assert.equal(fixture.count, count);
    const rows = (
      await api.aqlQuery(
        api.q("transactions").filter({ account }).select(["id"]),
      )
    ).data;
    assert.equal(rows.length, count);
  } else
    await api.runImport("Calculated context work fixture", async () => {
      account = await api.createAccount({ name: "History" }, 0);
      for (let start = 0; start < count; start += 5000) {
        await api.addTransactions(
          account,
          Array.from({ length: 5000 }, (_, i) => ({
            id:
              "42420000-0000-4000-8000-" +
              String(start + i + 1).padStart(12, "0"),
            amount: -1,
            date: "2026-01-01",
            notes: "History",
            cleared: true,
          })),
        );
      }
    });
  const total = (
    await api.aqlQuery(
      api.q("transactions").filter({ account }).calculate({ $sum: "$amount" }),
    )
  ).data;
  assert.equal(total, -count);
  if (!check) {
    const archive = await api.exportBudget();
    await writeFile(
      new URL("rule-context-work-source-26.10.zip", base),
      archive,
    );
    await writeFile(
      new URL("rule-context-work-source.json", base),
      JSON.stringify(
        {
          oracle: "@actual-app/api@26.10.0",
          count,
          account,
          total,
          archive: {
            filename: "rule-context-work-source-26.10.zip",
            sha256: createHash("sha256").update(archive).digest("hex"),
          },
        },
        null,
        2,
      ) + "\n",
    );
  }
  console.log(
    `${check ? "Verified" : "Exported"} ${count} genuine source transactions; total ${total}`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

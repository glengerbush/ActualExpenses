// Independent Actual26.10 observation for the custom-report merge regression.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { sourceRuntime } from "./reports-source-runtime.mjs";
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
const root = new URL("../tests/fixtures/", import.meta.url);
const source = JSON.parse(
  await readFile(new URL("custom-reports-source-26.10.json", root)),
);
const bytes = await readFile(new URL("reports-source-26.10.zip", root));
const original = JSON.parse(
  await readFile(new URL("reports-source-26.10.json", root)),
);
const sha256 = createHash("sha256").update(bytes).digest("hex");
assert.equal(sha256, original.archive_sha256);
const options = source.cases.find(
  (c) =>
    c.options.balanceTypeOp === "totalDebts" &&
    c.options.groupBy === "Category" &&
    c.options.showOffBudget === false &&
    c.options.showHiddenCategories === true,
).options;
const food = source.categories.list.find((c) => c.name === "food").id;
const replacement = source.categories.list.find((c) => c.name === "empty").id;
const directory = await mkdtemp(tmpdir() + "/custom-retirement-source-");
try {
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(bytes, { filename: "custom-retirement-source.zip" });
  async function total(category) {
    const runtime = sourceRuntime(api, engine);
    let result;
    await runtime
      .load("spreadsheets/custom-spreadsheet")
      .createCustomSpreadsheet({
        ...options,
        conditions: [{ field: "category", op: "is", value: category }],
        categories: await engine.send("get-categories"),
        accounts: await api.getAccounts(),
        payees: await api.getPayees(),
      })({}, (value) => {
      result = JSON.parse(JSON.stringify(value));
    });
    assert.ok(result);
    return result.totalDebts;
  }
  const before = await total(food);
  await api.deleteCategory(food, replacement);
  const result = {
    source: "Actual26.10.0 unchanged custom spreadsheet and deleteCategory",
    archive_sha256: sha256,
    food,
    replacement,
    before,
    retired: await total(food),
    target: await total(replacement),
  };
  assert.equal(result.before, -6000);
  assert.equal(result.retired, 0);
  assert.equal(result.target, -6000);
  const file = new URL("custom-report-retirement-source-26.10.json", root);
  if (process.argv.includes("--check"))
    assert.deepEqual(result, JSON.parse(await readFile(file)));
  else await writeFile(file, JSON.stringify(result, null, 2) + "\n");
  console.log(
    "PASS exact custom-report source: old category -6000, retired literal 0, replacement -6000",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

// Concrete source edge seams and independent ledger membership over frozen data.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { sourceRuntime } from "./reports-source-runtime.mjs";
const api = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  )("@actual-app/api"),
  fixtures = new URL("../tests/fixtures/", import.meta.url),
  directory = await mkdtemp(tmpdir() + "/sankey-details-"),
  path = new URL("sankey-details-source-26.10.json", fixtures),
  plain = (value) => JSON.parse(JSON.stringify(value));
try {
  const engine = await api.init({ dataDir: directory }),
    source = JSON.parse(
      await readFile(
        new URL("sankey-envelope-source-26.10.json", fixtures),
        "utf8",
      ),
    ),
    bytes = await readFile(
      new URL("sankey-envelope-source-26.10.zip", fixtures),
    );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    source.archive_sha256,
  );
  await api.importBudget(bytes, { filename: "sankey-details-source.zip" });
  const runtime = sourceRuntime(api, engine, {
      translate: (key, params = {}) =>
        key.replace(
          /{{(\w+)}}/g,
          (_, name) => params[name] ?? "{{" + name + "}}",
        ),
      theme: {
        toBudgetPositive: "positive",
        toBudgetNegative: "negative",
        reportsGray: "neutral",
        reportsBlue: "flow",
      },
      colors: ["palette0", "palette1", "palette2"],
    }),
    projection = runtime.load("spreadsheets/sankey-spreadsheet"),
    categories = (await engine.send("get-categories")).grouped,
    cases = [];
  for (const [name, extra] of [
    [
      "empty-named-condition",
      {
        conditions: [
          {
            field: "category",
            op: "is",
            value: source.ids.food,
            customName: "",
          },
        ],
      },
    ],
    ["spent-end-account", { layerTo: "account", showTransfers: true }],
    [
      "budgeted-end-group",
      {
        mode: "budgeted",
        layerFrom: "income_category",
        layerTo: "category_group",
      },
    ],
  ]) {
    const options = { ...source.cases[0].options, ...extra };
    let graph;
    await projection.createBaseGraphSpreadsheet(
      options.startDate.slice(0, 7),
      options.endDate.slice(0, 7),
      categories,
      options.conditions,
      options.conditionsOp,
      options.mode,
      options.groupAccounts,
      options.showTransfers,
    )({}, (value) => (graph = value));
    cases.push({
      name,
      options,
      baseGraph: plain(
        [...graph].map(([key, node]) => ({ key, ...node, to: [...node.to] })),
      ),
      result: plain(
        projection.buildSankeyData(
          graph,
          options.topNcategories,
          categories,
          options.categorySort,
          options.layerFrom,
          options.layerTo,
        ),
      ),
    });
  }
  const query = api
      .q("transactions")
      .filter({ date: { $gte: "2026-10-01" } })
      .filter({ date: { $lte: "2026-12-31" } })
      .filter({ is_parent: false })
      .options({ splits: "inline" })
      .orderBy("id")
      .select([
        "id",
        "date",
        "amount",
        "account",
        "category",
        "payee",
        "transfer_id",
        "notes",
        { account_name: { $id: "$account.name" } },
        { category_name: { $id: "$category.name" } },
        { payee_name: { $id: "$payee.name" } },
      ]),
    ledger = (await api.aqlQuery(query)).data,
    groups = await projection.createTransactionsSpreadsheet(
      "2026-11",
      "2026-11",
      categories,
      [],
      "and",
      false,
      true,
    )();
  const output = plain({
    oracle:
      "Unchanged full Sankey engine/projection for falsy-name and ending-layer seams; real AQL leaf membership and source grouping/transfer functions",
    archive_sha256: source.archive_sha256,
    source_files: runtime.hashes,
    cases,
    ledger_query: query.serialize(),
    ledger,
    entry_groups: groups.data,
    transfer_pairs: groups.transferData,
  });
  if (process.argv.includes("--check"))
    assert.deepEqual(output, JSON.parse(await readFile(path, "utf8")));
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS3 concrete whole Sankey edge cases, " +
      ledger.length +
      " genuine ledger rows and source entry/transfer groups",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

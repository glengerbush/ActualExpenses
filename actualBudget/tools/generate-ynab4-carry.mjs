// Synthetic YNAB4 source inputs, financial expectations executed by pinned importer.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
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
  api = require("@actual-app/api"),
  check = process.argv.includes("--check"),
  directory = await mkdtemp(tmpdir() + "/ynab4-carry-"),
  path = new URL("../tests/fixtures/ynab4-carry.json", import.meta.url),
  hash = (b) => createHash("sha256").update(b).digest("hex");
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
const records = [],
  original = await readFile(
    new URL("../tests/fixtures/ynab/ynab4-demo.zip", import.meta.url),
  );
try {
  await api.init({ dataDir: directory });
  for (const mode of ["gap-reset", "carry-after-last"]) {
    const input = execFileSync(
        "python3",
        [
          "-c",
          `import zipfile,io,json,sys
original=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); out=io.BytesIO()
with zipfile.ZipFile(out,'w') as result:
 for info in original.infolist():
  data=original.read(info)
  if info.filename.endswith('Budget.yfull'):
   d=json.loads(data); expense=next(t for t in d['transactions'] if t.get('categoryId')=='A5' and t['amount']<0); expense.update(date='2023-01-15',amount=-12.34); d['transactions']=[expense]
   def month(m,flag):
    return {'entityType':'monthlyBudget','entityId':'MB/'+m,'month':m+'-01','monthlySubCategoryBudgets':[{'entityType':'monthlyCategoryBudget','entityId':'MCB/'+m+'/A5','categoryId':'A5','budgeted':0,'overspendingHandling':flag,'parentMonthlyBudgetId':'MB/'+m}]}
   d['monthlyBudgets']=[month('2023-01','Confined')]+([month('2023-03','AffectsBuffer')] if sys.argv[1]=='gap-reset' else [])
   data=json.dumps(d,separators=(',',':')).encode()
  result.writestr(info,data)
sys.stdout.buffer.write(out.getvalue())`,
          mode,
        ],
        { input: original, maxBuffer: 2 * 1024 * 1024 },
      ),
      inputFile = `ynab4-carry-${mode}.zip`,
      actualFile = `ynab4-carry-${mode}-actual-26.10.zip`,
      base = new URL("../tests/fixtures/ynab/", import.meta.url);
    if (!check)
      await writeFile(new URL(inputFile, base), input, { flag: "wx" });
    assert.deepEqual(await readFile(new URL(inputFile, base)), input);
    await api.importBudget(input, { type: "ynab4", filename: inputFile });
    const categories = await api.getCategories(),
      groups = await api.getCategoryGroups(),
      months = [];
    for (const month of [
      "2023-01",
      "2023-02",
      "2023-03",
      "2023-04",
      "2023-05",
      "2023-06",
    ]) {
      const view = await api.getBudgetMonth(month),
        projected = [];
      for (const group of view.categoryGroups)
        for (const c of group.categories)
          projected.push({
            category: group.name + "/" + c.name,
            budgeted: c.budgeted ?? 0,
            actual: c.received ?? c.spent ?? 0,
            balance: c.balance ?? 0,
            carryover: c.carryover ?? false,
          });
      months.push({
        month,
        categories: projected,
        summary: Object.fromEntries(
          [
            "incomeAvailable",
            "lastMonthOverspent",
            "forNextMonth",
            "totalBudgeted",
            "toBudget",
            "fromLastMonth",
            "totalIncome",
            "totalSpent",
            "totalBalance",
          ].map((k) => [k, view[k]]),
        ),
      });
    }
    const category = categories.find((c) => c.name === "Tithing");
    assert.ok(category);
    assert.equal(groups.find((g) => g.id === category.group_id).name, "Giving");
    const actual = check
      ? await readFile(new URL(actualFile, base))
      : await api.exportBudget();
    if (!check)
      await writeFile(new URL(actualFile, base), actual, { flag: "wx" });
    records.push({
      id: mode,
      input: inputFile,
      input_sha256: hash(input),
      actual: actualFile,
      actual_sha256: hash(actual),
      months,
    });
  }
  const output = {
    oracle: "@actual-app/api@26.10.0",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    provenance:
      "Synthetic source ZIPs derive unchanged demo structure with one -12.34 A5 expense and genuine-format monthly carry settings. Expectations and retained Actual ZIPs come only from real public pinned importBudget/getBudgetMonth. No source SQLite/HLC fabrication.",
    cases: records,
  };
  if (check)
    assert.deepEqual(
      JSON.parse(JSON.stringify(output)),
      JSON.parse(await readFile(path)),
    );
  else
    await writeFile(path, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log("PASS real YNAB4 gap/reset and beyond-last carry history");
} finally {
  await api.shutdown().catch(() => {});
  await rm(directory, { recursive: true, force: true });
}

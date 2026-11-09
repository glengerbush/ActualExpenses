// Execute the pinned UI callback unchanged; no replacement arithmetic in the oracle.
import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import vm from "node:vm";
import ts from "typescript";
import assert from "node:assert/strict";
const source = await readFile(
  new URL(
    "../../reference/actual-26.10.0/packages/desktop-client/src/components/transactions/TransactionsTable.tsx",
    import.meta.url,
  ),
  "utf8",
);
const start = source.indexOf("    const onDistributeRemainder = useCallback(");
const end = source.indexOf("\n\n    function onCloseAddTransaction", start);
assert(start >= 0 && end > start);
const callback = source.slice(start, end);
const code = ts.transpileModule(
  callback + "\nglobalThis.run=onDistributeRemainder;",
  { compilerOptions: { target: ts.ScriptTarget.ES2022 } },
).outputText;
const cases = [];
for (const [total, amounts] of [
  [-5001, [-2000, 0, 0]],
  [-4000, [-2000, -1000]],
  [4000, [2000, 1000]],
  [-1, [0, 0, 0]],
  [1, [0, 0, 0]],
  [-101, [-100, 20]],
  [-5001, [-2000, 0]],
]) {
  const children = amounts.map((amount, i) => ({
    id: `child${i}`,
    parent_id: "parent",
    amount,
  }));
  const rows = [{ id: "parent", is_parent: true, amount: total }, ...children];
  const changes = [];
  const context = vm.createContext({
    useCallback: (fn) => fn,
    isTemporaryId: () => false,
    latestState: {
      current: {
        transactions: rows,
        newTransactions: [],
        newNavigator: { onEdit() {} },
        tableNavigator: { onEdit() {} },
      },
    },
    onSave: async (row) => changes.push(row),
    console,
  });
  vm.runInContext(code, context);
  await context.run("parent");
  cases.push({
    total,
    amounts,
    expected: children.map(
      (c) => changes.find((row) => row.id === c.id)?.amount ?? c.amount,
    ),
  });
}
const result = {
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  callback_sha256: createHash("sha256").update(callback).digest("hex"),
  cases,
};
const target = new URL(
  "../../tests/fixtures/split-allocation-26.10.0.json",
  import.meta.url,
);
if (process.argv.includes("--check"))
  assert.deepEqual(JSON.parse(await readFile(target, "utf8")), result);
else await writeFile(target, JSON.stringify(result, null, 2) + "\n");
console.log("PASS pinned Actual split allocation callback");

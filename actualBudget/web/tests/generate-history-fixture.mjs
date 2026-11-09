// Execute pinned undo.ts. Only I/O and mutator-context adapters are substituted.
import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import vm from "node:vm";
import ts from "typescript";
import assert from "node:assert/strict";
const source = await readFile(
  new URL(
    "../../reference/actual-26.10.0/packages/loot-core/src/server/undo.ts",
    import.meta.url,
  ),
  "utf8",
);
const code = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;
let contextState = {},
  rows = { entry: { amount: 0, tombstone: 0 } },
  tick = 0;
const io = {
  sendMessages: async (messages) => {
    for (const m of messages) {
      rows[m.row] ??= {};
      rows[m.row][m.column] = m.value;
    }
  },
};
const exports = {};
const sandbox = vm.createContext({
  exports,
  require: (name) => {
    if (name === "@actual-app/crdt")
      return { Timestamp: { send: () => String(++tick) } };
    if (name.includes("connection")) return { send() {} };
    if (name.includes("util"))
      return {
        getIn: (object, path) =>
          path.reduce((value, key) => value?.[key], object),
      };
    if (name === "./mutators")
      return {
        getMutatorContext: () => contextState,
        withMutatorContext: async (next, fn) => {
          const old = contextState;
          contextState = { ...old, ...next };
          try {
            return await fn();
          } finally {
            contextState = old;
          }
        },
      };
    if (name === "./sync") return io;
    throw new Error(name);
  },
});
vm.runInContext(code, sandbox);
async function edit(amount) {
  await exports.withUndo(async () => {
    const oldData = { transactions: structuredClone(rows) };
    const messages = [
      {
        dataset: "transactions",
        row: "entry",
        column: "amount",
        value: amount,
      },
    ];
    await io.sendMessages(messages);
    exports.appendMessages(messages, oldData);
  });
}
await edit(-1234);
await edit(-5678);
await exports.undo();
const firstUndo = rows.entry.amount;
await exports.undo();
const secondUndo = rows.entry.amount;
await exports.redo();
const firstRedo = rows.entry.amount;
await exports.redo();
const secondRedo = rows.entry.amount;
exports.clearUndo();
rows.entry.amount = 0;
for (let i = 1; i <= 25; i++) await edit(i);
for (let i = 0; i < 20; i++) await exports.undo();
const twentyUndo = rows.entry.amount;
await exports.undo();
const exhaustedUndo = rows.entry.amount;
for (let i = 0; i < 20; i++) await exports.redo();
const twentyRedo = rows.entry.amount;
await exports.undo();
await edit(900);
await exports.redo();
const branchRedo = rows.entry.amount;
const utility = await readFile(
  new URL(
    "../../reference/actual-26.10.0/packages/loot-core/src/shared/util.ts",
    import.meta.url,
  ),
  "utf8",
);
const findReplaceSource = utility.slice(
  utility.indexOf("export function applyFindReplace("),
);
const findReplaceCode = ts.transpileModule(findReplaceSource, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;
const utilityExports = {};
vm.runInNewContext(findReplaceCode, { exports: utilityExports });
const noteCases = [
  ["Shop 12 / Shop 34", "Shop", "Market", false],
  ["Shop 12 / Shop 34", "(\\d+)", "[$1]", true],
  ["a1 b2", "(?<letter>[ab])(\\d)", "$<letter>:$2:$$:$&", true],
  ["abc", "b", "$`-$&-$'", false],
  ["😀", ".", "x", true],
  ["ab", "(?=.)", "_", true],
  ["abc", "", "ignored", false],
  ["a1", "(a)", "$01-$10-$0", true],
].map(([text, find, replace, use_regex]) => ({
  text,
  find,
  replace,
  use_regex,
  expected: utilityExports.applyFindReplace(text, find, replace, use_regex),
}));
const result = {
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
  undo_sha256: createHash("sha256").update(source).digest("hex"),
  noteCases,
  ordinary: { firstUndo, secondUndo, firstRedo, secondRedo },
  retention: { twentyUndo, exhaustedUndo, twentyRedo, branchRedo },
};
const target = new URL(
  "../../tests/fixtures/history-26.10.0.json",
  import.meta.url,
);
if (process.argv.includes("--check"))
  assert.deepEqual(JSON.parse(await readFile(target, "utf8")), result);
else await writeFile(target, JSON.stringify(result, null, 2) + "\n");
console.log("PASS pinned Actual undo group/retention/redo branch oracle");

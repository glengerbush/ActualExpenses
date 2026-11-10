// Qualify the trusted asset before Rust integration. The public API tracer remains
// the acceptance seam; this check does not substitute for transaction workflows.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import vm from "node:vm";
const root = new URL("../", import.meta.url),
  fixture = JSON.parse(
    await readFile(
      new URL("tests/fixtures/rule-expressions-source.json", root),
    ),
  );
const script = await readFile(
    new URL("server/src/rules/vendor/rule-actions.js", root),
    "utf8",
  ),
  manifest = JSON.parse(
    await readFile(new URL("server/src/rules/vendor/manifest.json", root)),
  );
assert.equal(
  createHash("sha256").update(script).digest("hex"),
  manifest.asset_sha256,
);
const hf = await readFile(
    new URL("server/src/formulas/vendor/hyperformula-3.5.0.min.js", root),
    "utf8",
  ),
  language = await readFile(
    new URL("server/src/formulas/vendor/enUS-3.5.0.json", root),
    "utf8",
  );
const names = [
  "template unescaped text",
  "template numeric prefix",
  "template invalid number becomes zero",
  "template true boolean",
  "template uppercase boolean",
  "template valid date",
  "template invalid date marker",
  "template malformed",
  "template missing helper",
  "template missing property",
  "template prototype denied",
  "formula amount units",
  "formula numeric notes conversion",
  "formula string numeric prefix",
  "formula string invalid number",
  "formula boolean",
  "formula boolean text case",
  "formula date string",
  "formula date invalid",
  "formula missing equals",
  "formula unknown name",
  "formula divide zero",
  "formula wins over template",
  "formula id field unchanged",
  "formula formatting",
  "ordered mixed actions",
  "formula failure keeps later actions",
  "disabled flags retain existing formulas",
];
function relevant(row) {
  return Object.fromEntries(
    ["amount", "notes", "date", "cleared", "_ruleErrors"].map((k) => [
      k,
      row[k] ?? null,
    ]),
  );
}
for (const test of fixture.cases.filter((c) => names.includes(c.name))) {
  const context = vm.createContext({});
  vm.runInContext(
    "var self=globalThis;" +
      hf +
      ";var ActualRuleLanguage=" +
      language +
      ";" +
      script,
    context,
    { timeout: 2000 },
  );
  const result = JSON.parse(
    vm.runInContext(
      `(()=>{const row=${JSON.stringify(test.transaction)};try{const actions=${JSON.stringify(test.actions)}.map(a=>new ActualRuleActions.Action(a.op,a.field,a.value,a.options));for(const a of actions)a.exec(row);return JSON.stringify({row});}catch(e){return JSON.stringify({error:e.message,type:e.type});}})()`,
      context,
      { timeout: 2000 },
    ),
  );
  if (test.error)
    assert.equal(result.type, test.error.actionErrors[0], test.name);
  else {
    assert.ok(!result.error, test.name + ": " + result.error);
    assert.deepEqual(relevant(result.row), relevant(test.output), test.name);
  }
  assert.equal(
    vm.runInContext(
      "typeof process + ':' + typeof require + ':' + typeof fetch",
      context,
    ),
    "undefined:undefined:undefined",
  );
}
console.log(
  `PASS ${names.length} trusted asset comparisons against source Action outputs/errors, without host process/require/fetch`,
);

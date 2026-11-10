// Actual's running engine owns rule parsing, preparation, ordering and actions.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
process.env.TZ = "UTC";
const NativeDate = Date,
  clock = "2026-11-09T12:00:00Z";
globalThis.Date = class extends NativeDate {
  constructor(...args) {
    super(...(args.length ? args : [clock]));
  }
  static now() {
    return NativeDate.parse(clock);
  }
};
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api"),
  root = new URL("../tests/fixtures/", import.meta.url),
  check = process.argv.includes("--check");
const seedMeta = JSON.parse(
    await readFile(new URL("reports-source-26.10.json", root)),
  ),
  seed = await readFile(new URL("reports-source-26.10.zip", root));
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex"),
  id = (n) => `42420000-0000-4000-8000-${String(n).padStart(12, "0")}`;
assert.equal(sha(seed), seedMeta.archive_sha256);
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
const ids = seedMeta.ids,
  base = {
    id: id(1),
    account: ids.checking,
    payee: ids.market,
    category: ids.food,
    date: "2026-11-09",
    amount: -1234,
    notes: "expression café <x>",
    cleared: false,
    imported_payee: "Corner Store",
    sort_order: null,
  };
const set = (
  field,
  options,
  value = field === "amount"
    ? 0
    : field === "cleared"
      ? false
      : field === "date"
        ? "2026-11-09"
        : "",
) => ({ op: "set", field, value, options });
const formula = (field, text, value) => set(field, { formula: text }, value),
  template = (field, text, value) => set(field, { template: text }, value);
const definitions = [
  {
    name: "template transaction fields",
    actions: [
      template(
        "notes",
        "{{payee_name}}|{{amount}}|{{date}}|{{imported_payee}}|{{cleared}}|{{today}}",
      ),
    ],
  },
  {
    name: "template unescaped text",
    actions: [template("notes", "{{notes}} / {{{imported_payee}}}")],
  },
  {
    name: "template helpers",
    actions: [
      template(
        "notes",
        "{{#if cleared}}yes{{else}}no{{/if}}|{{#with _account}}{{name}}{{/with}}|{{lookup this 'imported_payee'}}",
      ),
    ],
  },
  {
    name: "template numeric prefix",
    actions: [template("amount", "-12.5 pounds")],
  },
  {
    name: "template invalid number becomes zero",
    actions: [template("amount", "not a number")],
  },
  { name: "template true boolean", actions: [template("cleared", "true")] },
  {
    name: "template uppercase boolean",
    actions: [template("cleared", "TRUE")],
  },
  { name: "template valid date", actions: [template("date", "2026-12-25")] },
  {
    name: "template invalid date marker",
    actions: [template("date", "bad date")],
  },
  {
    name: "template malformed",
    actions: [template("notes", "{{#if amount}}unterminated")],
  },
  {
    name: "template missing helper",
    actions: [template("notes", "{{missingHelper notes}}")],
  },
  {
    name: "template missing property",
    actions: [template("notes", "before {{missing}} after")],
  },
  {
    name: "template prototype denied",
    actions: [
      template(
        "notes",
        "{{__proto__}}|{{constructor}}|{{lookup this 'constructor'}}",
      ),
    ],
  },
  {
    name: "formula amount units",
    actions: [formula("amount", "=amount/100+1")],
  },
  {
    name: "formula numeric notes conversion",
    actions: [formula("notes", "=SUM(1,2.345)")],
  },
  {
    name: "formula string numeric prefix",
    actions: [formula("amount", '="12.5 pounds"')],
  },
  {
    name: "formula string invalid number",
    actions: [formula("amount", '="not a number"')],
  },
  { name: "formula boolean", actions: [formula("cleared", "=TRUE()")] },
  {
    name: "formula boolean text case",
    actions: [formula("cleared", '="TRUE"')],
  },
  { name: "formula date string", actions: [formula("date", '="2026-12-25"')] },
  { name: "formula date invalid", actions: [formula("date", '="bad date"')] },
  {
    name: "formula context aliases",
    actions: [
      formula(
        "notes",
        '=CONCATENATE(account_name,"|",category_name,"|",payee_name,"|",imported_payee)',
      ),
    ],
  },
  { name: "formula missing equals", actions: [formula("notes", "SUM(1,2)")] },
  {
    name: "formula unknown name",
    actions: [formula("notes", "=UNKNOWN_FIELD+1")],
  },
  { name: "formula divide zero", actions: [formula("notes", "=1/0")] },
  {
    name: "formula wins over template",
    actions: [set("notes", { formula: '="formula"', template: "template" })],
  },
  {
    name: "formula id field unchanged",
    actions: [formula("category", '="different"', ids.food)],
  },
  {
    name: "formula formatting",
    actions: [formula("notes", "=FORMATCURRENCY(amount/100)")],
  },
  {
    name: "balance template",
    actions: [template("notes", "balance={{balance}} / amount={{amount}}")],
  },
  { name: "balance variable", actions: [formula("notes", "=balance/100")] },
  {
    name: "balance literal account",
    actions: [formula("notes", `=BALANCE_OF("${ids.savings}")`)],
  },
  {
    name: "balance missing account",
    actions: [formula("notes", '=BALANCE_OF("missing account")')],
  },
  {
    name: "ordered mixed actions",
    actions: [
      template("notes", "first {{notes}}"),
      formula("notes", '=CONCATENATE(notes," / second")'),
      { op: "append-notes", value: " / final" },
    ],
  },
  {
    name: "formula payee name",
    actions: [formula("payee_name", '="Expression merchant"')],
  },
  {
    name: "template payee name",
    actions: [template("payee_name", "Template merchant {{amount}}")],
  },
  {
    name: "formula failure keeps later actions",
    actions: [
      formula("notes", "=1/0"),
      { op: "append-notes", value: " / still runs" },
    ],
  },
  {
    name: "disabled flags retain existing formulas",
    disabled: true,
    actions: [formula("notes", '="still runs"')],
  },
  {
    name: "split parent context",
    actions: [
      {
        op: "set-split-amount",
        value: null,
        options: {
          splitIndex: 1,
          method: "formula",
          formula: "=parent_amount/200",
        },
      },
      set("notes", {
        splitIndex: 1,
        formula:
          '=CONCATENATE(parent_notes,"|",parent_imported_payee,"|",parent_account)',
      }),
      {
        op: "set-split-amount",
        value: null,
        options: { splitIndex: 2, method: "remainder" },
      },
    ],
  },
  {
    name: "split parent text stays literal",
    changes: { notes: "=1+2", imported_payee: "123.50" },
    actions: [
      {
        op: "set-split-amount",
        value: null,
        options: {
          splitIndex: 1,
          method: "formula",
          formula: "=parent_amount/200",
        },
      },
      set("notes", {
        splitIndex: 1,
        formula: '=CONCATENATE(parent_notes,"|",parent_imported_payee)',
      }),
      {
        op: "set-split-amount",
        value: null,
        options: { splitIndex: 2, method: "remainder" },
      },
    ],
  },
  {
    name: "split invalid formula keeps remainder",
    actions: [
      {
        op: "set-split-amount",
        value: null,
        options: { splitIndex: 1, method: "formula", formula: "=1/0" },
      },
      {
        op: "set-split-amount",
        value: null,
        options: { splitIndex: 2, method: "remainder" },
      },
    ],
  },
];
const directory = await mkdtemp(tmpdir() + "/rule-expressions-");
try {
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(seed, { filename: "reports-source-26.10.zip" });
  for (const rule of await api.getRules()) await api.deleteRule(rule.id);
  const ledger = async () =>
    (
      await api.aqlQuery(
        api
          .q("transactions")
          .options({ splits: "all" })
          .select("*")
          .orderBy("id"),
      )
    ).data;
  const before = await ledger(),
    cases = [];
  const contextCases = process.argv.includes("--context");
  const chainCases = process.argv.includes("--chains");
  const executionCases = process.argv.includes("--execution");
  if (contextCases) {
    definitions.length = 0;
    const rows = before.filter(
      (r) => r.account === ids.checking && !r.is_parent && !r.is_child,
    );
    assert.ok(rows.length);
    for (const [i, row] of rows.entries())
      definitions.push({
        name: `existing-row running balance ${i}`,
        changes: row,
        actions: [formula("notes", "=balance/100")],
      });
    for (const order of [null, -1, 0, 0.5, 1, 100000])
      definitions.push({
        name: `same-day boundary ${order}`,
        changes: { date: rows[0].date, sort_order: order },
        actions: [formula("notes", "=balance/100")],
      });
    definitions.push(
      {
        name: "later action reads calculated payee name",
        actions: [
          template("payee_name", "Merchant {{amount}}"),
          formula("notes", '=CONCATENATE(payee_name,"|",payee)'),
        ],
      },
      {
        name: "original date survives action changes",
        changes: rows[0],
        actions: [
          { op: "set", field: "date", value: "2099-01-01" },
          formula("notes", "=balance/100"),
        ],
      },
      {
        name: "original account survives action changes",
        actions: [
          { op: "set", field: "account", value: ids.savings },
          formula("notes", '=CONCATENATE(account_name,"|",balance/100)'),
        ],
      },
      {
        name: "original category alias survives action changes",
        actions: [
          { op: "set", field: "category", value: null },
          formula("notes", '=CONCATENATE(category_name,"|",category_group)'),
        ],
      },
      {
        name: "split original payee context",
        actions: [
          {
            op: "set-split-amount",
            value: null,
            options: {
              splitIndex: 1,
              method: "formula",
              formula: "=parent_amount/200",
            },
          },
          set("notes", { splitIndex: 1, formula: "=parent_payee" }),
          {
            op: "set-split-amount",
            value: null,
            options: { splitIndex: 2, method: "remainder" },
          },
        ],
      },
    );
  }
  if (chainCases) {
    definitions.length = 0;
    const rows = before.filter(
      (r) => r.account === ids.checking && !r.is_parent && !r.is_child,
    );
    for (const [i, row] of rows.entries())
      definitions.push({
        name: `stored variables ${i}`,
        stored: true,
        changes: row,
        actions: [
          formula(
            "notes",
            '=CONCATENATE(sort_order,"|",is_parent,"|",is_child,"|",transfer_id)',
          ),
        ],
      });
    definitions.push(
      {
        name: "balance prepared before the first action",
        changes: { imported_payee: "balance" },
        actions: [
          template("notes", "{{lookup this imported_payee}}"),
          formula("cleared", "=balance>0"),
        ],
      },
      {
        name: "candidate literal prefetched before dynamic lookup",
        actions: [formula("notes", "=BALANCE_OF(account_name)")],
        following: [
          {
            conditions: [{ field: "amount", op: "is", value: 999999 }],
            actions: [formula("notes", '=BALANCE_OF("checking")')],
          },
        ],
      },
      {
        name: "noncandidate literal is not prefetched",
        actions: [formula("notes", "=BALANCE_OF(account_name)")],
        following: [
          {
            conditions: [
              { field: "imported_payee", op: "is", value: "Z elsewhere" },
              { field: "payee", op: "is", value: id(9999) },
            ],
            actions: [formula("notes", '=BALANCE_OF("checking")')],
          },
        ],
      },
      {
        name: "category group refreshed between rules",
        actions: [{ op: "set", field: "category", value: null }],
        following: [
          {
            actions: [
              formula(
                "notes",
                '=CONCATENATE(category_name,"|",IFERROR(category_group,"none"))',
              ),
            ],
          },
        ],
      },
      {
        name: "account alias preserved between rules",
        actions: [{ op: "set", field: "account", value: ids.savings }],
        following: [
          {
            conditions: [{ field: "account", op: "is", value: ids.savings }],
            actions: [
              formula("notes", '=CONCATENATE(account_name,"|",balance/100)'),
            ],
          },
        ],
      },
      ...[
        ["wildcard first-character bucket", "is", "* elsewhere"],
        ["empty string membership is discarded", "oneOf", ["", "Z elsewhere"]],
        ["normalized Unicode condition index", "is", "İnput"],
      ].map(([name, op, value]) => ({
        name,
        ...(name === "normalized Unicode condition index"
          ? { changes: { imported_payee: "İnput" } }
          : {}),
        actions: [formula("notes", "=BALANCE_OF(account_name)")],
        following: [
          {
            conditions: [
              { field: "imported_payee", op, value },
              { field: "payee", op: "is", value: id(9999) },
              { field: "amount", op: "is", value: 999999 },
            ],
            actions: [formula("notes", '=BALANCE_OF("checking")')],
          },
        ],
      })),
    );
  }
  if (executionCases) {
    definitions.length = 0;
    const excluded = [
      { field: "imported_payee", op: "isNot", value: "Z elsewhere" },
      { field: "payee", op: "isNot", value: ids.employer },
    ];
    definitions.push(
      {
        name: "live negative conditions excluded by both indexes",
        conditions: excluded,
        actions: [formula("notes", '="must not run"')],
      },
      {
        name: "live index union retains wildcard candidate",
        conditions: [excluded[0]],
        actions: [formula("notes", '="does run"')],
      },
      {
        name: "live candidates fixed before payee mutation",
        actions: [{ op: "set", field: "payee", value: ids.employer }],
        following: [
          {
            conditions: [
              excluded[0],
              { field: "payee", op: "is", value: ids.employer },
            ],
            actions: [formula("notes", '="must not run"')],
          },
        ],
      },
    );
    const split = (index, method, formula, value = null) => ({
      op: "set-split-amount",
      value,
      options: { splitIndex: index, method, ...(formula ? { formula } : {}) },
    });
    for (const text of ["-12.5 pounds", "-2000.5 pounds", "0.125"])
      definitions.push({
        name: `fractional split ${text}`,
        actions: [
          split(1, "formula", `=${JSON.stringify(text)}`),
          split(2, "remainder"),
        ],
      });
    definitions.push(
      {
        name: "fractional split with percentage and remainder",
        actions: [
          split(1, "formula", '="-12.5 pounds"'),
          split(2, "fixed-percent", null, 50),
          split(3, "remainder"),
        ],
      },
      {
        name: "fractional parent with split formula",
        actions: [
          template("amount", "-1234.5"),
          split(1, "formula", "=parent_amount/200"),
          split(2, "remainder"),
        ],
      },
      {
        name: "generated child variables",
        actions: [
          split(1, "formula", "=parent_amount/200"),
          set("notes", {
            splitIndex: 1,
            formula:
              '=CONCATENATE(sort_order,"|",IFERROR(is_parent,"unset"),"|",is_child,"|",reconciled,"|",starting_balance_flag,"|",IFERROR(notes,"unset"))',
          }),
          split(2, "remainder"),
        ],
      },
      {
        name: "unbalanced calculated split",
        actions: [split(1, "formula", "=parent_amount/200")],
      },
      {
        name: "large fractional split rounding",
        changes: { amount: Number.MAX_SAFE_INTEGER },
        actions: [split(1, "formula", '="0.5"'), split(2, "remainder")],
      },
      {
        name: "nested balance lookup and conditional text",
        actions: [
          formula(
            "notes",
            `=CONCATENATE("Balance ", balance, IF(BALANCE_OF("Checking")=BALANCE_OF("${ids.checking}"), "", " mismatch"))`,
          ),
        ],
      },
    );
  }
  function normalized(value) {
    if (Array.isArray(value)) return value.map(normalized);
    if (!value || typeof value !== "object") return value;
    const out = {};
    for (const [k, v] of Object.entries(value)) {
      if (k === "_account" || k === "_account_name" || k === "_category_name")
        continue;
      out[k] =
        k === "subtransactions"
          ? v.map((r, i) => normalized({ ...r, id: "generated-child-" + i }))
          : normalized(v);
    }
    return out;
  }
  for (const [i, definition] of definitions.entries()) {
    for (const rule of await api.getRules()) await api.deleteRule(rule.id);
    for (const flag of ["formulaMode", "actionTemplating"])
      await engine.send("preferences/save", {
        id: "flags." + flag,
        value: String(!definition.disabled),
      });
    const transaction = { ...base, ...definition.changes };
    const rule = {
      id: id(100 + i),
      stage: definition.following ? "pre" : null,
      conditionsOp: "and",
      conditions: definition.conditions ?? [
        { field: "account", op: "is", value: ids.checking },
      ],
      actions: definition.actions,
    };
    const added = await engine.send("rule-add", rule);
    const following = [];
    for (const [j, definitionRule] of (definition.following ?? []).entries()) {
      const next = {
        id: id(10000 + i * 100 + j),
        stage: "post",
        conditionsOp: "and",
        conditions: rule.conditions,
        ...definitionRule,
      };
      assert.ok(!(await engine.send("rule-add", next)).error);
      following.push(next);
    }
    let output = null,
      error = added.error ?? null,
      payeeName = null;
    if (!error) {
      output = await engine.send("rules-run", { transaction });
      if (definition.actions.some((a) => a.field === "payee_name")) {
        payeeName =
          (await api.getPayees()).find((p) => p.id === output.payee)?.name ??
          null;
        output.payee = "generated-payee";
      }
    }
    assert.deepEqual(await ledger(), before);
    cases.push({
      ...definition,
      rule,
      transaction,
      output: normalized(output),
      error,
      payee_name: payeeName,
      ...(following.length ? { rules: [rule, ...following] } : {}),
    });
  }
  const result = {
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    seed: { filename: "reports-source-26.10.zip", sha256: sha(seed) },
    cases,
  };
  const file = new URL(
    executionCases
      ? "rule-expression-execution-source.json"
      : chainCases
        ? "rule-expression-chain-source.json"
        : contextCases
          ? "rule-expression-context-source.json"
          : "rule-expressions-source.json",
    root,
  );
  if (check) assert.deepEqual(result, JSON.parse(await readFile(file)));
  else
    await writeFile(file, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    `PASS ${cases.length} actual engine rule-expression observations; original financial rows unchanged`,
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}

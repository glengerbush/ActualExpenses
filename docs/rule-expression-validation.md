# Calculated rule actions — ticket42 in progress

Reference: Actual26.10.0, source commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Ticket41 accepted at `5e9cb42`.
The initial checkpoint established source behavior and a reproducible action
asset. The current checkpoint adds native execution and guarded writes;
UI, broader workflow qualification and whole42 acceptance remain unfinished.

## Running-source witnesses

`generate-rule-expressions-source.mjs` invokes the actual engine's `rule-add`
and `rules-run` operations on a disposable copy of the existing genuine reports
archive. Forty observations retain action definitions, inputs, outputs and
constructor errors. The original ledger stays exactly unchanged. Only generated
child/payee identities and non-persisted account helper objects are normalized;
payee names, financial values, dates, split allocation results and errors remain
source observations. The installed oracle version and original archive digest
are checked. No replacement evaluator supplies expected values.

`generate-rule-expression-commits-source.mjs` adds a genuine saved-rule archive
through normal API writes/export and reloads it for five actual
`rule-apply-actions` operations. It proves amount changes, mixed error/success
actions, split parent context and fractional-unit refusal without partial writes.
Unrelated ledger rows remain exact. Immutable artifacts:

- `actualBudget/tests/fixtures/rule-expressions-source.json`
- `actualBudget/tests/fixtures/rule-expression-commits-source.json`
- `actualBudget/tests/fixtures/rule-expressions-saved-26.10.zip`

Generation and independent checks pass in
`/tmp/rule-expressions-source-check.log` and
`/tmp/rule-expression-commits-source-check.log`. The combined reproducible command
is `npm run test:rule-expressions-source` from `actualBudget/web`.

## Semantics the implementation must preserve

- Handlebars uses unescaped text and its normal helpers. Invalid syntax/missing
  helpers can fail rule construction; missing properties render empty text.
  Prototype properties are unavailable with the source defaults.
- Template numbers use numeric-prefix parsing; invalid numbers become zero.
  Template booleans recognize exactly `true`; formulas also recognize `TRUE`.
  Invalid template dates become `9999-12-31`; invalid formula dates keep the
  prior date and accumulate an error.
- Numeric formula results convert to minor units before field conversion,
  including numeric notes. Transaction `amount` inputs already use minor units.
  Formula failures can preserve the current field while later actions still run.
  Formula options take precedence over templates.
- Preview conversions can yield fractional minor units. The real persistence
  operation refuses them (`Can't convert to integer`) and leaves the ledger
  unchanged. Native writes must refuse before integer unwraps; never silently
  round or truncate them.
- `balance` and literal `BALANCE_OF` use balances before the original transaction,
  including source same-day ordering and ID exclusion. Rule prefetch values are
  minor-unit values; do not reuse report balance prefetch, which uses a different
  context and scaling. Unknown accounts return zero.
- Preparation supplies payee/account/category names and the account object.
  Preserve original preparation timing across a rule chain. Split formulas get
  parent fields separately; parent strings resembling formulas or numbers stay
  literal. Parent/split errors and remaining allocations must retain source order.
- Existing formula actions continue executing when their experimental UI flag
  is disabled. Opt-in controls must not silently disable saved automation.

## Trusted action asset preparation

`generate-rule-actions-runtime.mjs` bundles the unchanged source `Action` class,
date helpers and custom functions. Only module/environment wiring changes:
logging is inert, the sandbox gets a `globalThis` alias for source date helpers,
and the exact `assert` declaration is extracted from `rule-utils.ts` with its
original `RuleError`. This excludes unrelated scheduling initialization.
Source hashes, oracle binary hash, adapter hash, dependency versions and output
hashes are recorded in `server/src/rules/vendor/manifest.json`.

The actual oracle bundles Handlebars4.7.9; tooling had resolved4.7.10. The direct
tooling dependency and lockfile now pin4.7.9. Generation checks it against the
oracle's embedded version, along with HyperFormula3.5.0. The browser compiler
bundle needs no Node loader. Its MIT license is retained beside the asset.
The runtime Dockerfile now includes this license beside the formula licenses.

`check-rule-actions-bundle.mjs` compares28 applicable action/conversion/error
cases against the independent engine observations in a fresh JavaScript VM
without process/require/fetch. It first caught the missing `global` alias;
the corrected asset passes. This is asset qualification, **not** proof of the
Rust sandbox or complete rule integration. Final asset reproduction, all45
source cases and28 asset comparisons pass together in
`/tmp/rule-expressions-source-suite.log`; formatting passes in
`/tmp/rule-expressions-style.log`.

## Native seam and remaining work

`check-rule-expressions-http.mjs` exercises the existing public rule-preview
endpoint. The initial template case failed with422 `rule_requirements`,
recorded in `/tmp/rule-expressions-http-red.log`; native execution now passes it.
The checker deliberately compares source conversions, errors and unchanged
financial state. Its one non-financial `today` template adapts the captured date
to the server's current UTC date; explicit transaction-date expectations remain
unchanged.

## Native execution checkpoint

The pinned Action bundle now runs inside the existing host-isolated QuickJS
runtime. Report and rule modes compile/cache separate trusted plugins; they
share preference formatting initialization without overwriting one another's
plugin state. Each invocation retains the64MiB heap,512KiB stack and2-second
interrupt, with256KiB input and64KiB output limits. Rule commands additionally
account for10seconds of cumulative engine time. End-to-end limit/load
qualification is still required; these are implementation bounds, not a
whole-system resource acceptance claim.

Native rule preparation maps original account/category/payee context, running
balances and literal account lookups into source variables. Saved imported
transactions recover original fractional/null ordering; hypothetical preview
inputs keep their supplied ordering. Category groups refresh between rules,
while the original category-name alias remains fixed. Later actions see a
pending calculated payee name and the source `new` marker. Split actions retain
the original parent's payee and literal parent text.

Construction validates Handlebars through the source class. Runtime formula
errors accumulate in transient `rule_errors`, while later actions execute.
Native previews use empty child notes and null parent categories, matching the
source's persisted representation instead of its temporary split display
summary. Child amounts/references remain exact comparisons.

Writes validate computed rows before integer arithmetic or financial mutation.
Fractional minor units are refused atomically. The persisted split witness
exposed an existing rule-writer discrepancy: references were retained on the
parent instead of only its children. Rule composition now clears them inside
the guarded split mutation, before version/history capture. The internal flag
is not accepted from or serialized into public payloads. Ordinary split-editor
behavior remains unchanged. The served contract describes calculated actions,
transient diagnostics and preview-only noninteger results.

`generate-rule-expressions-source.mjs --context` adds22 independent engine
observations: existing-row/same-day running balances, changes to account/date/
category within a rule, original split payee and a later action reading a new
payee name. These caught actual ordering and preparation mismatches before
correction. `rule-expression-context-source.json` retains the observations;
the producer independently reproduces them with `--context --check`.

`check-rule-expression-commits-http.mjs` imports the genuine saved-rule archive,
checks retained definitions, writes through public rules/apply, compares raw
financial results through public query, verifies unrelated persisted fields,
and retries exact operations. The query seam avoids confusing a displayed
parent summary with persisted references. Derived running balances may change
after an earlier amount changes; unrelated stored fields/versions may not.

Focused evidence:

- `/tmp/rule-expressions-final-http.log`:40 original previews,22 context
  previews and5 saved operations, including atomic refusals and exact retries.
- `/tmp/rule-expressions-final-source.log`: independent source reproduction
  and28 trusted-bundle comparisons.
- `/tmp/rule-expressions-final-rust.log`:12 existing rule tests and13 split
  tests pass. Two historical archive assertions now expect existing schema26;
  the former template-capability refusal now expects successful construction.
- `/tmp/rule-expressions-final-clippy.log`: warnings denied, pass.
- `/tmp/rule-expressions-report-regression.log`: existing core and formatting
  formula comparisons pass after sharing preference initialization.
- `/tmp/rule-expressions-contract2.log`: served OpenAPI checks pass, including
  fractional calculated previews/diagnostics and activation of the formerly
  gated valid-template archive. The old409 expectation was obsolete.
- `/tmp/rule-expressions-final-style.log`: changed JavaScript formatting passes;
  Python contract checker compiles, Rust formatting and diff whitespace clean.

Root standards review against `b0a9591`: shared preference setup, separate
trusted plugin caches, data-only adapters and atomic mutation ordering checked.
Root spec review:42 remains partial. Remaining work includes opt-in editing and
visible diagnostics, old native/staged-import capability refresh and recovery,
broader Android/import/schedule/manual/forecast flows, complete named-variable
mapping (including stored source order and child fields), source-backed
cross-rule and candidate-wide balance-prefetch timing, fractional split preview
policy, and resource-limit/performance qualification. These remain42 work, not deferred
acceptance items for50–53. No full suite, container qualification, push,
release or production deployment in this checkpoint.

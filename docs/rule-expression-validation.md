# Calculated rule actions — ticket42 in progress

Reference: Actual26.10.0, source commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Ticket41 accepted at `5e9cb42`.
This checkpoint establishes source behavior and a reproducible action asset;
native execution, contracts, UI and whole42 acceptance remain unfinished.

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
When wiring production execution, include this license in the runtime image.

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
endpoint. The first template case currently fails with422 `rule_requirements`,
recorded in `/tmp/rule-expressions-http-red.log`; native support is still absent.
The checker deliberately compares source conversions, errors and unchanged
financial state. Its one non-financial `today` template adapts the captured date
to the server's current UTC date; explicit transaction-date expectations remain
unchanged.

Next, integrate the asset into the existing bounded Rust-owned expression
runtime and native rule engine. Preserve lazy original-transaction context,
source running-balance ordering, parent/split semantics and command-wide work
limits. Complete declaration/import validation, preview diagnostics, guarded
persistence and idempotency, served contracts, opt-in editing, native recovery
and real Android/import/schedule/manual workflows. Expand source cases for
cross-rule ordering and running-balance boundaries. None of this is deferred
to final qualification50–53.

Root standards review: no remaining finding in this evidence/asset checkpoint.
Root spec review: source semantics are evidenced;42 remains partial. No native
execution or sandbox acceptance claim, full suite, container run, push, release
or production deployment.

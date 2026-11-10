# Calculated rule actions — ticket42 in progress

Reference: Actual26.10.0, source commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Ticket41 accepted at `5e9cb42`.
The initial checkpoint established source behavior and a reproducible action
asset. Subsequent checkpoints add native execution, guarded writes, expression
editing and source-backed context timing. Broader workflow qualification and
whole42 acceptance remain unfinished.

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

## Expression editing and context checkpoint

The existing action editor now exposes formula/template methods when opted in,
plus split formulas. Existing expressions remain editable after disabling the
flags. Draft reload preserves expressions. Preview errors name failed actions;
fractional preview amounts render without integer-formatting crashes and cannot
be selected for saving. Privacy mode also masks fractional values. These changes
use the existing Operate design, Impeccable guidance and Svelte skills.

Nineteen new running-source observations cover stored flags/nulls/original order,
balance preparation before the first action, candidate-wide literal lookup
prefetch, category-group refresh and original account aliases across rules.
Additional cases distinguish wildcard index buckets, discarded empty string
membership and Unicode condition normalization. The first wildcard case failed
before correction (`/tmp/rule-chain-index-red.log`). Native context now preserves
these observed behaviors; `rule-expression-chain-source.json` retains the
independent expectations. Imported ID/raw metadata and supported source row
fields are carried into expression context, with wider field coverage still
requiring workflow qualification.

Focused evidence at this checkpoint:

- `/tmp/rule-chain-source-final.log`: all19 source observations independently
  reproduce; original financial rows unchanged.
- `/tmp/rule-chain-index-green.log`: all19 native public previews match;
  each preview leaves its captured checkpoint unchanged. Rule creation changes
  the overall snapshot, so the chain checker separately compares all financial
  payload fields while allowing that metadata cursor to advance.
- `/tmp/rule-chain-regression.log`: original40 previews,22 context cases and5
  persisted operations/refusals/retries pass after context preparation changes.
- `/tmp/rule-chain-rust2.log`:12 existing rules and13 split tests pass.
- `/tmp/rule-expressions-ui-green4.log`: plain-HTTP browser editing, retained
  draft, save/preview/apply, disabled-flag retention, formula errors, fractional
  results, privacy, split formulas, keyboard navigation and200% text at390px.
- `/tmp/rule-expressions-literal-browser.log`: existing typed-rule editing,
  selective apply, conflicts, committed-request retry, manual protection and
  learning controls pass.
- `/tmp/rule-chain-index-clippy.log`, `/tmp/rule-expression-checkpoint-types.log`
  and `/tmp/rule-expression-checkpoint-style.log`: warnings-denied Rust check,
  Svelte type check and changed-file formatting pass. Both changed Svelte
  components have clean autofixer results. Production frontend build passes.

Rendered desktop,390px mobile, fractional-error and enlarged-text captures in
`actualBudget/web/test-results/rule-expressions/` were inspected. Controls and
help remain readable; mobile has no horizontal page overflow. Browser scripts
now join the normal suite, with focused source/API/browser commands available.

Root standards review against `294d25f`: preference gating retains saved data,
preview financial validation remains separate from display, expression inputs
stay data-only, and source expectations remain independently generated. Fixed
the nullable/fractional formatter and keyed error-list findings. Root spec
review:42 is still partial, with no acceptance criteria closed. Remaining42
work includes old native/staged capability refresh and recovery, representative
Android/import/schedule/manual/forecast execution, remaining field/child context,
execution candidate filtering versus direct selected-action prefetch semantics,
fractional split previews, and resource/performance qualification. No push,
release, container qualification or live deployment at this checkpoint.

## Import capability and native recovery checkpoint

Migration27 refreshes unactivated Actual plans with rule converter version2.
Valid expressions no longer retain the obsolete ticket42 capability block.
Malformed syntax remains blocked with a reason, raw rule and original archive.
Refresh reads one archive at a time, preserves upload identities and operation
receipts, and excludes activated sources so later user edits survive. Fresh
upload review and native restore use the same capability version. Archive
validation accepts schemas6 through the server's current migration version.
The existing transactional migration makes a standalone26 backup first.

`check-rule-expression-recovery.py --capture <legacy-binary>` generated a
disposable fixture using source commit `5e9cb42`, executable SHA256
`876ca190be5a202d014f870a5fa57a93387f76f331892099e490238c49152ef3`.
`rule-expressions-schema26.json` records executable/archive/database hashes,
public responses and ledger observations. The producer stages the genuine
saved-expression archive, a deliberately malformed template archive, and an
ordinary source budget whose imported rule is then manually edited. The invalid
input is explicitly a damaged-source fixture, not a financial oracle. The
producer also records a manual expense, exact write receipt and genuine native26
archive. No current database is relabeled as an older version.

The running pre-change server fails the ready-stage assertion in
`/tmp/rule-recovery-current-red.log`. The final public-API checker passes in
`/tmp/rule-recovery-final.log`: upgrade preserves ledger and edited active rules,
refreshes valid stages, retains malformed diagnostics and original bytes, replays
old upload/write responses exactly, activates valid expressions, exports and
duplicates them, restores changed definitions, applies the restored formula with
the current epoch, retries exactly, restores the old native26 budget and survives
a second startup without another migration backup. Financial output comes from
the existing pinned-source commit fixture. The maintained command is
`npm run test:rule-expressions-native` from `actualBudget/web`.

Focused regression checks also pass:

- `/tmp/rule-recovery-rust.log`:17 automation-upgrade and12 rule tests.
- `/tmp/rule-recovery-manual.log`: genuine linked-source manual conversion and
  genuine24 database/archive external-connection retirement.
- `/tmp/rule-recovery-native-regression.log`: forecast/report schemas, archive
  duplicate/restore, old21 upgrade, receipts and epoch behavior. Historical log
  labels say native24/26; their current archive assertions explicitly check27.
- `/tmp/rule-recovery-clippy.log`: warnings-denied Rust check.

Root standards/spec review against `b433bbb`: migration is transactional,
source blobs/receipts stay immutable, active edits are untouched, archive support
tracks the migration registry, and recovery is tested through public operations.
Ticket42's expression-import/diagnostic/original-recovery criterion is now
verified. Whole42 remains open for execution-candidate/direct-action semantics,
remaining child/field contexts, fractional split previews, broader transaction
workflows and evaluation-limit/performance qualification. No push or deployment.

## Live selection and calculated split checkpoint

Eleven additional pinned-source engine observations verify live index selection,
candidate capture before payee mutation, fractional parent/child allocation,
percentage/remainder rounding, generated-child variables and unbalanced split
diagnostics. Six independently persisted source operations verify five fractional
write refusals and generated-child context using the original imported order.
The source generators reproduce both new fixtures with `--check`; existing
fixtures are unchanged. Explicit selected-action application bypasses live index
buckets, while automatic execution uses the source candidate union.

Fractional previews now retain source number arithmetic without admitting floats
to the financial writer. Unbalanced previews expose `SplitTransactionError` in
the served contract and interface. Saving requires balanced, exact minor units;
the public API verifies rejection leaves ledger and checkpoint unchanged. Missing
generated-child variables remain absent during evaluation, then notes acquire
their persisted empty-text representation. Review caught a mutable JSON lookup
that inserted null children on ordinary rows; corrected before verification.

Checks pass:

- `/tmp/rule-execution-source-final.log`:11 independent previews and6 persisted
  source observations reproduce.
- `/tmp/rule-execution-api-suite.log`:40+22+19+11 source previews and5+6 persisted
  operations/refusals/retries. Unbalanced application also leaves no partial write.
- `/tmp/rule-execution-import-regression.log`:5 existing import-rule workflows.
- `/tmp/rule-execution-rust.log`:12 rule and13 split tests.
- `/tmp/rule-execution-browser.log` and `/tmp/rule-execution-literal-browser.log`:
  calculated and literal rule browser workflows. Fractional/unbalanced390px
  captures inspected; diagnostics fit, selection is disabled, no page overflow.
- `/tmp/rule-execution-contract2.log`:real responses validate against served
  schemas, including split diagnostics and native restore schema27. This check
  caught and corrected the previously stale restore-version enum.
- Rust warnings-denied check, Svelte types/autofixer, frontend build and changed
  file formatting pass.

Root standards/spec review against `91a3736`: source expectations remain
independent, preview arithmetic is isolated from exact writes, and generated
variables follow observable source behavior.42 remains open for existing-child
metadata/context, broader calculated transaction workflows and cumulative
evaluation/resource/performance qualification. Nonfinite split projections
explicitly fail instead of returning a source NaN graph; this remains part of42
qualification. No additional acceptance boxes closed; no push or deployment.

## Calculated import and forecast workflow checkpoint

Five new whole-source CSV-import observations exercise proposed payees, existing
payee-name conditions, calculated amount changes before matching, template notes,
calculated split allocation and repeated imports. Genuine source archives and
results are separate from the earlier literal-rule fixtures. `--expressions
--check` reproduces the source behavior. The public checker compares the complete
ledger/reference results, ensures previews create nothing, and checks stable
transaction/child IDs and exact receipt replay after repeated imports.

The first native comparison exposed premature proposed-payee visibility inside
templates. Source normalization strips the proposed display name before rules;
only persisted payee names enter rule context. A second condition witness exposed
the need to retain canonical names for existing payees. The importer now honors
both cases and restores proposed names for preview display after evaluation.
The original failure is `/tmp/rule-expression-import-api.log`; the condition
witness is `/tmp/rule-expression-import-condition-red.log`. Final source and API
checks pass in `/tmp/rule-expression-import-source-check2.log` and
`/tmp/rule-expression-import-api3.log`. All five existing literal import workflows
also pass in `/tmp/rule-expression-import-literal-final.log`.

Twelve new whole-source forecast observations use calculated actions, covering
ordinary and accountless schedules, transfers, filters, ranges, paid schedules
and tracking-budget projections. The source check passes in
`/tmp/rule-expression-forecast-source-check.log`. Native initially returned404
for an accountless schedule (`/tmp/rule-expression-forecast-red.log`): the
expression context tried to load the synthetic account from persistent storage.
The runner now supplies the source virtual-account fields for these simulations.
The final browser-driven public API comparison passes in
`/tmp/rule-expression-forecast-final.log`, including cache reuse/invalidation,
feature opt-out/re-enable and unchanged companion balances/ledger cursors and
schedule state. Synthetic accounts are never persisted.

Maintained focused commands from `actualBudget/web`:
`npm run test:rule-expression-workflows-source` and
`npm run test:rule-expression-workflows-api`. Existing12 rule/13 split tests,
4 import/recovery tests, Rust warnings-denied checks and changed-file formatting
pass. The import recovery check caught a stale assertion for a newly exported
archive's schema22; updated to27 (`/tmp/rule-expression-import-rust2.log`).
Historical fixtures were not changed. Other older suites still contain version
assertions to reconcile during final integrated verification. Root standards/spec
review against `45148b8`: source fixture generation remains independent,
normalization retains its existing display contract, and virtual context is
restricted to the simulation caller.42 still needs calculated Android/manual
entry, schedule posting and schedule-template workflows, existing-child context,
nonfinite split behavior and cumulative resource/performance qualification.
No additional acceptance boxes closed, push or deployment.

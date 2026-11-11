# Monte Carlo planning —39 accepted;40 open

Pinned Actual26.10.0 source commit:
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Ticket39 started after42 acceptance
at `c1cbac2`. Ticket39's baseline is accepted2026-11-10 after public API,
saved-plan recovery and browser qualification. Ticket40 retains the remaining
withdrawal strategies/rules, charts, cash flows and individual-run exploration.

## Source contract

`generate-monte-carlo-source.mjs` executes the unchanged frontend simulation
module through the existing pinned-source loader. Its22 scenarios capture raw
saved metadata, normalized configuration and complete results: summary values,
percentile bands, depletion histogram/timing, every run's ending balance and
withdrawal total, worst path and the first run's detailed cash flows. Typed arrays
become JSON arrays; financial values are not rounded or recomputed by the tool.
Source module hashes accompany the fixture.

Covered: new versus legacy defaults; proportional withdrawals; alternate seeds;
locked-pot bridge failure; income covering an empty portfolio; saved surplus;
fixed/rate fees; flat/progressive taxes; before/after-tax income-funded
contributions and caps; external contributions; age windows; spending phases and
minimum spending; stochastic inflation and today's-money presentation;
historical bootstrap/sequence; valid/incomplete custom asset mixes; empty
collections and minimum/maximum count/horizon clamps. Historical inputs retain
the pinned US1928–2025 series and its source caveats. Additional withdrawal
rules/strategies and broader run exploration remain40 work.

The saved-plan generator uses the actual engine's dashboard and preference
commands, then exports/reloads a genuine archive. Four plans preserve null/new
defaults, legacy metadata, rich progressive-tax inputs, an account-linked pot
and historical custom mixes. The feature flag remains explicitly false and all
original transactions remain exact. No fabricated native database supplies this
migration oracle.

Commands from repository root:

```sh
node actualBudget/tools/generate-monte-carlo-source.mjs --check
node actualBudget/tools/generate-monte-carlo-plans-source.mjs --check
```

Both reproduce (`/tmp/monte-carlo-source-check.log`,
`/tmp/monte-carlo-plans-check.log`). Artifacts:
`actualBudget/tests/fixtures/monte-carlo-baseline-source.json`,
`monte-carlo-plans-source.json`, `monte-carlo-plans-26.10.zip`.

## Native-runtime feasibility measurement

`benchmark-monte-carlo-runtime.mjs --release` builds an isolated optimized probe
with the server's pinned rquickjs0.13.0 dependency, bundles the unchanged source,
and runs it with64MiB heap,1MiB stack and20s interruption limits. Only the source
display constant is extracted instead of bundling unrelated utility imports.
No host callbacks, application database or network is exposed. Temporary build
and script files are removed. Cargo dependencies must already be cached for the
offline build. This is a diagnostic tool, not production routing or a substitute
for public API tests.

One local optimized run (`/tmp/monte-carlo-runtime-release.log`):

| Plan | Runs × years | Elapsed |
| --- | --- | --- |
| Proportional baseline |1000 ×20 |140ms |
| New-plan defaults, including surplus |5000 ×30 |1355ms |
| Historical sequences |98 ×20 |20ms |
| Progressive tax/contributions |1000 ×20 |498ms |
| Maximum horizon,1000 runs |1000 ×100 |391ms |

All captured output fields match the independent V8 source observations. Integer
outputs require exact equality; fractional simulation outputs permit floating
point variation of max(1e-6,1e-12 relative), capped at0.01 minor units. The first
debug probe's stricter absolute1e-6 threshold failed on a768294443-unit fractional
ending balance differing by approximately0.0000011; this is documented floating
point variation, not a financial-write tolerance. Ledger writes remain exact.

Debug default execution took4793ms; use optimized builds for deployment resource
decisions. These are single isolated elapsed measurements, including fresh
runtime initialization and JSON serialization, not p50/p95, energy, peak-RSS,
whole-app or maximum-workload claims. The heap setting is a bound, not measured
usage. Node/source and browser performance are not compared here.

The unchanged engine is a feasible candidate for bounded server execution without
a Node service or client simulation. Integration must keep work outside the
SQLite connection lock, limit concurrent jobs, support actual cancellation,
cache unchanged resolved inputs, and resolve account-linked balances at a captured
budget revision. Validate structured inputs before invoking trusted code. Revisit
the engine choice if whole-system resource qualification finds a regression.

## Remaining feature delivery

40 adds selection and qualification of all remaining strategies/rules,
portfolio/cash-flow charts and accessible individual-run inspection. Existing
advanced settings remain preserved and execute through the source engine; that
does not establish completion of40's controls, fixtures or inspection workflow.

## Native calculation API checkpoint

`POST /api/v1/budgets/{budget}/reports/monte-carlo` accepts version1, a client UUID
`request_id`, source plan metadata, a32-bit seed, optional captured-run index and
today's-money display flag. The read-only result contains normalized configuration,
the complete simulation result, captured epoch/snapshot, cache information,
warnings and assumptions. `GET`/`DELETE` on the request-ID child path inspect or
cancel active work. Completed requests disappear; these are ephemeral read jobs,
not durable mutation receipts. Duplicate active IDs refuse409; concurrent work
refuses429 without queue growth. Feature opt-in is checked before result/cache use.

The unchanged25KiB source asset executes in a fresh bounded QuickJS context.
Trusted bytecode is cached; JSON plan fields never become code. No Node process,
network/filesystem API or host callback is present.64MiB heap,1MiB stack and20s
interrupt limits apply; plan input is64KiB with at most256 entries per collection,
and response construction refuses more than8MiB. The router admits one worker.
Captured balances/default inputs are read in a short transaction; computation
runs on a blocking worker after releasing SQLite. Request drop/cancellation sets
the interrupt flag and releases the worker when it stops.

Resolved account balances, budget identity/epoch and all calculation options form
the cache key. Cache limits are8 entries/16MiB including keys. Linked balances
clamp to zero; unavailable links retain saved values with an explicit warning.
Ledger changes affecting a linked balance invalidate the result, while unchanged
inputs reuse it. Cache responses carry the current captured snapshot. No ledger,
plan, history or receipt is written by a calculation. The existing container's
Actual/QuickJS license files cover this asset; the generator verifies the exact
Actual license against its shipped copy and records its hash.

Public evidence:

- `/tmp/monte-carlo-http-red.log`: missing endpoint404 before implementation.
- `/tmp/monte-carlo-http-all.log`: all22 independently captured source results
  match through HTTP, including normalized inputs, detailed financial/distribution
  output, exact integer results, documented fractional tolerance, cache reuse and
  unchanged ledger/checkpoint.
- `/tmp/monte-carlo-workflows-final.log`: opt-in, malformed inputs, collection
  bounds, active duplicate/concurrency refusal, budget-scoped request lookup,
  actual cancellation, concurrent ledger writes, linked-balance refresh/fallback,
  cache invalidation and safe recovery.
- `/tmp/monte-carlo-limits.log`: an oversized128-pot/10000-run/100-year calculation
  refuses within25s, preserves ledger/checkpoint and permits a fresh calculation.
  This proves bounded refusal, not support for that workload or measured RSS.
- `/tmp/monte-api-contract.log`: the served contract validates real success/cache
  and structured error responses; all22 source result shapes, including conditional
  cash-flow fields, validate. The generated fragment equals the served aggregate.
- `/tmp/monte-api-clippy.log`: warnings-denied Clippy; Rust/JS formatting, asset and
  contract reproduction, and existing public SDK-map static coverage pass.

Maintained commands: `npm --prefix actualBudget/web run test:monte-carlo-source`
and `npm --prefix actualBudget/web run test:monte-carlo-api`. Release qualification
runs these source/API/resource/contract checks. Root review against `f0513a5`
checked source fidelity, budget isolation, cancellation/permit ownership, SQL-lock
lifetime, cache inputs, input validation and contract coverage. No unresolved
native-API finding. Later checkpoints below complete39's persistence and UI.

## Saved-plan migration and recovery checkpoint

`node actualBudget/tools/check-monte-carlo-plans-http.mjs` imports the hash-checked
genuine Actual archive into a disposable server. All four widget identities,
layouts and raw metadata match the source observations, including null/defaults,
legacy fields, linked accounts, historical mixes, income, contributions, spending
and taxes. The imported experimental flag remains disabled; enabling it permits
calculations without changing reporting documents or the financial state.

The check edits the rich plan's income, contributions, spending, taxes and pot
fees together. It verifies exact persisted fields, one revision increase,
immutable request replay and stale-revision refusal. A real process restart
retains those fields and receipts and reproduces the calculated output with a
cold cache. Native backup download, upload and restore recover a subsequently
deleted plan, preserve all reporting documents and the enabled preference, and
reproduce the saved calculation. Restore changes the epoch, invalidates the
calculation cache and refuses new writes carrying the old epoch. Every financial
row and reported balance remains unchanged across these operations.

This is recovery qualification of existing storage, not a newly implemented
storage feature or an independent financial oracle. Financial equivalence remains
covered by the22 source scenarios above. The first check incorrectly compared a
preference change's snapshot with the earlier snapshot; it was corrected to
compare complete financial rows/balances separately from the budget revision.
No production change was needed. Final evidence: `/tmp/monte-carlo-plans-http.log`.
The check runs in `test:monte-carlo-api` and release qualification. Root review
against `7d72bf5` covers standards and ticket39's persistence/migration requirements.
This checkpoint was committed at `ad9a000` before the UI work below.

## Baseline browser delivery and final39 acceptance

The existing dashboard editor now creates/edits Monte Carlo plans. Inputs cover
ages, run count, return model, inflation, pots/account links/access ages/asset
mixes/fees, surplus cash, income, contributions, spending phases, minimum spending
and flat/progressive taxes. The9KiB unminified configuration asset contains the
unchanged pinned normalization/default/preset helpers; simulation execution stays
on the server. Its generator checks source/asset hashes, the shipped MIT license,
absence of the simulation entry point and reproducibility from either working
directory. Custom mixes start from the departing preset's shares, as upstream does.

Opt-in controls creation, command search and calculation. Each saved widget has
an explicit Run action, seed, today's-money option and Cancel. Opening/reloading
the dashboard or changing theme does not run simulations. Results are labeled
experimental estimates with calculation time/options, warnings, assumptions,
success/depletion summaries and accessible percentile/depletion tables. Linked
account fallback and incomplete historical mixes are disclosed. Cached views
require the same document revision and budget epoch; offline use is read-only.
Privacy masking covers calculated financial values and probabilities.

`npm --prefix actualBudget/web run test:monte-carlo-browser` passes against the
running native server over plain HTTP. It verifies disabled opt-in, keyboard run,
the source baseline's median and every displayed percentile, exact persisted
income/contribution/spending/tax/fee edits, invalid amount retention after reload,
real active-job cancellation/recovery, cached reload without recomputation,
offline refusal/readability, privacy, command navigation, new-plan defaults and
source-equivalent results, preset/custom-mix editing and explicit fallback notices.
Desktop1280px, mobile390px,200% text and dark-theme captures were inspected.
The input test caught a section collapsing after adding an entry; binding the
native details state fixes it. A new-plan expectation initially changed inflation
volatility to zero while comparing the default2% source fixture. Matching inputs
passes without changing the financial oracle. Root review also corrected
preset-to-custom-mix seeding against the pinned UI implementation.

Evidence: `/tmp/monte-carlo-browser-final.log`,
`/tmp/monte-carlo-ui-check-final.log`, `/tmp/monte-carlo-ui-build-final.log`,
`/tmp/monte-carlo-autofix-*-final.log` and
`actualBudget/web/test-results/monte-carlo/`. Svelte/TypeScript report zero errors
or warnings; all touched Svelte files passed the Svelte analyzer. The production
build retains the existing large-chunk advisory; whole-system sizing remains51.
Source generation checks pass for both server and browser assets. The maintained
browser suite includes this workflow, so container/release qualification picks it up.

`/tmp/monte-carlo-workflows-ui-final.log` additionally verifies that the companion's
ordinary cleared-expense API can write during an active long simulation, within
the same two-second responsiveness bound as account creation. The subsequent
linked balance includes that expense exactly. Android has no Monte Carlo screen
or protocol change in this slice; no new device run is claimed. Full native-device
upgrade/regression qualification remains50, not a missing Monte Carlo feature.

|39 criterion|Authoritative evidence|
|---|---|
|Persist/import pots, income, contributions, spending and taxes|Four genuine source widgets; HTTP edits/restart/native restore; browser rich-input persistence|
|Baseline seeded simulation, cancellation, bounds and distributions|22 independent complete source outputs; API admission/limit/cancellation checks; real browser cancellation and displayed percentiles|
|Reference equivalence and visible assumptions/errors|Source fixtures; invalid draft retention; explicit experimental/estimate labels, source/fallback warnings and structured API errors|
|Agreed observable seams, including companion continuity|Running HTTP/browser workflows; cleared-expense public API during calculation; unchanged financial rows through recovery|
|Complete vertical behavior|Reporting persistence/receipts, versioned calculation contract, source import/native restore, editor/results and error/offline behavior shipped together|
|UI craft/accessibility and architecture boundaries|Impeccable/Svelte review; inspected responsive/theme/large-text renders; keyboard/labeled controls/tables; plain HTTP, no login, no offline calculation|

Root consolidated standards/spec review against `ad9a000`: no unresolved39
finding after the fixes above. Whole replacement qualification remains50–53;
additional Monte Carlo strategies/inspection remain40. No push or deployment.

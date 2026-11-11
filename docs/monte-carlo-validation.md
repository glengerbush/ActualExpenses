# Monte Carlo planning — tickets39/40 in progress

Pinned Actual26.10.0 source commit:
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Ticket39 started after42 acceptance
at `c1cbac2`. Source fixtures and the native calculation API are implemented;
saved-plan workflow/migration qualification and the interface remain unfinished.
Neither39 nor40 is accepted yet.

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

39 still requires saved-plan editing and migration qualification, opt-in labeled
UI with assumptions/errors and summary/distributions, browser verification and
final review. Preserve linked-account live balance/fallback semantics and exact saved
metadata.40 adds all remaining strategies/rules, charts/cash flows and accessible
individual-run inspection. Existing reporting-document storage is a reuse
candidate, not proof these workflows already work. No acceptance criteria closed.

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
native-API finding; whole39 still needs the saved-plan and UI work listed above.

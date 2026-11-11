# Monte Carlo planning — tickets39/40 in progress

Pinned Actual26.10.0 source commit:
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Ticket39 started after42 acceptance
at `c1cbac2`. This checkpoint establishes independent source inputs/results and
runtime feasibility; the application feature is not implemented or accepted yet.

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

39 still requires the versioned API and contract, bounded execution/cancellation,
saved-plan editing and migration/capability handling, opt-in labeled UI with
assumptions/errors and summary/distributions, real API/browser verification and
review. Preserve linked-account live balance/fallback semantics and exact saved
metadata.40 adds all remaining strategies/rules, charts/cash flows and accessible
individual-run inspection. Existing reporting-document storage is a reuse
candidate, not proof these workflows already work. No acceptance criteria closed.

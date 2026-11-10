# Formula cards and calculated rule actions — in progress

Tickets41/42; no full acceptance claim. Reference commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`, Actual API26.10.0,
installed/pinned HyperFormula3.5.0.

## Source seam

`tools/generate-formula-source.mjs` runs the unchanged production
`useFormulaExecution.ts`, including all three discovery/prefetch/evaluate passes,
unchanged custom functions and real Actual engine messages. The React adapter
only supplies state/ref/effect lifecycle and selected `en-US` locale; it does not
calculate expressions, substitute QUERY text, or supply financial results.
Trusted source modules execute in the same realm as HyperFormula because its
cell-array validation uses `instanceof Array`. A separate VM realm initially
caused a harness-only array error; this was corrected before recording results.

The genuine accepted `reports-source-26.10.zip` is reused read-only and checked
against its original digest. Clock is fixed to2026-11-09 UTC. Original account,
category and transaction identities remain unchanged. Source file hashes and
exact expressions/configuration/results are retained in `formula-source.json`.
Forty observations cover spreadsheet functions, sum/count/filtered queries,
case-sensitive/missing queries, balances by real name/ID, category/timeframe
extraction, formatting, named expressions/color, Unicode comparisons, date and
statistical expressions, explicit errors and all five compositional budget
query dimensions. Statistical cases declare a1e-12 tolerance; their original
numeric observations stay intact. Source errors (including extra FIXED argument
and bare FALSE lookup) are retained rather than replaced with guessed results.

Generation and independent `--check` passed:
`/tmp/formula-source-corpus.log`, `/tmp/formula-source-corpus-check.log`.
No replacement implementation is called by the producer.

## Current public tracer

Read-only POST `/api/v1/budgets/{budget}/reports/formula` is the public seam.
First missing-route404: `/tmp/formula-http-first-red.log`.
Initial spreadsheet result6 and unchanged ledger/checkpoint passed:
`/tmp/formula-http-first-green.log`. Query-sum/count then failed before custom
function integration; three initial cases now pass against source1204/19:
`/tmp/formula-query-http-green.log`. Remaining runtime, UI, import, rule-action,
contracts and representative companion coverage are unfinished.

## Evaluator investigation

A disposable prototype loaded the full trusted HyperFormula bundle in
Rust-owned QuickJS (no Node process/server), with64MiB heap, stack and deadline
limits. Approximate debug measurements:8.9MiB allocated after a simple formula;
trusted bytecode initialization60ms after one process-local compilation.
These are local feasibility observations, not a latency/power SLO or release
benchmark. Production work uses ICU collation, not a lexical approximation;
comparison text/work are bounded. Runtime dependencies and source assets remain
under development and are excluded from this source-only review checkpoint.

The small budget supplement reuses both genuine `multiline-long-goal` preapply
archives, executes the real `budget/overwrite-goal-template` message, proves no
transaction changes, and retains genuine resulting Actual ZIPs. Ten unchanged
production-hook observations establish envelope budget42.97/spent−15/goal1000/
starting carry70/ending97.97 and tracking42.97/−15/1000/40/67.97.
Generation and independent check passed in `/tmp/formula-budget-source.log` and
`/tmp/formula-budget-source-check.log`; no previous forty observations changed.

## Bounded evaluator checkpoint (partial41)

Full trusted HyperFormula3.5.0, enUS dictionary and unchanged Actual custom
functions execute inside Rust-owned QuickJS-ng/rquickjs0.13.0. The standalone
Node producer is test tooling only; production serves no Node process or browser
calculation engine. Bundled asset generation/check is reproducible and records
source/artifact hashes; upstream GPLv3/MIT/Unicode licenses are retained.

Every evaluation has fresh JS state and no filesystem, network, process or
module-loader bindings. Only bounded ICU4X collation is exposed as a host
function. Trusted bytecode is compiled/cached once per process; only successful
initialization is cached, so contention cannot poison subsequent requests.
Limits:64MiB JS heap,512KiB stack,2s monotonic execution deadline;64KiB input,
8192-byte expression,32 named queries/20 conditions,512 named text/numeric values;
32 custom-function requests per family,20000 cumulative prefetched values,
4MiB collation text work,32million shared SQLite VM steps and500000 cumulative
budget/category calendar comparisons. Budget intervals permit240 months and
1000 selected IDs. Refusal never truncates values or returns a fake zero.

Both discovery passes and final evaluation match the production ordering.
Initial speculative budget requests are discarded after query extraction, as
in the source. Query/count/account sums use exact live-leaf predicates and
minor-unit totals before the source's display-unit conversion; categories and
budget data reuse authoritative existing financial tables/calculation paths.
The budget helper adds scoped goal inclusion and a shared work counter without
changing existing unscoped budget/report outputs. Register matching also shares
one regex budget across all formula SQL/extraction passes.

Current public source40 plus budget10 comparisons/guards pass:
`/tmp/formula-core-http-green.log`, `/tmp/formula-core-budget-http-green.log`.
Asset reproduction passes `/tmp/formula-runtime-assets-check.log`; bin Clippy
`-D warnings` passes `/tmp/formula-core-clippy.log`. The large supported matrix
probe fails safely under the configured runtime bounds, retains the upstream
secondary null-error diagnostic with actionable range-size guidance, and then
successfully evaluates a fresh SUM and the original source NAME error. See
`/tmp/formula-limits-http-first.log` (unhelpful diagnostic red) and
`/tmp/formula-limits-http-green.log` (recovery green). No custom unsafe allocator
or guessed OOM classification was added; the stock enforced allocator remains.

Checkpoint exclusions: preferences/volatile evaluation-time integration, card
and named-query editing/rendering/import flows, served contracts, representative
companion checks and all42 rule/template integration are still unfinished.
No whole-ticket or release qualification is claimed here.

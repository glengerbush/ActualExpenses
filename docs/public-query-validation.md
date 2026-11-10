# Public query/API automation — in progress

Ticket47 on codex/query47. Initial base0a284f2; root owns final review/qualification.
No live data, deployment, release or version changes. Plan /tmp/actualbudget47-handoff.md;
public seams and source scope will move into the caller guide as implementation lands.

## First independent query witness

`tools/generate-public-query.mjs` imports the reviewed genuine
`transaction-export-base-26.10.zip` through public Actual26.10 importBudget, then
executes actual q/aqlQuery. Source commit2bebdba and four compiler/schema/executor
file digests recorded. No replacement query engine, formulas or expected result
calculation. The genuine archive/IDs are reused without rewriting. All queries
and exact JSON results/errors retained in public-query-source.json. Source
ledger before/after is exact; all reads/errors leave financial rows unchanged.

52 observations cover inline/grouped/all/none splits, grouped child match and
aggregates, three-level joins, grouping, ordered pages, null/inequality/membership,
LIKE/regexp, date transforms, scalar/window functions and structured source
failures. JSON wire normalization handles negative zero. `--extend` asserted all
original40 observations unchanged while adding12 separate function observations;
`--check` independently reruns all52.

Evidence: /tmp/public-query-source-{first,extended,check}.log PASS. Source quirks
are retained rather than cleaned up: documented $regex fails (compiler uses
$regexp); a multi-alias select object uses only its first key, so the twelve
functions have separate actual evaluations; some projected literal strings cause
source SQLite syntax errors. Native query literals must be bound data and cannot
become an unrestricted SQL escape. Those two misleading/inapplicable source
cases remain in the fixture, explicitly excluded from direct output comparison;
native validation/literal behavior needs separate public assertions.

`check-public-query-http.mjs` is the public consumer. True route RED404 is
/tmp/public-query-http-route-red.log. The earlier first attempt's upload400 was
only a missing ZIP Content-Type in the harness, corrected before route red.
No implemented query endpoint or acceptance claimed yet.

## Planned joins

Closed query registry/validated typed expressions and fixed budget-scoped joins,
exact money aggregates, split/ordering semantics, read-only POST snapshots/pages,
shared bounded work and structured refusal. Scripted command batches reuse domain
financial writers and original epoch/HLC/receipt/history/cancellation contracts.
Real external Python caller reads/imports/updates budget without a local engine;
caller guide maps existing feature routes and provider capability errors.

Generic arbitrary-ID notes currently remain blocked/preserved in original Actual
imports (conversion.rs explicitly reports unmatched notes). Root reserved schema24
after banking23; standalone genuine source/public witness precedes implementation.
No placeholder23, relabeled migration, or bank-secret projection permitted.

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
Read-only transaction query checkpoint implemented; whole47 remains incomplete.

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


## Frozen transaction read slice

GET query-capabilities and read-only POST budgets/{id}/query expose a closed,
budget-scoped transaction AST, fixed joins, literal bindings, exact money checks,
inline/grouped/all/none splits and snapshot-bound pagination. Bounds:64KiB body,
32 selected expressions,8 group/order expressions, depth8/nodes256,500 values,
1000/page,100000 matched rows, shared regex and2million SQLite progress steps.
No query receipt, history action or clock is minted. Native safe literals and
quoted aliases deliberately avoid the source raw-literal SQL error behavior.
Only transactions are advertised at this checkpoint; remaining table registry,
notes24, atomic command batches, public contracts and external caller are WIP.
Unicode normalization parity is also an explicit remaining query gate.

Strict50 source cases PASS /tmp/public-query-read-50-green.log; bounded public
validation/literal/overflow/stale-page checks PASS
/tmp/public-query-read-guards-green.log. Bin Clippy -Dwarnings PASS
/tmp/public-query-read-clippy.log. Earlier grouped unmatched-row and joined-null
boolean failures were genuine red→green; missing-upload-header and stale binary
runs were harness/setup failures, not product defect evidence.

## Retired category identities and imported saved filters

`generate-retired-query-source.mjs` executes real deleteCategory independently
with and without replacement, then real ActualQL and unchanged source custom
spreadsheet for old equality/membership, explicit null, target equality and
mixed membership. `--saved` creates actual saved filters before deletion.
Four genuine original ZIPs retain financial rows, references, saved IDs and
predicate literals. Source checks PASS:
/tmp/retired-query-{source,saved-source}-final-check.log. Only generated saved
filter IDs are normalized by name when reexecuting the oracle; immutable ZIP
IDs remain preserved. Grouped outputs were added with --extend after asserting
all earlier observations unchanged. Source reports omit presentation legend.

`check-retired-query-http.mjs` imports genuine saved-filter ZIPs, duplicates
pristine templates publicly, and compares source financial graphs, literal
saved predicates, register grouped membership, inline ActualQL and complete
custom spreadsheet results. Native backup/restore then repeats the same proof
and exact saved settings comparison. True RED
/tmp/retired-query-http-red.log: retired equality unexpectedly selected6 roots.
The category-only canonicalization correction preserves literal old IDs for
both deletion modes; mutation resolution and payee identity remain untouched.
A second genuine report red showed category-is-null included transfers unlike
source rule-condition semantics; report-only translation adds transfer/parent
exclusion while direct register/ActualQL null behavior stays unchanged.
All comparisons and both native roundtrips PASS
/tmp/retired-query-http-green.log. No monetary expected value was adjusted.

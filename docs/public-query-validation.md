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

## Read-checkpoint review corrections

Review identified aggregate pagination cardinality, invalid aggregate/window
placement, and missing cumulative grouped-output limits. Public reds retained:
/tmp/public-query-review-{aggregate,placement,output}-red.log. Sole SUM advertised
next_offset1; group SUM returned500; two40-allocation split families with repeated
long-note projections returned an unbounded response. No source expected values
changed. Aggregate metadata now uses result cardinality; group/filter/order only
accept scalar row expressions. One cumulative allowance covers all roots and
expanded children:10000 emitted rows,100000 values,4MiB serialized JSON including
escaped text. Refusal occurs before retaining the over-budget record. Ordinary
pagination still uses complete matching cardinality. Tests verify untouched
checkpoint and complete financial graph after all refusals. A premature run
/tmp/public-query-review-green.log used the old binary while linking; it is not
correction evidence. Final built run is recorded separately.
Final focused correction PASS /tmp/public-query-review-built-green.log;
all50 source reads PASS /tmp/public-query-review-source50.log;
existing bounds PASS /tmp/public-query-review-guards.log;
binClippy-Dwarnings PASS /tmp/public-query-review-clippy.log.

## Standalone notes — schema24 checkpoint

The published SDK's arbitrary `getNote/updateNote` identity is a genuine import
gap: original conversion blocked unmatched note rows. Independent source
`generate-public-notes.mjs` executes arbitrary colon/slash identity, original
Unicode/multiline prose, update and clear; source financial rows stay exact.
Original genuine export is retained, hash checked, never rewritten. Missing
source note is null; cleared existing note is an object with empty text.
Source generation/check PASS /tmp/public-notes-{source,source-check}.log.
Official exports have no CRDT clocks; this witness makes no source-clock claim.

Schema24 follows the frozen banking23 foundation5489ac1. Budget-scoped notes
store only standalone identities. Known account/category/group/month IDs remain
projections of authoritative domain notes fields. Read-only POST notes/read and
owned notes/commands preserve literal IDs, original HLC, snapshot/epoch,
receipt-first recovery, cancellation and field-level history. IDs are bounded to
512 characters, text10000. Feed records include note clocks; retained tombstones
support creation undo/redo. Import preserves original source bytes and any
present source note clocks. Old blocked stages receive a converter while their
original receipt remains immutable. Native archive/duplication/current history
registries include notes; older archives enrich only during versioned restore.

True blocked-import RED /tmp/public-notes-http-red.log. Focused public financial,
known category projection, incremental feed, explicit HLC, update/clear,
creation/update undo/redo, old-clock refusal, cancellation and native exact
receipt recovery PASS /tmp/public-notes-focused-final.log. Earlier cancellation
harness expected200; existing contract correctly returned409 operation_canceled;
harness aligned without runtime weakening. No idle/background worker added.

Genuine23 fixture generator `generate-public-notes-schema23.py` runs immutable
865402b executable SHA6012812abb9c6cd9dea026a08128c705f34d6d8a6250f1c7fd5050264b4a0e82.
This is banking23 foundation +reviewed query read/35, not final28 financial writer.
SQLite23 integrity/FK/version and native manifest23 checked readonly; no version
edits. Ordinary money/history/receipts plus blocked original arbitrary-note stage
retained. Public consumer `check-public-notes-upgrade-http.mjs` verifies unchanged
old ledger/checkpoint/history/receipts, ready reviewed stage/original bytes and
activation, then real native23 restore with exact money/receipts and expected
rotated epoch/history reset. PASS /tmp/public-notes-upgrade-final.log.
BinClippy PASS /tmp/public-notes-final-clippy.log.

The replacement's explicit-ID create API additionally permits choosing a future
reference UUID, whereas SDK account/category creation strips supplied IDs.
Parent-approved replacement identity guarantee: nonempty retained standalone
notes refuse conflicting account/category/category-group (including inline
category) creation with note_identity_conflict. Caller chooses a different ID
or explicitly clears the standalone note first. Genuine public RED
/tmp/public-notes-identity-red.log showed creation silently shadowed retained
prose; focused correction checks all3 domain kinds, unchanged checkpoint/lists,
retained prose and successful explicit-clear resolution. This is explicitly a
replacement safety guarantee, not mocked source UUID parity. Native validation
also refuses conflicting live standalone/domain note identities while retaining
legitimate historical tombstones. Whole47 remains WIP as listed above.

## Notes review corrections and Unicode query witness

Unowned notes originally bound body only; owned history already bound epoch.
True unowned RED /tmp/public-notes-unowned-epoch-red.log returned200 for a changed
valid epoch. All notes receipts now bind {request,epoch}. Focused checks prove
owned/unowned original old-epoch replay after native restore and changed-epoch
rejection. Earlier UUID-shaped epoch attempt was invalid400 (harness error), not
that product red. True month RED /tmp/public-notes-month-red.log returned200 with
null after create→undo→fresh write. Month upsert now revives tombstones, guards
both note and tombstone generations, preserves buffered, and refuses a clock
between original creation and newer undo without changing history. PASS
/tmp/public-notes-epoch-month-final-green.log.

`generate-query-unicode.mjs` executes51 genuine SDK mutations and ActualQL reads,
recording input hash, compiler/normalization digests and executed SDK entry hash.
Every mutation is observed publicly before querying: source updateTransaction
returns before its batch promise settles. The initial unobserved provisional
fixture/logs were discarded from evidence (/tmp/query-unicode-unsettled-source.json
retained diagnostically); corrected generation/check PASS
/tmp/query-unicode-source-{settled,final-check}.log. Extension asserted all42
existing settled observations unchanged while adding9 empty-text/pattern cases.

Genuine source red /tmp/query-unicode-http-settled-red.log: cafe failed to match
Café. AQL-only LIKE normalization now lowercases, expands ł/ø/ß/œ, decomposes NFD
and removes ECMAScript Diacritic property via the existing bundled regex engine.
Pinned unicode-normalization0.1.25 supplies NFD; no external process or application
server. Regexp stays literal. Source empty LIKE pattern is false, while percent
matches empty text; these observed distinctions are retained. All51 exact public
matches/errors and unchanged per-query ledger/checkpoint PASS
/tmp/query-unicode-http-final-green.log; original50 source reads PASS
/tmp/query-unicode-query50.log. One cumulative64MiB input/output scalar-text work
allowance covers count and page; public composed50-transform query refuses with
typed400, preserving graph/checkpoint (/tmp/query-unicode-text-budget-green.log).
BinClippy-Dwarnings PASS /tmp/query-unicode-epoch-clippy.log. No broad domain suite
repeated. Known-note row-presence markers/registry, atomic batch, caller/contracts
and maintained integration commands remain47 work, not accepted completion.

## Root notes/Unicode composition review

2026-11-10: Reviewed `3b5926f` plus `22cf0bb`. Closed captured-epoch and
month-note revival findings; exact unowned retry now rejects changed epochs,
and stale revival checks the undo tombstone generation. Source51 Unicode
evidence and cumulative text-work guard reviewed without another domain-suite
rerun. Known-note presence/registry and remaining47 scope are still active.

Composed with35 and bank financial checkpoint790779e. Conflicts resolved by
retaining both capability sets and both bank/notes change-feed arms; history
tracks both note and bank-origin changes. Combined build PASS
`/tmp/bank-notes-compose-build.log`. Running notes/receipt/history/native checks
PASS `/tmp/bank-notes-compose-notes.log`; genuine foundation23→24 database,
blocked-stage, original receipt/history and native restore PASS
`/tmp/bank-notes-compose-upgrade.log`. Forecast native24 and reporting genuine
21→24/native24 round trips PASS `/tmp/bank-notes-compose-{forecast,reporting}-native.log`.
Maintained native consumers now assert actual current24 exports; genuine21/23
input fixtures remain untouched. Earlier native22 observations remain earlier
evidence, not relabeled24 runs.

Preserved richer pre-notes schema23 executable for future bank upgrade work:
`.scratch/actualbudget-schema23-8aae0cf-server`, SHA256
`96d679629e9ac4ad1097dfd1baae13a8416ea3f2b54e43980db17b50991bd9b0`.
It includes financial28+35+47reads, unlike the earlier foundation-only23 fixture;
no completed bank migration/UI or full47 qualification is claimed.

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

Schema24 follows the frozen banking23 foundation5489ac1. Budget-scoped notes initially
stored standalone identities; the extension below adds known-ID presence markers. Known account/category/group/month IDs remain
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


## Known-note identities, shared clocks and guarded binding

`generate-note-identities.mjs` captures11 genuine SDK/AQL states: initially missing,
then written/cleared account, category, group, month and standalone notes. Empty
source note rows remain present after clearing. Original post-clear ZIP retained;
independent check PASS /tmp/note-identities-source-final-check.log. A separate
--prefix witness retains5 states and genuine ZIP for arbitrary IDs budget-2026-13
and budget-automation; SDK accepts these as literal note IDs. Native source import
previously400 rejected them as months (/tmp/note-identities-prefix-http-red.log).
Only canonical valid month IDs now enter the budget-month converter/clock map;
other IDs remain standalone with their literal identity.

Known IDs now have empty identity markers; their authoritative text stays in
existing domain fields. Absent versus explicitly empty is no longer inferred
from default domain text. Read-only notes query exposes only id/note and excludes
private history tombstones. Identity-only domain SQL triggers preserve presence
on real annotation edits, without duplicating prose. Eight IF NOT EXISTS trigger
checks at startup perform no source/ledger scan; domain backfill runs only during
versioned migration or explicit restore. Native table order installs identities
before domains, avoiding trigger-created duplicate insertions. No new migration
slot/column beyond24 or original fixture version changes.

Public consumers compare exact source reads/query rows, original bytes and both
cleared-source import/native roundtrips. Owned initially absent known notes,
including empty text, undergo write→undo(query absence)→redo(presence)→undo and
then sequential category-creation undo; triggers do not resurrect markers.
PASS /tmp/note-identities-history-final-green.log and
/tmp/note-identities-prefix-final-green.log. Original notes/genuine23 upgrade
regressions remain green; binClippy PASS /tmp/note-identities-freeze-clippy.log.

The approved explicit-ID clear→reference recovery exposed actual stale-clock
bugs: notes commands and account metadata accepted an older annotation after
binding; the category budget-note alias also bypassed its retained newer clear.
True reds /tmp/note-identity-{clock,metadata-clock,budget-clock}-red.log. Shared
logical note clocks now guard these aliases, metadata responses expose their
max original HLC, and metadata stamps mirror their existing clock into captured
note history (no minted/rebased clock). An old standalone history action also
could run after binding, hiding restored prose; true red
/tmp/note-identity-binding-history-red.log. Notes history captures its binding
and conservatively refuses an inverse across a new reference identity, with
unchanged data/checkpoint/history. Retained physical identities keep that guard
after reference deletion; use a fresh reviewed note change, not a forced old
inverse. This is a replacement explicit-ID safety guarantee, not SDK UUID parity.

Fresh notes/metadata intents after erasing a standalone note must clear retained
inactive marker payloads; otherwise portable restore422 rejected the resulting
state (/tmp/note-identity-revive-red.log). Both routes now preserve fresh text,
native restore and the exact original old-epoch erased-note receipt. Finally,
financial month revival exposed raw undone text despite an absent logical note
(/tmp/note-month-revival-red.log). Budget view/feed mask deleted annotations;
reset-hold→undo→reset-hold preserves absence and buffered values without deleting
retained raw prose. Financial fields/calculation semantics unchanged.

Focused greens: /tmp/note-identity-{clock-final,metadata-clock-final,binding-history-final,budget-clock}-green.log;
/tmp/note-identity-revive-green.log; /tmp/note-month-revival-green.log;
/tmp/note-identities-owned-final.log. Final binClippy-Dwarnings PASS
/tmp/note-alias-projection-clippy.log. Core resource query registry, batch/caller,
served contracts and maintained qualification wiring remain47 work.

## Root identity/clock composition review

2026-11-10: Reviewed `e9c623a`: known-note markers, literal invalid-month IDs,
logical HLC aliases, inverse binding guards, trigger/install ordering and masked
undone month text. No remaining finding in this slice. Integrated at `166cc77`
with36 scoped budget reads and28 migration/recovery. Resolved conflicts preserve
scoped reads, visible month annotations, bank23 enrichment and note24 backfill;
formatter-only changes normalize the new shared hooks.

Combined build PASS `/tmp/note-identities-compose-build.log`. Public source11
identity/history states, five literal-prefix states, fresh note/metadata revival,
portable/exact-receipt recovery and financial month revival PASS
`/tmp/notes-compose-{identities,prefix,revive,month}.log`. Shared archive/migration
checks PASS `/tmp/notes-compose-bank-upgrade.log` (genuine22 database/archive,
current24 clone/restore/tombstones/late-response fencing) and
`/tmp/notes-compose-notes-upgrade.log` (genuine23 database/native archive,
blocked stages/history/exact receipts). Input fixtures unchanged. Existing alias
clock red/green and source reproduction evidence reused; no full-suite rerun or
whole47 acceptance claimed.

## Closed resource profiles and query types (47 checkpoint)

`query-capabilities` now lists the closed profiles, typed fields/references,
functions/operators, pagination contract and cumulative work/output limits.
Accounts/groups, payees, categories/groups, rules, schedules, notes and native
budget_cells share the bounded compiler; arbitrary storage/provider tables stay
unavailable. References follow only advertised budget-scoped paths (depth4).
`*` expands only advertised fields. Structured automation definitions and raw
bank metadata are JSON; substring/pattern/order operations on these fields
refuse400 rather than promising original serialized key order. Original source
bytes remain retained. Rule fields use documented canonical account/payee names.

`generate-query-resources.mjs` executes21 real Actual26.10 queries on seven
reviewed genuine archives; original observations remain unchanged when extended.
The source category-group executor automatically expands children even with
explicit selection. The native profile is intentionally flat: the consumer
compares every group and every child's identity/count/fields through a bounded
categories(group=ID) read. The SDK's implicit transaction ID is requested
explicitly when translating scalar-only projections. Both source allocation
tables map to budget_cells(method/month/category): exact amounts, carry flags,
goals and ISO months are compared; native cell IDs are opaque, not claimed equal
to source concatenated IDs. Unsupported private source positions are not exposed
as financial identities. The final SDK/CLI guide must map remaining resources
and operations to their existing typed feature routes.

`generate-query-types.mjs` independently executes24 typed/calendar/composition
queries. Real public reds exposed boolean-null casting, invalid numeric/text
coercions, date granularity, scalar-over-SUM and grouped aggregate ordering.
The compiler now matches these source observations: null boolean predicates
mean false, membership retains source scalar stringification with bound values,
calendar fields/functions are typed, and source-valid aggregate composition/order
works. Aggregate/window placement guards and exact-money/output budgets remain.
The source accepts structurally formed invalid calendar dates in read predicates;
that observation is retained and does not relax financial write validation.

Source checks PASS /tmp/query-registry-source-check.log and
/tmp/query-types-source-final-check.log. Public21 PASS
/tmp/query-registry-http-final.log; public24 PASS
/tmp/query-types-http-final-green.log (includes original financial/checkpoint
preservation). Guards also verify wildcard allowlists, unknown/private resources,
structured-field refusal and invalid aggregate ordering. BinClippy-Dwarnings PASS
/tmp/query-registry-clippy-final.log. Batch/caller, final served contracts,
representative browser/native observation and maintained wiring remain47 work.

Registry follow-up: the schedule reference was initially advertised but dotted
transaction paths refused400 (root/public red plus
/tmp/query-schedule-reference-red.log). A22nd genuine source case compares seven
linked rows' schedule.name and schedule.rule.stage through the same closed,
budget-scoped resolver. Source22 check and focused public green:
/tmp/query-schedule-reference-source-check.log,
/tmp/query-schedule-reference-green.log. No unrelated financial writer change.

## Root resource/type review

2026-11-10: Reviewed46e56fe closed profiles, budget-scoped references, typed
predicates/composition, wildcard expansion and bounded output. Root found one
catalog inconsistency: advertised transaction schedule references returned400
for schedule.name and schedule.rule.stage. Disposable public proof:
`/tmp/query-registry-reference-review.log` (scalar schedule200, both dotted
paths400). Corrective44313f5 adds the closed resolver and an independent source
case covering seven linked entries. Finding closed.

Combined at `d22496f`; build PASS `/tmp/query-registry-compose-build.log`;
all22 resource and24 type/calendar source comparisons PASS
`/tmp/query-registry-compose-{resources,types}.log`, preserving ledger/checkpoint.
Flat group expansion and structured JSON representations are explicit API
mappings, with original observations retained. Earlier50 reads/guard/Clippy
passes reused. Batch/caller/final contracts and whole47 acceptance remain pending.

## Atomic transaction command checkpoint (47)

POST `/budgets/{b}/transactions/batch` accepts a captured snapshot and1–100
ordered create/update/delete operations with distinct targets. Existing financial
writers own row validation, graph propagation, reconciliation and per-field HLCs.
The adapter adds one transactional receipt/history boundary. Any refused action
or newer field aborts the entire command (no partial success/no failed receipt).
One operation per connected financial graph is required; conflicting intents
across selected pair/split members refuse before writes. Resolve inline payee/
category names through their existing resource APIs first; batch inputs use UUID
references. Connected work is bounded100000 members and conservative4MiB response
serialization. This is an explicit replacement contract, not an unrestricted
executor or arbitrary server-side SDK callback.

`generate-public-batch.mjs` independently executes genuine SDK add/update/delete
inside source batchBudgetUpdates on the reviewed original transaction archive,
waits for settled public rows, and captures the complete graph and per-account
aggregate. The baseline helper batches spreadsheet refreshes and does NOT promise
rollback; replacement atomic refusal is deliberately stronger, separately tested.
Original archive/engine hashes and all untouched split/transfer/reconciled rows
remain in `public-batch-source.json`.

Running-API tracer first failed405 at the absent endpoint
(/tmp/public-batch-http-red.log). Final public proof PASS
/tmp/public-batch-freeze-http.log: exact SDK graph/aggregates, original field HLCs,
one owned undo/redo, mid-command reconciled refusal with no new rows/checkpoint/
history/receipt, stale-field refusal despite a fresh cursor, graph overlap/bounds,
exact replay/lookup and changed-epoch refusal, durable cancel fencing, portable
restore then process restart with exact original old-generation receipt and no
extra financial effect. With an external test URL, process ownership stays with
the caller/container; isolated local execution proves restart. Source independent
check PASS /tmp/public-batch-source-freeze-check.log; binClippy PASS
/tmp/public-batch-clippy-green.log. Final caller guide, served contracts,
representative web/native observation and maintained wiring remain47 work.

Batch review correction: the first adapter checked the graph cap only after
unlimited members() and ledger::get expansion. That did not bound discovery.
`members_bounded` now walks indexed live parent/child/reciprocal-peer edges with a
bounded discovered map/queue and an early-stopped child cursor; no get_many or
high-fanout recursive queue is materialized first. Ordinary confirmation's
members() implementation is unchanged. Optional `max_graph_members`1..100000
(default100000) lets callers choose a stricter captured allowance; overflow
refuses the whole command with graph_limit, never returns a truncated graph.

Public red /tmp/public-batch-graph-bound-red.log specifically distinguishes
unknown-field400 from a real graph_limit. A genuine parent+two-child graph with
allowance2 refuses without ledger/checkpoint/history changes; the following
independent mixed command succeeds with allowance2, proving guard reset. All
batch/replay/restore/restart assertions remain green
/tmp/public-batch-graph-bound-green.log; binClippy PASS
/tmp/public-batch-bound-clippy.log. This is a functional bounded-discovery proof,
not a100000-row timing/energy claim (qualification51 remains separate).

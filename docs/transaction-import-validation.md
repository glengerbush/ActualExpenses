# Transaction imports — working evidence

Tickets25/26 remain incomplete. Accepted integration base includes main45 (`145a761`); current accepted main will be merged before final qualification. Working branch:
`codex/imports25-26`; implementation checkpoint `adc8a03`, reviewed source
evidence `d97f329` / `5ac7751`. No release, push or live deployment.

## Contract and ownership

One implementer owns the complete25/26 slice, its remaining public consumers,
UI, OpenAPI, conversion and tests. Root supplied independent source evidence and
owns consolidated review/integration/qualification. Review coherent completed
checkpoints; reuse unaffected evidence. Keep this record current rather than
repeating handoffs. Ticket45 is accepted and merged; reuse its navigation registry and privacy formatting.

Public import routes under `/api/v1/budgets/{budget}`:

- POST `/accounts/{account}/transaction-imports?filename=...`: retained bytes.
- POST `/transaction-imports/{id}/parse`: raw parser, visible errors, no writes.
- POST `/transaction-imports/{id}/preview`: captured snapshot, mapping/options,
  proposed financial rows/matches; no writes.
- POST `/transaction-imports/{id}/commit`: explicit row choices; owned, atomic,
  epoch/HLC guarded, immutable receipt before stale-snapshot checks.
- Manual merge agreed: POST `/transactions/merge/preview` and
  `/transactions/merge`, body `{snapshot,ids:[a,b]}`. Preview exposes `keeper_id`
  and full affected before/after graph; commit uses existing protection proofs.

## Verified development checks

Genuine pinned Actual26.10.0, source commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`; all source generators below independently
rerun with `--check`. No replacement parser/matcher/financial math supplies their
expectations.

| Evidence | Coverage | Current public consumer |
|---|---|---|
| `transaction-import-manual.json` + source ZIP | Parser→frontend normalization→manual match→repeat→CSV export | Initial Rust HTTP tracer1 passes |
| `transaction-parser-source.json` |48 genuine raw parser cases, input bytes/hashes/options;21 CSV/TSV,27 other formats/errors | `check-transaction-parser-http.mjs --csv`:21 pass |
| `transaction-normalization-source.json` |73 unchanged frontend finalization observations (prior71 retained; exact ±1.1250 half-cent cases added) | `check-transaction-normalization-http.mjs`:46 CSV financial/error cases pass; selection/format cases separate |
| `transaction-matching-source.json` |14 genuine matching/repeat scenarios | `check-transaction-matching-http.mjs`:8 CSV workflows pass; all14 now pass, including bank-ID/deleted/locked scenarios |
| `transaction-merge-source.json` +14 source ZIPs | Keeper precedence, fields/status, split reparent, transfers, four atomic refusals | Consumer/implementation pending |
| `transaction-import-rules-source.json` +4 source ZIPs | Lazy payees, no unused payee on manual match, rules before matching, split rule once, repeat identity,19 saved source options | Public source4 workflows pass (rules before matching, split allocations, append once, lazy payees, repeat IDs) |
| `transaction-import-schema20.{sqlite3,zip,json}` | Genuine20 owned expense/edit/history/receipts, balances, two blocked source stages, original bytes and native backup |Public Rust database/native consumers2 pass (working21 joins) |

Logs: `/tmp/transaction-parser-http-csv-green.log` (21),
`/tmp/transaction-normalization-http-green.log` (44),
`/tmp/transaction-matching-http-green.log` (8),
`/tmp/transaction-import-matcher-green.log` (initial Rust tracer),
`/tmp/transaction-import-checkpoint-clippy.log` (Clippy).
The matching consumer additionally retains first-import IDs across repeats;
generated-ID normalization cannot hide replacement purchases.

Reviewed oracle limitations: official Actual budget ZIP exports erase CRDT messages.
Saved-option ZIPs prove values, **not source per-key clocks**. Validate independent
option clocks using current native commands and native archive restoration.
Manual merge graph comparisons canonicalize generated IDs and incidental AQL
row ordering, preserving all fields, multiplicity and graph relationships.
Invalid-second-row normalization records an intermediate prefix plus an error;
the source aborts submission, and public consumers require atomic refusal.

The preserved20 executable is `.scratch/actualbudget-schema20-32f27ab-server`,
SHA256 `fabd86650e86bbdab91e2796040d3e455ffef67e713721740f046694b80f6eed`.
Fixtures were created by that binary through HTTP, then checked read-only for
schema20/integrity/FKs. No database relabeling or direct financial writes.

## Remaining acceptance gates

- Complete importer/exporter/manual-merge UI, durable original requests and
  preview recovery, keyboard/narrow/dark/200% captures and consolidated review.
- Real committed QIF/CAMT file workflows and additional format-option recovery;
  raw parser48 and structured preview22 are already green, while OFX/CSV cover
  the14 financial matcher cases.
- Served OpenAPI and contract smoke for all new seams; current schemas/reports21,
  cancellation/receipt probes and source capabilities; meaningful Android feed
  case and maintained CI/container commands for every public source consumer.
- Final integrated HTTP/native/container qualification once the complete batch is
  frozen. Reuse reviewed preservation/writer/merge and unchanged source evidence.

Do not mark25/26 done until their whole acceptance criteria pass. Further genuine
source cases should answer concrete gaps, not expand into unrelated tickets.

## Current21 preservation join (in progress)

`transaction_import_upgrade.rs`: genuine20 database and native archive consumers2
PASS `/tmp/transaction-import-upgrade-first-green.log`. Old financial/reference
views, owned history, checkpoint and exact original expense receipts remain;
retained blocked stages become explicitly activatable while their old upload
receipts and original ZIP bytes remain unchanged. Native restore rotates epoch
and resets history; old receipts still replay exactly. True old-binary control
refuses the newly supported stage (`/tmp/transaction-import-upgrade-old-red.log`).
An earlier harness incorrectly expected200 for replayed transaction creation;
that setup assertion was corrected to201 before capturing the true control red.

New metadata/settings history/feed and schema21 archive registries are joined.
Ordinary responses gain no empty metadata defaults. Imported bank IDs are opaque
source values; settings retain independent source preference IDs/value fields.
Official source ZIPs contain no CRDT clocks; existing clocks are translated only
when present, never minted during enrichment. Current per-key clock/native
roundtrip proof remains a required test.

Concrete remaining semantic check: selective input changes must preserve Actual's
finalization/matching scope (unselected changed rows versus ignored existing rows,
and explicit force-add). The current full-file preview matcher passes the eight
source financial scenarios but this selection interaction needs its own executed
source witness before acceptance. File ordering of multiple new distinct rows
also needs an observable fixture; do not rewrite existing private order anchors.

## Rules/payee checkpoint

Source5 public workflows PASS `/tmp/transaction-import-rules-green.log`; raw
preview leaves references, checkpoint and financial rows unchanged. The consumer
compares every parent/child monetary/reference/status field and first-import IDs
across repeats; account-managed transfer payees use the established separate
public projection. A provisional reference is saved only when a resulting new
row or matched empty reference uses it. Rules run before matching and the
evaluated graph reuses the guarded writer without executing primary rules twice.
One command work allowance spans evaluation, matching and counterpart effects.
Pinned title vocabulary/character class comes from the unchanged26.10 title
module; `title-data.json` records those static tables.

Existing automatic-entry2 and linked-schedule1 regressions PASS
`/tmp/transaction-import-rule-entry-regression.log`; bin Clippy PASS
`/tmp/transaction-import-rules-clippy.log`. Exact-half source generator/check73
and public normalization46 PASS. Preservation checkpoint `ce92da3` independently
reviewed Standards/Spec PASS; current per-key clock/native roundtrip remains open.

## Structured formats, bank IDs and saved clocks

Raw source parser48 PASS `/tmp/transaction-parser-formats-green.log`; structured
frontend normalization22 PASS `/tmp/transaction-structured-second.log`. New
`formats.rs` handles source QIF records, OFX/QFX XML and optional SGML leaf
closures, declared encodings, CAMT entry/detail amounts and memo/payee options.
XML uses pinned `roxmltree0.21.2`, with500000-node cap and no external DTD
resolution ([primary API](https://docs.rs/roxmltree/0.21.2/roxmltree/struct.ParsingOptions.html)).
All raw fields, row order and success/error decisions match the independent
corpus. Unknown file formats reject before storing; supported malformed files
retain their original bytes and visible parse errors.

Financial matcher14 PASS `/tmp/transaction-bank-matching-first.log`. Genuine
pre-import archives in `fixtures/transaction-matching/` expose bank IDs, hidden
tombstones and reconciliation without direct database setup. Manifest IDs map
only newly generated source reference IDs; existing transaction IDs remain
exact. Every archive digest and initial financial projection is checked. Source
generator `--check --export-seeds` re-executed the original14 outcomes. Frozen
date/rank ties in force-add may differ only by random UUID; its check now
canonicalizes those semantic ties while preserving meaningful date/rank order.
Original source JSON remains unchanged.

Current21 saved-option public/native test PASS
`/tmp/transaction-import-options-clock-native.log`: independently newer encoding
and older notes intent both survive, stale/equal same-field intents refuse
without mutation, mappings preserve source inOut semantics, complete native
backup retains exact per-key clocks and staged bytes, restored epoch rotates,
and the original old-epoch receipt still replays exactly. No source ZIP clocks
are fabricated. Source preference writes remain per-format independent cells.

Still open: selected-scope rematching/ignored rows, source file order for new
distinct purchases, duplicate same-bank-ID rows within one command; graph-aware
manual merge, CSV export, format commit/browser/native paths and complete UI
contracts/qualification. These remain incomplete acceptance gates.

## Selection and ordering edge checkpoint

New independently executed source/frontend fixture
`transaction-import-edges.json` and five genuine pre-import ZIPs cover exact file
ordering, repeated primary IDs, a matched split status update, selected-scope
rematching and unselected ignored rows. Generation and separate `--check` PASS
`/tmp/transaction-import-edges-source-check.log`. Only random UUID ties at equal
date/rank are canonicalized; distinct file order remains asserted.

Four real public reds are retained in `/tmp/transaction-import-edges-red.log`;
all five now PASS `/tmp/transaction-import-edges-green.log`. Existing matching14,
rule/payee5, current Rust import2 and bin Clippy also pass changed-scope checks.
No whole-suite qualification was repeated.

Captured `rows` narrows evaluation/matching before preview and commit. `auto`
uses the captured snapshot's selected-scope result; explicit `merge` requires
its captured match ID; `add` forces a new purchase; `skip` excludes a source row.
Ignored source rows remain included with `auto`/`merge` to consume their match;
explicitly adding one remains a visible force-add decision. Invalid excluded
rows do not invalidate a separately reviewed valid selection.

Repeated primary-ID proposals compare against one original state, then only
changed proposals coalesce to the final source update. Actual can alternate the
imported-payee text when a file repeats one bank ID with differing payee text;
both genuine after/repeated states are retained and matched. Original operation
receipt replay remains exact and never repeats that update. Existing split
status updates use the guarded aggregate writer, preserving allocations and
paired protections. New file ordering touches only the graph inserted by that
command, with its original HLC; existing private native/Android anchors remain.

Reviewer correction: choices are an unordered selection, never a reorder feature.
The writer now sorts validated row references by original source index before
coalescing proposals and assigning new-row order; receipt hashing retains the
original captured payload. Reversed equivalent choices produced both source-order
and duplicate-bank-ID true reds (`/tmp/transaction-choice-order-red.log`); five
edge workflows and bin Clippy now PASS (`/tmp/transaction-choice-order-green.log`,
`/tmp/transaction-choice-order-clippy.log`).

## Manual merge checkpoint

`check-transaction-merge-http.mjs` consumes all14 genuine pre-merge archives and
complete persisted source graphs. Initial route404 red is retained in
`/tmp/transaction-manual-merge-red.log`; first ordinary tracer and all14 financial
workflows PASS (`/tmp/transaction-manual-merge-tracer.log`,
`/tmp/transaction-manual-merge-first.log`). Every original keeper, child and peer
ID, field, status, opaque bank ID and graph relationship is compared; generated
identities are not substituted during this mutation. Preview writes nothing;
protected members require a captured complete-graph token; one owned action
undoes/redoes the whole merge and original receipts remain exact.

Two focused Rust guards PASS `/tmp/transaction-manual-merge-recovery-green.log`:
cancel fencing and receipt lookup precede stale snapshot checks, restart keeps
the original receipt, newer paired-peer edits refuse inverse without changing
financial rows/history/checkpoint/receipt, and a separately isolated new lock
generation refuses inverse. The missing receipt/cancel middleware scope produced
a real400 red before the allowlists were joined. An initial test assumed paired
undo must accept a newer peer note; the established conservative graph policy
was reaffirmed, not weakened. An intermediate statement-balance setup forgot the
peer's independent cleared flag; clearing every original row before the action
isolates the actual later lock generation. Neither was a claimed product defect.

An ordinary unaffected-note inverse remains allowed and is independently covered
by `/tmp/transaction-manual-merge-independent-note.log`. Bin Clippy PASS
`/tmp/transaction-manual-merge-clippy.log`. Complex source graphs that would leave
invalid split totals/status or nonreciprocal transfers refuse atomically with a
review message; they are never flattened or silently normalized. Existing
source IDs, ranks and original financial clocks remain untouched unless that
specific field is changed by the captured merge.

Reviewer proof delta: the newer-lock inverse refusal now also preserves the exact
post-lock history state. Affected test1 PASS
`/tmp/transaction-manual-merge-lock-history.log`; no runtime change or broad rerun.

## CSV export checkpoint

POST `/transactions/export` is read-only with a captured snapshot, optional
account, existing register query, selected IDs, and `standard` totals or default
`detailed` allocations layout. Predicate/regex work runs once across the whole
export, not once per page. ID/row hydration uses500-row batches; at most100000
rows and64MiB of output are accepted, with actionable narrowing errors. Existing
register predicates are reused rather than a second filter engine. Standard
selection includes selected transaction totals; detailed selection expands
selected parents into allocations and applies the filter to each exported row.

Initial genuine normal CSV byte tracer PASS
`/tmp/transaction-csv-export-tracer.log`. `transaction-export-source.json` plus
genuine source ZIP capture nine real serializer outputs: standard/detailed,
selected/filter/category/status/negative, empty, quotes/newlines, formula text
and split labels. Generation and independent `--check` PASS
`/tmp/transaction-export-source-check.log`. Native request operator metadata was
corrected from `is` to the existing register API's `eq`; original source CSV
outputs were retained.

One deliberate source bug correction: raw source cell `'\rTransfer note` is
emitted as `"'\rTransfer note"` (the `\r` denotes its original CR, not two text
characters). The source bytes remain in the fixture. The public consumer defines
that exact corrected cell independently, requires every other byte unchanged,
and uses the separate JS `csv-parse` reader to verify every row/cell and the
original note plus formula-protection prefix. Empty exports follow source's
empty string. All other displayed monetary strings use exact integer cents;
negative numeric amounts are never prefixed as formulas.

Nine matrix outputs plus the original normal tracer and readonly invalid-ID/
selection/bounds/stale/shared-regex guards PASS
`/tmp/transaction-csv-export-guards-green.log`. A real resource red initially
returned500 instead of the existing actionable regex-budget400; checking the
shared matcher failure before propagating SQL errors corrected that boundary.
No partial CSV or financial/checkpoint mutation escapes. Bin Clippy PASS
`/tmp/transaction-csv-export-clippy.log`.

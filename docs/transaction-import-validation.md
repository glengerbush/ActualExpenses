# Transaction imports — working evidence

Tickets25/26 remain incomplete. Accepted main: `4d7498c`. Working branch:
`codex/imports25-26`; implementation checkpoint `adc8a03`, reviewed source
evidence `d97f329` / `5ac7751`. No release, push or live deployment.

## Contract and ownership

One implementer owns the complete25/26 slice, its remaining public consumers,
UI, OpenAPI, conversion and tests. Root supplied independent source evidence and
owns consolidated review/integration/qualification. Review coherent completed
checkpoints; reuse unaffected evidence. Keep this record current rather than
repeating handoffs. Ticket45 runs separately on `codex/navigation45`; keep
App integration thin to simplify merging.

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
| `transaction-matching-source.json` |14 genuine matching/repeat scenarios | `check-transaction-matching-http.mjs`:8 CSV workflows pass; six bank-ID/deleted/locked scenarios pending |
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

- Rules/new payees and split/transfer writers joined to import application;
  safe graph protection, concurrent edits, exact owner/HLC/epoch receipts,
  cancellation, undo/redo and change-feed/Android behavior.
- Source bank metadata/deleted import choices, independently clocked saved
  options, source conversion, genuine20 DB/native upgrade and current native
  roundtrip; original staged-upload receipts/bytes remain immutable.
- Full manual merge graph preview/commit and CSV export (including filter/split,
  exact dates/amounts, quoting/formula handling), with source comparisons.
-26: OFX/QFX/QIF/CAMT parsing and options through same reviewed workflow;
  format-specific IDs/memo/date behavior, malformed inputs, saved settings,
  duplicate/deleted choices. Bank providers remain28–32.
- Complete Impeccable/Svelte UI: upload/map/review/select/merge/import/export,
  saved/backdated/error states, keyboard/focus, narrow/dark/200%, plain HTTP,
  durable lost-ACK recovery and resource bounds. No full offline budget engine.
- OpenAPI/capabilities and user documentation; meaningful source/public API,
  browser and Android verification. One consolidated review/qualification after
  implementation, resolving changed findings without broad repeated reruns.

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

Source4 public workflows PASS `/tmp/transaction-import-rules-green.log`; raw
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

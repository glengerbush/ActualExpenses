# YNAB migration — development evidence

Ticket27 is in progress on `codex/ynab27`, integration base `8a49adb` (schema22).
No live budgets, credentials, deployment, release or version changes. Root owns
consolidated review and qualification; this branch owns the complete migration
slice. Initial design reuses existing staged imports, original-byte retention,
provenance and atomic activation; no schema migration is currently needed.

## Public seams and first source tracer

Planned replacement seams: binary POST `/imports/ynab4?filename=...` and
`/imports/nynab?filename=...`, existing staged report/list/read and activation,
and a format-correct generic original-file download. Staging creates no active
budget; activation must be atomic and repeat-safe across independent keys,
restart and lost acknowledgements. The interface must retain the original file
and immutable upload/activation identities before sending. Tests use running
HTTP, actual browser workflows and the companion's real API adapter, not private
conversion helpers.

`tools/generate-ynab-source.mjs` executes `@actual-app/api@26.10.0` public
`importBudget` for the unchanged pinned desktop demo YNAB4 ZIP and nYNAB JSON.
Source commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`; importer/type file hashes
are recorded. Inputs, their exact digests and genuine post-import Actual ZIPs
are retained under `tests/fixtures/ynab/`; observations are `ynab-source.json`.
The source public accounts, groups, categories, payees, complete account ledgers,
account balances, selected historical budget months, schedules and registered
UI `tags-get` records supply expectations. No replacement parser, converter or
budget calculation supplies oracle values.

Generated UUIDs are labeled by public reference names and ordered ledger/child
positions; integer/fractional `sort_order` values alone are omitted, preserving
relative array order, financial multiplicity and reciprocal graph references.
JSON normalization preserves the actual wire representation (including numeric
negative zero becoming zero). Historical months outside Actual's available
budget horizon retain their actual `No budget exists` result, rather than a
fabricated zero allocation. Independent `--check` reimports both originals and
compares observations while verifying the committed original/export digests.

First source facts: the YNAB4 baseline imports accounts, categories, payees,
transactions and monthly allocations/carry settings; its importer does not
migrate YNAB4 scheduled transactions or edit/undo history. The nYNAB demo imports
15 schedules and6 flag-derived tags in addition to financial history. No source
edit log is represented as new native undo history, and YNAB entity versions are
not relabeled as HLCs. Supported history means the historical transactions,
month allocations and rollover state exposed by the baseline import workflow.

## Required source-model explanations

YNAB credit-card payment reserves and Ready to Assign differ from Actual's
ordinary account transfers and To Budget/hold model. Imports must preserve the
baseline financial records while explaining debt-category rollover, existing
allocation review and optional holds; they must not claim cross-product totals
or credit-card reserve semantics are identical. Duplicate category/group names,
hidden categories, split payee presentation, source goals and any unsupported
source configuration require explicit conversion/reporting. Original exports
remain available unchanged for review and cleanup.

## Remaining gates

- Source-backed public YNAB4 tracer, then complete nYNAB conversion, including
  wrappers, references, milliunit rounding, splits/transfers, flags/locations,
  schedules and historical allocations. Additional independent source fixtures
  answer concrete uncovered behaviors, not copied replacement formulas.
- Malformed/bounded ZIP/JSON parsing, latest-device selection, repeat/restart,
  atomic refusal, original bytes, native archive and companion projection.
- Served contracts, maintained CI/qualification consumers, durable source-picker
  UI, plain HTTP/browser recovery and responsive/privacy/keyboard renders.
- Consolidated review and one integrated qualification after the vertical slice
  is complete. Do not mark ticket27 accepted from the initial oracle alone.

## First YNAB4 public financial slice

The running-HTTP tracer consumes the independent YNAB4 observation and compares
all imported financial rows, account balances, multiplicity, parent/peer edges,
historical category allocations/carry and every selected budget summary field.
Staging creates no active budget, original bytes remain exact, upload receipts
repeat exactly, and repeated activation cannot reinstall history or advance the
budget checkpoint. Initial unavailable-route405 is retained in
`/tmp/ynab-http-first-red.log`; the built implementation passes
`/tmp/ynab4-checkpoint-final-http.log`. An intermediate consumer assumed the
budget view exposed categories directly; adapting it to the existing
`data.months[month]` contract corrected the harness, not financial expectations.
A stale-binary run after a failed compile is not used as checkpoint evidence.

The source adapter emits canonical rows into the existing validation/install
pipeline. A report-owned `source_format` discriminant avoids trusting arbitrary
Actual metadata fields. Source hashes, original IDs/mapping and entity versions
stay in retained provenance; YNAB versions are never minted into HLCs. ZIP paths,
duplicate names, encryption, device knowledge and expanded byte/row limits are
validated. Source-specific reference lookups are indexed. Shared Actual import
regressions12 pass `/tmp/ynab4-existing-imports.log` using a fresh local test
build. This is a financial checkpoint, not ticket acceptance: generated learned
defaults still need a source-backed join, and nYNAB, full failure/restart/archive
coverage, contracts and interface remain in progress.

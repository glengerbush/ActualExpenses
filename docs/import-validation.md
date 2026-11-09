# Actual ledger import — ticket04

Accepted through `8fe9b38`, against baseline `0b66dc6`. This qualifies the bounded,
disposable ordinary-ledger slice; it does not qualify a production migration or
cutover. Original published oracle fixtures/provenance are recorded in
[IMPORT.md](../actualBudget/tests/fixtures/IMPORT.md).

## Public behavior

`POST /api/v1/imports/actual` accepts the original ZIP bytes with an
`Idempotency-Key`. Its SQLite transaction retains the byte-identical archive,
SHA-256, immutable conversion audit, proposed UUID budget identity and durable
receipt. Identical archive hashes return the existing stage even under another
upload key. No budget or financial row is active at this point.

Review the source balances, source identities, counts, preferences, integrity
checks and explicit conversion requirements. `GET /api/v1/imports` lists the
latest 100 stages, including completed imports. Individual stages and original
archives remain retrievable by ID. `GET /api/v1/imports/{id}/original.zip` returns
unchanged bytes as a non-cacheable attachment.

`POST /api/v1/imports/{id}/activate`, body `{}`, activates only a ready stage.
Budget, reference/transaction rows, shared preferences, original mapped field
clocks, source identity mapping, derived totals and activation receipt commit in
one transaction. Target totals must equal the source audit before commit. A
retry after restart returns the original result; another activation key returns
the same budget. An existing budget at the reserved ID is never replaced.

The assigned API budget UUID is separate from source local slug, cloud file ID
and group ID, all retained in `imported_sources` and the public source audit.
Account/transaction/category/ordinary payee IDs are unchanged. Source NULL
positions retain their lowest ordering rank; signed zero positions remain equal.
Finite sort positions convert to integer ranks while preserving equality and
relative order. Register order follows pinned Actual: date descending, starting
balance last within a date, sort rank descending, then ID ascending. Reverse-order
balance validation and paginated running balances use the same ordering. Deleted
transactions remain tombstoned; they are not exposed in the active register.
Per-field source HLCs seed ticket05 without migration-time replacement; missing
field clocks retain its ZERO semantics. Original unknown history remains in the
ZIP and imported raw clock records. `resetClock` is not a request to reorder
existing edits. Invalid clocks are rejected; future clocks beyond five minutes
block activation and remain unchanged in the original.

Automatic transfer payees are explicitly retained in the archive and audit but
excluded from ordinary payee selection. A transaction using one requires the
transfer converter. Obsolete unused category aliases exist even in the public
baseline fixture; they are retained/reported. Every converted transaction must
resolve its references to actual source entities. This avoids treating obsolete
unused aliases as active dangling financial relationships.

## Limited supported input

The demonstrated source is the published unencrypted Actual 26.10.0 ordinary
ledger fixture. Source schema columns are pinned in
`server/src/imports/source-columns-26.10.json`, derived from that independent
export inspection. Extra/missing inspected columns block activation. Rules,
transfer transactions, schedules, bank/import metadata, goals, hidden or
deleted reference records and other unsupported populated feature tables require
explicit later converters. Their records remain in the original ZIP. No promise
of arbitrary existing-budget parity is made by this first importer.

Shared preferences use ticket43's converter. Original raw preference rows are
also in the public audit and archive. In particular, disabled experimental
currency settings use the baseline effective default while the original currency
code remains retained. Global/device preferences and theme files are absent from
ordinary Actual ZIP exports; this importer does not invent or promote them to
shared budget state. Ticket44 handles explicit device theme conversion separately.

Blocked stages cannot yet be reconverted automatically: later feature tickets
must add explicit re-review using the retained source and versioned converter.
Existing active financial data must never be replaced as a side effect.

## Resource and failure boundaries

- Upload: 16 MiB; expanded aggregate/entry: 64 MiB; entries: 128; inspected table rows:
  100000 each; metadata: 1 MiB. Limits are explicit in the chooser and errors.
- Reject ZIP64/multi-disk, ambiguous DB/metadata pairs, duplicate names (including
  exact duplicates collapsed by zip-rs's index), unsafe names, symlinks,
  encryption and CRC/decompression failures. Archive paths are never extracted.
- SQLite is copied to an anonymous temporary file and opened read-only, query-only
  with trusted schema disabled. Reject virtual tables and generated columns.
  Limit source SQL text to 1 MiB, cells to 16 MiB, columns to 256; a progress handler
  interrupts source SQL after ten seconds. Temporary storage must have room for
  the expanded DB; write/disk failures return an error without activation.
- Validation uses the existing bounded database worker. A failed staging or
  activation transaction leaves existing budgets and durable receipts intact.
  Malformed inputs are not saved as usable stages; the user's source file is
  never changed. Valid blocked sources remain downloadable.
- An aborted process relies on SQLite transaction recovery. Real restart and
  post-commit lost-ack recovery are exercised; no claim of deterministic
  mid-instruction process-kill or full-disk fault injection is made here.

## Browser recovery

Before transmitting an upload, IndexedDB stores the exact Blob and operation key.
Before activation, it stores the original activation key/body `{}`. Reload then
reopening **Import Actual budget** restores **Retry saved upload** or **Retry saved
activation**. Confirmed source stages can also be discovered from the server.
Quota failures prevent the request from being sent. Retry does not regenerate
payloads or identities. Stale tabs cannot overwrite another upload or clear an
unconfirmed activation. Definitively rejected corrupt archives return to the file
chooser; uncertain outcomes retain their original recovery payload.

## Evidence

- `cargo test --test imports`: 12 real HTTP workflows covering public oracle IDs,
  counts, balances, references/preferences, original bytes, inactive staging,
  activation/restart/replay, unsupported rule blocking, corrupt/encrypted input,
  dangling references, duplicate names/unknown schema, original clocks/tombstone,
  transfer payee exclusion, upload/expansion limits, future-clock blocking and
  nonreplacement of an existing budget, equal/fractional source ordering, pinned register/running-balance order and atomic money-boundary rejection.
- `node tests/imports.mjs`: real plain-HTTP browser recovery, quota failure,
  competing tabs, lost upload and activation acknowledgements, reload, imported
  register, blocked source download and corrupt-input recovery.
- Browser visual evidence in ignored `web/test-results/import-*.png`: desktop,
  390px blocked state and 320px at 200% text. Balances stack without clipping;
  activation/review buttons retain spacing. Screenshots inspected after the fix.
- Final full native suite: 50 public HTTP tests pass across concurrency, ledger,
  imports, preferences, snapshots and themes; Clippy all-targets with warnings
  denied passes. Svelte check, Svelte autofixer, Impeccable detector and owned
  frontend formatting pass. The full container/browser suite passed at `8fe9b38`,
  including populated-server import recovery, container replacement and exact
  replay. Independent Standards and Spec reviews have zero remaining findings.
  [Combined qualification](../actualBudget/tools/container-smoke.md).

Primary implementation references: [zip-rs 6 archive API](https://docs.rs/zip/6.0.0/zip/read/struct.ZipArchive.html),
[SQLite limits](https://www.sqlite.org/limits.html),
[SQLite trusted schema](https://www.sqlite.org/pragma.html#pragma_trusted_schema).

Ticket14 adds qualified split import, category allocations and native archive
round-trip evidence in [split validation](splits-validation.md). Full migration
qualification remains ticket50.

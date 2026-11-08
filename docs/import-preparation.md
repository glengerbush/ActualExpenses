# Actual archive import preparation

Ticket04 remains queued behind ticket03 acceptance. This is read-only research,
not an implemented importer or a migration acceptance claim.

## Observed source behavior

The assigned import worker exercised published `@actual-app/api@26.10.0` in a
disposable budget. `exportBudget()` returns a `Uint8Array` ZIP containing
`db.sqlite` and `metadata.json`; `importBudget(bytesOrPath, {type: 'actual'})`
returns `{id}`. Temporary data was removed; no user budget was accessed.

- Source `metadata.id` is a slug, while account/transaction IDs are UUIDs.
  Preserve source local/cloud/group identities explicitly. The replacement's
  UUID resource ID requires a documented mapping, not silent identity replacement.
- An ordinary fresh export already has dashboard/page defaults, category mappings
  and transfer payees. The inspected sample had twelve dashboard rows and one
  page. Rejecting every nonempty table without a current converter would reject
  ordinary exports. Retain and report these records for the relevant feature
  importers; reject activation of unsupported financial relationships.
- Archive export clears calculation caches and marks `resetClock`. That concerns
  new client identity; retain original per-field ordering, CRDT history and source
  `messages_pending`. The latter is not the Android expense outbox.
- The local-only API fixture had no `messages_crdt`. Generate ordering fixtures
  through the pinned disposable sync-server harness, rather than inventing clocks.
- Shared preferences are archive rows. Device/global-store settings are absent
  from ordinary ZIPs; treat separately supplied device preferences explicitly.
  Reuse ticket43's converter and retain its warnings/unmapped records.

## Proposed first public contract

Upload a pinned disposable ZIP and produce a staged conversion report: original
archive SHA-256, source identities, raw/active/tombstone counts, relationships,
independent Actual all-date financial totals, preference conversion and retained
records. No active budget exists until activation. Commit the converted rows,
derived totals and idempotency outcome atomically; retry/restart yields one budget.
Retain the source archive byte-for-byte for recovery.

Malformed/encrypted archives, duplicate identities, dangling references and
unsupported financial relationships cannot activate or change an existing budget.
Use bounded upload and streamed decompression; the pinned importer has 500 MiB
archive/entry/aggregate limits, which are upper bounds rather than target memory
allocations. Reject ambiguous paths/entries and never extract archive-controlled
paths into application storage. Inspect isolated SQLite read-only, query-only,
with untrusted schema execution disabled. The final API and limits will be chosen
against ticket04 and tested at its public boundary.

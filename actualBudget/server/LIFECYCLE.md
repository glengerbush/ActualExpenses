# Budget lifecycle and native recovery

The public API owns budget identity, active state and recovery points. Closing a budget only changes the device's selected view. Deletion removes active financial/settings data, while retaining its identity, recovery points, original import archives and operation receipts. `GET /budgets?include_deleted=true` and `GET /budgets/{id}?include_deleted=true` expose retained identities for recovery.

Rename, duplicate, manual backup, delete and restore require an immutable `Idempotency-Key`. Duplication creates a new budget UUID and epoch while copying supported records (including record UUIDs), settings and ordering metadata. Its future edits are independent. Original import files remain provenance; active legacy/cloud mappings and original budget operation scopes are not reused.

## Native archive version 1

A ZIP contains `manifest.json`, `budget.json` and content-addressed binary originals. The manifest identifies format version 1, schema version 6 and every entry's SHA-256/length. Limits apply to both creation and validation: 64 MiB compressed, 256 MiB expanded, 1,000 entries. Encrypted, duplicate, unknown, corrupt, unsupported-version and out-of-budget entries fail before installation. Every success from export satisfies the same structural limits as restore.

The explicit registry includes accounts, payees, categories, ordinary and hidden transactions, preferences, per-field clocks, original Actual archives and conversions, source identity metadata/raw clocks, duplicate provenance, and exact budget-scoped operation receipts. Derived balances and change indexes are rebuilt and checked. A new SQL feature table blocks export until its archive policy is registered; later tickets must extend the round-trip contract. Browser device settings and pending outboxes are device data, not server budget contents.

Upload `POST /restores` with a raw ZIP and stable operation key. Staging validates an isolated SQLite database and retains the original bytes without changing active budgets. Activation and local-point restore require the affected budget's current name. Existing active state first becomes a `before_restore` recovery point; deletion first creates `before_delete`. Recovery point, replacement data, new generation and operation receipt commit together in one SQLite transaction. Failure rolls back all of them. Original uploads and existing recovery points remain available.

Receipts are unioned, never replaced by an older snapshot. Identical identities must have identical payload/outcome bytes, otherwise restore rejects the archive. Schema-5 migration links existing receipts by verified HTTP resource scope or Actual import activation identity, not by guessed operation key. Pre-upgrade standalone SQLite files remain in the data directory's `backups/`; migration also creates usable budget recovery points from the preserved logical state before new user writes.

## Generation fence and pending clients

`GET /budgets/{id}/checkpoint` returns an atomic snapshot token, epoch and monotonic generation. Bootstrap reads for accounts, references and preferences accept a snapshot guard. A changed checkpoint returns `snapshot_changed`; reload the complete snapshot.

Capture `X-Actual-Budget-Epoch` when the user begins an intent, and persist it unchanged with the original body, key and edit clock. Receipt lookup runs first. Known old requests replay the original outcome exactly; clients then refresh current state, because that old outcome may describe a row absent after restore. Unknown requests with retired epochs return `budget_restored`; after the first restore, missing epochs are also rejected. Deleted budgets return `budget_deleted`. Restore always rotates epoch and increases generation; even a new-server archive installation starts above generation zero.

Never automatically rebase pending work. Show current state, retain original recovery evidence, and require an explicit new intent to reapply reviewed changes. Account/settings drafts preserve their captured epoch across reload; only a definitive retired/deleted-generation response permits archiving their fenced request. Their fresh reapplication receives a new operation identity. Network errors never authorize discard.

For lifecycle operations, check the `budget_lifecycle_cancellation` capability before sending `X-Actual-Cancel-If-Uncommitted: true` with the unchanged method/path/key/body/epoch. The server atomically returns the original receipt or records terminal `operation_canceled`. Persist cancellation intent through reload and lost requests, then retry cancellation until authoritative. The header is supported on budget rename, duplicate, backup creation, delete, point restore and staged activation. Transaction cancellation remains its separately advertised capability.

## Automatic recovery

Committed changes mark a durable deadline. The worker sleeps until the earliest deadline and wakes on writes; it does not poll idle budgets or continuously sync clients. Default interval: 900 seconds. `ACTUALBUDGET_BACKUP_INTERVAL_SECONDS` accepts 1–86400 for deployment policy and accelerated public integration tests. A process restart resumes persisted deadlines.

Retention follows pinned Actual's policy: at most three automatic points today, one per older server-local calendar day, ten automatic points total. Manual, pre-action and upgrade points are never automatically pruned. The new point and pruning commit atomically; an unsuccessful backup retains earlier points and dirty state. Failures expose `backup_error` when storage permits, log the failure, and retry after 60 seconds. Budget metadata includes nullable `auto_due`, `last_auto` and `backup_error`.

Changing system time can postpone wall-clock deadlines; no request clock is rewritten to compensate. A backup with field ordering too far ahead of server time remains preserved but cannot activate until time is corrected. Full-server disaster recovery still requires copying the entire data volume, including retained SQLite recovery originals.

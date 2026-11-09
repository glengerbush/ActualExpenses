# Account organization and closure

Ticket 09 uses the pinned Actual 26.10.0 account lifecycle and group APIs. `GET /accounts` returns live accounts, including closed accounts, with immutable on/off-budget classification, notes, group ID, ordering, balance and per-field clocks. Deleted account IDs remain reserved. Their explicit detail/change records carry `deleted: true`; they disappear from account selection and visible transaction history.

`PATCH /accounts/{id}` edits name, notes and group membership. `POST /accounts/{id}/move` accepts `before_id` (null means end) and optional `group_id`. Account groups support create, rename, move and delete. Deleting a group leaves its accounts ungrouped. Ordering uses exact integer ranks; imported fractional source order is preserved as ranks, with original archives and original edit clocks retained.

`POST /accounts/{id}/close` requires a zero balance. An account with visible transactions becomes closed with its history intact. An account with no visible transactions is tombstoned. Reopen restores the open flag. As in pinned Actual, close/forced deletion of an already-closed account is a no-op: reopen before deleting. Explicit account deletion requires `confirm_name`, tombstones visible transactions atomically and recalculates balances. Stale deletion cannot erase newer account or financial edits. Safe-integer overflow rolls back all effects. Transfer-based closure remains ticket 15.

All organization writes require immutable idempotency keys. Edit clocks participate in receipts and per-field conflict resolution. Epoch validation runs after receipt lookup. Browser account intents persist their original epoch, clock, key, body and method before transmission; uncertain outcomes remain retryable after reload. Retired intents need current-state review and an explicit fresh intent. Original recovery evidence remains stored locally. No account mutations enter the expense outbox.

Migration 7 adds account groups, group membership, ordering and tombstones. Native archive schema 7 registers these fields and tables; exact schema-6 archives upgrade in memory before validation. Original schema-6 receipts preserve their original response shape. New group rows and account fields also participate in change cursors, automatic backups, duplicate, deletion and restoration.

Actual imports convert account groups (including deleted groups), membership, order, notes, closed/deleted status, and corresponding source clocks. The existing `flags.newSidebarUI` preference conversion remains intact. Actual's `ui.showClosedAccounts` and `sidebar.accountsOpenState` are device-local preferences absent from its exported budget archive. This UI remembers its own visibility/collapse choices locally; it does not invent missing source device settings. Unsupported bank/reconciliation metadata remains explicitly blocking for its owning tickets.

Qualification lives at public seams:

- `tests/accounts.rs`: eight independently captured Actual lifecycle snapshots; closed-delete no-op; Android companion closed/reopened selection; exact imported notes/classification/balances and native archive defaults; ordering/conflicts/replay/restore fences; boundary failure rollback.
- `web/tests/accounts-browser.mjs`: plain-HTTP desktop/mobile organization, group CRUD/move, close/reopen/history/delete, lost committed response + reload/retry, generation review/reapplication, keyboard submit and 200% zoom width.
- `tools/contract-smoke.py`: served OpenAPI validation, including account/group CRUD and moves.
- `tools/generate-account-lifecycle.mjs --check`: repeat the published pinned API oracle independently.
- `tools/generate-import-fixture.mjs --accounts --check`: pinned public import/export roundtrip plus archived source schema/checksum/clock checks. The `--accounts` fixture is separate from the original import fixture.

The native schema-6 fixture was captured through the prior server's public creation/backup-download APIs against a disposable directory. Its companion JSON records the source binary hash, advertised capabilities and archive checksums. No primary data directory was opened or changed.

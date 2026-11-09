# Ordered edits and incremental refresh

New transaction clients persist `X-Actual-Edit-Clock` alongside the operation's
`Idempotency-Key`, method, path and body **before sending**. The clock is an Actual
HLC string: UTC ISO milliseconds, four uppercase hexadecimal counter digits and
sixteen hexadecimal node digits. Compare the complete string per field; counter
precedes node tie-breaking. One request clock applies to its supplied fields.
Independent fields can both win, even when an older operation arrives later.
PATCH responses include `conflicts` with field names and both clocks.

Same operation/key with changed clock conflicts, just like changed payloads.
Exact retries return their original response; never generate a new retry clock.
Headerless historical callers and durable pre-upgrade receipts remain supported.
The server assigns clocks to new headerless transaction operations and to new
opening-balance transactions inside the account creation/receipt transaction.
Existing historical rows without clocks use the deterministic epoch-zero floor.
Imported clocks are preserved exactly; `resetClock` does not rewrite field history.

## Clock failures and recovery

Clocks over five minutes ahead of server wall time are rejected; old clocks remain
valid. Counter overflow also rejects before mutation, receipt or change-cursor
advancement. Validation does not consume the operation identity. The browser keeps
an already prepared request unchanged after clock rejection. Correcting device
time cannot immediately make a far-future captured timestamp valid: the user may
retry unchanged when valid, or explicitly discard the confirmed rejected intent
and prepare a new one. Discard resets only the local generator; other pending
operations retain their captured clocks. An unsent form can explicitly use the
corrected device time without discarding its fields. No reset changes OS time.

## Deletion boundary

Before publishing a deletion, compare its original clock with every known field
clock. A newer edit rejects the deletion with HTTP409 `deletion_superseded` and the
retained row. This terminal rejection is durable and replayable. Look up an
existing receipt **before** checking deletion ordering: a lost acknowledgement of
an accepted deletion must not turn it back into an unpublished deletion.

Published tombstones remain hidden. Later winning ordinary fields still merge
into the hidden record, matching pinned Actual behavior, without resurrection or
balance effects. The mutation response has `data.deleted=true`; the browser
explains that the entry remains deleted. Explicit restore is a separate feature.

## Change cursors

Register pages return an atomic `snapshot` with their rows and balances. Use it as
`GET /api/v1/budgets/{budget}/changes?cursor=...&limit=100`. Results contain changed
transactions, retained tombstones, accounts, payees, categories and preferences,
plus `next_cursor` and `has_more`. Only advance to returned cursors. Repeat while
`has_more`; an empty response keeps the checkpoint and writes nothing.

The opaque cursor identifies a budget epoch and a durable server sequence. It is
independent of all client edit timestamps. The change index coalesces each entity
to its latest sequence; a concurrent update moves that entity later so a paged
consumer still receives its current state. Cursor checks and returned data share
a read transaction. Other budgets do not invalidate a register snapshot.

Keep outcomes, field versions, change index entries and tombstones indefinitely
until an explicit acknowledgement/retention protocol exists. Wrong-budget,
wrong-epoch and future checkpoints require a fresh snapshot; never silently skip
history. Any future backup-restore path must rotate budget epochs before clients
resume (ticket49); copying a backup is not a client-cursor recovery protocol.
The browser refreshes explicitly, checks changes first, and reloads its bounded
visible register only when data changed. It runs no recurring background refresh.

## Android migration boundary

Ticket05 defines the shared API ordering contract. Existing Android outbox
migration is owned by tickets06/20. Legacy queued fields can have different
original HLCs: preserve them with stable per-field child operations or an explicit
atomic batch contract. Do **not** promote every field to the maximum clock.
Preserve original identities and evidence of uncertain acknowledgements before
switching transport. This ticket does not claim the legacy outbox migration is
implemented.

## Evidence

`tests/concurrency.rs` exercises the real server: per-field merge, exact retry and
changed-clock conflict, rejected/published deletion, late older arrival, cursor
coalescing during pagination, budget isolation, invalid/future clocks, overflow,
new opening ordering, and exact genuine schema3 receipt replay after upgrade.
It consumes the independently generated pinned Actual concurrency fixture.

`web/tests/concurrency-browser.mjs` uses two independent browser contexts against
the server, with interrupted PATCH/DELETE and original-clock replay, conflict
notices, an unchanged one-request refresh, hidden edits, and explicit transaction
and action clock recovery. Existing register/date-recovery cases remain required.
Root owns final combined container qualification and independent review evidence.

The root-owned `tools/change-refresh-scaling.py` probe measured 100 unchanged
refreshes before/after 5,000 transactions: median 1.534→1.546 ms, p95 1.662→1.810 ms,
95-byte responses in both phases, and zero process storage-write bytes. CPU was
100/90 ms and RSS 12,644,352/15,020,032 bytes. These are local debug observations;
Ticket 51 owns broader production/baseline resource and power comparison.

# Ticket 13 — bulk changes and undo/redo validation

Private implementation on `codex/bulk13`, base `8a941e9`. No production data used. Root composes reconciliation/transfer/large-split dependencies and independently reviews before ticket acceptance.

## Independent baseline evidence

Reference is Actual 26.10.0, pinned commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577` under `actualBudget/reference/actual-26.10.0`. `web/tests/generate-history-fixture.mjs` executes transpiled pinned `packages/loot-core/src/server/undo.ts`; only storage/context adapters are substituted. Generated `tests/fixtures/history-26.10.0.json` records source SHA and independent expected financial values, 20-group retention, sequential undo/redo and redo truncation. The same generator executes pinned `applyFindReplace` JavaScript for literal, captures, named captures, replacement substitutions, surrogate-pair, zero-width and empty-find cases. `--check` passed.

Supported boundaries were inspected in baseline transaction/account/payee/tag handler registration and internal calls. Account `closeAccount` in `server/accounts/app.ts:611` wraps its local financial changes with `withUndo` although the outer method lacks `undoable`; forced deletion participates. Remote unlinking occurs before that wrapper. Nonfinancial presentation preferences are not advertised as supported financial undo.

Intentional correction: pinned `shared/transactions.ts:97–110` `recalculateSplit` retains an invalid total and marks an error; update paths at lines 269–306 call that function. Replacement instead rejects invalid final aggregate sums atomically. This does not prohibit valid multi-child changes: changing -2000/-3000 to -2500/-2500 in one request passes, with parent total -5000. Invalid regex now gives an explicit atomic refusal instead of silently doing nothing.

## Public seam coverage

`actualBudget/server/tests/bulk_history.rs` exercises the running HTTP binary with isolated temporary data, using `ACTUALBUDGET_BINARY` when supplied. Tests cover ordinary bulk grouping/replay, newer unrelated notes, same-value overlapping edits, remote deletion, overflow rollback, sequential undo/redo, pinned retention/branching, owner isolation, split creation/replacement/duplication/deletion, child selection and notes modes, inline reference lifetime, metadata safety, restart, receipt identity binding, cancellation fencing, native archive export/restore and excluded history, and companion read compatibility. Account close/forced-delete undo was added after a failing public assertion exposed the missed baseline boundary.

Browser `web/tests/bulk-history-browser.mjs` runs Chromium against plain HTTP `actualbudget.test`, a fresh server and isolated data. It covers Space/Shift selection, bulk update, keyboard undo, reload redo, committed response loss, exact duplicate retry after reload, quota-failed retry and cancellation with zero sends, recovered cancellation, child amounts/append/find-replace, simultaneous first-tab owner initialization and cross-tab reload undo. It asserts confirmed API rows/balances, request identities and absence of page errors. Mobile and 200% text assert no document overflow.

Screenshots generated at `actualBudget/web/test-results/bulk-history/{desktop,dark,mobile,text-200,quota-failure}.png` and visually inspected. Register uses existing design tokens and in-place controls; selection and saved recovery remain visible, keyboard controls work outside text inputs, narrow containers switch table rows to stacked layouts. Impeccable detector returned `[]`. Mandatory Svelte autofixer returned no issues; retained suggestions are transient local Set construction, existing persistence effects and component-instance bindings (not reactive Set state).

## Commands and results

- `CARGO_BUILD_JOBS=2 cargo build --locked` and public `bulk_history` target; final result recorded below.
- Existing Rust integration targets passed with test threads capped at 2: accounts, budget_management, categories, categories_schema7, companion, concurrency, http, imports, lifecycle_cancellation, payees_schema8, payees_tags, preferences, register_integration, register_snapshot, splits, themes. One existing long-category-merge test hit its five-second client timeout under concurrent host load; isolated serial rerun passed (5.35 seconds total). No product failure reproduced.
- `npm run check`: zero errors/warnings. `npm run build`: passed; existing >500 KB bundle advisory remains.
- New browser workflow and pinned fixture check passed. Existing register CRUD/calculator/drafts and register-recovery suites passed. Remaining targeted regression results recorded below.

## Integration review requirements

Server contract: `actualBudget/server/BULK-HISTORY.md`; machine contract: `actualBudget/server/src/openapi.json`. History storage is auxiliary and excluded from native archives/clone/import; original receipts remain replayable. Migration number 012 is provisional here; root plans history15 after reconciliation12/transfer13/locations14. No Android bulk interface added.

Combined acceptance still requires root's transfer/reconciliation/large-split join: expand both transfer aggregates and guard reciprocal links, validate transfers after inverse, wrap reconciliation as owned financial action, preserve exact original reconciliation/membership proof, and add sparse `Change.changes` compatibility. Test create/update/delete/undo/redo through those combined public APIs and browser flows. Root also coordinates independent code/spec and Impeccable finish/documenter review. This evidence does not claim those private branches were merged here.

Final private results: 11/11 HTTP bulk/history tests passed; `cargo clippy --locked --all-targets -- -D warnings` passed; format/diff checks passed. Browser bulk/history, register, register-recovery, concurrency, accounts, categories, payees/tags, metadata clock recovery, register settings and foundational creation/recovery suites passed. Existing request-count assertion now excludes auxiliary history reads, preserving its cursor-only register check. Existing split lost-response proxy forwards and compares the captured history owner; assertions wait for editor closure rather than previously visible unchanged register text.

Final split browser regression passed, including immutable retry, stale-tab ownership, unsplit protection, retained removed-payee references and mobile layouts. Final changed-file Prettier check passed. The verified temporary node_modules symlink was removed before freeze; screenshot artifacts remain available locally for root review.

## Financial composition — schema15, 2026-11-09

Composed bulk/history checkpoint `f9e53f0` with sparse/protected transfer graph
checkpoint `936ec6b`. Reconciliation12, transfer13 and locations14 precede
history15. Transfer create/link and reconciliation/unlock now use the same owned
receipt wrapper as ordinary/split mutations. Browser forms capture the durable
engine owner before saving a new pending request; saved legacy requests retain
their original absent owner. Exact retry/cancel/receipt identity stays immutable.

Journal evidence captures the union of complete connected before/after active
reciprocal-transfer/split components once per action. Exact row identities, all
field versions and separate raw reconciliation generations guard inverses.
Only explicitly captured IDs authorize protected inverse touches. A newer peer
field, relationship, membership or lock refuses all writes and leaves the action
cursor unchanged. Own unlock followed by undo cannot alias away a newer lock
generation to authorize an earlier action. After a successful inverse, guards
refresh for that same action's next navigation. Transfers are validated after
the complete inverse. Ordinary field-only edits retain their existing unrelated
newer-note merge behavior. Sparse expansion preserves untouched transfer fields.

Public red/green: transfer creation initially failed the missing-history assertion
(`/tmp/history-financial-red.log`); final creation/undo/redo and reconciliation
lock/undo/redo pass. Both-leg selected deletion initially returned409
(`/tmp/history-both-selected-red.log`); the exact pinned desktop message oracle
`generate-bulk-transfer-pairs.mjs` shows two selected duplicate legs create TWO
fresh pairs, while deleting both original legs succeeds. The rewrite preserves
that duplication outcome and skips only roots removed by an earlier leg in the
same deletion transaction, after every original selection was read live. Public
financial assertions compare the executed fixture, then undo deletion.

`cargo test --manifest-path actualBudget/server/Cargo.toml --test bulk_history`:
18 pass (`/tmp/history-composed-final.log`), including confirmed protected pair
undo/redo, remote peer-note/relock and own-unlock-inverse refusal,1001-child
sparse edit/undo/redo/exact receipt replay, and genuine schema14 upgrade/archive.
The schema14 fixtures were generated through immutable main `e9d4725` executable
using public endpoints, with executable SHA256 recorded in fixture metadata.
Original transfer/reconciliation/location receipts, ready source, protected rows
and native archive survive; engine history remains excluded from portability.

Large debug qualification uses a bounded30s timeout matching the existing
1001-member transfer harness; no financial assertions were removed. Profiling
found repeated graph/lineage work: adjacency/components now build once per action,
lineage lookup batches per row, and row/version/seed statements are cached.
Measured one debug1001 sparse POST at3.24s (operation1.10s, capture2.14s); inverse
redo crossed the former generic5s client timeout. The final18-case run took16.32s
total. These measurements are host/load dependent and are not product latency
claims. A lightweight budget edge scan remains linear per owned action.
Lineage storage still retains old inverse aliases; pruning/bounding those aliases
needs separate qualification and is not claimed here.

Paired-protection, reconciliation and bulk/history browser workflows pass
(`/tmp/history-{paired,reconciliation,bulk}-browser.log`), including owner capture
and identical owner on lost acknowledgement/reload/retry, and public history
availability. Svelte check/build, autofixers, Clippy, served OpenAPI and detector
pass (`/tmp/history-composed-*`). Existing bulk Impeccable finish review: ship;
documenter found no changes, after five composed captures and two detail crops.
Incumbent Operate design retained, no new shipping rasters. Root owns independent
delta review and the stable-batch full suite/container qualification.

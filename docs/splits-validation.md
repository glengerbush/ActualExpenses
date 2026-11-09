# Ticket 14 — split transactions

Accepted2026-11-09 through `4796e11`. [Final integrated qualification](1314151646-validation.md) resolves the historical integration/review gates recorded below.

Implementation worktree baseline: `8ad9315`. Pinned Actual: **26.10.0**, commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Synthetic data only; no release,
production import or live-data access. Root integration/review remains required.

## Behavior and independent evidence

- `shared/transactions.ts` supplies child inheritance, parent editing and
  `makeAsNonChildTransactions` behavior. Children retain IDs, amount, payee,
  category and notes; account/date/cleared status follows the parent. Shared
  payees follow a changed parent payee. Reconciled records require unlocking.
- `TransactionsTable.tsx:onDistributeRemainder` is executed directly by
  `web/tests/generate-split-fixture.mjs`. Seven positive, negative, empty and
  mixed-sign examples are checked into `tests/fixtures/split-allocation-26.10.0.json`.
  Public Rust tests compare server results with this independent output.
- `tools/generate-import-fixture.mjs --splits` creates a real public Actual
  export and independently reimports it through `@actual-app/api@26.10.0`.
  The checked-in ZIP/JSON preserve IDs, HLC history, categories, child notes,
  register records and public AQL balances. `--splits --check` reexecutes the oracle.
- `server/tests/splits.rs` uses real HTTP: exact/even/proportional allocation,
  rollback on invalid sums, stale snapshots, replay, cancellation fences,
  duplicate children, parent deletion cascade, child-delete protection,
  selected/all-child unsplitting, recombination, companion recent/retained
  exclusion, pinned import, category allocations and native archive restore.

POST `/api/v1/budgets/{budget}/transactions/{id}/splits` writes one aggregate
with a stable snapshot, key, clock and budget epoch. `replace` tombstones removed
children; `recombine` retains the parent total/ID; `unsplit` detaches allocations
and implements Actual's single-child collapse. Parent rows count once in account
and running balances. Category drilldowns count ordinary rows and children,
excluding parents. GET on the same path reads the parent, children, snapshot and
epoch atomically. A stale snapshot can refresh without discarding entered values
only when the transaction and its children have not changed.

The API rejects non-exact saved allocations rather than persisting a temporary
split error. Zero-sum proportional weights produce an explicit correction error.
Transfers and other not-yet-supported import features remain activation guards.
Children are exposed through `children` and their individual transaction IDs;
ordinary transaction response shape remains backward compatible.

## Recovery and interfaces

`SplitEditor.svelte` extends the incumbent Operate register with native controls,
keyboard focus, labeled child fields, explicit retry/cancel, and editable saved
drafts. IndexedDB compare-and-write owns split drafts across tabs, including
plain HTTP. An uncertain request retains its original serialized operation,
key, clock and epoch through reload. Split writes require connectivity and never
enter the ordinary-expense background outbox.

`web/tests/splits-browser.mjs` verifies split/recombine, exact lost-ack retry after
reload, stale-tab refusal/recovery, unrelated-change snapshot refresh, keyboard
navigation and 1280/390px layouts. Existing register CRUD, register recovery and
category browser regressions pass. Impeccable detector ran once: no findings.
Svelte autofixer reports no issues; its remaining `bind:this` suggestion is an
advisory for the explicitly focused editor heading. A final replay rerun found a cached-register reopen race; new drafts now read
the current aggregate atomically and keep ownership until refresh completes.
The regression passed after the fix. Parent finish review is still required. The established PRODUCT/DESIGN direction was inherited, with routine
choices authorized by the task; no new visual identity was proposed.

Android excludes both parent and child records. If an ordinary pending edit was
converted remotely, its original operation remains durable. An explicit recovery
button discards only a confirmed `split_transaction` refusal for that exact
original request. A native public-API test verifies identity retention across
restart, no financial mutation, exclusion after recovery and successful resume.
The Compose recovery-screen test passes on isolated API 26: phone, dark phone at
1.3 font scale, and tablet. Screenshots were inspected; platform controls, 48dp
recovery action, readable wrapping and the existing tablet width limit remain.
No physical-device, API36 or TalkBack qualification is claimed.

Screenshots: [desktop](screenshots/splits14/desktop.png),
[mobile](screenshots/splits14/mobile.png),
[Android phone](screenshots/splits14/android-phone.png),
[dark/larger text](screenshots/splits14/android-dark-large.png),
[tablet](screenshots/splits14/android-tablet.png).

## Checks

- `cargo test -- --test-threads=1`: full existing Rust suite passed before review.
  Review run completed 87 tests across the full invocation plus focused retries:
  the added imported/restored test initially omitted the required epoch; corrected
  setup passes. The existing long-category restore hit its old 5s harness timeout
  under load; root's scoped 30s restore timeout passes unchanged assertions.
  `rust-full.log`, `rust-green.log`, `category-final.log`, `themes-final.log` record
  coverage; the intermediate timeout is retained in `rust-full-final.log`.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`: pass.
- Web `npm run check`, build, formatter, both changed-component autofixers: pass.
- Pinned allocation generator and public import fixture checks: pass.
- Native `:core:test --tests '*NativeApiIntegrationTest*'`: 19 public integration
  tests pass. App and Android-test Kotlin compile; new Compose recovery test passes.
- First full parallel Rust run exposed an additive ordinary-row response change;
  fixed by retaining the previous ordinary response shape. A concurrent long
  category test timed out; serial full-suite rerun passed without changing it.

## Review corrections

- Duplicated child versions use the original duplicate request clock, not server
  wall time. A January duplicate remains deletable by a later January request;
  retry returns the identical child IDs and versions.
- Ordinary field merges on tombstoned split parents/children remain ordered and
  hidden. Active split PATCH refusal remains unchanged; later hidden notes never
  restore register rows or balances.
- Moving existing split allocations off budget preserves latent categories.
  Subsequent edits retain them; selecting a different non-null category is
  rejected atomically. Public coverage includes pinned Actual import followed
  by native archive restoration with the restored budget epoch.
- Individual/all-child unsplit actions refuse unsaved child edits before making
  a request. Notes, amount, payee and category values survive the refusal and
  reload. The error instructs saving edits first; the immutable pending-request
  retry/cancel path remains unchanged.

Review regression logs are under `.scratch/ticket14-review/`: `rust-red.log`
(three intended failures), `browser-red.log` (unsplit discarded the draft),
`rust-green.log` (eight passing split tests), `splits-browser-final.log` (focused
browser checks plus error captures), `web-check.log`, `web-build.log`, `web-format.log`, `clippy.log`,
`rust-format.log`, and `svelte-autofixer.log`. The autofixer has no issues; its
existing heading-focus `bind:this` advisory remains.

All web browser suites passed across the full invocation and continuation.
`browser-full.log` records passing oracle/predecessor suites and the original
ordinary-expense recovery assertion timeout at concurrency-browser line 482.
Its focused retry passed. Root's exact `c01a749` lease-aware test timeout change
is copied; the in-flight retry passed even with the old stricter timeout.
`concurrency-browser-final.log` and each subsequent `*-browser-final.log` record
receipt feedback, themes, offline/epoch/edit-race/edit-revert, lifecycle, budget
epoch, accounts, categories and splits. Root's exact `d6a5005` category restore
harness change is copied too. Neither harness adjustment changes product code
or weakens financial/replay assertions. Temporary fixture dependency link was
removed after validation.

Desktop/mobile error captures were inspected: [desktop](screenshots/splits14/unsplit-error-desktop.png),
[mobile](screenshots/splits14/unsplit-error-mobile.png).

## Integration requirements (not yet qualified here)

Root integrated this candidate after payee11 as **schema10**, preserving schema6–9
archive conversion and payee metadata. Register12's new `ledger::get_many`/batch hydrator must attach
children; both its fast and filtered paths must exclude child rows from register
counts and balances, use `is_parent` for the parent predicate, and use child
matching for allocation category/payee/tag searches without double counting.
Those modules do not exist at this worktree baseline and were coordinated with
register12, not edited across worktrees.

Ticket13 owns undo/redo and is not implemented at this baseline. Aggregate split
mutations, deleted children and parent snapshots must join that history before
claiming the applicable undo acceptance criterion. Receipts are replay safety,
not undo. Ticket14 should not be marked completely accepted until these integration
checks and root's independent review pass.

## Root integration evidence

Candidate `560ce00` follows main `20b6eb9`; schema10 preserves both payee/tag and
split fields in transaction hydration, imports, archives and exact old receipts.
Merged child payees now resolve aliases and obey removed/transfer selection
rules. Existing removed references survive unrelated split edits; the editor
shows an explicit removed-payee choice. Duplicated children retain imported text.

101 HTTP tests are covered by `.scratch/14-integrated-public.log` plus the final
9-case `.scratch/14-payee-integration-green.log`. The initial combined run caught
the new alias regression; its red evidence remains in
`.scratch/14-payee-integration-red.log`. Integrated split browser checks pass,
including removed-reference preservation/reload and unsplit; desktop/mobile
renders inspected (`.scratch/ticket14-visuals/removed-payee-*.png`). An initial
browser assertion counted every removed-payee option; it now targets the actual
first-child selector. No application assertion was weakened.

22 native public-API tests pass, zero skips; debug and instrumentation APKs plus
lint build successfully (`/tmp/native14-integrated.log`). This integration did
not repeat the unchanged device flow already qualified above. Clippy, rustfmt,
Svelte check/build/autofixer and served OpenAPI pass. Integrated independent
review, register12 and undo13 gates remain; ticket14 is not yet accepted.

Full production container qualification at `c69d215` passed all browser suites,
read-only receipt misses/replay and persistence after container replacement:
`.scratch/14-container-final.log`. Image
`sha256:b67be3b87af784bbbbd8b592131975eea2c6eb9d0dbf2aabaaaa3b5896e3c7cd`,
128,449,658 bytes, runtime user10001:10001; disposable resources removed.

Integrated Spec review found one additional payee inheritance defect: a removed
parent payee compared as display-null, so changing it reassigned blank children
instead of children retaining that payee. Root now compares retained canonical
identities before inheritance. Public regressions cover removed→active,
removed→null and alias selections. Ten split tests and Clippy pass; logs
`.scratch/14-parent-payee-{red,green,clippy}.log`. Focused integrated split browser
passes again (`.scratch/14-parent-payee-browser.log`). Total API coverage is now102.
Corrective independent review remains pending.

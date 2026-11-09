# Ticket15 validation and handoff

Isolated tree `ticket15`, base570806f; candidate committed by root as `a3d9508`.
No production data, deploys or APK
identity/signing changes. Skills: implement/TDD, Impeccable, Svelte writer/core.
Public API/browser/native seams were authorized by the parent task. Existing
Operate surface extended inline; delegated brief supplied purpose and constraints.
PRODUCT/DESIGN remain authoritative and unchanged. No generated artwork.

## Independent evidence

- `actualBudget/tools/generate-transfers.mjs`: published `@actual-app/api@26.10.0`,
  pinned commit2bebdbaae0b701120a8cad9d56aafd64fa04c577. Five settled public snapshots,
  genuine source ZIP, public account-close results. Generated IDs use aliases;
  close results omit only nondeterministic IDs/current-day dates. Financial values
  remain unmodified. Polling waits for observable asynchronous transfer propagation.
- `generate-split-transfers.mjs`: genuine split-transfer source ZIP plus four public
  snapshots. Records the pinned parent/child-total defect; see TRANSFERS.md gate.
  Both generators pass `--check` without rewriting source ZIPs.
- `generate-import-fixture.mjs --transfers`: genuine synced Actual fixture, public
  API financial round-trip and original source HLCs. `--transfers --check` passes
  (`/tmp/ticket15-synced-check.log`); Rust verifies exact mapped transfer clocks,
  original ZIP bytes, IDs, balances and shared date preference. Root must merge
  this flag branch with ticket14’s `--splits` branch.
- `server/tests/transfers.rs`: real Rust HTTP create/retry, pinned comparisons,
  per-field sharing, independent/synced dates, deletion fence/restart, rollback,
  cancellation, incremental payees, link/duplicate, close/forced delete,
  genuine Actual import, original-source retention and native backup/restore.
  Tracer failed HTTP404 before implementation; import tracer was blocked before
  its converter. Logs `/tmp/ticket15-red.log`, `/tmp/ticket15-import-red.log`.
- `web/tests/transfers-browser.mjs`: Chromium plain HTTP, create, real committed
  lost acknowledgement and reload replay preserving body/key/clock/epoch,
  linked-date edit, paired deletion, existing-entry link, balance closure,
  1280/390px layouts and keyboard navigation.
- Native `NativeApiIntegrationTest`: a real transfer mutation arrives through
  incremental changes, never merchant choices or expense lists; retained exclusion,
  process recreation and subsequent merchant expense creation verified.

## Render evidence

`docs/screenshots/transfers/desktop.png` and `mobile.png`. Incumbent spacing,
colors, typography, focus ring, DateInput and check-label retained. Input labels
are explicit. Form/requests retain values on failure; pending fields freeze;
source account balance is captured for closing. Offline operations disabled.
Impeccable detector: empty (`/tmp/ticket15-impeccable.json`). Svelte autofixers:
no issues/suggestions on TransferForm and Register. Independent finish/documenter
and Standards/Spec review are root-owned, still required before acceptance.

## Validation status

Focused transfer HTTP6 passed (`/tmp/ticket15-transfer-release.log`); historical payee/category regression25 passed after
copying root's12bec4c schema-default expectations. Full Rust suite ran96 tests:
93 initially passed, three old-response-shape tests failed; those failed domains
and all transfer tests passed after correction. Ordinary rows retain historical
shape by omitting absent transfer_id and not inventing transfer clocks.
Typecheck/build passed (`/tmp/ticket15-check-final.log`,
`/tmp/ticket15-build-final.log`). Full incumbent browser suite25 passed
(`/tmp/ticket15-full-browser.log`); extended transfer browser separately passed
(`/tmp/ticket15-browser-finish.log`). Full native API suite19 passed, zero skips
(`/tmp/ticket15-native-all.log`); root’s newer cases must remain in the union.
Served OpenAPI existing and new transfer mutations passed
(`/tmp/ticket15-contract.log`). Broad suites were not repeated after green.

Dependency-only changes copied from root: payee projection exclusion in
ApiExpenseService; accounts cancellation GET/payees test; category/schema7
comparison normalization. Root must preserve its newer native test union.

## Acceptance gates

Ticket12 query/hydration integration, ticket14 split-transfer composition,
ticket13 atomic undo, ticket16 reconciliation guards, final schema renumbering,
combined full validation and independent review. Ordinary transfer implementation
is reviewable; this document does not claim full rewrite or whole-ticket acceptance.

Root merged main `c69d215` (split schema10) into this isolated tree, renumbering
transfers to provisional schema11. Main remains unchanged by transfer work.
Both import generator modes survive; archive defaults compose schema6–10;
transaction hydration and native exclusion retain split and transfer fields.
Shared register actions disable while a split editor owns the draft.
Focused merge checks: 9 split +6 transfer public tests,23 native API tests,
served OpenAPI, Rust check/fmt, Svelte check/autofixer pass. Evidence:
`/tmp/merge14-15-{tests,contract,webcheck2,autofixer}.log`,
`/tmp/native14-15-merged.log`. Combined split-transfer financial behavior still
requires implementation, tests and review; the merge alone does not satisfy it.

## Combined split/transfer candidate on efa600f

The formerly pending ticket14 composition is implemented in
`server/src/transfers/composition.rs` and the split receipt boundary. Split child
creation/replacement owns stable counterpart IDs, paired amount/notes, categories,
account destinations and clocks. Opposite-side amount edits update parent totals
by delta; synchronized dates traverse linked split aggregates. Parent-to-transfer
and transfer-to-parent operations are refused. Ordinary-side deletion retains an
unlinked child allocation. Replacement, child removal, recombination, unsplitting
(including one-child promotion), duplication and parent deletion maintain pairs.
A stale parent deletion checks independent peer changes and returns a durable
refusal. The pinned parent-total defect remains recorded, not copied.

The split editor exposes a destination per child, restores it on reload, freezes
saved request details and allows existing ordinary entries to link to nested
children. The transfer picker excludes parents. Parent link and transfer split
buttons are absent. Rendered 1280/390px layouts preserve the incumbent controls,
focus styling and responsive grid. New captures: `screenshots/transfers/split-desktop.png`
and `split-mobile.png`. The native recovery copy explicitly covers transfers;
`screenshots/transfers/native-split-transfer-recovery.png` captures the actual
API36 Compose surface. No signing, application ID or version changes.

The new companion `X-Actual-Expense-Only` precondition protects queued edits and
deletes even when their original clock is newer than remote conversion. Rejected
work retains body/key/clock/epoch through process recreation and can be discarded
only after read-only receipt confirmation. Generic register pair editing remains
available. The native client requires the new protection capability from servers
advertising transfers. This closes a race that incremental eligibility alone could
not protect. Existing serialized recovery fields keep their names for compatibility.

Evidence after composition:

- Red public child-transfer tracer: `/tmp/ticket15-combined-red.log` (HTTP400 before
  the split schema accepted counterpart fields).
- Full Rust suite: **112 pass**, zero failures/skips,
  `/tmp/ticket15-combined-full-rust.log`. Includes ten transfer and ten split cases.
  Root's `3bef1aa` retained-payee inheritance fix/test is included unchanged in meaning.
- Final focused transfer suite: **11 pass** (`/tmp/ticket15-combined-transfers-final.log`),
  including a later historical-edit check after the counterpart account closes
  with a balance transfer. Existing paired history remains editable; new
  destinations still require open accounts. Full suite112 preceded this focused
  addition; the broad suite was not repeated.
- Genuine combined pinned source import, original ZIP retention, companion
  recent/retained exclusions and native archive restore:
  `/tmp/ticket15-combined-import.log`.
- Browser: combined child creation, lost committed acknowledgement/reload/replay,
  editing, linking existing entry to nested child, opposite-parent delta and
  responsive keyboard flow: `/tmp/ticket15-combined-browser.log`. Existing split
  and transfer suites: `/tmp/ticket15-combined-splits-browser.log`,
  `/tmp/ticket15-combined-transfers-browser.log`. Split keyboard assertion now
  includes the new destination selector between amount and payee.
- Native API: **24 pass**, zero skips/failures, including four conversion-race
  combinations (edit/delete × queued before/after conversion),
  `/tmp/ticket15-combined-native-build.log`. Same invocation builds debug APK,
  instrumentation APK and lint successfully.
- Focused actual API36 recovery rendering/action: **1 pass**, zero skips,
  `/tmp/ticket15-combined-device.log`. Prior disposable AVD was copied to
  `/tmp/ticket15-avd`; primary AVD untouched, temporary emulator stopped.
- Served OpenAPI ordinary + combined transfer schemas and expense-kind refusal:
  `/tmp/ticket15-combined-contract.log`.
- Svelte check/build, autofixers and Clippy: `/tmp/ticket15-combined-check.log`,
  `/tmp/ticket15-combined-build.log`, `/tmp/ticket15-combined-svelte-*.log`,
  `/tmp/ticket15-combined-clippy.log`. Autofixers find no issues; SplitEditor keeps
  the existing heading `bind:this` advisory. Impeccable detector already ran once
  this session; manual craft-floor and rendered review cover the new controls.

Still required: independent Standards/Spec and Impeccable finish review; register12
hydration/query integration; ticket13 atomic undo; ticket16 exact lock-version
confirmation for every affected leg/aggregate; final schema renumbering. Provisional
schema11 is unchanged by composition. This is not whole-ticket or rewrite acceptance.

## Independent review corrections on ca040e5

Two confirmed P1s reproduced through public seams, then corrected:

- `/tmp/ticket15-hidden-red.log`: editing a deleted ordinary transfer peer changed
  the surviving split allocation and parent through its retired reference. Hidden
  PATCH now uses the ordinary local tombstone merge path. Its retained historical
  transfer payee can remain unchanged, but cannot select a new transfer identity.
  Live transfer propagation additionally requires two active reciprocal rows.
  Hidden date-sync metadata is validated but cannot update shared preferences.
  Regression covers amount/date/cleared/notes/account/category, a later child
  relink, restart, prohibited relationship editing and pairs with both rows deleted.
  `/tmp/ticket15-hidden-green.log` records the focused green tracer.
- `/tmp/ticket15-storage-red.log`: after quota rejection, Retry sent a pending
  request absent from durable storage. TransferForm now advances its ownership
  checkpoint only after successful storage and persists before every dispatch,
  including retry/cancel. Failed cancellation cleanup preserves the original
  pending operation. Public browser fault injection covers quota save/retry,
  reload of unsent fields, quota cancellation, failed cancellation-draft storage
  followed by reload/retry, and stale-tab ownership. Every observed outgoing
  request is checked against durable body/key/clock/epoch. Green evidence:
  `/tmp/ticket15-storage-green.log`.

Related-form audit: SplitEditor awaits `writeSplit` on every send and assigns its
saved revision only after success; AccountManagement persists before every send
and advances its draft only after success. Neither has the reviewed send bypass.
No sibling product changes were needed. The existing transfer deletion-copy
assertion was updated to the already-shipped split-aware wording; financial and
replay assertions remain unchanged.

Focused validation: concurrency12 + splits10 + transfers12 all pass
(`/tmp/ticket15-corrections-http.log`); transfer and combined browser suites pass
(`/tmp/ticket15-corrections-transfer-browser.log`,
`/tmp/ticket15-corrections-combined-browser.log`). Build/check/autofixer/Clippy pass
(`/tmp/ticket15-corrections-{build,check,svelte,clippy}.log`). Broad suites were not
repeated. Independent corrective review and root integration remain required.

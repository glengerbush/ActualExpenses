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

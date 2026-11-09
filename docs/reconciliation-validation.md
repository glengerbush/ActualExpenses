# Ticket16 reconciliation evidence

Private base: `20b6eb9`; branch `codex/reconcile16`; no commits or deployment. Canonical ticket16 and spec stories13–15/22 define scope. Root owns final integration and independent review.

## Delivered

- Exact statement comparison with total/cleared/uncleared amounts and progress counts. Cleared entries across all dates lock only at zero difference; stale preview and old lock clocks reject atomically.
- Ordinary adjustment transactions preserve credit/debt signs and off-budget revaluations. Stable IDs and receipts prevent duplicate adjustments after lost acknowledgement and restart.
- Explicit unlock and confirmed ordinary edits; confirmed edits keep reconciled state. DELETE stays protected until explicit unlock.
- Nullable account reconciliation timestamp preserved from Actual import through confirmed edits, unlock, restart and portable restore. Imported field-clock mapping and old archive defaults included.
- Versioned API schemas, capability discovery, budget-epoch fences, immutable retry/cancellation and explicit clock recovery. Browser drafts retain statement input and pending identities; storage failure cannot claim a saved mutation.
- Responsive inline account workflow and unlock controls from the all-account register. No login; plain HTTP; offline reconciliation disabled. Native expense eligibility follows reconciliation state.

## Independent baseline

`actualBudget/tools/generate-reconciliation.mjs` uses public `@actual-app/api@26.10.0` against disposable data. `actualBudget/tests/fixtures/reconciliation.json` records six snapshots: initial debt, adjustment, lock, confirmed edit, unlock and asset revaluation. Credit cleared balance −10000 reconciles to −12500 via −2500; uncleared −300 stays excluded. House value23110000 becomes23460000 via350000. Source commit: `2bebdbaae0b701120a8cad9d56aafd64fa04c577`.

The pinned public transaction-update API returns before its internal batch promise completes; fixture generation waits for the requested visible public read before recording the confirmed-edit/unlock snapshot. It never patches the oracle engine. `--check` reproduces stored snapshots. The disposable engine emits its existing unauthenticated cloud-upload warning while local import/export succeeds; no external budget is used.

Pinned source anchors: `packages/desktop-client/src/accounts/reconciliation.ts`, `components/accounts/Reconcile.tsx`, `components/accounts/Account.tsx`, `packages/docs/docs/accounts/reconciliation.md`, and `packages/loot-core/src/server/api.ts`. The orchestration source hash is stored in the fixture. The source ZIP and identity metadata are committed alongside it.

## Verification

- Four new public-HTTP tests in `server/tests/reconciliation.rs`: signs/adjustments, lock/protected edits/unlock, unchanged replay after restart, stale snapshot and mismatch rollback, uncleared exclusion, cancellation before/after commit, future/old clocks, retired epoch, safe-money boundary, independent oracle snapshots, Actual import and portable restart/restore.
- All96 private-tree server tests passed. The full run found three old exact-account expectations missing the additive null field; corrected only those expectations and resumed failed/unrun suites. No repeated broad green suite.
- Served OpenAPI validation passes, including reconciliation progress, lock and unlock responses and schema10 native archives. Rust formatting and Clippy warnings-as-errors pass.
- Focused real native `ApiExpenseService` test passes: saved expense disappears after reconciliation, reappears after explicit unlock and native-service restart, amount unchanged.
- `web/tests/reconciliation-browser.mjs` passes on a running server: plainHTTP, negative statement, committed/lost adjustment acknowledgement, reload and byte-identical key/body/HLC retry, lock, confirmed edit, account/all-account unlock, offline boundary, quota failure, future clock rejection, safe cancellation, corrected local clock and new request, keyboard focus, mobile/dark/200% text. Added to standard browser script.
- Svelte typechecking: zero errors/warnings. Autofixer: no issues; reconciliation's effect intentionally fetches server progress when the external register snapshot changes (its advisory is not a local derived-state replacement). Impeccable mechanical detector: zero findings. Existing design retained; scope/direction contract in `actualBudget/.impeccable/surfaces/actualbudget-web-src-lib-reconciliation-svelte.md`.
- Screenshots: `docs/screenshots/reconciliation/` desktop,390px mobile,dark and200% text. Large text revealed page overflow; a keyboard-scrollable table region contains the dense register while preserving table semantics.

## Required root joins and review gates

This isolated base predates concurrent tickets12/14/15. Do not mark aggregate reconciliation complete until these joins are exercised:

1. Split14: parent-only progress counts/balances; lock/unlock status and clocks propagate to children. Confirmed edits on any member protect the entire aggregate. Opening parent payee rules remain owned by14.
2. Transfer15: read both legs before confirmed patch; guard every changed protected leg and preserve its lock. Deletion requires explicit unlock of both protected legs. Local clearing/reconciliation status does not blindly copy to the partner. Worker15 identified its patch guards/extraction seam for root integration.
3. Register12: preserve filtered-query/visible-column behavior, cleared/reconciled filters for story22, keyboard navigation and table scroll wrapper. Reconciliation always compares all account dates, independent of current register filters.
4. Bulk/undo13: protect all affected settled records, including split/transfer expansion; supply explicit confirmation/unlock contracts and tests.
5. Provider tickets: add available balance selection; current source provider metadata stays explicitly blocked pending those converters. Rules ticket: route reconciliation adjustments through ordinary creation/rule execution when that capability lands.
6. Root renumbers provisional schema10, archive writer/reader/report enum and union tests. Source conversion and receipt histories must stay intact.

Root will run independent Standards/Spec and Impeccable finish review after freeze; this record does not claim those reviews completed. Existing Impeccable context reports unrelated buildPath/orphaned-surface drift; left unchanged. No live deployment, publication or version bump.

## Corrective review: stale confirmation

Standards review reproduced an unpublished confirmed edit from before an unlock/relock changing a newly settled amount. Fixed with explicit captured `reconciled_version` and an intent-HLC freshness guard. `protect` now takes `Option<Confirmation { version, clock }>`; a bare boolean cannot bypass protection. Browser confirmation persists its displayed row version in the immutable request. Native has no confirmation override and safely receives protection refusal.

New public regression covers lock1 → confirmed intent2 → unlock3 → lock4 → delayed delivery2; also rejects an old proof with a newer clock, missing proof, and a current proof with an older clock. The row and its versions remain unchanged; an already-committed original receipt replays exactly after server restart. A fresh version/newer-clock confirmation succeeds and retains the lock. Five reconciliation tests now pass; focused browser workflow, Svelte typecheck/autofixer, formatting and warnings-as-errors Clippy also pass. Broader previously-green suites are not rerun.

Aggregate integration must capture a map keyed by every affected protected transaction ID, compare each member’s own current `reconciled` field version, and apply the same intent-clock guard. Never reuse the initiating row’s proof for a protected transfer counterpart or split member. Split/transfer models are absent from this private baseline; root owns those combined public regressions.


## Corrective review: captured adjustment date format

Spec review found the adjustment date hardcoded to ISO. New reconciliation drafts now capture the budget date preference; the existing DateInput displays and parses that captured format. Restore validates it; older drafts without a format keep their original ISO interpretation. Submission parses and validates the date again before creating the immutable request, including restored raw text that never receives blur/Enter. Canonical API dates remain YYYY-MM-DD.

Focused browser regression first failed on the old ISO placeholder, then passed with day/month preference: raw `03/04/2026` survives an unblurred reload after the budget switches to month/day, submits `2026-04-03`, and retains its captured format after a lost committed acknowledgement and a second preference switch to ISO. Retry preserves body/key/HLC exactly; public transaction read confirms April 3. Fresh drafts use the current preference. Svelte check reports zero errors/warnings; production build, autofixer (no issues; existing server-fetch effect advisory only), and focused reconciliation browser workflow pass. No backend changes or broad test rerun.

Refreshed desktop/mobile/dark/large-text captures were inspected. Desktop intentionally shows current register month/day beside the recovered reconciliation's captured day/month: the test changes preferences mid-draft to prove its date meaning cannot change. Later captures show new ISO drafts after exit/restart. Existing layout, contrast and responsive behavior retained.


## Combined integration: split14 + query12 (schema12)

Resolved the private merge of `6eb2657` with `5a9efa1`; no abort, commit or primary-tree edits. Reconciliation migration is now012 after split10/register-settings11. Writer/report schema12 and archive reader schemas6–12 preserve register settings, split members, nullable account dates and historical receipt bytes.

Decisions and completed joins:

- Statement totals and counts use logical parent rows only. Lock updates every cleared parent/child and publishes each protection version/change.
- Unlocking either parent or child requires captured versions of all members and atomically unlocks the entire split, preserving cleared state. Ordinary unlock retains its singular version contract.
- Split mutations require explicit confirmation and a version map for every existing protected member. Missing/stale member proofs refuse atomically; subsequent unlock/relock invalidates earlier proof maps. New allocations inherit protection; replacement/recombination/unsplitting preserve aggregate invariants. Ordinary parent/child PATCH remains an explicit split-recovery refusal.
- Default confirmation fields are omitted when serializing split receipts. Genuine schema11 requests replay byte-for-byte after upgrade and archive restore. Protected-edit retries also replay after newer locks and restart.
- Query visibility/columns/sort/keyboard behavior and the focusable horizontal table region coexist. Reconciliation still includes transactions hidden by register filters. The split editor exposes confirmation, persists captured per-member proofs, and retries the original body/key/HLC after a committed lost response/reload. Captured adjustment-date preference and stale ordinary-confirmation fixes remain intact.

Evidence:

- Full combined Rust run:111 public tests green. Added genuine schema11 upgrade test then passed focused seven-test reconciliation suite (112 total coverage); no repeated broad suite. Clippy all-target warnings-as-errors and formatting pass.
- Genuine schema11 database/native archive generated through a copied primary `5a9efa1` server binary, SHA256 `8448fe4c1d81e283ec41fda93084d980c9947aad39b114c66fc05ea0aa34504b`, using disposable data only. Fixtures preserve split/payee IDs, register settings, exact account/split receipts, an active Actual split import, and a blocked source with a reconciliation timestamp. Upgrade keeps active history unchanged; a new upload intent re-inspects the retained blocked source, recovers its timestamp, while the old upload receipt remains blocked. Native schema11 archive restores with null new dates; new schema12 archive restores locked splits/settings/dates. Generator and fixture metadata are included with the tests.
- `generate-reconciliation-splits.mjs --check` passes against public `@actual-app/api@26.10.0`. Four independent snapshots cover parent-only cleared totals, whole-split lock, confirmed allocation edit and whole-split unlock. The public child-update API needs its unchanged amount supplied explicitly; omitted child amount defaults to zero. Unlock oracle follows the public parent action; our child-origin unlock safely resolves that same aggregate. No oracle internals patched.
- Native:23 existing/combined cases passed in full run; the new split eligibility test passed after adding the necessary queued-expense refresh (24 total). It proves locked splits remain excluded through confirmed recombination until explicit unlock.
- Focused reconciliation browser plus split, register-query, register-settings, register-review and regex-budget suites pass. Combined browser covers hidden-cleared and hidden-reconciled rows, parent-only progress, protected child edit confirmation, three-member proof capture, lost response/reload/identical retry and whole-split unlock. Served OpenAPI schemas pass. Svelte check/build and autofixer pass; only existing effect/bind-this advisories remain.
- Captures under `docs/screenshots/reconciliation/` refreshed for the merged register, with split confirmation and settled-split captures added. Incumbent design retained.

The earlier root joins1/3 are now exercised. Transfer15 is still absent from this tree; it must expand both leg member sets before `reconciliation::protect_members`, without reusing one row’s proof. Shared `members(db,budget,id)` resolves parent/children and `protect_members(rows, optionalVersionMap, clock)` checks each protected member. Bulk/undo13 and provider/rules joins remain as recorded. Root owns independent review and the merge commit.


## Corrective integration review: bounded aggregate proof

Spec review found an accepted Actual101-child import could lock but its102-member version map could not unlock. Added a fixed64-hex SHA256 token, versioned and bound to the budget, canonical logical parent ID, complete sorted member IDs, reconciled flags and each member’s reconciled field version. Existing public split reads expose the token at top level, including reads by child ID. Unlock and confirmed structural split recovery validate it atomically inside the existing receipt transaction, with original intent-clock freshness against every member. Changed membership or unlock/relock rejects old proof. Receipt lookup remains first; legacy singular/map request shapes and genuine pre-feature receipts remain unchanged.

UI split drafts persist the captured token. Unlock fetches the current row/proof before presenting confirmation and stores that proof in the immutable request; no retry rebases it. Large unsplit/recombine payloads omit children as before, after checking unsaved child edits.

Evidence: public `@actual-app/api@26.10.0` generated `reconciliation-large.zip` containing101- and1001-child aggregates (generator and identity metadata included). Both import/activate, lock, bounded-token unlock and relock successfully. The1001-child legacy map exceeds64KiB; token unlock is under128 bytes. Tests reject stale relock/membership tokens and fresh tokens carried by older clocks, preserve rows on refusal, replay committed unlock after server restart, and recover the101-child aggregate by confirmed unsplit and1001-child aggregate by confirmed recombination. Parent-only cleared totals remain unchanged.

Focused eight-test public reconciliation suite passes, including genuine schema11 upgrade/receipt/archive regression. Clippy all-target warnings-as-errors passes. Focused reconciliation browser verifies lost-response/reload/identical token retry, then imported101-child UI unlock under128 bytes and confirmed unsplit under1KiB with empty children payload. Split browser regression retains the unsaved-change guard. Its retry assertion initially raced an already-visible register row; it now waits for the second captured request and pending-editor completion before asserting identical bytes. Served OpenAPI schemas, Svelte check/build/autofixer and formatting pass. No repeated full suite; prior112 broad coverage remains recorded above, with one new large-import test.

**Remaining ticket14 gate:** replacing/editing allocations above100 children (or beyond64KiB) remains unsupported by the current replacement API/editor. This fix provides bounded unlock and supported unsplit/recombine recovery without weakening import validation or rejecting previously accepted sources. Root owns that separate allocation-editing integration and corrective Standards/Spec reviews. Bulk13 has been informed that exact server-captured membership/flags/versions are equivalent evidence; undo must not mint a new token or alias away a newer reconciliation lock.

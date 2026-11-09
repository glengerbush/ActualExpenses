# Tickets 09 and 10 acceptance

Accepted through `c8de872`. Independent Standards and Spec corrective reviews pass; production-container/browser verification passes. Late ticket11 review exposed shared clock recovery, followed by async cancellation and validation recovery findings; all are fixed with public API/browser regressions. No release, version change or production cutover occurred.

## Delivered

- **09 accounts:** groups/order, names and notes, immutable on/off-budget classification, closed visibility, zero-balance closure, reopening, empty-account removal and explicitly confirmed deletion. Historical balances, tombstones and newest-field ordering remain intact. Nonzero transfer-based closure belongs to15.
- **10 categories:** groups/order, hide/show, names and safe GFM notes, history-preserving merge/delete/reassignment, income/expense distinctions, refunds, reimbursements and uncategorized drilldowns. Null source order remains editable without changing its order. Retired notes and delayed category references survive.
- **Migration/recovery:** schema8 includes both domains. Genuine schema6 and accounts-schema7 databases/ZIPs preserve accounts, category defaults or original imported metadata, native post-import edits and exact historical receipts. Ready old import stages acquire only missing metadata; blocked old receipts remain immutable. Long valid alias chains remain restorable; actual cycles fail.
- **Android consumers:** current account/category choices reflect closures, deletions and inherited visibility. Unknown pending work on unavailable accounts keeps its original identity and recovery choice. Read-only receipt lookup reconciles committed work before lifecycle gates; metadata-only preflight allows more than100 pending expenses to drain. No legacy state rewrite or silent reassignment.

## Evidence

| Boundary | Result |
| --- | --- |
| Rust public HTTP | Prior79 integration tests plus3 new metadata cases; final20 account/category tests pass, zero skips; strict Clippy and rustfmt passed |
| Served OpenAPI | Accounts/categories, history/merges, lifecycle and read-only receipt success/miss contracts passed |
| Native core | 18 current-server public API tests, zero skips;16 unchanged legacy regressions passed separately against disposable Actual26.10.0 |
| Android build | Debug APK and lint passed; no device rerun for core-only corrections |
| Browser | Plain HTTP, desktop/mobile, keyboard, exact amounts, stale-tab ownership, offline metadata gates, lost-ack retries, null order and safe GFM notes passed |
| Full production container | Passed at `2e71288`: complete browser suite, bundled assets, exact balances/receipts after container replacement |
| Corrective production container | Read-only receipt miss/replay, concurrency, offline recovery and replacement persistence passed at `c01a749` |
| Final management recovery container | Four clock-recovery flows, delayed capability response, whitespace validation recovery, accounts/categories, read-only receipts and replacement persistence passed at `c8de872` |

Full-suite image: `sha256:dcb39f18b3120ed43606c2d158b17833e2a094100c073d326558c9a3cca038f2`,127946437bytes, user10001:10001. All test containers/volumes/images are disposable; live `actualbudget-data` was never opened.

Receipt corrective image: `sha256:42200b81fc12ec00ab2900e0cb3971913cbff274a173f0be6cfc1e55b9029689`,127921906bytes, user10001:10001. Log: `.scratch/0910-container-receipt-lease-final.log`.

Final management image: `sha256:43fed69d49a36094ab5a13488505778a7560a7c0eb01c8dc8351168446937c94`,127926608bytes, user10001:10001. Log: `.scratch/0910-container-clock-final.log`. Disposable resources removed after every run.

Logs: `.scratch/0910-native-final.log` (78 before receipt addition), `.scratch/0910-receipt-native.log` (79), `.scratch/0910-receipt-clippy.log`, `.scratch/0910-receipt-contract.log`, `.scratch/0910-container-final.log`, `.scratch/0910-container-receipt-final.log`. Native final: `/tmp/native0910-final-review-green.log`. Original fixtures and generators are documented in `actualBudget/tests/fixtures/{CATEGORY-LIFECYCLE,CATEGORY-SCHEMA7}.md` and `actualBudget/server/ACCOUNTS.md`.

## Review

Independent Standards and Spec reviews pass through `0dbda5e`, including corrective verdicts. Fixed findings: stale-tab account/category pending overwrite, offline metadata writes, hidden transaction edits after account deletion, valid long category aliases rejected as cycles, null-order browser validation, missing Markdown notes, native queue preflight limit and blocked lost-ack reconciliation after restore/closure/deletion. Public regressions cover each. Read-only receipt misses neither consume operation identity nor authorize cancellation.

Final independent corrective verdicts pass at `c8de872`. Metadata cancellation records a durable terminal receipt before clearing an attempted request, preserving original identity and any already committed outcome. Corrected device time resets only the local generator, retaining fields and other pending clocks. Capability negotiation locks the editor and rechecks ownership/identity. Invalid category/group names can be canceled, corrected and saved; semantic validation follows the fence. Logs: `/tmp/metadata-cancel-race-{red,green}.log`, `/tmp/metadata-invalid-{red,green}.log`, `.scratch/0910-metadata-{api-final,clippy,contract}.log`.

Impeccable finish reviews: accounts ship; categories/shared notes preview ship across seven desktop/mobile/offline captures. Documenter preserved incumbent design files and reported pre-existing drift without changing it. Root inspected category Markdown mobile and account preview desktop renders.

New recovery finish review inspected12 desktop/mobile/reset captures with no actionable finding; detector clean. Svelte analysis reported no issues, retaining existing Categories persistence-effect/bind suggestions. A five-second long-history test timeout occurred under concurrent load; unchanged old binary restored in3.066s versus3.221s current in the same public timing probe. Original test and final account/category suite subsequently passed unchanged. No performance improvement is claimed; `.scratch/0910-restore-timing.log` records measurements.

One corrective container run exposed a browser test's five-second expectation after reload while a prior delivery claim can legitimately remain leased for60seconds. The assertion now allows the documented lease to expire while still requiring unchanged request identity and exactly one server transaction; this changes test timing only.

Remaining boundaries:15 transfer closure;17/18 budget planning;19/20 rule learning and full legacy Android upgrade/background qualification;49–53 repair and whole-replacement qualification. Uncommitted deleted-account native work remains visible for manual reconciliation; no automatic discard or reassignment is claimed.

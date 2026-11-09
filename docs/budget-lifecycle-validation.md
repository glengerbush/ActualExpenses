# Ticket 08 validation

Accepted through `d77a46b`; [06–08 acceptance](060708-validation.md) records final tests, artifacts and resolved independent reviews.

Public seams: real Rust process HTTP API, browser workflows and exported native artifacts. No destination database queries or mocked financial engine.

- `cargo test --manifest-path actualBudget/server/Cargo.toml`: root final backend run passed all60 public integration tests, zero skipped. Clippy across all targets with warnings denied and rustfmt check passed. Logs: `.scratch/060708-native-final.log`, `.scratch/060708-clippy-final.log`, `.scratch/060708-fmt-final.log`.
- `--test budget_management`: restore rotates identity, preserves post-backup receipts, restores deleted budgets after restart; portable staging/new-server activation; automatic dirty/idle/restart; independent duplication; genuine schema5 upgrade with all14 exact receipt replays including raw ZIP; imported budget financial/settings/clocks/sourceZIP round-trip; interrupted export/upload with retry; three-current-day automatic retention preserving manual points.
- Red→green: backup route404 before first slice; automatic deadline never produced a point before worker; genuine schema5 had no usable upgrade point before migration integration; account draft lacked captured epoch before browser guard.
- `npm run check` and `npm run build` passed. `node tests/budget-epoch-browser.mjs` passed account/settings generation review and explicit fresh reapply, including delayed original rejection after a second restore. The previous date-recovery transaction retry UI changed with ticket07; its owner is adapting that existing test without weakening immutable date/key assertions.
- Svelte autofixer: AccountForm no issues; Settings reports the pre-existing named `update` handler as a state-mutation suggestion (handler delegates to a session object, not a Svelte state replacement).

Native archive/recovery design and future-domain registration are in `actualBudget/server/LIFECYCLE.md`. Root owns final lifecycle UI tests, cancellation tests, Docker ENOSPC/process-failure rehearsal and independent two-axis review; those outcomes must be recorded before08 completion.

## Root qualification of candidate `3ee4d7d`

- `lifecycle_cancellation` public HTTP test: six lifecycle mutation routes fence delayed originals; cancellation after commit returns the original backup/rename receipt. Test failed with400 before implementation, then passed.
- `lifecycle-browser.mjs`: rename lost acknowledgement across reload with exact body/key/epoch; cancellation followed by delayed original; backup/download; independent duplicate balances; deletion and restore; native archive review/activation. Browser quota failure sends zero uploads; lost upload/activation responses retry unchanged archive hash/body/key/epoch, with one restored generation. Desktop and320px/200% management captures inspected, no overflow.
- Expanded `contract-smoke.py`: real metadata/checkpoint, backup, rename, duplicate, cancellation, epoch conflict, deletion/recovery, native ZIP staging/activation and invalid archive responses validate against the served OpenAPI.
- `python3 actualBudget/tools/lifecycle-container.py`: production Docker image, UID/GID10001, isolated128MiB data tmpfs filled to real ENOSPC. Restore returns failure without changing name/generation/accounts/transactions; original backup hash remains exact. Freeing the test filler permits original request retry.
- Separate disposable Docker volume: SIGKILL while the restore request is still in flight (explicitly asserted). Restart yields whole old/new state; identical retries return one committed restore generation and preserve the original backup. This tests process interruption, not a physical power-loss guarantee.
- Fault image:`sha256:3032faf6c7eef828e3f8ffef7ae8cd77bd8d3058a25576aa167fe311b16ca945`,126852391bytes. Test containers, image and volume removed. Logs:`.scratch/060708-container-fault-final.log`. No live volume or deployment touched.

The initial combined review found an offline-editor reconnect race; the final acceptance record documents its resolution.

Lifecycle UI finish review: initial disposition `fix` identified confirmation focus and missing selected archive filename. `ce1bdd6` resolves both. The browser regression verifies mobile200% confirmation focus/in-view and Cancel focus return; refreshed desktop/mobile archive captures show the filename. Verdict pass: `ship` for the two scored fixes. Documenter checked the source/styles/captures and preserved the incumbent design files; unrelated pre-existing documentation drift was not canonized. This UI verdict does not close the separate07 functional review findings.

The complete composed browser suite passed in the production container at `0563ff3`, including prior formatting/date/import/concurrency/theme workflows and new offline/race/lifecycle/generation cases. Image `sha256:7ed349ccf6467e7b338b24b469ab9d89dbd80ef78a978e0a9147a203b758c747`,126863026bytes,UID10001:10001. Container replacement preserved exact opening balance and replay identity. Log `.scratch/060708-container-browser-repaired.log`; isolated resources removed. A later independent07 review found lost offline edit eligibility after reloading an unfinished pending editor; that scoped fix and recheck remain required before acceptance.

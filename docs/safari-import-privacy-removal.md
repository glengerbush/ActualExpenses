# Safari import recovery and privacy removal

2026-11-12. Prepared for actualBudget 0.2.4 publication by the release workflow.
Coolify updates remain manual.

## Import failure

The user confirmed an Actual desktop ZIP containing `db.sqlite` and
`metadata.json`. Safari failed immediately after **Review archive**, before any
upload, with a misleading instruction to free browser storage.

Reproduced that exact error in Playwright WebKit 27.2 using the genuine Actual
26.10 export fixture. WebKit rejected the File/Blob stored in IndexedDB. This is
consistent with [WebKit's documented Blob storage limitation](https://bugs.webkit.org/show_bug.cgi?id=286561).
Neither source account identities nor ZIP size caused this failure.

Recovery records now store the original archive as an ArrayBuffer, read before
opening the database transaction. Record version 2 retains both request keys;
version 1 Blob records remain readable and upgrade on their next save. Only an
actual QuotaExceededError now prompts users to free storage. Failed persistence
still prevents sending a new upload, and uncertain operations keep their retry
identity. Server archive validation and financial conversion are unchanged.

## Privacy removal

Removed the privacy toggle, numeric redaction, hidden charts/previews, pending
expense reveal controls, font asset and related help. Retired the shared setting
and API property; old stored/imported settings are ignored. Old committed
requests can still resolve their original receipts. Financial calculations and
formatting remain; report loading/epoch guards still prevent stale displays.
This supersedes privacy-mode requirements in the earlier parity specification.

## Verification

- WebKit: real import, lost acknowledgement, reload, exact archive SHA-256,
  both source accounts' review balances, activation into transaction history.
- Chromium: same flow with Blob writes prohibited, legacy Blob record upgrade,
  and accurate non-quota failure messaging without an upload.
- Existing import suite: quota refusal, stale tabs, retries, source rejection,
  financial oracle and retained original archive.
- All 14 affected browser groups passed across corrected runs; desktop/mobile
  navigation screenshots reviewed. Financial oracle expectations unchanged.
- 459 unique Rust cases passed across completed runs. One regex-budget request
  timed out under parallel browser load; its complete group passed serially.
- Svelte check: zero errors/warnings; production build and component autofixer
  passed. Autofixer suggestions concern existing lifecycle effects, local Maps
  and focus bindings, not new defects.

Regression: `actualBudget/web/tests/import-storage-browser.mjs`, included in the
ordinary browser release suite. Set `IMPORT_BROWSER=webkit` to exercise WebKit
without the Chromium restriction simulation. Linux WebKit was verified here;
the user's own Safari installation has not yet run the updated app.

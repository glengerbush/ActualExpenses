# Android API transition: ticket 06

Accepted through `d77a46b`; [06–08 acceptance](060708-validation.md) records final tests, artifacts and resolved independent reviews.

This slice replaces the companion's active transport with actualBudget API v1. Production replacement remains gated on the remaining tickets. Legacy migration is not claimed.

## Behavior and persistence

- Server → budget → account, no credentials. The adapter checks API version and explicit companion/ordering/cursor/generation capabilities before using the server.
- Existing application ID `org.actualexpenses.android`, Android 8.0/API 26 minimum, SDK 35 build target, version 0.1.2/code 3 and release signing configuration remain unchanged.
- New encrypted state uses `expenses.api-v1.state`; existing `expenses.state` and Keystore alias `actualexpenses.state.v1` remain untouched. Any legacy state opens an explicit recovery screen until ticket 20 supplies its migration. Tests prove no legacy writes and no transport requests through this guard.
- Scope is canonical server origin/base path + API family + original budget UUID; remembered account and backdated retention are scoped accordingly. Changing servers/budgets requires an empty native queue. Incompatible responses retain state and pending payloads.
- Pending native intents persist original method, path, JSON bytes, operation ID, transaction ID, HLC and budget epoch before transmission. A committed response lost in transit is retried exactly once financially, including after recreating the service. Restore never silently replaces an intent's captured epoch.
- Forms capture original account/budget/server, epoch and currency precision when opened. Compose saves the edit baseline/context with its fields across state restoration. Currency changes cannot reinterpret entered money. Restored generations require review before a new intent.
- Only additions and unsynced offline-created rows can be edited offline; edits opened online survive connection loss. Changed fields alone are patched. Native published tombstones and server ordering remain authoritative; rejected unpublished deletions restore the visible expense with an explanation.

## Read boundary

`GET /api/v1/budgets/{budget}/companion` returns at most ten eligible recent expenses, at most 100 explicitly retained eligible IDs, unavailable IDs, a consistent change snapshot and epoch. Recent order is date descending, source-preserving sort rank descending, ID ascending. Income and reconciled rows are excluded; negative opening balances retain the existing companion behavior. The partial eligibility index avoids scanning intervening income history. Transfer/split activation must extend this predicate in tickets 14/15; unsupported source records currently block activation.

Reference data and preferences are loaded at one guarded checkpoint; concurrent changes restart bootstrap rather than mix snapshots. Foreground refresh applies incremental changes and refreshes the bounded companion view. An unchanged refresh leaves encrypted state bytes untouched; selecting another cached account reuses reference data. Native cached rule suggestions are ticket 19/20, not claimed here.

## Transport and toolchain

[OkHttp](https://lysine.dev/okhttp/) supplies pooled PATCH-capable HTTP with bounded 8 MiB responses, explicit timeouts, disabled redirects and disabled automatic mutation retries. The old JDK URLConnection transport rejects PATCH. Version 5.3.0 preserves the current Android build toolchain: published AAR metadata for 5.4.0 requires compile SDK 36, and 5.5.1 requires 37; no unrequested SDK/AGP upgrade was introduced. These metadata checks used Maven Central artifacts. Native core files were formatted with [ktfmt 0.64](https://github.com/Kotlin/ktfmt), downloaded with its published SHA256 checked.

## Public qualification

All test data is disposable. Native integration starts isolated Rust servers; legacy oracle tests use the pinned Actual 26.10.0 fixture. The imported fixture checks original account/transaction identities and expense amounts (future −100, corrected −1200), excluding source income/refund/deletion.

Current checks:

- Rust public companion integration: passed; 15 expenses, intervening income, retained old row, stable checkpoint, concurrent insertion rejection, process restart, invalid IDs.
- Native core: 25 tests, zero skips, including nine new running-API tests and the existing genuine Actual integration cases.
- Android debug APK and instrumentation APK build; debug lint.
- API 36 emulator: all 11 tests passed, including real Android HTTP setup/save and Compose state restoration. Separate real setup/save runs also passed in dark mode at 200% font scaling and a 1067 dp-wide tablet window. The Android 8.0/API 26 emulator also passed all 11 tests, including the real HTTP workflow; no device-test skips.

The native tests cover exact lost-ack retry bytes/headers, automatic clearing/optional fields, newest ten and retained replacement, dependent inline payee preservation when an unsent expense is deleted, per-field merge, deletion ordering, source archive import, incompatible server recovery, currency/generation context and safe origin/account selection.

No new release was signed or published by ticket 06. No production database/volume, remote deployment or user device was touched. Background OS scheduling reliability and legacy upgrade migration receive their full acceptance in ticket 20.


## Native design review

Impeccable native guidance applied to the existing Material 3 Operate design. Inspected rendered setup, register and editor in phone light, dark 200% text, and tablet configurations. Controls remain reachable, labels readable, the save action scrolls into view, and tablet content stays centered at 640 dp. Native semantics tests exercise optional fields, disabled offline rows, deletion identity, recovery, and restored editor values; debug lint passed. TalkBack speech output and physical-device performance were not measured.

Review artifacts (ignored local output): `.scratch/android06/screenshots/final-light/review/`, `dark2x/`, `tablet/`. Each set contains setup, register, editor and acknowledged expense. Verdict: no blocking issue found in inspected layouts; independent root review remains required. No decorative motion was added, and existing labels/system controls were retained. Emulator display/font/night overrides are restored after capture.


Final local evidence: `/tmp/actualexpenses06-final2-build.log` (Gradle successful, 25 tests/zero skips, debug + instrumentation APKs and lint); `/tmp/actualexpenses06-device-final.log` (API 36: 11 passed); `/tmp/actualexpenses06-device-api26.log` (API 26: 11 passed); `/tmp/actualexpenses06-device-dark2x.log` and `/tmp/actualexpenses06-device-tablet.log` (one real flow each, passed); `/tmp/actualexpenses06-companion.log` (public Rust companion test, passed). Companion fragment and served OpenAPI paths/schema compare equal. `git diff --check` passed. Application build/signing settings and manifest have no diff. API 26 screenshots are also in `.scratch/android06/screenshots/api26/`.

Root independently inspected the phone setup, dark200% editor and tablet acknowledged-expense captures: labels/actions remain readable and reachable; native Material controls and the existing focused workflow are preserved. Root checked the API36 instrumentation result (`OK (11 tests)`). Final combined review and qualification passed; see the acceptance record.

## Account/category metadata integration: tickets 09–10

Native refresh now evaluates account changes before requesting the companion view or delivering queued work. Closed/deleted accounts disappear from ordinary choices; an unavailable selection clears when it has no pending work. Accounts with pending work remain selectable as explicitly labeled recovery choices. Their original expense payloads and delivery identities remain intact, including edits whose server row became a tombstone. New saves to those accounts are rejected. Other available accounts remain selectable, and returning to the recovery choice restores the pending expense view.

A reachable server with an unavailable pending account produces an explicit blocked recovery message, distinct from network disconnection. Reopening a closed account resumes an uncommitted original request. Already committed requests reconcile from their receipts without reopening. Uncommitted deleted-account work is retained for reconciliation; this change does not invent automatic reassignment, discard, or a complete deleted-account reconciliation workflow. Legacy `expenses.state`, signing/version configuration and API compatibility requirements remain unchanged.

Category choices exclude income, hidden, parent-hidden and deleted entries. Existing expense labels remain available, and edits may retain an existing hidden category. A category-group change reloads only category references at the change cursor's guarded snapshot, so inherited visibility updates without an unguarded full bootstrap.

Public native regressions use disposable real Rust HTTP servers. Evidence so far: account closure/deletion and pending recovery tests passed on ticket09; category hide/unhide, group hide/unhide and deletion passed on the isolated ticket10 binary. Account recovery includes restarting the native service and switching away/back. Logs: `/tmp/native09-all-green.log`, `/tmp/native09-receipt-green.log`, `/tmp/native10-confirmed-red.log`, `/tmp/native10-green.log`. Combined schema8 qualification is recorded below when available. No live storage or credentials were used.

Combined schema8 server `6d46da7`: all 13 native public API tests passed, zero skips (`/tmp/native0910-integrated-green.log`). The 16 unchanged legacy regressions passed separately against disposable Actual 26.10.0 (`/tmp/native0910-legacy-tests.log`); debug APK build and lint passed (`/tmp/native0910-android-build.log`). Run Gradle with the installed JDK17; the machine's default JDK27 fails during Gradle configuration before tests.


### Final native review corrections

Refresh first reconciles attempted requests through the optional `transaction_receipt_lookup` capability and `X-Actual-Receipt-Only: true`. Probes preserve the original method, URL, body, operation key, edit clock and epoch; unknown requests remain queued and are neither applied nor canceled. Committed receipts therefore clear uncertain delivery before restore cursor or account-availability checks. The subsequent current-budget refresh replaces historical receipt data, so restoring or deleting an account cannot resurrect its old expense. Servers without the capability receive no probe headers and retain uncertain unavailable-account work safely.

Pending-work preflight now refreshes metadata without loading the companion view. The bounded retained-ID read occurs after delivery, preventing its 100-ID cap from deadlocking larger offline queues. No pending identity, timestamp, epoch or payload is rebased.

Public red/green evidence: 101 offline expenses plus a remote account change failed before the fix, then drained to exactly 101 server rows totaling −10,100 minor units (`/tmp/native0910-queue101-red.log`, `/tmp/native0910-queue101-green.log`). Lost acknowledgement followed by restore, account closure, or account deletion failed independently before receipt reconciliation (`/tmp/native0910-lifecycle-receipt-red.log`); all passed afterward with the account still unavailable and immutable request identity preserved (`/tmp/native0910-receipt-native-green.log`). The uncommitted closure/reopen regression proves unknown probes do not fence or cancel the original request. No UI, signing, application version, legacy state, production storage or credentials changed.

Final corrective qualification: 18 native public HTTP tests, zero failures/skips, plus debug APK assembly and debug lint all passed (`/tmp/native0910-final-review-green.log`). The optional-capability regression additionally exposed and fixed stale selection after a receipt clears the last pending item with no newer metadata delta; unavailable selections now clear in that case too. Its initial failure is recorded in `/tmp/native0910-receipt-capability.log`. Tests cleaned up their disposable native servers; no device test was rerun for these core-only changes.

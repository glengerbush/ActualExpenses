# Validation — ActualExpenses 0.1.0

Date: 2026-10-16. All server mutations used disposable local budgets. The deployed server and the user's financial data were not accessed.

## Integration and build

The final JVM integration suite passed **12 tests, 0 failures, 0 skipped**, using the real published Actual 26.10.0 server and an independent `@actual-app/api` oracle:

- Authentication, budget discovery, snapshot decoding, direct create/update/delete, exact amounts and cleared state.
- Latest-ten eligibility, backdated retention, pending visibility, cached associations and per-expense overrides.
- Offline permissions, restart durability, stable creation IDs, and interrupted online edits.
- Dropped responses after real server acceptance, deduplication, per-field conflict ordering, and unpublished deletion cancellation.
- A stalled refresh does not block local saving; untouched remote fields survive editing; dependent new payees survive cancelling another offline expense.

Command (JDK 17, SDK 35):

```sh
ACTUAL_FIXTURE="$PWD/tools/compatibility/.fixture/fixture.json" \
  ./gradlew :core:test :app:testDebugUnitTest :app:lintDebug \
  :app:assembleDebug :app:assembleDebugAndroidTest
```

All tasks succeeded. `app:testDebugUnitTest` has no source and is not included in the test count. Lint reported no errors; dependency-update and deprecated-backup-attribute recommendations remain. The signed release build and APK signature verification also passed.

## Android evidence

Android 16 / API 36 emulator:

- Six Compose instrumentation checks passed: amount-only entry, zero rejection, offline edit restriction, category override, delete confirmation, and completion after editor recreation.
- Real-server device flow passed: sign in, select budget/account, decode the downloaded snapshot through Android SQLite, save 4.27, and observe confirmed sync. Restart restored the account/session.
- Impeccable visual review covered the real setup/register/editor/result screens, dark mode, native date dialog, 1.5× text, and tablet width. Screenshots are in `docs/screenshots/`.
- Signed release: completed setup, disabled connectivity, saved 8.91 locally, and verified the official oracle had no matching record. An in-place `adb install -r` of the same signed 0.1.0 APK preserved the account, session, and pending expense.
- With `RUN_ANY_IN_BACKGROUND=ignore` and the app force-stopped, restoring connectivity did not publish the expense. Foreground reopening emptied the queue; the independent oracle confirmed exactly one `-891`, cleared expense. Network and app restrictions were restored afterward. Screenshots: `release-pending-before-update.png`, `release-pending-after-update.png`, and `release-recovered.png`.

The reinstall check validates Android's update installation path and data retention with the same signing identity. It does not exercise a future version's state migration. Background restriction plus force-stop validates retained work and foreground recovery; it does not isolate every manufacturer's background policy or guarantee delivery timing.

Android 8.0 / API 26 emulator (minimum supported OS):

- The same six Compose instrumentation checks passed (14.7 seconds).
- The real-server device flow passed (9.0 seconds), including Android snapshot decoding and durable local storage. The independent official client confirmed exactly one `-427`, cleared expense.
- The signed release installed and launched its setup screen successfully; see `docs/screenshots/release-api26-setup.png`.

Intermediate Android releases and physical devices were not individually exercised.

## Release identity

Artifact: `dist/ActualExpenses-0.1.0.apk` (local generated output, excluded from Git).

APK SHA-256:
`b96a9d021301d8baaa52a541142b0bb5933b96848fc5e42330e1bf2b73eb8815`

Signing certificate SHA-256:
`bb420a6b8dfdbcfd849a29f7c831d6b72b824fd411648dd8d92e9c6f85d8064c`

APK Signature Scheme v2 verified. The stable RSA signing identity is outside the repository under `~/.local/share/actualexpenses/signing`, with private directory/file permissions. Back up that directory privately; it contains both the key and its password. Future updates must reuse it. Install updates over the existing application; uninstalling removes pending work.

## Qualification boundaries

- Compatibility is deliberately pinned to **Actual 26.10.0**, unencrypted budgets and migration `1788468782000`. Another server version must be qualified before enabling writes.
- HTTP emulator-to-host access was exercised. Physical-phone LAN/tailnet routing and the deployed server's version were not available for verification. Tailnet configuration remains external to the app.
- The official API oracle independently verified server records. Actual Desktop's graphical interface was not manually exercised.
- Full-history sync and in-memory snapshot decoding are correctness-first choices. Large/long-lived budgets are not performance-qualified; see [limits](compatibility.md#current-protocol-limits).
- Background delivery is best-effort. Published-deletion races retain Actual's native semantics; the app does not promise an atomic server-side conflict check.

Review results: [standards and spec](review.md). Installation and rebuilding: [ReadMe](../ReadMe.md).

## Hosted release 0.1.1 — 2026-10-16

[GitHub Actions run 38996712861](https://github.com/glengerbush/ActualExpenses/actions/runs/38996712861) succeeded from commit `b21c0e26eaa613117e0cbbbffca2c285aed7b1f4`: version-bump detection, nine automation tests, disposable-server integration tests with no skipped cases, release lint, signed build, signature verification, and publication. The first run failed because the extra SDK setup action requested the obsolete `tools` package; using the hosted runner's existing SDK resolved it.

[Release v0.1.1](https://github.com/glengerbush/ActualExpenses/releases/tag/v0.1.1) contains the APK and `SHA256SUMS`. The downloaded APK was independently checked: package `org.actualexpenses.android`, versionCode `2`, versionName `0.1.1`, minimum API 26; signature matches the original certificate above. APK SHA-256: `626615000817e6f094701ab59072ed16d3d4d8cb29c275fd8caab15b2e575a71`.

This patch updates release tooling and the app version; it introduces no app behavior changes. Earlier device evidence remains the baseline; no additional device tests were claimed for this hosted artifact.

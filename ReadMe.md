# ActualExpenses

A focused native Android companion for recording expenses in one Actual budget. Select an account, add an expense, and correct recent purchases. New expenses are cleared automatically.

## Install

Install the signed `dist/ActualExpenses-0.1.0.apk` (or `app/build/outputs/apk/release/app-release.apk` after building) on Android 8.0 (API 26) or newer. Allow installation from your chosen file manager when Android asks. Updates must use the same signing key and retain the application ID; install over the previous version rather than uninstalling, which deletes local pending work.

Setup: server address → existing Actual password → budget → account. HTTP LAN and tailnet addresses are supported. This version supports **Actual 26.10.0, unencrypted budgets only**. It checks server and snapshot compatibility before writes. It requires no extra server service.

The app remembers the account, caches recent expenses, and saves pending additions/modifications/deletions durably. Offline editing is limited to unsynced offline-created expenses; edits begun online survive connection loss. Background delivery is best-effort and runs only for pending work.

## Build

Requires JDK 17, Android SDK platform 35/build tools 35.0.0, and network access for Gradle dependencies. Set `JAVA_HOME` and `ANDROID_HOME`, or put `sdk.dir=/your/android-sdk` in ignored `local.properties`.

```sh
./gradlew :app:assembleDebug
python3 tools/build-release.py --initialize-signing
```

The release helper creates a stable private signing identity under `~/.local/share/actualexpenses/signing`, then builds `app/build/outputs/apk/release/app-release.apk`. Back up that directory securely; it contains the key and its password. Subsequent builds use `python3 tools/build-release.py`. Never commit signing files. For externally managed signing, supply the `ACTUALEXPENSES_KEYSTORE`, `ACTUALEXPENSES_STORE_PASSWORD`, `ACTUALEXPENSES_KEY_ALIAS`, and `ACTUALEXPENSES_KEY_PASSWORD` environment variables directly to Gradle.

## Verify

Recorded results, release fingerprint, and qualification limits: [validation](docs/validation.md). Independent review: [standards and spec](docs/review.md).

Start the disposable real Actual server and independent official API oracle described in [compatibility documentation](docs/compatibility.md). Then:

```sh
ACTUAL_FIXTURE="$PWD/tools/compatibility/.fixture/fixture.json" ./gradlew :core:test
./gradlew :app:lintDebug :app:connectedDebugAndroidTest
```

The integration suite skips real-server cases when `ACTUAL_FIXTURE` is absent; skipped tests are not a compatibility pass. Never point the fixture at a real budget. The official Node API is used only as an independent test oracle; the Android app is entirely native Kotlin.

## Boundaries

Only ordinary, unreconciled expenses appear. Income, transfers, splits, and reconciled entries remain in Actual. Ten newest transaction dates are shown, with pending changes and the last backdated addition retained as described in the [design](docs/design.md).

Native per-field logical ordering decides conflicts. An unpublished phone deletion is cancelled if a newer remote edit is discovered before publication; a publication race or already-published deletion follows Actual's native behavior without automatic restoration.

The initial protocol implementation replays the sync log for correctness, and holds the snapshot/cache in memory. Large budgets need performance qualification; see [protocol limits](docs/compatibility.md#current-protocol-limits). Android encrypts local session and budget data using a Keystore key and excludes it from backup. Authentication/server failures retain pending work.

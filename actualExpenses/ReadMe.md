# ActualExpenses

A focused native Android companion for recording expenses in one Actual budget. Connect to the server, add an expense, and correct recent purchases. New expenses are cleared automatically.

## Install

Download the signed APK from the [latest GitHub release](https://github.com/glengerbush/ActualExpenses/releases/latest), or use `app/build/outputs/apk/release/app-release.apk` after building. Install on Android 8.0 (API 26) or newer. Allow installation from your chosen file manager when Android asks. Updates must use the same signing key and retain the application ID; install over the previous version rather than uninstalling, which deletes local pending work.

Setup: actualBudget server address → expenses. The server’s single budget opens automatically and is created on first access. No budget name or chooser. No login or password. HTTP LAN and tailnet addresses are supported. The app checks the versioned API and required capabilities before connecting. It uses the Rust actualBudget server in this repository.

Updates support the immediately previous patch. The original Actual sync protocol and its saved-state migration are no longer supported.

The app caches recent expenses, and saves pending additions/modifications/deletions durably. Offline editing is limited to unsynced offline-created expenses; edits begun online survive connection loss. Background delivery is best-effort and runs only for pending work.

## Build

Run the following commands from `actualExpenses/`. Requires JDK 17, Android SDK platform 35/build tools 35.0.0, and network access for Gradle dependencies. Set `JAVA_HOME` and `ANDROID_HOME`, or put `sdk.dir=/your/android-sdk` in ignored `local.properties`.

```sh
./gradlew :app:assembleDebug
python3 tools/build-release.py --initialize-signing
```

The release helper creates a stable private signing identity under `~/.local/share/actualexpenses/signing`, then builds `app/build/outputs/apk/release/app-release.apk`. Back up that directory securely; it contains the key and its password. Subsequent builds use `python3 tools/build-release.py`. Never commit signing files. For externally managed signing, supply the `ACTUALEXPENSES_KEYSTORE`, `ACTUALEXPENSES_STORE_PASSWORD`, `ACTUALEXPENSES_KEY_ALIAS`, and `ACTUALEXPENSES_KEY_PASSWORD` environment variables directly to Gradle.

## Releases

Android and actualBudget release independently. Android keeps its existing application ID and signing key; new tags use `actualExpenses-vX.Y.Z`, while historical `vX.Y.Z` tags remain valid. Both `versionName` and `versionCode` must increase for an Android release. See [release automation](../docs/releases.md) for signing secrets, dry runs, candidates, checksums and publication gates. No release is created by this integration work.

## Verify

Current API/device evidence and qualification limits: [Android API validation](../docs/android-api-validation.md). Historical protocol/signing evidence remains in [validation](../docs/validation.md).

From `actualExpenses/`, build the sibling server and run Android checks:

```sh
cargo build --manifest-path ../actualBudget/server/Cargo.toml
ACTUALBUDGET_BINARY="$PWD/../actualBudget/server/target/debug/actualbudget-server" \
  ./gradlew :core:test
./gradlew :app:lintDebug :app:connectedDebugAndroidTest
```

Native API tests start isolated Rust servers and never use a real budget. The Android device HTTP test additionally needs instrumentation argument `fixtureServer` pointing to an isolated fixture with a budget named `Native device fixture` and account `Spending`. Without it that device case is not qualified.

## Boundaries

The companion displays ten newest eligible expenses, pending changes, and the last backdated addition. New expenses clear automatically; optional fields stay optional. Existing synced expenses cannot be edited offline; edits opened online retain their original values, currency precision and budget generation across connection loss and screen restoration.

The server resolves per-field logical ordering. A newer edit can reject an unpublished deletion. Published deletions remain deleted. Unknown operations captured before a budget restore stay queued for reconciliation; the app never changes their generation automatically.

Only bounded expense views and reference/change data cross the new API. Idle foreground refresh does not download full transaction history or rewrite unchanged cache data. Background delivery is best effort, only while changes are pending. Android encrypts local data with its existing Keystore identity and excludes it from backup.

Transfers, income and splits stay outside the expense companion. Payee/category suggestions and manual overrides use the native API.

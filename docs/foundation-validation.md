# Ticket 01 — combined repository foundation

Validated locally on 2026-11-08. Review start: `c73bbc3` (approved planning baseline). This is evidence for repository relocation and reference setup, not completion of the web rewrite.

## Preserved application

- Android source, Gradle project, development design context and Android-only tooling now live under `actualExpenses/`. Shared documentation and GitHub workflow remain at the repository root. Root `ReadMe.md` preserves existing external links and points to both applications.
- Compared 36 Android source/build files against the review baseline: unchanged, with existing Windows wrapper CRLF normalized only for comparison. No Kotlin, manifest, version, SDK target, application ID or signing configuration behavior changed.
- The release workflow runs inside the Android directory. Release version detection checks both historical and current repository paths from the Git root, so relocation alone cannot create a release and a later version bump remains detectable.

## Executed checks

Commands below use the existing local JDK 17 and Android SDK 35/build tools 35.0.0. Android commands run inside `actualExpenses/` unless stated otherwise.

| Check | Result |
| --- | --- |
| `python3 -m unittest discover -s tools -p test_release.py -v` | 12 passed. Three new real-Git-history CLI cases failed before the change and passed afterward: unchanged relocation, a bump across layouts, and invocation from the Android directory. |
| `node tools/compatibility/fixture.mjs` with a fresh temporary `ACTUAL_FIXTURE` | Published Actual server/API 26.10.0 started on loopback with disposable state. |
| `node tools/compatibility/smoke.mjs` using that fixture | Passed independent setup, seeded association, exact amount, cleared create, update and deletion. |
| `./gradlew --no-daemon :core:test :app:lintRelease :app:assembleDebug` using that fixture | Successful. XML reports: 16 tests, 0 failures, 0 errors, **0 skipped**. Debug APK built. Release lint: 0 errors, 9 warnings (8 dependency-version suggestions and 1 DataExtractionRules warning). |
| `python3 tools/build-release.py` | Signed release built using the existing local signing identity, without generating/uploading keys. |
| SDK `apksigner verify --verbose --print-certs` | APK Signature Scheme v2 verified. Signing certificate matches both the prior 0.1.2 APK and the recorded original certificate. |
| SDK `aapt dump badging` | `org.actualexpenses.android`, versionName `0.1.2`, versionCode `3`, minSdk 26, targetSdk 35. |
| Reference importer `--check` and staged Git blob verification | All 3,088 retained working files and index blobs match the original SHA-256 manifest. Scoped Git attributes disable text normalization, preserving CRLF financial fixtures in fresh checkouts. |
| Python compilation; release workflow YAML parse; `git diff --check`; README link checks | Passed. Git ignores local signing/configuration, fixture credentials, build artifacts and node dependencies. |

Built APK: `actualExpenses/app/build/outputs/apk/release/app-release.apk` (ignored build output).

APK SHA-256: `8f3519f433d5aa922d08dcab1cd0a5d2ef927da3d6f8b03c5d9bd5b6c070dd56`.

Signing certificate SHA-256: `bb420a6b8dfdbcfd849a29f7c831d6b72b824fd411648dd8d92e9c6f85d8064c`.

## Reference provenance

The source-only baseline is Actual 26.10.0 at commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`. The importer retains relevant web, engine, API/CLI, synchronization, shared component, test and written documentation sources. It excludes 1,121 documented paths, including Electron/native-mobile support, upstream workspace/build/release configuration and unrelated agent/editor/tooling assets. The imported tree contains no Git repository and no executable upstream workspace; retained files are byte-for-byte copies with original SHA-256 hashes. The MIT notice is retained.

The [reference guide](../actualBudget/README.md) explains reproduction and the independent published-package oracle. The [parity ledger](parity-ledger.md) assigns all 107 stories to tickets and identifies reference evidence families; replacement parity remains pending its feature tests.

## Limits

No live server, Coolify configuration, GitHub release, signing secrets or version fields changed. No device workflow was rerun for this mechanical relocation; the previously recorded device evidence remains historical. The user cannot access the Docker socket, but rootless runc is installed and user namespaces work; container rehearsal remains a separate qualification step. The source reference is deliberately not a second buildable React/Node application; the successful runnable baseline checks above use the pinned published server/API fixture as normal user processes.

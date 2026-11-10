# Import and dashboard integration — accepted

Accepted locally on2026-11-10 through `79ef888`: reviewed25/26 through
`133bb22`,33 through `f817118`, and previously accepted49.34/custom reports are
separate work. No release, push, version change or live deployment.

## Review and merge

Consolidated25/26 Standards/Spec/Impeccable review passes. Source/backend, native
feed and contract evidence is in `transaction-import-validation.md`; final UI
fixes preserve exact request identity, privacy, numeric success counts and
quota-failure recovery through discard/reupload.33 review passes after fixing
fresh-connection predicates and replacing a blanket historical horizon with
actual output/work bounds; see `reports-validation.md`.

Merge retains both router and cancellation/receipt registrations,75 advertised
capabilities, independent register/export predicate helpers, both sets of served
OpenAPI operations and63 maintained qualification commands. Native restore
schemas now include22 without dropping6–21. Svelte check, format, production web
build, Rust build and bin Clippy with warnings denied pass. Running served-schema
consumer and reporting API consumer pass on the combined runtime:
`/tmp/import-report-merge-contract.log`,
`/tmp/import-report-merge-reports-api.log`.

## Completed qualification

- Full Rust run:454 passed,0 failed,0 ignored across36 targets,
  `/tmp/import-report-full-rust-final.log`. Initial run stopped at a stale
  current-schema assertion20; updated that assertion to22, retaining the genuine
  schema17 fixture and every financial/history/receipt assertion.
- Full native API suite:44 passed,0 skipped/failures/errors,
  `/tmp/import-report-full-native.log` and the Gradle JUnit XML. Runtime and native
  test source are unchanged by the subsequent UI-only corrections.
- Reporting native/public-contract consumer passes genuine21→22 upgrade,
  exact receipts, companion money, duplication, original source field clocks,
  native22 layout round trip and genuine21 restore/epoch fences:
  `/tmp/import-report-reporting-native.log`.
- Production container batch:62/63 commands passed in
  `/tmp/import-report-container.log`; the remaining concurrency browser command
  passed separately on the same immutable image after test-only correction
  `79ef888`, independently reviewed by root. This is coverage across recorded
  runs, not an uninterrupted63/63 batch. A valid reachability wakeup retried a
  saved cancellation after its lost acknowledgement; exact attempt count2 was
  racy. Phase-based checks now require the lost-ACK barrier, repeated fenced
  retry and terminal recovery, asserting unchanged body/key/HLC/epoch/owner on
  every request. Delayed original409, no resurrection and exactly-once financial
  counts remain checked. Minimal red/green and whole-file green logs:
  `/tmp/concurrency-cancel-repro-red.log`,
  `/tmp/concurrency-cancel-focused-green.log`,
  `/tmp/concurrency-cancel-whole-green.log`.

Exact image used for scoped verification:
`actualbudget-qualification:imports-reports-8a49adb`,
SHA256 `0916cc06e931484de0bd77f28171d553693966e74453a894dceabb8b5a964799`,
139213264 bytes, user `10001:10001`. All checks use disposable data/volumes.
Disposable containers, volumes and image alias/image removed after qualification.
Screenshots retained under `.scratch/qualification-imports-reports/`.
No unmeasured latency or power-efficiency claim; whole-system qualification
remains ticket51. No new Android UI/device/APK claim; unchanged device evidence
is reused.

Immutable schema22 executable preserved as
`.scratch/actualbudget-schema22-8a49adb-server` (mode0555), SHA256
`764c8b0cf48e61d8ca36376a94adf35dec1dd771ee97e79398fc5c3035eb94ff`.
Server source and Cargo inputs verified unchanged from `8a49adb`; subsequent
changes were tests, UI and records. Never relabel older fixtures as schema22.

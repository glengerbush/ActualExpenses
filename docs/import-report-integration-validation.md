# Import and dashboard integration — in progress

Development branch `codex/integrate-imports-reports`, current integration
`8a49adb`: reviewed25/26 through `133bb22`,33 through `f817118`, and previously
accepted49.34/custom reports are separate work. No main acceptance, release,
push or live deployment claimed until the remaining qualification closes.

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

## Qualification so far

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
- Production container batch still running all63 commands with keep-going:
  `/tmp/import-report-container.log`. Existing `concurrency-browser.mjs` saw
  cancellation attempt count3 rather than2; diagnosis is isolated while the
  remaining batch continues. No claim of a green container batch yet.

Exact image retained for scoped verification:
`actualbudget-qualification:imports-reports-8a49adb`,
SHA256 `0916cc06e931484de0bd77f28171d553693966e74453a894dceabb8b5a964799`,
139213264 bytes, user `10001:10001`. All checks use disposable data/volumes.
Remove the retained image alias after qualification. No unmeasured latency or
power-efficiency claim; whole-system qualification remains ticket51.

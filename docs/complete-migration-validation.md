# Complete migration and Android upgrade qualification

Ticket50 qualification, 2026-11-10. Review baseline `b4640cc`. Disposable data
only; no version bump, publication or production deployment.

## Combined installation witness

`actualBudget/tests/fixtures/complete-migration/` contains four genuine pinned
Actual26.10.0 exports. The generator uses the source API/engine, exports/reloads
each budget and records independent AQL observations. Its `--check` mode passed.
Source commit: `2bebdbaae0b701120a8cad9d56aafd64fa04c577`.

| Budget | Source transaction rows | Purpose |
| --- | ---: | --- |
| Empty | 0 | Default records and empty financial state |
| Complete | 49 | Reports/plans, transfers/splits, hidden/closed/off-budget references, rules, schedule, notes, tags, location, merged payee, grouping, saved filter, templates and cleanup pool |
| Large | 10,049 | Same features plus 10,000 genuine purchases |
| Deleted history | 10,049 | Same features after deleting those purchases; 10,000 tombstones retained |

All four coexist in one replacement installation. The maintained HTTP check
verifies every source table count, exact original ZIP download and conversion
audit; restarts after staging; activates each budget; restarts and replays the
same activation receipt. It reconciles financial rows, IDs, relationships,
allocation/goal values, automation definitions, saved report metadata, notes,
rules, schedules, references and selected operational preferences. Independent
source account balances match. Each native backup is restored and reconciled
again; original archives/audits remain byte/value exact. Inspection uses HTTP
and the public portable archive format, never the running server database.

The separate genuine linked-account witness passes: accounts become manual,
financial/import history is preserved, operational provider state is absent,
retired preference flags are refused, and backup/restore retains that behavior.
Preserved original source archives remain available for audit. See
[manual retirement](bank-retirement-validation.md).

## Recovery and Android

Seventeen focused Rust HTTP tests pass: four Android/schema-upgrade cases,
twelve Actual-import cases, one lifecycle-cancellation case. They cover corrupt
and encrypted inputs, ZIP/column ambiguity, limits, invalid/future clocks,
atomic activation, cancellation fencing, duplicate/replayed requests and genuine
schema17 data/archive upgrades. The browser import workflow passes lost upload
and activation acknowledgements, reload, stale-tab exclusion, quota failure,
financial source parity, unknown-content blocking/original download and corrupt
input recovery over plain HTTP.

The current companion passes **52 native HTTP integration tests, zero skipped**,
using Java17, the current Rust server and the preserved schema17 server. This
includes app-first upgrade, original pending-operation identity/order, independent
field merging, lost acknowledgements and recovery across the later feature set.

A fresh API36 emulator run installs the released `2b25b07` APK, seeds its actual
encrypted store, upgrades to the current APK, follows visible recovery controls,
drops a real post-commit acknowledgement, reinstalls the current signed APK,
then recovers on the foreground. Both install boundaries preserve ciphertext
exactly. All four original queued operations retain their body, ID and original
field clocks; no replacement batch timestamp is sent. The independent Actual
oracle matches two final rows with total/cleared **−1801 minor units**, uncleared
zero. The newer edit defeats the old deletion and produces visible feedback.
The recovered screen was visually inspected. Owned emulator/proxy/backend stopped.

This is an in-place **test-signer** upgrade, not a production-signing exercise or
physical-phone test. Both APKs have certificate SHA256
`b0c6a407a17a2c5951093fb31cff5cfad5fbb017dee6a6f2bd97fff675bbc102`.
Production certificate/artifact verification remains in
[release validation](release-validation.md). Previously accepted background
restriction/idle and LAN/tailnet relay evidence remains in
[Android upgrade validation](android-upgrade-validation.md); this run does not
claim a new remote Tailscale tunnel test.

## Evidence and review

Maintained commands:

```sh
npm --prefix actualBudget/web run test:migration-source
npm --prefix actualBudget/web run test:migration-api
cargo test --offline --manifest-path actualBudget/server/Cargo.toml \
  --test imports --test android_upgrade --test lifecycle_cancellation
cd actualBudget/web && node tests/imports.mjs
```

Source and combined HTTP qualification are included in the release workflow.
Native test command uses `:core:test --tests
org.actualexpenses.core.NativeApiIntegrationTest` with `JAVA_HOME` set to the
local Java17 installation, `ACTUALBUDGET_BINARY` pointing at the current binary
and `ACTUALBUDGET_PRE_UPGRADE_BINARY` at `.scratch/actualbudget-schema17-server`.
Current debug and instrumentation APK builds pass.

Local logs: `/tmp/qualification50-source-check.log`,
`/tmp/qualification50-http-final.log`, `/tmp/qualification50-manual-source.log`,
`/tmp/qualification50-server-tests-final.log`,
`/tmp/qualification50-browser-final.log`, `/tmp/qualification50-native.log`,
`/tmp/qualification50-apk-build.log`, and
`/tmp/qualification50-device-{seed,recovery,foreground}.log`.
Retained device evidence: `.scratch/qualification50-device/evidence.json` with
APK, encrypted-state, install-boundary, screenshot and financial-verification
hashes. Verification JSON SHA256:
`748816b71305e82aa780eb424d72ff243f079f6e5fdcc9315272b56179376583`.

Consolidated root standards/spec review found stale historical expectations,
not a financial converter failure: template-rule activation became supported
in42; schema27 deliberately invalidates downloaded cursors on connector
retirement. Updated tests retain exact financial/receipt/epoch checks and keep
unknown archive content blocked. Schema assertions now use the public restore
report instead of inspecting server SQL. Formatting and patch checks pass.

This qualification combines the accepted feature converters; it is not the
107-story parity audit or a whole-system performance claim. Tickets51/52 retain
those obligations, including measuring large-import/restore costs in optimized
builds. Ticket53 retains the copied-storage Docker rehearsal and operator
cutover/rollback runbook. Historical Coolify settings are not freshly inspected;
no dashboard/SSH access is required for local rehearsal.

# Payees and tags verification — ticket11

Accepted through `d6a5005`; independent corrective/integration Standards and Spec
reviews found zero actionable findings. Full production-container browser checks
passed. No version bump,
release, push or production cutover.

Delivered: payee rename/favorite/merge/remove and unused search; durable aliases
and retained removed references; imported payee text; exact note-tag discovery,
rename/metadata/hiding/removal; register payee/tag filters; online management with
durable drafts, immutable retries, cancellation fences and corrected-clock recovery.
Android choices exclude deleted/transfer payees and refresh after merge/removal.
Rules/category learning remains19; real incremental transfer producer coverage15.

## Validation

- 92 running-server HTTP tests pass, including9 payee/tag suites and genuine
  schema8 database/ZIP migration preserving both account/category domains.
- 21 native public-API tests pass, zero skips. New cases cover incremental payee
  merge/remove, unchanged queued alias retry after restart, inline dependencies
  and genuine imported transfer-payee exclusion. No device-test claim for this
  projection-only native change.
- Clippy with warnings denied, rustfmt, Svelte check/build and served OpenAPI pass.
- Full production-container browser suite passes, including payee/tag, category,
  account and management recovery, exact receipt replay, read-only misses and
  persistence after container replacement. Disposable test storage removed.
- Android debug APK and lint pass. No production signing or publication performed.
- Pinned Actual26.10.0 public fixtures and the pinned tag helper are independent
  oracles. Synthetic long-chain/cycle input is explicitly robustness evidence.

Logs: `.scratch/11-integrated-{public,clippy,contract}.log`,
`.scratch/11-schema8-integration.log`, `/tmp/native11-integrated.log`,
`/tmp/native11-app-build.log`, `.scratch/11-container-locale-final.log`.
Qualified image: `sha256:0d3df955bf6645350d0e5ff5c3c2e825b1f9b8b9dc6f0c50dba225453c1fa1ec`,
128,203,479 bytes; runtime user `10001:10001`.
Source/fixture contracts: `actualBudget/server/PAYEES-TAGS.md`,
`actualBudget/tests/fixtures/PAYEES-TAGS.md`.

## Review fixes and integration

Original independent reviews found: native reference choices included removed or
transfer payees; payee-only visibility leaked into tag view; a losing name rejected
an independently winning field; a corrected device clock could not recover the
local generator. Each has a public/native/browser regression. Cancellation also
captures and rechecks its original identity across capability negotiation.

Schema9 follows account7/category8. Combined routing and change-feed metadata
retain both domains. Archive defaults accept6–8; source backfills preserve native
edits, raw historical clocks, original imports and exact old receipts. Hidden
account-transaction edits retain the earlier deletion-boundary fix. Superseded
generic reference routes were removed after both dedicated domains replaced them.

Incumbent Impeccable design retained. Svelte analysis found no issues; integrated
desktop/mobile tags and clock-recovery renders passed independent finish review.
Existing Impeccable context-path drift was reported and left untouched.

Container qualification initially exposed a locale-test timing assumption: after
a remote format change, cached views intentionally remained usable before refresh.
The test now waits for the new visible decimal format before opening an editor,
which correctly captures the displayed format. No application behavior changed.
Original failure: `.scratch/11-container-final.log`; focused check:
`.scratch/11-register-locale-green.log`.

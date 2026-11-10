# YNAB and custom-report integration — accepted locally

Accepted2026-11-10: frozen production composition `0a284f2`, followed by reviewed
test-only fixture isolation `0f21abb`. Includes custom reports through locale
correction `055e79e` and complete YNAB migration `bbfa627`. Tickets27 and34 are
complete. No push, version bump, release or deployment.35 and28 remain separate;
47 started from this reviewed composition and now consumes28's frozen schema23
foundation for its independently reserved schema24 work.

##34 review and merge

Consolidated Standards/Spec/Impeccable review of `f817118...602394a` found two
P2s: an inner category-group join silently omitted supported ungrouped planned
allocations, and Net Payment disclosure contradicted its positive magnitude.
Both fixed in `84e5e08`; root independently reviewed the bounded correction and
its public/browser proof. Those two findings and the subsequent locale
integration finding are closed. See
[report evidence](reports-validation.md) for source77 cases, native/contracts,
accessible renders, failure recovery and focused logs.

Root merge audit against `a5c784c`: all102 public paths retained;75 old
capabilities plus `custom_reports`; all63 prior qualification commands plus
the custom API and browser consumers. Router and middleware registrations
retained. Only intended report query/cell/projection/inspection schemas changed;
native schema22 remains unchanged. Source-independent public regression proves
ordinary inline ungrouped category allocation5000 remains in Category/nested
totals and one-row allocation details. Net Payment browser58.00 and accurate
disclosure pass; unchanged source expectations retained. Focused corrections
reuse existing77 source/API proof, Svelte/build/autofixer and Clippy logs.

Root follow-up review found reporting controls bypassed the existing `t()` layer,
including Payment/Net Payment/Monthly/Category/Account for which the shipped
German catalog has exact translations. `055e79e` integrates the existing
translator without translating user-owned names or changing option/API values.
Real Settings German selection exposed nested dotted catalog keys rejected by
the old loader. The narrow loader correction preserves literal string keys,
flattens valid nested paths and rejects conflicts/invalid leaves. Root reviewed
code, visible German render, language-switch/save/60.00 invariance and existing
French/fallback/retry checks. A read-only structural audit confirmed all15
shipped catalogs compatible; German and Ukrainian contain nested objects.
Catalog bytes unchanged. New missing prose retains English fallback. Exact logs
and rendered screenshot are in the report evidence; no further broad rerun.

Bank28 source-only preparation `510128b` is also joined and independently
reviewed; it adds six core normalization/writer witnesses, not provider
implementation or28 acceptance. See `bank-sync-source-validation.md`.

YNAB4/source-learning checkpoint `e71bafd` passed consolidated Standards/Spec
review with no findings. It is not joined yet: nYNAB, failure/restart/native,
contracts and interface remain27 work before the complete slice is integrated.

Root reviewed nYNAB corrective/recovery delta `c2d98cd..630e4a6`: Standards and
Spec pass within that checkpoint. Two concrete P2 findings are closed: absent
starting-payee identity no longer aliases a null payee; blank schedule names
remain null and nonempty names use exact-case source collision semantics.
Five independent source cases cover income identity and22 unnamed/trim/case
schedules. Recurrence pattern discriminants remain in the public comparison.
Actual-only historical enrichment now excludes authoritative YNAB stage formats;
duplicate provenance identifies its format and native archives retain linked
upload receipts. Root inspected source, consumer assertions and passing logs
for both-format restart, duplicate, same-budget/fresh-server restore, original
bytes/receipts and malformed-input atomicity. Existing Actual archive regression
also passes. Remaining27 UI/contracts/companion qualification is not implied by
this bounded review; see `ynab-import-validation.md` in its pending branch.

Bank28 private-custody/claim checkpoint `510128b..00cf0a1` also passed bounded
root review: secret hashes in generic receipts, private credential exclusion,
durable claim intent before remote work, conservative one-use uncertainty,
generation/epoch fences and bounded HTTPS transport. It remains unmerged and
partial: financial/link/settings/replacement, genuine schema22 migration,
native lifecycle, interface and full contracts still require qualification.

Bank discovery/replacement delta `00cf0a1..46ca97a` review found one P2:
canceling an in-flight discovery writes a cancellation receipt but does not
remove its private capture, and the late response can still populate that
capture. Receipt replay then bypasses capture deletion, retaining canceled
responses against the64MiB recovery limit. Worker is correcting cancellation
cleanup and late-response fencing with a public delayed-provider witness.
Other reviewed metadata/transport changes have no actionable findings; this
checkpoint remains pending that correction and is not joined here.

The cancellation finding is closed in `7f71f2a`: deletion shares the canceled
receipt transaction, and late capture probes that receipt before writing bytes.
Root reviewed code and the true public capacity red/green: four canceled held
near16MiB responses no longer prevent the next2MiB capture, canceled outcomes
remain exact, metadata/ledger stay unchanged and requests still count to quota.
Known captured error codes are preserved, without claiming a separate
interrupted-error restart witness. A second byte-bound issue is corrected in
`f45aaa1`: both SQLite sums count UTF-8 blob bytes rather than TEXT characters.
This narrow correction is reviewed; fresh foundation build/runtime and Clippy
pass at `5489ac1` (`/tmp/bank28-foundation-{green,clippy}.log`). Root also reviewed
that foundation's final23 DDL and portable metadata registry. It is not whole28
acceptance and is not joined to this production composition.

## Final27 review and composition

Root reviewed final interface/contracts/companion changes and the bounded
YNAB4 sparse-carry correction through `bbfa627`. The source importer propagates
carry flags through its current-month-plus12 horizon; missing months now retain
zero-amount carry cells with explicit resets and100000 cell/work limits. Two
genuine SDK executions of labeled synthetic inputs expose gap/reset behavior;
all nine public cases pass. No source formula is copied into expected outputs.

Standards/Spec/Impeccable review has no unresolved27 findings. Root inspected
mobile and dark enlarged-text renders, saved format/filename/operation behavior,
visible source-model differences, privacy-aware balances, typed API contracts
and companion offline delivery assertions. Existing Actual import behavior,
retained originals and schema22 remain covered. Source-specific limitations are
visible; no physical-device test or full offline budgeting claim is made.

Merge conflicts were limited to capability and browser-command lists. Both
intents are retained:105 paths with every parent operation definition unchanged,
both capability sets and68 maintained browser commands. The six YNAB source
checks remain wired into release verification. Svelte check, format, build and
merged App autofixer pass. Broad Rust, native and container checks are running;
their results are not yet acceptance evidence.

## Integrated qualification

- Full Rust:454 passed,0 failed/ignored,36 targets;
  `/tmp/ynab-custom-full-rust.log`. Bin Clippy with warnings denied and build
  pass in `/tmp/ynab-custom-{clippy,build}.log`.
- Web check0 errors/warnings, formatting, build and merged App autofixer pass;
  `/tmp/ynab-custom-web-{check,format,build}.log` and
  `/tmp/ynab-custom-app-autofix.log`. Existing bundle-size advisory remains.
- Served OpenAPI consumer passes, including both import formats;
  `/tmp/ynab-custom-contract.log`.
- Native integration:44 passed plus one initially skipped because root supplied
  the prior binary under an incorrect environment name. Corrected
  `ACTUALBUDGET_PRE_UPGRADE_BINARY` and reran only that case:1 passed,0 skipped.
  Thus all45 cases pass across the two runs, not one uninterrupted45-case run.
  Logs `/tmp/ynab-custom-full-native.log` and
  `/tmp/ynab-custom-native-upgrade.log`; original/focused XML preserved in
  `.scratch/qualification-ynab-custom/`.
- Production container batch:66/68 commands passed in
  `/tmp/ynab-custom-container.log`. Both failures were shared-server fixture
  contamination: reports-api renamed the original dashboard before the custom
  browser duplicated it; prior YNAB consumers had already activated its original
  files. Product behavior and financial expectations were correct.
- Test-only isolation `0f21abb` adds unique ZIP comments or JSON whitespace.
  Every ZIP member and complete parsed JSON were independently checked unchanged.
  All original first-commit/lost-ACK/hash/download/financial assertions remain.
  Existing importer worker independently reviewed the narrow diff: Standards
  and Spec pass, no findings. Same-container sequence reports-api → custom
  browser → YNAB browser twice passes4/4 in
  `/tmp/ynab-custom-scoped-container.log`. All68 maintained commands are covered
  across recorded runs; no claim of an uninterrupted68/68 batch.

The final scoped container also passes plain HTTP/no-login assets, exact opening
balance, persistence and receipt replay after replacement. Image
`sha256:ddacf20346d5c27096e392eb5cfab983fbdcba26ec54e68761bfebafa1d7e8fb`,
139904307 bytes, user10001:10001. Both runners removed their disposable containers
and volumes; root removed the retained scoped image alias/image. Render artifacts
are retained under `.scratch/qualification-ynab-custom/`; YNAB captures also remain
in `actualBudget/web/test-results/ynab/`. No new physical-device, live-provider or
resource-efficiency claim; unchanged earlier device evidence remains valid and51
owns whole-system measurements.

Immutable genuine22 executable for later upgrades:
`.scratch/actualbudget-schema22-0a284f2-server`, mode0555,
SHA256 `fa0146112b54a00ab78f5b7b1c9d4363256efb25c45aff49356a3b45b22172e0`.
Earlier immutable executables remain untouched. No unresolved27/34 review or
qualification gate remains.

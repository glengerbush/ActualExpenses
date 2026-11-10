# YNAB and custom-report integration — pending

Integration branch `codex/integrate-ynab-custom`, frozen composition `0a284f2`:
reviewed custom reports through locale correction `055e79e` and complete YNAB
migration `bbfa627`. Accepted main remains `a5c784c`; no27/34 acceptance, push,
release or deployment yet.35 and28 remain separate.47 starts from the reviewed
composition while root qualifies it. One combined verification batch is running.

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
This narrow correction is reviewed; its next financial build/lint is pending.

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

## Remaining gates

- Qualify the frozen composition; reuse unchanged prior acceptance evidence.
- Record exact checks, limitations and disposable resource cleanup before
  fast-forwarding main and closing notes. No new acceptance claimed here.

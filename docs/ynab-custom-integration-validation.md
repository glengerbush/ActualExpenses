# YNAB and custom-report integration — pending

Integration branch `codex/integrate-ynab-custom`, initial reviewed custom-report
head `84e5e08`, followed by reviewed locale correction `055e79e`. Accepted main
remains `a5c784c`; no34 acceptance, push, release
or deployment. Ticket27 is still independently implementing;35 has its own
worktree and is not included. Combine final qualification where possible.

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

## Remaining gates

- Complete and review27 before joining its public import/API/UI/native slice.
- Qualify the frozen composition; reuse unchanged prior acceptance evidence.
- Record exact checks, limitations and disposable resource cleanup before
  fast-forwarding main and closing notes. No new acceptance claimed here.

# Ordered edits and incremental refresh — ticket 05

Accepted source: `8fe9b38444b98626f5e7670d228b2213ba3d376c`.
The [public contract](../actualBudget/server/CONCURRENCY.md) defines per-field HLC
ordering, durable receipts, cursor retention and explicit cancellation. The
[independent baseline](concurrency-baseline.md) records seven real Actual 26.10.0
sync/API scenarios and the intentional stronger unpublished-deletion policy.

## Observable results

- Independent fields merge regardless of arrival order; older same-field changes
  lose with conflict feedback. Equal physical time uses counter then node order.
  Off-budget moves retain a newer latent category in either arrival order;
  duplication and unrelated notes edits preserve it.
- Original body, operation identity and edit clock survive interruption/reload.
  Genuine schema3 receipts replay exactly after migration. Historical receipts
  need not contain fields added to later read responses, such as `sort_order`.
- An unpublished deletion older than a known edit returns a durable rejection.
  Published tombstones remain hidden while later winning ordinary fields and
  clocks still merge, matching the independent baseline.
- Change cursors use a budget epoch and server sequence, separate from edit time.
  Late older edits remain discoverable; coalesced pagination retains current
  entities. Unchanged refresh reads a bounded response without rewriting data.
- Skew/overflow failures retain the original request. Explicit cancellation
  atomically returns its existing receipt or records a terminal fence against
  delayed delivery. Browser intent survives a request lost before delivery,
  acknowledgement lost after commit, reload and overlapping tabs. An older
  server lacking the capability receives no cancellation mutation. Recovering
  an existing receipt preserves conflict/deletion notices and observed clocks.

## Verification

Root verification at the candidate: 50 real HTTP tests pass, with no skipped
tests: 11 concurrency, 16 ledger/core, 12 import, 5 preferences, 2 snapshot and
4 theme groups. Rust formatting, Clippy with warnings denied, all frontend
formatting and expanded served-OpenAPI response validation pass. Cancellation
schemas are exercised for create, PATCH, DELETE and duplicate, including exact
late replay and already-committed outcomes.

The browser scenarios exercise two clients, superseded fields/deletions, hidden
tombstone edits, immutable retry, unchanged refresh, clock recovery, safe
cancellation in both arrival orders, failure before/after delivery, unsupported
server capability, and notes-only preservation of a latent category. Svelte
checks/autofixers and Impeccable checks pass. Root inspected desktop conflict and
mobile clock-recovery screenshots; controls and explanations remain readable.

The local debug refresh probe measured 100 unchanged refreshes before/after
5,000 transactions: median 1.534/1.546 ms, p95 1.662/1.810 ms, 95-byte responses and
zero process storage-write bytes. These scoped measurements do not establish
whole-system production or power efficiency; ticket 51 owns that qualification.

Independent Standards and Spec reviews report zero remaining findings through
`8fe9b38`. The Standards reviewer independently reproduced the original held
request race: cancellation now makes the delayed request return409, leaving zero
unintended transactions. The Spec reviewer independently verified both recovered
conflict and hidden-row notices. Final combined container qualification passed
the complete browser suite, including the new receipt-feedback cases, plus
persistence and exact retry after container replacement. Image:
`sha256:fe5b671a6f35a5a1727b94bcf043073f8551f7bee463e9fa45feb9e5e72910cf`,
125,457,314 bytes, runtime user `10001:10001`. Disposable resources were removed.
Earlier findings for this frontier were resolved:
uncertain retry handling, independent account/category merge, parsed dark-theme
context, imported equal/NULL ordering, and recovered-receipt feedback. No
production deployment occurred.

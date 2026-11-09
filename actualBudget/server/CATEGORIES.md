# Category organization

`/api/v1/budgets/{budget}/categories` and `/category-groups` support paginated GET and POST. Item URLs support GET, PATCH and DELETE. Writes require `Idempotency-Key`; optional `X-Actual-Edit-Clock` orders each submitted field independently. Capture `X-Actual-Budget-Epoch` when starting the intent. Receipt lookup precedes generation checks. Category management is online; unfinished requests retain their original body, key, clock and epoch across reload.

Names, notes, hidden state, group and fractional order are persisted. Order is ascending, then name and ID; a preserved null position sorts first. Income/expense classification is immutable. New moves require a matching group. Existing Actual imports can contain independently classified categories/groups; both source flags are preserved, and the interface places the category under its own financial type.

DELETE requires `{ "replacement_id": "category UUID" }` to merge/reassign or `{ "replacement_id": null }` to uncategorize. Group deletion applies to all its categories. A target must be active, of the same category type and outside the removed group. Mixed-type imported groups must reassign their categories separately before deletion to a single target. A stale deletion cannot overwrite a newer category edit.

Reassignment atomically updates all transaction references, including reconciled and deleted transaction history. IDs, signed integer amounts, dates and transaction notes do not change. Categories retain tombstones, original category notes and replacement links; delayed clients resolve retired references without resurrecting categories. Source notes remain accessible in the retained-notes section, matching pinned Actual's behavior.

`/categories/{id}/transactions` exposes visible history and exact minor-unit totals. `id=uncategorized` selects null references. Pagination accepts a snapshot guard; any intervening budget change rejects continuation. Hidden categories remain available to management/history, and new-entry selectors exclude hidden categories/groups.

## Imports and recovery

The native archive registry includes category groups, all category metadata, tombstones, replacement links and field clocks. Schema-6 archives receive explicit metadata defaults; only category order fields accept fractional archive numbers. Financial numbers remain exact integers.

For genuine pre-category-management databases and schema-6 archives, metadata previously ignored by the importer is reconstructed from retained original Actual files. Only newly introduced metadata on existing imported categories is enriched; transaction changes, native categories, names and classification remain authoritative. Original per-field ordering is retained. Ready legacy stages enrich their category metadata during activation. Re-uploading a formerly blocked archive with a new operation identity explicitly reviews the old stage with the current converter; its old operation key still returns its original receipt.

Actual's category deletion may materialize zero-valued budget cells. Only cells with amount=0, carryover=0 and null goals are accepted as empty defaults; their original rows remain in provenance. Nonzero allocations, goals and other unsupported budgeting features retain explicit blocking conversion requirements. Those features belong to the budgeting tickets, not category organization.

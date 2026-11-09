# Payees and note tags

Ticket 11, stories 27/30. Rule learning and rule management remain ticket 19.
No login or secure-context API is required. Nonexpense management starts online.

Payees have durable IDs, favorite state, removal tombstones, optional transfer
account/category metadata, and retained merge aliases. Ordinary lists exclude
removed/transfer payees. `include_deleted`, `include_transfers`, `q`, and `unused`
allow management searches. Transfer payees derive their name from the account;
ordinary transaction entry and payee management cannot turn one into a merchant.

`PATCH /payees/{id}` updates name/favorite. `POST /payees/batch` atomically merges,
removes, favorites or unfavorites selected IDs. Merge requires an active ordinary
target outside the source set. It redirects alias chains and all transaction
references, including hidden history, without changing money, dates, imported
payee text or original transaction field clocks. Old queued IDs resolve through
the retained aliases. Removing a payee leaves raw history intact: public
`payee_id`/`payee_name` become null, `retained_payee_id` retains its identity,
and `imported_payee` retains source text. Later unrelated edits preserve that
retained identity. Duplicate copies imported text and uses the visible payee.

Tags are case-sensitive tokens in transaction notes. `#Food` differs from
`#food`/`#Foodie`; `##Food` is escaped. Unicode whitespace follows the pinned
ECMAScript definition, including BOM whitespace and excluding NEL. Adjacent
`#Food#trip` tokens work. `PATCH /tags/{id}` renames matching active notes
atomically and edits optional color/description/hidden state. Newer note clocks
reject the rename before any partial update. Removing metadata leaves notes
unchanged. Creating/discovering a removed tag reuses its original identity.
Hidden metadata stays available through `include_hidden=true`; note filtering
continues to find hidden tags. Discovery is an explicit mutation, never a GET.

Register queries accept `payee_id` and exact `tag` without `#`, combined with
account/date/pagination/snapshot guards. Totals remain full account/budget totals;
filtered rows have null running balances. The browser does not overwrite its
unfiltered offline cache with filtered pages.

All mutations carry immutable idempotency keys, optional HLCs and captured budget
epochs. Exact receipt lookup precedes epoch validation; changed clock/payload
conflicts. Payees created inline with a transaction inherit its creation HLC. Per-field edits retain newer values and return conflicts; stale bulk
intents reject atomically. The `reference_cancellation` capability permits the
original request plus `X-Actual-Cancel-If-Uncommitted: true`: replay the committed
receipt or durably record `operation_canceled`. Browser cancellation intent stays
saved through lost acknowledgements. Cross-tab draft revision checks also guard
Done, so a stale tab cannot discard another tab's pending intent.

Schema9 adds payee metadata, imported transaction text and tags after category schema8. Native archives
register every new column/table. Schema6–8 defaults and genuine schema7/schema8
activated-import backfill preserve native edits, original field ordering and
historical receipt response shapes. Duplicated import provenance is backfilled too. Original archives remain provenance.
Unactivated stages are re-inspected without rewriting upload receipts; invalid
newly supported metadata leaves the original blocked stage intact.

Corrected-time recovery requires explicit action after attempted work is canceled
or acknowledged. It resets the local clock generator while keeping form fields
and other immutable pending operations. Capability negotiation captures the
original operation, locks competing actions and rechecks draft ownership.

Evidence: `tests/payees_tags.rs`, `web/tests/payees-tags-browser.mjs` and fixture
provenance in `tests/fixtures/PAYEES-TAGS.md`. Tests use real HTTP/browser seams.

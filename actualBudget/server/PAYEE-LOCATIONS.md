# Payee locations — API v1

`GET /budgets/{budget}/payee-locations` pages saved budget data with optional
`payee_id`, `include_deleted`, `limit`, `offset`, and snapshot guard.
`GET .../{id}` retains tombstones. Creation is `POST` with stable UUID, finite
latitude ±90/longitude ±180, and exactly one of `payee_id` or `new_payee`.
The latter creates an ordinary payee atomically. `PATCH .../{id}` accepts only
`{"deleted":true}`. Fields are immutable except the ordered tombstone.

Mutations preserve the original key/body/edit clock/epoch. Unknown old epochs
fail after restore. Exact receipts replay first. `location_cancellation` fences
uncommitted intents, including stale epochs/skewed clocks; it replays already
committed results. Only explicit online user actions initiate metadata writes.
The saved immutable request survives disconnect/reload, with manual Retry/Cancel.
Expense outboxes do not contain or send location metadata.

Nearby calculations occur on-device, after explicit Request location. A Save
location tap requests a fresh position before persisting it. No current position
is sent for lookup. Saved coordinates stay in budget storage, native archives,
backups, duplication and change pages; no map/geocoding/third-party endpoint.
Permission denial, unavailable location, timeout, or insecure HTTP leaves normal
expense entry usable. Browser geolocation itself is controlled by the browser/OS.

Pinned Actual26.10 uses radius6371000m, inclusive500m, closest location per payee,
nearest10, no favorite override. Save hides if any selected-payee branch lies
within500m, even beyond the ten suggestions. Forget targets only that branch.
Ties have a deterministic UUID fallback; the source has no stable tie contract.
Merged/deleted payee coordinates remain attached to original payee identities
and no longer suggest those payees. Transfers are excluded from expense choices.

Source `payee_locations` keeps identities, coordinates, timestamps, tombstones
and exact available clocks. Invalid/orphaned source coordinates block conversion;
clockless exports gain no fabricated ordering. Schema11 blocked stages refresh
without rewriting original uploads/receipts. Legacy native archives gain an empty
location domain, then backfill preserved source metadata only. Native edits,
financial values, aliases and receipts are not replayed from original sources.

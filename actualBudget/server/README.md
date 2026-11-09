# actualBudget server

No-login budgets, accounts, exact opening balances and transaction registers,
durable retries, shared preferences, and static web serving.

From this directory, after building `../web`:

```sh
cargo run --locked
```

| Variable | Default | Purpose |
| --- | --- | --- |
| `ACTUALBUDGET_DATA_DIR` | `./data` | Durable directory containing `actualbudget.sqlite3` and SQLite WAL files |
| `ACTUALBUDGET_WEB_DIR` | `../web/dist` | Built static assets and SPA index |
| `ACTUALBUDGET_HOST` | `127.0.0.1` | Bind address; container uses `0.0.0.0` |
| `ACTUALBUDGET_PORT` | `5006` | HTTP port; `0` allocates a port for tests |
| `ACTUALBUDGET_PUBLIC_ORIGIN` | unset | Exact browser origin, e.g. `https://budget.lan`, without trailing slash; useful behind a TLS proxy |

When no public origin is configured, browser API requests must match `http://`
plus the request Host. Cross-site fetches and foreign/null origins are rejected.
Forwarded headers cannot change this policy. Native callers omit Origin; no login,
cookie, token, or device identity is needed. Static files and API share one origin.

`GET /api/v1/info` advertises implemented capabilities. `GET /api/v1/openapi.json`
is the machine-readable contract. Other successful responses use `{ "data": ... }`;
errors use `{ "error": { "code": ..., "message": ... } }`.

- `GET/POST /api/v1/budgets`
- `GET/POST /api/v1/budgets/{budget}/accounts`
- `GET/POST /api/v1/budgets/{budget}/transactions`
- `GET/PATCH/DELETE /api/v1/budgets/{budget}/transactions/{id}`
- `POST /api/v1/budgets/{budget}/transactions/{id}/duplicate`
- `GET/POST /api/v1/budgets/{budget}/payees` and `/categories`
- `POST /api/v1/amounts/evaluate`
- `GET/PATCH /api/v1/budgets/{budget}/preferences`
- `GET /api/v1/budgets/{budget}/changes?cursor=...`

Register queries accept optional `account_id`, inclusive `as_of=YYYY-MM-DD`,
`limit`, and `offset`. Responses include `data`, exact `balances`, `as_of`, and
`next_offset` and an atomic `snapshot` checkpoint. All-date balances include future entries; an explicit cutoff gives
balances through that date. Account registers include per-row running balances;
the all-account register returns null running balances.

Create income/refunds with positive amounts, expenses with negative amounts.
PATCH changes only supplied fields; null clears a payee/category. `new_payee`
and `new_category` can create references atomically with a transaction. Duplicate
copies ordinary fields into the caller's new ID, resetting cleared/reconciled and
starting-balance status. Delete retains a tombstone; IDs cannot be reused.

Financial mutations and preference updates require a stable `Idempotency-Key` (1–128 visible ASCII characters).
An operation is scoped to its method and resource path. Equivalent normalized
request bodies replay the original status and result; changed bodies return 409
`idempotency_conflict`. Clock headers also participate in identity matching.
Validation/failed writes do not consume the key; terminal deletion conflicts are
durable outcomes. Success
and replay outcome commit in the same SQLite transaction. Outcomes have no expiry;
future compaction requires an explicit acknowledgement protocol.

Budget/account IDs are caller-assigned canonical lowercase UUIDs. Account IDs are
budget-scoped. Money is signed integer minor units, limited to JavaScript's exact
integer range. Dates are calendar dates `YYYY-MM-DD`, years 0001–9999. Names allow
120 characters, notes 10000, request bodies 65536 bytes. Transaction pages are
bounded to 1000 entries and offset 1000000. Budget/account lists include all items.

Nonzero opening balances create real cleared, unreconciled transactions with a
Starting Balance payee. On-budget entries use the Starting Balances income
category; off-budget entries are uncategorized. Zero creates no transaction.
Account totals are derived from ledger entries and cached transactionally. Checked
integer arithmetic protects totals and running balances. Common latest-date
appends and newest-page reads use indexed rows and cached aggregates; historical
financial edits recompute only affected accounts. Metadata-only edits skip totals.

The calculator preserves pinned Actual binary64 expression/rounding behavior
(including negative half-cent ties and accepted expression prefixes), then checks
and converts to integer minor units. Ledger storage/arithmetic never uses floats.
`decimal_places` is 0 or 2; browser drafts capture their number/currency format so
later preference changes cannot reinterpret a pending write.

Storage uses foreign keys, WAL, synchronous FULL, and transactional schema
initialization and migrations. Before upgrading an existing schema, SQLite writes
a consistent standalone backup (including committed WAL data) under
`ACTUALBUDGET_DATA_DIR/backups/before-v{schema}-{uuid}.sqlite3`; migration failure
leaves the original schema intact. Restore a backup into a separate data directory
with its matching old binary. Newer schema versions fail startup. A
single asynchronous permit bounds blocking SQLite work; two Tokio event threads
serve network traffic. SIGTERM/SIGINT drain active requests; successful writes
also survive abrupt process termination.

## Verification

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

`tests/http.rs` launches real server processes on ephemeral ports and uses only
public HTTP responses. It checks restart durability (including abrupt kill),
concurrent/restarted retries, conflict rejection, isolation, integer limits,
validation, origin policy, static serving and published contracts. Seven opening
balance cases consume the independent pinned Actual 26.10.0 fixture from
`../tests/fixtures/opening-balances.json`; no test reads SQLite internals.


Register validation additionally covers pinned Actual transaction snapshots,
calculator cases, per-field PATCH, duplicate/delete, atomic references, aggregate
overflow rollback, and a genuine schema-1 database upgrade/backup fixture. See
[register validation](REGISTER-VALIDATION.md) for reproduction and limits.

See [ordered edits and change cursors](CONCURRENCY.md) for clock, deletion,
retention, explicit recovery and Android adapter boundaries.

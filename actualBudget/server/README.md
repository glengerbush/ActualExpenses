# actualBudget server

Ticket 02: create/list budgets and accounts, exact opening balances, durable retries,
and static web serving. Financial editing arrives through subsequent tickets.

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
- `GET /api/v1/budgets/{budget}/transactions?account_id=UUID&limit=100&offset=0`

POST requires a stable `Idempotency-Key` (1–128 visible ASCII characters).
An operation is scoped to its method and resource path. Equivalent normalized
request bodies replay the original 201 result; changed bodies return 409
`idempotency_conflict`. Validation/failed writes do not consume the key. Success
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
Account balances are calculated from ledger entries, not stored mutable totals.

Storage uses foreign keys, WAL, synchronous FULL, and transactional schema
initialization. Newer schema versions fail startup. No upgrade from an older
released schema exists yet; future migrations require pre-upgrade backups. A
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

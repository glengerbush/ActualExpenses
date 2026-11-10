# SimpleFIN connections

Bank connections are foreground actions. Opening a budget, reading cached metadata and Android pending-expense delivery do not poll a provider. Use Bank connections to stage/claim a token, discover accounts, explicitly map a local account, choose refresh preferences/opening behavior, and refresh a 1–90 day range. Provider balances/currency/date remain separate from ledger balances; no exchange-rate conversion occurs.

Credentials stay in server-private tables. Browser drafts retain no setup/access token. An initial token submission is preceded by a durable nonsecret intent: original fields, key, epoch and token SHA256. After lost acknowledgment/reload, a hash-only receipt probe or cancellation resolves that same intent. Hash-only ordinary writes cannot create credentials. Claim is one-use: an uncertain claim requires an explicit new setup token, not an automatic fresh identity. Existing trusted LAN/tailnet HTTP and no-login boundaries remain unchanged.

Each financial request retains its original body, key, owner, HLC and epoch. Quota or cross-tab checkpoint failure prevents delivery; cancellation is persisted before transmission. Stale snapshots, credentials or capture revisions refuse without financial writes. Failed account scopes do not advance their successful windows or tombstone omitted cached accounts; unaffected accounts can complete. Explicit unlink also releases a mapping whose local account was deleted, without changing financial history.

Schema23 stores portable connection/mapping/cache/origin metadata. Private credentials, captured responses and request accounting are excluded from ordinary archives/feed/history. Restore/duplicate keep financial/reference identities and original receipts, rotate connection generations and require credentials again. Existing native mappings and unlink tombstones supersede retained Actual source bytes. Unsupported provider configurations remain gated for tickets29–32.

| Public seam                                                  | Purpose                                                                              |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| `GET /budgets/{budget}/banking`                              | Cached public connections/accounts/mappings, snapshot, epoch and observed edit clock |
| `POST /budgets/{budget}/banking/connections`                 | Stage a write-only SimpleFIN token; original receipt/cancel recovery                 |
| `POST /budgets/{budget}/banking/connections/{id}/commands`   | Claim, replace, discover, remove or bounded financial refresh                        |
| `GET /budgets/{budget}/accounts/{account}/banking`           | Live local account mapping and independent source preference cells                   |
| `POST /budgets/{budget}/accounts/{account}/banking/commands` | Link, unlink or update typed refresh preferences                                     |

Paths above are under `/api/v1`. Served OpenAPI is authoritative. `setup_token_sha256` is allowed only with receipt-only or cancel-if-uncommitted headers; retain every original nonsecret field and header. Cancellation cannot erase a committed receipt. A canceled original cannot be delivered later.

Provider retrieval uses bounded verified HTTPS, validated/pinned public destinations, scoped Basic authentication, five redirects, four workers and a 16MiB response ceiling. Local connections enforce24 requests per rolling24 hours and64MiB aggregate private capture bytes per budget. Unsupported continuation fields refuse explicitly. Sanitized provider errors expose no arbitrary diagnostic text, URLs or credentials.

Pinned Actual26.10 source observations, genuine22 upgrades and declared numeric/disabled-import compatibility boundaries are recorded in [bank28 validation](../docs/bank28-validation.md) and [provider plan](../docs/bank-provider-plan.md). The current [SimpleFIN protocol](https://www.simplefin.org/protocol.html) is a draft; source parity and controlled transport evidence are distinct. The pure JavaScript SHA256 dependency is pinned; its [MIT license](web/public/licenses/noble-hashes-MIT.txt) ships with the web bundle.

Maintained checks: `test:banking-api`, `test:banking-source`, `test:banking-provider`; external-server-compatible `banking-browser.mjs` is also in `test:browser`. Controlled-provider browser tests explicitly require their own disposable local server. Supplied production-URL browser coverage stages synthetic HTTPS tokens but never claims them. No real provider account, live secret, signed device or production deployment is claimed by these tests; final composed qualification belongs to the integration gate.

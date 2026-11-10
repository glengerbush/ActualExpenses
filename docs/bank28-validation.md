# SimpleFIN28 development contract

Branch `codex/bank28`, source witness510128b, runtime basea5c784c. Schema23 reserved. Work in progress, not acceptance. Agreed seams: public HTTP, real browser, representative native HTTP adapter; disposable loopback provider responses and unchanged pinned source witnesses. No real provider access, credentials or budget data.

## Public contract to implement vertically

- GET `/api/v1/budgets/{budget}/banking`: sanitized connections, local/remote account mappings, provider balances, settings, errors, snapshot/epoch. No credential/claim/access URL or Authorization in any response.
- POST `.../banking/connections`: `{id,provider:"simplefin",name,setup_token}` stages a private setup token and returns a public pending connection. Immutable key+epoch, secret fingerprint only in generic receipt. Claim is a distinct recoverable phase, never a repeated one-use POST after an uncertain claim.
- POST `.../banking/connections/{id}/commands`: explicit claim, credential replacement, discovery, refresh and remove; generation/epoch protects in-flight work. Financial requests capture their original context. Public status and receipts remain secret-free. Remote GETs run outside ledger locks; captured fetched batches apply atomically through shared guarded bank normalization/matching.
- POST `.../accounts/{account}/banking/commands`: link/unlink and per-account settings, preserving transactions/import IDs. Provider balances remain separate from ledger/reconciliation totals. First-sync opening choices are explicit. Existing manual/native overrides remain protected.

External HTTPS required; loopback HTTP is accepted only for a separately configured exact test origin. Bound response bytes, records, date windows, account count and transport duration. Support legacy and current protocol errors; no invented SimpleFIN continuation cursor. Rate warnings/partial data do not imply successful complete refresh. No idle provider polling.

Secret store is server-only and absent from ordinary archive/history/feed/receipts. Nonsecret metadata is portable; restored/duplicated connections require explicit credentials instead of accidentally reusing another live connection. Source bank metadata converters remain provider-specific; no blanket unblocking of29–32.

## Evidence

Source core six cases already PASS in `bank-sync-source-validation.md`; no replacement calculator expected values. First public lifecycle tracer will establish route red, secret-free exact replay and preserved exported original state before extending claim/link/refresh. Further checkpoints append actual red/green evidence; remaining protocol/UI/native/migration criteria stay open until proven.

## Checkpoint 1 — staging, one-use claim and custody (partial)

Public lifecycle tracer: `node actualBudget/tools/check-bank-http.mjs`.
Original staging and claim routes each failed404 against the prior runtime:
`/tmp/bank28-lifecycle-red.log`, `/tmp/bank28-claim-red.log`.
Current focused green: `/tmp/bank28-checkpoint-green.log`.

The tracer uses only a disposable server and exact opt-in loopback provider origin.
It proves staging has no financial effects; a committed claim and its exact retry
consume the token once; an interrupted provider response records a sanitized
uncertain outcome and refuses a new-key automatic reclaim; both receipts survive
server restart. Same-key changed secrets conflict. Durable cancellation blocks a
later original create. Public state, changes, logs, and every uncompressed native
backup entry exclude original setup/access secrets. Request/result values validate
against the actually served OpenAPI. No generic receipt retains the submitted
secret: its immutable comparison uses SHA-256. Private credential rows are never
registered as portable tables. Restored/duplicated metadata rotates its generation
and requires credentials again; old epochs cannot install a late claim result.

The claim-intent phase commits before remote POST. A crash between remote success
and local access-key persistence cannot safely reconstruct a one-use response;
the connection remains `claim_inflight` and requires explicit replacement. Provider
requests execute outside the SQLite writer, with four bounded workers, a30second
response deadline, HTTP connect/read deadlines, pinned public DNS resolutions,
validated HTTPS, no claim redirects and an8KiB response ceiling. Production does
not inherit the loopback-only fixture exception or an ambient HTTP proxy.

`generate-simplefin-transport-source.mjs` executes the unchanged helper tail of
pinned `packages/sync-server/src/app-simplefin/app-simplefin.js`; the fixture stores
its full-source and executed-tail SHA-256. Six genuine observations preserve raw
input and full output: booked/pending, posted-zero fallback, legacy account-attention
errors, rejected token, refused claim redirect, and cross-origin credential stripping.
Only actual JSON serialization omits undefined properties; money/IDs/edges are not
normalized. Fresh generation and independent `--check` pass in
`/tmp/bank28-transport-source.log` and `/tmp/bank28-transport-source-check.log`.
This source helper witness does not qualify source SSRF policy, live institutions,
provider quota availability, or the newer protocol's structured errors.

Focused build and bin Clippy logs: `/tmp/bank28-contract-build.log` and
`/tmp/bank28-checkpoint-clippy.log`. No broad suite/container execution.

Still pending for complete28: replacement/remove, discovery and transport failure
matrix, rate/work bounds, financial linking/settings/refresh through the shared
writer and six source financial witnesses, reference conversion/genuine22 upgrades,
full contract consumer, browser UI and representative Android qualification.

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

## Checkpoint 2 — discovery and explicit replacement (partial)

Replacement and discovery produced public400 reds before implementation:
`/tmp/bank28-replacement-red.log`, `/tmp/bank28-discovery-red.log`.
The extended same public tracer now proves new-token replacement rotates generation;
old-generation work refuses; explicit remove deletes private custody; discovery
sends authenticated balances-only GET and never modifies the ledger. Local remote
identities include both the access connection and institution connection, so equal
remote account IDs under distinct institutions remain distinct.

Provider GETs reserve a rolling24-request/day connection quota before transport,
capture their immutable response privately before applying metadata, and reuse
committed original receipts without another request (including after restart).
Public scope is at most500 accounts/download and5000 retained remote accounts/budget;
16MiB responses and64MiB unfinished private captures are explicit refusal bounds.
SimpleFIN has no documented continuation cursor: a returned continuation refuses
rather than silently truncating. A captured response is not re-fetched under its
original identity. Request/response and rate ledgers remain private archive exclusions.

Sanitized HTTP fixtures prove partial v2 account errors, legacy warnings, HTTP429,
revoked403, malformed JSON, oversized response, unsupported continuation, exact
failure replay and quota refusal before the25th provider GET. The one-use fixture
now refuses a setup URL consumed by an earlier claim. Provider strings containing
HTML and synthetic secret URLs never appear in public diagnostics. Partial/error
accounts retain explicit diagnostics; no error case fabricates a ledger entry.

Current runtime matrix: `/tmp/bank28-discovery-final.log`; focused bin lint:
`/tmp/bank28-discovery-clippy.log`. This is metadata discovery, not yet financial
refresh acceptance. Linking/settings, guarded financial import and all previously
listed UI/native/genuine-upgrade/restore gates remain open.

Transport follows [SimpleFIN protocol](https://www.simplefin.org/protocol.html)
(current2.0.0-draft and legacy shape): institution-scoped account IDs and structured
account/connection diagnostics. Unknown diagnostic subcodes remain an attention
warning; raw diagnostic prose is not returned. TLS verification stays enabled;
the only HTTP exception is the exact explicitly configured loopback fixture origin.

### Discovery cancellation correction

Independent review found that a canceled in-flight GET could leave a private
capture behind: the final generic receipt replay skipped cleanup inside its write
closure. Public red `/tmp/bank28-capture-cancel-red.log` reproduced permanent
capacity loss after four held near16MiB responses, canceled before release, made
the next valid2MiB download fail409. No database inspection was used.

Cancellation now removes its matching private job in the same transaction as the
terminal canceled receipt. Late retrieval checks that original receipt before
capturing bytes; it cannot recreate canceled private work. Public green
`/tmp/bank28-capture-cancel-green.log` verifies all four delayed originals and exact
retries return the same canceled outcome, cursor/ledger stay unchanged, a subsequent
valid download succeeds, and canceled remote requests still consume the24/day
quota. Original receipt/key/body/epoch and cancellation semantics stay intact.
Focused bin Clippy passes `/tmp/bank28-capture-cancel-clippy.log`. Persisted known
provider error codes are also retained on capture recovery; interrupted error
capture/restart is not claimed as a separately executed public witness yet.

### Frozen schema23 foundation (financial behavior still pending)

Schema23 now includes connection custody; institution-scoped remote account labels,
original balance strings/currency/date; per-account links with explicit initial
opening choice and successful-window marker; per-transaction provider-origin bindings;
and private request/capture quota state. Bank preference cells reuse existing
`transaction_import_settings` so their source IDs and individual clocks can survive.
No further23 DDL is planned. Empty financial/link/origin columns are a foundation,
not a claim that the financial writer/converter is complete. Portable table registry
and pre23 archive defaults include the complete nonsecret shape.

Fresh lifecycle/discovery/secret-boundary matrix and bin lint pass after the DDL
change: `/tmp/bank28-foundation-green.log`, `/tmp/bank28-foundation-clippy.log`.
The independent source financial consumer now has a real404 red at public account
linking (`/tmp/bank28-financial-red.log`); six original source archives/expected
financial states remain unchanged.

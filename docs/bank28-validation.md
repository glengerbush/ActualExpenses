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

## Financial checkpoint — linking/settings/shared writer (partial)

Public source consumer `check-bank-financial-http.mjs` imports each of the six
unchanged genuine prelink ZIPs, verifies its original digest, then links and
refreshes through real local SimpleFIN HTTP responses. Initial public account-link
red404: `/tmp/bank28-financial-red.log`. Six financial cases now pass in
`/tmp/bank28-financial-final-http.log`: opening balance, original manual matching,
pending→booked stable IDs, separate payment/deposit field mappings, excluded
pending/notes, bank-date updates, and balance-only preference. Comparisons include
signed money, dates, notes, cleared/reconciled/starting flags, original category and
manual/payee IDs, exact bank IDs/imported payee, references and multiplicity. Active
payee counts/names prohibit unused provisional references. Repeat preserves every
actual first-import ID. Source UUIDs generated during linking are not expected to
match replacement-generated IDs; original archive identities remain exact.

The public consumer also proves owned undo/redo of opening+bank expenses+origin
metadata, receipt replay after undo without reapplication, and older-clock preference
refusal despite a fresh snapshot. A second untouched remote account in discovery
exposes scoped-response handling: true red `/tmp/bank28-scoped-metadata-red.log`
was corrected by distinguishing complete discovery from a scoped financial response.
A scoped refresh never tombstones another provider account. All76 observed request,
result, settings and change-feed values validate against actually served schemas.
Shared import metadata/settings now have explicit feed schemas alongside bank types.

Bank rows reuse the existing guarded rule-before-match planner and graph writer;
file imports retain their prior defaults/title behavior. Bank imports retain source
payee spelling, use non-strict bank-ID fuzzy matching, preserve existing nonempty
manual values, and optionally update dates. Per-field preference cells reuse original
source IDs; same/equal newer field clocks refuse changed values. Financial graphs,
original provider bindings and the original owned receipt are one SQLite transaction.
Captured raw downloads stay outside history/portable state; generic receipt identity
also binds original clock, owner and epoch. Existing five rule-import workflows and
14 matching outcomes remain green (`/tmp/bank28-existing-import-rules.log`,
`/tmp/bank28-existing-matching.log`). Bin Clippy passes
`/tmp/bank28-financial-clippy.log`; exact latest API build
`/tmp/bank28-financial-final-build.log`. Lifecycle/secret matrix still passes
`/tmp/bank28-financial-final-lifecycle.log`.

Representative native public test passes (`/tmp/bank28-native-financial.log`,1test,
zero skips): queued native work delivers once across new bank feed entities, bank
expense amounts/IDs survive pending→booked, restart/unchanged refresh preserves
cache without extra writes, and native refresh never contacts the provider. Native
saved state contains no setup/access secret. No Android product code changed.

Compatibility qualifications: provider balances remain separate from ledger totals;
known discovery balances stay visible immediately (the source's first-link account
balance column stays null until a later refresh). Decimal provider balances use
numeric minor-unit conversion rather than the source adapter's formatting-sensitive
`parseInt(balance.replace('.', ''))`. No FX conversion is inferred. The six original
source financial observations/archives are unchanged. Full linked-source conversion,
genuine22 DB/native upgrade/restore/duplicate, account unlink/relink matrix, parallel
capture ordering/partial missing-account recovery, final contracts/CI and UI are
still pending. This checkpoint is not complete28 acceptance.

### Starting-reference correction and explicit first-link boundaries

Three bounded genuine source cases now supplement the unchanged original six.
`generate-bank-sync-source.mjs --check --check-edges --export-linked` verifies the
old observations, new edge JSON/prelink ZIP hashes, and one genuine already-linked
export. Capture only adds new artifacts; existing observations/ZIPs are never
rewritten. Two fresh checks pass (`/tmp/bank28-source-edges-check.log`,
`/tmp/bank28-source-edges-check-repeat.log`). Original helper/API bundle hashes stay
pinned, including `accounts/payees.ts` for reference lookup. Official exports clear
CRDT; these artifacts prove values/identities, not per-field source clocks.

The exact source boundaries are retained, not patched in the oracle:

- Empty existing account + `sync-import-transactions=false`: source first link still
  inserts opening and expense, source AQL total25000. Replacement intentionally keeps
  the explicit balance-only setting: zero ledger rows, provider balance25000 retained.
- Initial transaction `-1.2`: source normalizes the expense to−120 but its formatting
  dependent opening is10012, source AQL total9892 against provider10000. Replacement
  retains the source expense and all opening fields/references but uses the numeric
  money invariant, yielding the provided10000 total. No FX or source expected-value
  substitution is involved.
- SDK-created starting payee renamed `STARTING BALANCE`, starting category renamed
  `starting balances`, while another income category remains: source reuses both
  original IDs. Public red `/tmp/bank28-opening-reference-red.log` showed replacement
  creating a second cased payee and choosing Income. Payee-only correction isolated
  the category red `/tmp/bank28-opening-category-red.log`. Final Unicode-lower payee
  lookup and SQLite-lower category preference preserve the original IDs; the normal
  mutation duplicate guard is unchanged. Payee lookup refuses beyond100000 rows.

All three public edge workflows and27 served-schema observations pass
`/tmp/bank28-opening-edges-green.log`; focused bin Clippy passes
`/tmp/bank28-opening-fix-clippy.log`. Fresh source checking also exposed incidental
UUID ordering of otherwise exact `(date, sort_order, starting flag)` ties. CHECK
canonicalizes only those ties by bank ID/fixed-anchor/financial signature before
UUID aliasing. Raw original rows, fixed IDs, references, multiplicity and values stay
untouched; no generated-UUID tie-order parity is claimed. The unpublished prelink
artifact from the interrupted check remains `/tmp/bank28-unpublished-rename-prelink-26.10.zip`.

### Genuine accepted22 upgrade inputs

`generate-bank-schema22.py` executes the immutable accepted0a284f2 server
(SHA256`fa0146112b54a00ab78f5b7b1c9d4363256efb25c45aff49356a3b45b22172e0`).
Only public HTTP creates the active source import, two file imports with remembered
options, owned expense/edit/history and native backup. The delimiter's second write
has a distinct HLC while unchanged mapping/date cells retain the first HLC. Real
SQLite remains version22, readonly integrity/FK checks pass, and graceful shutdown
precedes copying. Outputs refuse overwrite; no SQL data writes or schema relabeling.
Capture log `/tmp/bank28-schema22-capture.log` passes.

The DB also retains genuinely blocked linked-SimpleFIN and custom-bank-preference
Actual stages plus an unactivated nYNAB stage. Source22 correctly refuses bank
preferences; an initial attempt to activate that unsupported archive was rejected,
so the final active seed uses the compatible unchanged manual-bank prelink archive.
No gate was bypassed. The nativeZIP contains active-budget state only; those other
budget/global stages are DB-only. All referenced original source ZIP/JSON hashes
are recorded in the manifest. These are inputs for forthcoming upgrade/restore
consumers; no23 migration green is claimed yet.

## Actual-source and genuine22 upgrade checkpoint

`check-bank-upgrade-http.mjs` copies the immutable genuine22 SQLite fixture, then separately restores its genuine active-budget native archive. Public assertions retain complete original register/reference/settings envelopes and field clocks, four original operation responses, and database history/checkpoint. Native restore excludes only operational epoch/cursor comparisons and independently requires the epoch to rotate. Receipt replay must leave the new checkpoint unchanged.

The original blocked Actual SimpleFIN mapping and bank-preference stages become ready; the unrelated nYNAB stage stays byte-for-byte equivalent at the public response. Activating the linked source retains original transaction/payee/category IDs, dates, money, notes, status, bank IDs and reciprocal references. Credentials are missing; no source credential is inferred. Explicit replace/claim/discover/relink and refresh through a disposable loopback provider preserve every source transaction ID and avoid duplicate opening/import entries. The user must select the rediscovered account: source exports lack SimpleFIN connection selectors.

Actual source institution/mapping/cache is installed without changing schema23 DDL. Supported source preference cells use the existing independent settings representation. Original account selector clocks are retained separately and participate in subsequent link guards. Newly imported ownership requires matching original normalized account plus financial ID; explicit same-connection/same-remote-ID relink can move its legacy namespace to the user-selected discovered mapping. Existing metadata, tombstones and settings are not overwritten on enrichment. Actual-only source-format/provenance filters keep YNAB originals out of SQLite conversion.

Provider currency, provider balance date and completed request window are absent from these Actual exports. They remain explicitly unknown (`""`, `0`, null), with reconnect/review diagnostics; no USD, timestamp or request window is invented. Original `last_sync`/status and all source bytes remain in the retained original. Unsupported provider configurations and unrepresented credit-limit/mask/type/subtype metadata remain blocked.

Evidence: `/tmp/bank28-upgrade-first-red.log` proves the genuine blocked mapping remained blocked before conversion. `/tmp/bank28-upgrade-reconnect.log` passes both database and native routes plus original-identity reconnect. `/tmp/bank28-upgrade-build.log` and `/tmp/bank28-upgrade-clippy.log` are focused build/bin-Clippy evidence. Initial consumer corrections used the actual public `accounts` key and excluded only native operational epoch/cursors; no financial expectation was changed.

Remaining ticket28 gates: current23 portable restore/duplicate credential exclusion and delayed-response fences, remaining partial/parallel recovery cases, complete client/API contracts and rendered web UI. This is a converter/upgrade checkpoint, not full acceptance.

### Current23 portability correction

Public `/tmp/bank28-portable-red.log` reproduced duplicate→native backup/restore creating a second connection from retained Actual provenance. Source enrichment now fills only missing account mappings; an existing native mapping or intentional unlink tombstone supersedes the original source. The same public tracer additionally creates an unlink tombstone, compares every original connection/remote/link identity and tombstone after restore and duplicate-of-duplicate, checks all portable ZIP entry bytes for the controlled setup/access secrets, and verifies financial graph identity throughout. The original connected budget remains independent of its copies.

A real held provider GET is released only after same-budget current23 restore. It refuses409, leaves restored metadata/cursor/financial graph exact, and original committed refresh receipt replay remains exact after epoch rotation. `/tmp/bank28-portable-green.log`, `bank28-portable-build.log`, and `bank28-portable-clippy.log` pass. The first ZIP inspection attempt incorrectly decoded a retained binary source ZIP as UTF8; the corrected test inspects raw entry bytes without discarding any binary entries.

### Ordering captured provider reads

`check-bank-order-http.mjs` holds an actual older provider GET (111.00), commits a newer GET (222.00), then releases the older response. Before correction it returned200 and replaced the cache with111.00 (`/tmp/bank28-order-red.log`). Private capture now persists a versioned connection-revision witness alongside the raw result; commit requires that revision to remain current. The stale capture refuses409 without any metadata/cursor/ledger write; explicit original-identity cancellation frees it and subsequent original replay cannot resurrect it. Captured data missing an ordering witness conservatively requires cancellation rather than claiming freshness. No schema23 DDL or generic receipt shape changed.

`/tmp/bank28-order-green.log` passes the reverse-order public proof. The six source financial cases/76 served contract observations and existing lifecycle/cancellation/capacity/transport checks pass in `/tmp/bank28-order-financial.log` and `/tmp/bank28-order-lifecycle.log`. Bin Clippy/build pass `bank28-order-{clippy,build}.log`. An initial new harness draft omitted required budget ID and assumed cancellation200; corrected to the existing public schema and terminal409 cancellation contract before claiming the actual behavioral red/green.

### Missing-account partial errors

The public two-account tracer `check-bank-partial-http.mjs` first reproduces lost `act.missingdata` scope when the failed account is omitted from the new response (`/tmp/bank28-partial-red.log`). Scope validation now also recognizes retained provider identities. It publishes only known IDs and sanitized messages. The unaffected account imports once; the omitted account retains its cached metadata and unadvanced success window. A subsequent complete refresh imports the recovered account without changing the original unaffected transaction ID. Financial subset responses never tombstone an omitted cached account; complete discovery remains the authoritative missing-account path. `/tmp/bank28-partial-green.log`, `bank28-partial-build.log` and `bank28-partial-clippy.log` pass. Raw diagnostic text remains absent from public results.

### Write-only browser credential recovery

Initial setup/replacement still submits the raw token once. A client can instead retain the token's SHA256 plus every original nonsecret field/key/epoch for recovery. Hash-only bodies are accepted exclusively with receipt-only or cancel-if-uncommitted headers, bind the identical canonical server receipt identity, and cannot stage/replace credentials or call the provider. Thus reload/lost ACK does not require browser token storage or an unsafe new identity. Changing the original label, generation, token digest or epoch cannot retrieve another receipt.

`check-bank-secret-recovery-http.mjs` RED400→GREEN covers original201/200 results via hash-only probes; mismatched identity refuses409; unknown-intent cancellation refuses409 terminally and delayed original raw submissions remain canceled; hash-only regular submissions refuse400. It asserts exactly one intended connection and zero provider calls. Actually served request/result schemas are validated with JSON Schema. Logs `/tmp/bank28-secret-recovery-{red,green,build,clippy}.log`. No generic receipt shape, private credential representation or schema DDL changed.

## Complete web/interface checkpoint

The web slice extends the incumbent Operate system: connection state/claim/replacement, scoped discovery and explicit local mapping, automatic/none/custom opening choice, independent typed payment/deposit mappings and flags, manual bounded refresh, provider balance/currency/date separate from ledger, unlink/removal and explicit unavailable-account recovery. No background bank worker is registered. `BankState.clock` exposes the existing edit-clock type so a newly prepared owned action observes current server ordering; prepared requests retain their original clock.

Application IndexedDB/cache/localStorage never retain setup/access tokens. Bundled exact `@noble/hashes2.4.0` SHA256 works when `crypto.subtle` is absent on LAN HTTP; its MIT notice ships in `web/public/licenses`. Source: [maintainer usage](https://github.com/paulmillr/noble-hashes). Only a nonsecret digest recovery envelope is persisted before token transmission. CAS protects every preparation/recovery/cancel; failed saves block sending, raw numeric/date drafts survive reload, and only a successful durable discard resets failed draft state. Budget restore requires explicit draft review; pending requests keep original epoch/identity. Cached views always use the current shared privacy preference.

Maintained public/browser proof:

- `/tmp/bank28-ui-first-red.log`: Bank connections was absent before implementation.
- `/tmp/bank28-ui-recovery-final.log`: external-server-compatible staging, real committed201 before lost ACK, digest-only original receipt recovery, quota fence/current-field recovery, durable failed cancellation/reload, delayed-original409, failed-save→discard→new request without reload, offline cache and shared privacy. Plaintext tokens are absent from saved draft/localStorage. No token is claimed and no real provider is contacted.
- `/tmp/bank28-provider-ui-final.log`: separate local controlled-provider flow, one genuine unchanged source case. Claim/link/settings/raw `-` draft/reload; real committed200 before abort; exact body/key/HLC/owner/epoch across reload and receipt retry; complete source financial/reference/status projection and all generated-ID survival. Explicit repeated refresh, unlink/relink and replacement/re-discovery preserve the same financial graph. Removal preserves transactions. Duplicate sanitized provider messages remain readable and raw diagnostics remain hidden. Unavailable-account cleanup uses an unrelated zero-balance mapping so the source-backed financial graph stays intact.
- `/tmp/bank28-unavailable-red.log` reproduces unlink404 after local-account deletion. `/tmp/bank28-unavailable-green.log` proves explicit metadata-only unlink, exact receipt, bank-link-only change feed, unchanged remaining financial/reference/version envelope, and reuse of the freed mapping. Only unlink permits a retained soft-deleted account; ordinary reads/link/settings retain live-account guards.
- `/tmp/bank28-ui-served-contract.log`: real served fragment equality, schema metaschemas, strict custody/settings requests/results, original digest receipts/cancel and meaningful400/409 errors. First contract iteration caught an incorrect clock schema reference and description-only error responses; both were corrected before this checkpoint.
- `/tmp/bank28-ui-final-{check,build,clippy}.log`: Svelte0errors/0warnings, production web build and bin Clippy. Build retains the pre-existing large-chunk advisory; no threshold was weakened. Svelte component/App autofixers and targeted formatter evidence are recorded alongside these logs. Mechanical Impeccable detector returned `[]`; no shipping rasters were added.

Captures under `actualBudget/web/test-results/banking/`: custody `desktop/mobile`, `recovery-desktop`, `cancellation-desktop`, `quota-failure`, `offline-desktop`; controlled-provider `provider-desktop/mobile`, matching viewport images, `provider-large-text`, `provider-dark-large-text`, their viewport/control/refresh images, `provider-attention`, and `unavailable-account`. Large-text uses root32px at390px width; dark captures use the actual dark CSS tokens. These qualify layout, not a new theme preference implementation. The independent finish reviewer owns the final UI disposition. PRODUCT/DESIGN/token files remain unchanged; the new surface brief records only this extension.

CI maintains the reviewed source checks, seven focused API invocations, the explicitly local controlled-provider browser, and the external-compatible custody browser in the ordinary container browser list. Earlier source/oracle, financial, genuine migration/archive and representative native evidence is reused. No broad Rust/Android/container rerun, live provider call or deployment was performed for this checkpoint. Full composed qualification remains the root integration gate; do not mark ticket28 accepted before independent review and that gate.

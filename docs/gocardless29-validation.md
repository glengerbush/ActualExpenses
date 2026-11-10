# GoCardless29 validation — WIP

## Root financial checkpoint review (open correction)

2026-11-10:2c28e9c reviewed independently from unimplemented bank families,
paging, renewal continuity and UI. Default/ING normalization feeds the existing
owned capture/writer; booked-only source opening math, source date choices,
original idempotency/snapshot checks, private quota storage and public error
sanitization were inspected. Worker reports five exact whole-source financial
graphs and80 served shapes; unchanged SimpleFIN financial/custody checks pass.
This checkpoint is not whole29 acceptance.

One correction remains: transaction download unconditionally fetches balances
and computes an opening amount. The pinned `/transactions` route uses
`getNormalizedTransactions` alone when `includeBalance=false`; genuine existing
account/renewal requests select that path. An unnecessary balance endpoint
failure currently blocks valid transaction refreshes and consumes extra quota.
Carry the scoped balance/opening need into capture, preserve truthful prior
balance freshness, and prove existing-account refresh without that request.
Initial automatic opening must still require its actual balance data.

Renewal source checkpointa111b09 reviewed; independent reproduction
`/tmp/gocardless-renewal-root-source-check.log` PASS. An immutable genuine linked
export is explicitly relinked through the registered source handler to new
provider account/requisition IDs, then refreshed again. All original financial
rows/IDs remain literal and unchanged; only new metadata UUIDs normalize.
Transport responses reuse previously reviewed source normalization. No native
replacement supplies expected financial values. Renewal runtime remains open.


Ticket29 is not accepted. Custody and genuine24 preservation are reviewed; the five-case default/ING financial checkpoint has the open balance-need correction above. Other bank families, paged discovery, renewal continuity, provider conversion, UI/native integration and final qualification remain pending. Earlier sections retain their historical checkpoint scope. SimpleFIN28 and notes24 behavior are retained; no real provider/account calls, setup, deployment or broad suite run.

## Source reference

The original eight service/API/factory witnesses remain unchanged from `fb5547d`; independent root review/check passed `/tmp/gocardless29-root-source-check.log`. `docs/gocardless29-plan.md` records pinned26.10 paths, exact normalization boundary and current official token-shape discrepancy. Those observations qualify EUR default/ING translation, not the other distinct source bank transformations. Financial source graphs/IDs/amounts remain authoritative and have not been rewritten.

## Public custody/consent tracer

`node actualBudget/tools/check-gocardless-http.mjs` owns a disposable local server/provider, refuses a supplied production test URL and never calls the real provider. Source-genuine requests use the fixed GoCardless APIv2 path structure with an explicitly configured loopback transport exception only. Public TDD evidence:

- Staging: genuine24 binary refused400 (`/tmp/gocardless-stage-red.log`); new schema25/runtime stages GoCardless and recovers the identical receipt by SHA256 of UTF8 JSON `[secret_id,secret_key]` (`gocardless-stage-green.log`). Digest-only ordinary writes refuse400.
- Institution listing: absent route RED (`gocardless-institutions-red.log`) → refresh-only token/new, access exchange, private token reuse GREEN (`gocardless-institutions-green.log`). Both officially documented token shapes are supported; no token is returned in ordinary results.
- Consent: RED unsupported action → source-shaped institution limits/agreement/requisition, private no-store handoff, waiting→LN, rotated generation and immutable prior-status receipt GREEN (`gocardless-consent-{red,green}.log`). Consent URLs stay outside generic receipts, history/change payloads and ordinary native backups. A new authorization is an explicit generation; a pending/unknown non-idempotent requisition is never silently repeated. Unlike source's broad fallback retry, uncertain remote creation requires review/cancel rather than another remote POST.
- Replacement/removal: RED unsupported credential shape → new private credentials/generation, digest-only exact recovery and removed-access refusal GREEN (`gocardless-replace-{red,green}.log`). Removal deletes credentials, tokens, consent URL and captured bank jobs while retaining nonsecret financial metadata.
- Canceling an actual held status GET initially erased the working consent (`gocardless-check-cancel-red.log`). Corrected cancellation clears only its status intention; working consent/public connection remain exact. An actual held requisition POST canceled before completion cannot publish a late authorization URL or repeat provider calls. These and portable duplicate/secret exclusion pass `gocardless-custody-green.log`, including48 actually served request/result/error schema observations. The tracer compares receipts exactly and confirms no financial rows were created by this custody scope.

GoCardless transport pins the fixed provider origin, reuses resolved/public HTTPS validation, refuses redirects and bounds each response to16MiB, institutions to1000, consent accounts to1000 and concurrent provider work to the shared four slots. Raw error text/URLs never become public diagnostics. Typed provider failures and the original request identity remain visible. Account endpoint quota/reset persistence is part of the pending financial adapter, not claimed here.

Schema25 copies/rebuilds only the existing banking family inside one migration transaction to widen the provider CHECK, preserving every column and child table before dropping the old parent. It adds a private GoCardless credential/consent table. Portable table shapes are unchanged; the private table is explicitly excluded through the archive auxiliary registry. Current archives declare25 and accept real6–25. Genuine24 nonempty-state migration/native proof is pending; the immutable root binary SHA is recorded in the plan. Notes24 DDL/triggers/enrichment are unchanged.

Focused build/bin Clippy PASS `/tmp/gocardless-custody-{build,clippy}.log`; existing SimpleFIN digest/cancel recovery passes `/tmp/gocardless-existing-secret-recovery.log`. No Rust/native/browser/container full suite was rerun for this checkpoint.

## Review correction: pending identities before cancellation

A true held-GoCardless-status public red showed an altered generation could install a terminal canceled receipt and erase the original intent (`/tmp/gocardless-cancel-identity-red.log`). Shared SimpleFIN download and one-use claim variants reproduced the same mismatch (`bank-capture-identity-red.log`, `bank-claim-identity-red.log`). The owned financial `remote::Request::cancel` path also reproduced against immutable genuine24 (`bank-refresh-cancel-identity-red.log`): a different refresh body/owner/clock canceled another captured request using the same scope/key.

A common pending-fingerprint check now runs inside the atomic command transaction before any canceled receipt or custody mutation. It compares the captured full receipt identity (including original body/epoch and, for financial requests, owner/clock). Same-key altered cancel/remove/replace refuses409 without metadata, cursor or receipt changes. New SimpleFIN claims retain their original private intent ID/fingerprint in nullable schema25 credential columns; interrupted pre25 claims remain conservative/replace-only. Schema23 is unchanged. Successful/canceled original receipts keep their existing shapes.

`gocardless-cancel-identity-green.log` passes65 served-schema observations, altered-body/epoch refusal, exact original receipt absence before completion, correct original cancellation and no late resurrection. `bank-capture-identity-green.log` and `bank-claim-identity-green.log` pass the shared held-provider cases; the capture case explicitly exercises the owned financial cancel route with another body/owner/clock. All use public HTTP and actual held provider requests, without private SQL inspection. Build/bin Clippy and targeted JS formatting are in `gocardless-identity-{build,clippy,format}.log`. No broad suite rerun.

## Root custody checkpoint review

2026-11-10:27cddf9/c0e093a reviewed independently. Private token/consent storage,
fixed-origin bounded transport, no-store handoff, generation/epoch fences,
portable exclusions and transactional25 banking-family reconstruction inspected.
Root found the altered-key-payload cancellation gap; the common guard and private
claim identities above close it, including same-scope replacement/removal and
owned download clocks. Original23 migration remains unchanged. No remaining
custody finding; genuine nonempty24→25 proof and complete29 acceptance remain open.

Composed atb68379d with the accepted reporting/query work: build PASS
`/tmp/gocardless-root-build.log`;65 served-contract/custody observations PASS
`/tmp/gocardless-root-custody.log`; held SimpleFIN capture/claim identity checks
PASS `/tmp/gocardless-root-{capture,claim}-identity.log`; existing digest-only
original receipt/cancel recovery PASS `/tmp/gocardless-root-secret-recovery.log`.
No real provider call or broad suite rerun.

## Genuine24 preservation fixture

`generate-gocardless-schema24.py` requires the immutable accepted24 binary SHA from the plan. It imports the unchanged genuine pinned linked-source ZIP, uses public controlled-provider claim/discovery/link/refresh, adds an owned expense/edit and arbitrary notes24 identity, then holds a real second GET while a note edit makes its snapshot stale. The actual24 server retains that captured request. Every banking table is nonempty before migration, including private credentials, request log/capture and financial origins. Native backup is taken through the public API; the server stops before a read-only integrity/FK/schema24 inspection and SQLite copy. No SQLite/version/expected result is rewritten; outputs refuse overwrite.

`/tmp/gocardless-schema24-source.log` passes. `gocardless-schema24.{json,sqlite3,zip}` retain exact public financial/reference/metadata/history/receipt snapshots, original controlled request, source/binary/artifact hashes and notes24. Only synthetic credentials/provider are used. The first setup attempt used an incorrect link property (`opening_mode` instead of the documented `opening_balance_mode`) and published no fixtures; corrected before capture. This fixture is provenance, not a replacement-derived financial oracle or a claim of completed25 upgrade acceptance.

The public genuine24 consumer now passes both database and native archive routes (`/tmp/gocardless-upgrade-green.log`). Database migration preserves complete financial/reference/account rows and field HLCs, all bank metadata/cursor/clock, notes24 and owned history exactly. An altered-owner cancellation of the old captured request refuses without a terminal receipt, proving its original full fingerprint survived migration; original cancellation/replay remains exact. Native restore changes only its documented operational cursor/epoch and credential generation/state, retaining all financial rows/HLCs, remote IDs, link/settings/window metadata and notes. Original committed receipts replay exactly in both modes; new GoCardless staging leaves old connections/money unchanged. Initial native harness used the wrong activation route/status, corrected to the existing budget-scoped confirm-name contract; only the documented archive envelope snapshot is excluded from financial-row comparison. Original fixture bytes/expectations remain unchanged.

Root reviewed0c725c1/031d440 provenance and consumer, independently verified
original artifact hashes/bytes, actual24 SQLite/native versions, integrity/FKs
and nonempty seven banking tables plus notes. Composed at6ce2042; both public
database/native modes PASS `/tmp/gocardless-upgrade-root-http.log`, reusing the
unchanged b68379d runtime build. Genuine24 preservation gate closed. Provider
financial/conversion/UI/native feature gates remain open; no29 acceptance claim.

## Whole-source financial witnesses

`generate-gocardless-financial-source.mjs` feeds the unchanged registered `gocardless-accounts-link` and bank-refresh handlers with exact results from the reviewed, executed server-adapter fixture (input SHA retained). Five genuine workflows cover default and ING first opening, existing manual match, custom opening and pending exclusion; every explicit repeat preserves all first transaction IDs and the complete financial graph. Original preapply ZIPs and one linked GoCardless export retain source identities; the linked archive is read back through the genuine SDK to capture its own original IDs. No replacement money/normalization code computes expected results. Generated UUIDs and genuine date/sort-order ties normalize only for independent check; raw provider ordering, fixed manual ID, fields/edges/multiplicity remain exact. Official exports clear CRDT, so these are values/identity witnesses, not exported per-field clocks.

Generation/check pass `/tmp/gocardless-financial-source-{first,check}.log`. Source default opening is11284 with−1234 booked/−200 pending; source ING opening is12109 with its chosen balance/sort and−1234/+125 booked/−200 pending. These recorded results require GoCardless-specific opening handling; SimpleFIN's subtract-all path is not reused blindly. Initial unpublished harness attempts corrected the sync-server success envelope and used string `"false"` for the source preference SQL type. Partial preapply artifacts were resumed by importing their original bytes through the genuine SDK; no published fixture or expected financial value was overwritten.

Review tightened independent financial checking: UUIDs present in each frozen preapply snapshot, original adapter input and immutable linked input remain verbatim; only genuinely new IDs are aliased. `/tmp/gocardless-financial-original-ids-check.log` passes once after this checker-only change. No source archive, financial graph or expected value changed.

## First financial runtime checkpoint — partial institution coverage

The public financial tracer reaches controlled raw provider HTTPS-shaped routes, then the shared immutable capture and guarded writer. Five source workflows pass with exact financial/reference/status/raw-bank graphs, original manual ID, distinct provider balances, booked-only/source-family opening, custom opening/date, pending preference, exact original receipts and every first-import ID surviving a new refresh. It validates80 actually served request/result shapes (`/tmp/gocardless-financial-checkpoint-http.log`). Its initial genuine red was discovery409 because only SimpleFIN access custody was wired; an earlier setup-only missing account opening pair was corrected before that product red. Automatic GoCardless opening can take its source explicit start date; the existing SimpleFIN custom-only date restriction remains intact.

Implemented institution transforms here are **default plus ING only**. The exact44-module registry is present; known unported families refuse explicitly before applying financial data. They are pending implementation/source-family qualification, not silently treated as default. Unknown institutions use the same fallback as pinned BankFactory. Discovery currently refuses catalogs over20 accounts; explicit bounded paging remains pending. Renewal continuity/aliases, broader partial/quota cases, conversion and UI/native acceptance also remain pending.

Provider transport now has a28-second whole-operation deadline, aggregate16MiB input bound and private official reset observations. Private `bank_gocardless_limits` is keyed by credentials digest plus endpoint (and general scope), independent of local connection/budget, so another connection cannot evade a known reset. Expired rows prune,4096 windows per credential and10000 globally bound storage. Observed quota consumption is private operational state independent of cancelable ledger application; token publication/capture still requires original generation/epoch. The table is absent from archives/feed/history. Global/endpoint restart/error edge qualification is pending; no full quota-matrix claim here. Provider failures retain sanitized typed codes and account scope; expired consent/credentials require explicit recovery.

Focused build/bin Clippy pass `gocardless-financial-checkpoint-{build,clippy}.log`. Shared SimpleFIN six financial workflows/76 served shapes, both held identity guards and GoCardless65 custody observations pass `gocardless-existing-financial.log`, `gocardless-existing-{capture,claim}-identity.log` and `gocardless-custody-transport-regression.log`. No source fixture changed and no broad/native/container suite rerun.

## Explicit renewal identity witness

`generate-gocardless-renewal-source.mjs` imports the immutable genuine linked GoCardless export, invokes the unchanged registered explicit account link with a different requisition/account ID but the original local account, then repeats refresh. Source retains every original financial row/ID/raw field exactly, while account/requisition metadata changes. Both real transport requests use the new IDs and request no opening balance. Generation/independent check pass `/tmp/gocardless-renewal-source-{first,check}.log`. Every original UUID from before/input remains literal; only genuinely new metadata UUIDs normalize. This confirms a needed explicit continuity join; it is not yet a replacement-runtime renewal PASS or permission for automatic cross-account/provider reassignment.

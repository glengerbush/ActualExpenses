# GoCardless29: source and contract checkpoint

Preparation only, based on frozen28 `62626e5`. No replacement runtime, migrations, provider/account calls or acceptance claim. Ticket29/spec58/60/61/62 require consent/linking, transaction delivery, renewal/error recovery, usable imported metadata, server-only credentials, web/native/public contracts together.

## Independent source evidence

`actualBudget/tools/generate-gocardless-source.mjs` executes the pinned26.10 server's complete GoCardless service/API, BankFactory and bank modules. TypeScript is transpiled, not replaced with normalization formulas. The only adapters supply Vite's eager file glob, synthetic secret storage and a strictly intercepted fetch; no network socket is opened. The fixture retains source-module hashes, dependency versions, original transport response bodies, requests and complete returned data/error details. Only a generated consent-reference UUID is aliased; fixed transaction/account IDs, ordering, amounts, dates, raw fields and multiplicity remain exact.

Eight observations in `actualBudget/tests/fixtures/gocardless-source-26.10.json` cover booked/pending and balance priority; date/array-note fallback and missing-date filtering; ING-specific note extraction, ID ordering and balance choice; EX consent; foreign-account membership refusal;429; expired-agreement401; institution-controlled consent creation. Fresh generation and independent `--check` pass `/tmp/gocardless-source-{first,check}.log`. This is provider translation/transport evidence, not a new financial-writer oracle. Reuse28's independent normalization/rules/matching/ledger witnesses after translating these exact source results.

Source paths below are relative to `actualBudget/reference/actual-26.10.0/packages/sync-server/src/`:

| Source | Responsibility / trap |
| --- | --- |
| `app-gocardless/services/gocardless-api.ts` | Real APIv2 paths, bearer transport, token/exchange, agreements/requisitions/details/balances/transactions. Source logs raw error bodies; replacement must sanitize them. |
| `app-gocardless/services/gocardless-service.ts` | LN membership guard before download, separate booked/pending normalization and sorting, starting balance, typed errors; institution feature/history/access limits and consent fallback. Reuse executable source rather than inferred financial arithmetic. |
| `app-gocardless/bank-factory.ts`, `banks/*_*.ts` | Full bank-specific selection, with `banks/integration-bank.ts` fallback. A default-only port cannot claim all source institutions. |
| `app-gocardless/utils.ts`, `util/payee-name.ts`, `util/title/` | Exact money rounding, date ordering, debtor/creditor naming/IBAN and title behavior. The source two-decimal conversion needs an explicit supported-currency decision; these eight cases qualify EUR only. |
| Loot-core `server/accounts/{sync.ts,link.ts}` | Existing shared import preferences, bank linkage, rule-before-match and reconciliation. Already exercised for28; keep one guarded financial writer. |

## Current official protocol constraints

Checked2026-11-10 against primary docs, without provider calls. The [quickstart](https://docs.gocardless.com/docs/bank-account-data/quickstart-guide) describes token/new returning refresh-only followed by token/refresh. The [endpoint reference](https://docs.gocardless.com/docs/bank-account-data/endpoints) also documents an access/refresh pair. Accept both documented shapes; the pinned source assumes access immediately. Retain refresh credentials privately and rotate access without resubmitting user secrets unnecessarily. Account endpoints may be limited to four calls/day; use returned endpoint quota/reset headers and429 rather than28's SimpleFIN-specific24/day rule. Institution limits govern consent/history; do not hardcode every institution to90 days.

The [status reference](https://docs.gocardless.com/docs/bank-account-data/statuses-and-error-code) distinguishes CR/GC/UA/RJ/SA/GA/LN/EX/SU; SU can occur after LN. Only verified LN with the requested account in the requisition authorizes a financial download. EX/SU require explicit renewal; rejected or reused authorization links need a fresh consent attempt. Provider errors must preserve typed actionable status while withholding raw diagnostic text.

## Planned public contract and ownership

- Extend the established banking provider discriminator with `gocardless`, preserving all existing SimpleFIN request/result shapes. Connection creation/replacement accepts write-only secret ID/key; durable browser recovery retains only the canonical digest and original nonsecret fields/key/clock/owner/epoch. Generic receipts/history/changes/portable archives never hold submitted secrets or access/refresh tokens.
- Keep institution listing (country), consent preparation, original-consent status/discovery and explicit account mapping distinct. Preparing consent is a keyed owned command, with durable local intent before the remote call; an ambiguous non-idempotent provider outcome requires review/cancel, not another requisition under the same identity. Status GETs do not silently create consent or financial rows.
- Consent URL/token material stays in private provider state, outside ordinary connection/cache/export results. A dedicated no-store handoff read can return the current URL to the operator. Open it only by explicit user action; callbacks/status checks bind the original connection/generation/requisition, never arbitrary callback account IDs. Existing plain-HTTP LAN app access remains supported; provider requests remain validated HTTPS.
- Connected discovery returns only accounts actually authorized by that requisition. Refresh reuses28's immutable captured-batch commit, account-scoped errors, remote-ID namespace, mapping/preferences, source financial writer, cancellation/receipt and generation/epoch fences. Provider balances/currency stay separate from computed ledger balances; no silent FX or balance adjustment.
- Renewal stages a new authorization generation and explicitly remaps verified accounts while preserving ledger/import identities. A stale old-generation callback, provider response or stored browser draft cannot reattach old consent. Missing/expired credentials after native import remain explicit, with imported usable nonsecret metadata preserved.
- Schema25 is reserved for29. Schema23 currently CHECKs provider=`simplefin`, so there is a real migration need. Before runtime/DDL, merge root `7fc2de9` (notes24 plus reviewed28) and use immutable genuine24 binary `.scratch/actualbudget-schema24-278e51f-server`, SHA256 `e0d425e2c5c9809f673f98ce700987f8286be0957fb12f03d05331513249edd9`. Preserve notes24 triggers/enrichment and all SimpleFIN/private exclusion behavior; no version relabeling.

## Focused implementation gates

Public TDD first: synthetic credentials → institution/consent → pending status → LN discovery → explicit local mapping → source-exact refresh → booked transition/repeat → EX/SU renewal/relink. Cover refresh-only token response, quota/reset429, invalid membership, partial account failure, missing dates, bank-specific ordering and no raw diagnostics. Compare full financial graphs and original IDs, not row counts alone.

Reuse28's storage/receipt/native evidence; add changed-provider joins: genuine24 DB/native upgrade, usable legacy GoCardless metadata without secrets, secret-free portable backup/duplicate/restore, delayed old-generation response/cancel, lost ACK/reload and original-identity replay. Browser qualification needs actual consent/recovery states, privacy, immutable drafts, plainHTTP and responsive/a11y renders. Representative native feed must observe newly delivered financial rows without provider credentials. No full-suite/container repetition before root composition. Live-access qualification remains separately recorded; no real provider/account access is authorized here.

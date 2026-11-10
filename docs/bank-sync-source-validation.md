# Bank28 core source checkpoint

Preparation only, branch `codex/bank28-source`, accepted basea5c784c. No replacement runtime/UI, provider credentials, real bank/account, claim or external network call. Root review precedes use by28.

`node actualBudget/tools/generate-bank-sync-source.mjs [--check]` executes installed pinned `@actual-app/api@26.10.0`, source commit2bebdbaae0b701120a8cad9d56aafd64fa04c577. JSON records package bundle SHA, original source hashes, frozen UTC2026-11-09, full synthetic input, transport requests/responses, exact transactions (including raw_synced_data/IDs/status/rank/edges), frontend accounts, payees, categories, preferences and link/refresh/repeat results.

Seam: genuine registered `simplefin-accounts-link` → `accounts-bank-sync` handlers, unchanged `accounts/sync.ts` normalization/reconciliation/opening-balance writer. Only `/simplefin/transactions` on a disposable127.0.0.1 server is stubbed, at the **source sync-server response boundary**. Stub accepts a synthetic local token and rejects every other HTTP route. Source registered `set-server-url`/`subscribe-set-token` supply this isolated configuration. Budget setup/export/preferences use genuine API operations; no source module erasure, SQL writes, replacement parser/matcher/calculator or provider HTTP implementation. The normal offline `runImport` can log its optional unauthorized cloud-upload warning; bank handler success/errors and exact transport-call counts are asserted separately.

| Case | Captured source behavior |
| --- | --- |
| first-sync-opening | Booked−1234 plus pending−200; source creates11434 opening and total10000. Initial link provider balance_current is null; repeated refresh sets10000. Both states remain original evidence. |
| manual-pending-booked-same-id | Fixed manual−1234 ID/notes survive pending→booked under one remote ID; booked clears it, default date behavior retains manual date; repeats preserve identity. |
| custom-payment-deposit-mappings | Different payment/deposit date/payee/note mappings; exact source `#`→`##` escaping, signs, dates and raw data retained. |
| exclude-pending-and-notes | Pending preference excludes the pending record; booked purchase retained with notes disabled. |
| update-bank-date | Same remote ID survives a changed bank date with `sync-update-dates=true`. |
| balance-only-preference | `sync-import-transactions=false` retains ledger while source updates provider balance25000. |

All six genuine **pre-link** ZIPs are retained, including their original source-generated IDs. `--check` executes fresh budgets independently; comparison aliases generated UUIDs consistently across fields, preference keys and embedded raw JSON, preserving fixed2828 manual IDs, remote IDs, money, graph edges and multiplicity. Original fixture UUIDs/values are untouched. No source-clock claim: official export is not synchronized CRDT evidence. Repeated source refresh must preserve every original financial row and ID.

Evidence: generation `/tmp/bank28-source-first.log`; independent checks `/tmp/bank28-source-check.log` and `/tmp/bank28-source-check-final.log`, all PASS6. Read-only audit: all six ZIP digests, SQLite integrity, exact original transaction IDs/amounts and absence of pre-existing bank links PASS. Generator formatted; diff whitespace clean. No broad suite or new CI registration.

Limits: this proves **core** bank normalization/writer behavior, not `sync-server/app-simplefin` raw v1/v2 translation, setup-token claim/rotation, balance-string parsing, institution/request limits, redirects, rate/error handling, source HLC migration, rules/split/transfer bank matrices, new pending IDs, provider pagination, replacement API/UI/native behavior or live access. Those remain28 owner work, using [bank-provider-plan.md](bank-provider-plan.md); this checkpoint does not accept28 or authorize provider calls.

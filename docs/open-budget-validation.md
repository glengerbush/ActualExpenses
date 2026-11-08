# Persistent budget slice — verification record

Ticket 02 is in progress. This record distinguishes completed checks from pending acceptance; it does not establish full rewrite parity.

## Independent opening-balance oracle

Generated `actualBudget/tests/fixtures/opening-balances.json` using the published, locked `@actual-app/api@26.10.0` in a disposable local budget. Only public account creation, account balance, transaction, payee and category operations supply the observations. Random identities are normalized to names. The clock is fixed at 2026-10-15 UTC; async timers run normally.

Seven cases cover zero, positive and negative opening balances, on/off-budget accounts, and positive/negative one-cent entries. Actual creates no entry for zero. Nonzero entries are cleared, unreconciled starting balances with the Starting Balance payee. On-budget entries use the Starting Balances income category; off-budget entries have no category.

Reproduction:

```sh
npm ci --prefix actualExpenses/tools/compatibility
node actualBudget/tools/generate-opening-balances.mjs --check
```

Generated and independently reran `--check` successfully on 2026-11-08. The engine's attempted end-of-import upload reports unauthorized because this oracle deliberately uses no sync server; subsequent public reads and comparison succeed. No production data is accessed.

- Fixture SHA-256: `d4a98a1dc42248f7a9f5cc9e6a17918c6a1f34cbade710cefec93c73e1a9d7a9`.
- Oracle package-lock SHA-256: `39b652f581ddbfeadf1454111da26aec95ac82e25b7b36367360396fa7b3c751`.
- Source provenance: Actual commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`, especially `packages/loot-core/src/server/accounts/app.ts`, `accounts/payees.ts` and `server/api.ts`.

## Outstanding acceptance

Replacement API comparison, process/container restart persistence, machine-readable contract, browser accessibility/responsiveness/error recovery and independent implementation reviews remain pending. Later tickets cover ordinary register writes, importing real budgets, Android transport migration and the remaining financial features.

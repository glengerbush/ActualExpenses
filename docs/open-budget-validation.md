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

## Browser and contract review

The browser worker's real-process suite passed creation, remembered selection, exact signed balances, validation with retained fields, and a lost committed response retried with identical identity/payload and one resulting account. Its default runner launches a disposable server and maps `actualbudget.test` to loopback; it asserts `isSecureContext === false` and unavailable SharedArrayBuffer. This exercises the plain-HTTP constraint rather than localhost's secure-context exception.

Keyboard navigation, failed-load recovery, 390px mobile layout and 320px with 200% text passed. Root reviewed the rendered desktop, mobile, enlarged-text and uncertain-write screenshots. No additional blocking visual findings. Review caught and corrected navigation retaining an old account-load error after returning to the budget chooser. The regression is covered in the browser suite. Browser emulation is not a physical Android-device upgrade test; that remains in the Android transition tickets.

Root independently ran `uv run actualBudget/tools/contract-smoke.py` successfully. The tool starts a disposable real server, validates the served JSON Schema definitions and checks actual success, invalid-input, idempotency-conflict, cross-origin and missing-resource responses against the OpenAPI contract. Financial correctness remains supported by the independent Actual fixture, not by the contract's own schemas.

## Integrated checks

- Root reran all 9 real-process HTTP tests: passed, none ignored. These include seven independent Actual opening-balance cases, abrupt restart durability, concurrent/restarted retries, changed-payload rejection, validation without partial writes, budget isolation, exact integer limits, configured-origin handling and static/API routing.
- Backend worker: `cargo check --locked`, strict Clippy for all targets and formatting passed.
- Frontend worker: typecheck/build, four Svelte autofixer checks, Impeccable detector and formatting passed. Production dependency audit reported zero vulnerabilities. Built JavaScript: 52.66 KB (20.19 KB gzip); CSS: 5.78 KB (1.88 KB gzip). These are bundle measurements, not overall resource-qualification claims.
- `python3 actualBudget/tools/container-smoke.py --browser` built the real Dockerfile and passed the same-origin HTML/JS/CSS checks, exact opening balance, complete container replacement with retained volume, and replay leaving one account/one opening entry. The browser suite also passed against that exact image over an insecure HTTP origin.
- Verified image: `sha256:ff20c3fa6962fbbecf476eef0a918ecf7a880fdae7dad32e2fd0b3e1e94de9c2`, 120,339,646 bytes, runtime UID/GID `10001:10001`, read-only root filesystem during the test. Temporary image/container/volume removed; no live data accessed. See [container evidence](../actualBudget/tools/container-smoke.md).

## Outstanding acceptance

Independent implementation reviews remain pending before ticket closure. Later tickets cover ordinary register writes, importing real budgets, Android transport migration and the remaining financial features.

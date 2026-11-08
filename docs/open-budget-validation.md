# Persistent budget slice — verification record

Ticket 02 is complete in `6f3661d` plus recovery fix `8d09012`. This record establishes the persistent budget/account slice, not full rewrite parity.

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

## Independent review — `611c1d2...6f3661d`

### Standards

Fresh Astra/high reviewer: **0 findings**. Reviewed production API/storage, Svelte UI, tests, oracle generation and container build against documented standards; no material Fowler smells. Svelte autofixers and Impeccable detector clean; mobile/enlarged rendering inspected.

### Spec

Fresh Astra/high reviewer: **1 P2 finding**, no scope creep or other incorrect behavior identified. Creation drafts and pending request identities lived only in component memory. Verified reproduction: enter account details, interrupt creation, reload; retry state disappears and reopening the form loses values. A lost committed response similarly loses the identity needed for safe reconciliation. This violates the parent specification's unsaved-form recovery and retry-after-restart requirements; the later expense-outbox ticket does not own budget/account creation.

Fixed in `8d09012`: validated, versioned drafts preserve original payload/entity/operation identity before submission, scoped to origin and budget. Reload restores fields and requires explicit retry of an unconfirmed creation. Storage failure prevents a new POST; deterministic identities prevent duplicate creation across tabs. Confirmed-rejection cleanup cannot replace a different saved pending request.

## Follow-up acceptance — `6f3661d...8d09012`

- **Standards: 0 findings.** Shared creation state removes duplicated retry logic; persistence/recovery boundaries are clear. Svelte and Impeccable checks clean.
- **Spec: resolved, 0 remaining findings.** Reviewer independently verified interrupted creation → reload → restored fields/request → explicit retry. No scope creep.
- Root independently reran the browser suite: all five groups passed, including a lost committed response followed by reload and identical request replay, one resulting account/entry, ordinary draft recovery, storage failure without POST, and two tabs creating one budget.
- Final `container-smoke.py --browser` rebuilt source commit `8d090124820d2bafdfe4f32a27ebf6abfdd2ce19` and passed container replacement/persistence plus all five browser groups. Image `sha256:583740a3ebd7e39fb7204152ee5ba2332221817282c2da893d584a9e08aa0eff`, 120,329,422 bytes, UID/GID `10001:10001`. Temporary resources removed. This supersedes the pre-fix image above.
- Updated frontend bundle: 58.95 KB JavaScript (21.83 KB gzip). Typecheck/build, formatting, changed Svelte checks and Impeccable detector passed.

All ticket 02 acceptance criteria met; parent rewrite remains active. Later tickets cover ordinary register writes, real-budget import, Android transport migration and remaining financial features. No release publication or production cutover occurred.

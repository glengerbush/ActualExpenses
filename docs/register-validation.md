# Register slice — verification record

Ticket 03 is complete in `0256ca6`, `ca9040d` and `0b66dc6`. Public API,
browser and container qualification passed; independent Standards and Spec
reviews report zero remaining findings.

## Baseline distinctions

- Actual's account/register UI sums all recorded dates (`packages/desktop-client/src/spreadsheet/bindings.ts` and `components/accounts/Account.tsx`). Its public `getAccountBalance` defaults to today's cutoff (`packages/loot-core/src/server/api.ts`). The replacement must distinguish all-date totals from as-of balances rather than compare unlike values.
- Duplicating a transaction resets cleared and reconciled flags (`packages/desktop-client/src/hooks/useTransactionBatchActions.ts`). A duplicate is a new stable entity, with its own idempotent creation operation.
- The calculator supports addition, subtraction, multiplication, division, exponentiation and parentheses. Exponentiation associates right-to-left. Negative numbers are primary values before exponentiation, so `-2^2` follows the baseline's result `4`.
- The baseline explicitly tests valid-prefix evaluation: `1+2)foo` → `3`, `(1+2)x` → `3`, `10+20 trailing` → `30`, and `1+2(3` → `3`. See `packages/loot-core/src/shared/arithmetic.ts` and `arithmetic.test.ts`; a conventional strict-end-of-input parser would change observable behavior.
- Public Actual amount conversion confirms `1.005` → `100`, `-1.005` → `-100`, `0.005` → `1`, `-0.005` → `0`, `2.675` → `268`, `-2.675` → `-267` minor units. Exact-rational rounding would differ. Binary64 expression evaluation with JavaScript-compatible rounding is confined to calculator input; stored ledger amounts remain checked exact integers.

`actualBudget/tools/generate-register.mjs` now generates `actualBudget/tests/fixtures/register.json` through the published Actual 26.10.0 API. Generation and an independent `--check` run matched without arbitrary sleeps or replacement-engine calculations. Fixed transaction IDs and normalized opening identity make the fixture reproducible. The duplication input follows the pinned UI hook; the public API itself has no duplicate method.

The first replacement HTTP comparison exposed stale rows from Actual's asynchronous query-cache invalidation immediately after an acknowledged edit. The generator now polls the public read until the requested identity/field change is visible, bounded by a monotonic five-second deadline, before recording financial queries. The regenerated fixture reproduces and the full HTTP comparison passes. It compares normalized rows, references, cleared/uncleared and calendar-cutoff balances through restart/replay.

The fixture covers income, expense, refund, uncategorized entries, leap-day/backdated/future dates, edits, duplication, deletion, cleared/uncleared totals, reference names and per-date cutoffs. All-date totals are 28,400 → 28,434 → 27,234 → 27,000 minor units; today's totals differ by the future expense. Replacement public API comparisons pass through restart and replay.

## Integration boundaries

The register worker owns ledger/reference operations and register components. The preferences worker owns locale/formatting helpers and settings; the root agent integrates shared application navigation and account views. Both use the shared request helper, including its explicit PATCH/DELETE method option, and the same error/retry semantics. Shared database migrations and OpenAPI integration are coordinated by the backend owner.

The integrated register/settings build passed Svelte checks and the five existing ticket02 browser groups, including insecure HTTP, creation recovery after a lost response, mobile layout and cross-tab creation. Final register/settings and recovery coverage passed against the real container as recorded below.

## Qualification

See [detailed API, upgrade, resource and browser evidence](../actualBudget/server/REGISTER-VALIDATION.md)
and [preference evidence](../actualBudget/web/docs/preferences-validation.md).
All 23 real-process HTTP tests pass (16 register/foundation, 5 preferences,
2 snapshot pagination), as does real-response OpenAPI validation. Pinned Actual
oracles cover seven opening-balance cases, four full financial snapshots,
24 calculator cases, 220 displayed amounts and 120 date-input cases.

The genuine ticket02 binary created the upgrade fixture through HTTP. The new
server preserved it and replayed the original operation; its pre-upgrade backup
opened successfully in the old binary. No production data was used.

The full browser suite passed against the final `0b66dc6` container, including CRUD,
inline references, balances, calculator, durable lost-acknowledgement recovery,
cross-tab submission, storage failures, mobile/large-text layouts and plain HTTP.
Concurrent insertion/edit/deletion invalidates a continuation snapshot and
refreshes coherent rows; failed cutoff requests retain confirmed totals/labels.
Original pending create/delete requests can be retried without preference data.

Image `sha256:7fc8a8aa7b3dd0916291a3aaec3011ef33112b14a88cc907a8ec9045d431652d`:
124,191,107 bytes, user `10001:10001`, persistence/replay verified across container
replacement. Disposable test resources removed.

## Review

Initial independent reviews of `9b6b29c...0256ca6` found two Standards issues
(unconfirmed cutoff labels and mixed concurrent pagination) and two Spec issues
(currency opt-in and abbreviated dates). `ca9040d` resolves all four.
Spec follow-up identified one additional valid-date draft recovery issue;
`0b66dc6` normalizes only the first unsent request using its captured format.
Eight direct saves across four formats pass for account and transaction drafts;
lost acknowledgements retain the exact original request across year changes.
Both final independent reviews report zero findings. This accepts tickets 03
and 43, not the unfinished replacement or production deployment.

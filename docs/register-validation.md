# Register slice — verification record

Ticket 03 is in progress. These baseline observations guide implementation; they do not establish completed replacement parity.

## Baseline distinctions

- Actual's account/register UI sums all recorded dates (`packages/desktop-client/src/spreadsheet/bindings.ts` and `components/accounts/Account.tsx`). Its public `getAccountBalance` defaults to today's cutoff (`packages/loot-core/src/server/api.ts`). The replacement must distinguish all-date totals from as-of balances rather than compare unlike values.
- Duplicating a transaction resets cleared and reconciled flags (`packages/desktop-client/src/hooks/useTransactionBatchActions.ts`). A duplicate is a new stable entity, with its own idempotent creation operation.
- The calculator supports addition, subtraction, multiplication, division, exponentiation and parentheses. Exponentiation associates right-to-left. Negative numbers are primary values before exponentiation, so `-2^2` follows the baseline's result `4`.
- The baseline explicitly tests valid-prefix evaluation: `1+2)foo` → `3`, `(1+2)x` → `3`, `10+20 trailing` → `30`, and `1+2(3` → `3`. See `packages/loot-core/src/shared/arithmetic.ts` and `arithmetic.test.ts`; a conventional strict-end-of-input parser would change observable behavior.
- Public Actual amount conversion confirms `1.005` → `100`, `-1.005` → `-100`, `0.005` → `1`, `-0.005` → `0`, `2.675` → `268`, `-2.675` → `-267` minor units. Exact-rational rounding would differ. Binary64 expression evaluation with JavaScript-compatible rounding is confined to calculator input; stored ledger amounts remain checked exact integers.

`actualBudget/tools/generate-register.mjs` now generates `actualBudget/tests/fixtures/register.json` through the published Actual 26.10.0 API. Generation and an independent `--check` run matched without arbitrary sleeps or replacement-engine calculations. Fixed transaction IDs and normalized opening identity make the fixture reproducible. The duplication input follows the pinned UI hook; the public API itself has no duplicate method.

The first replacement HTTP comparison exposed stale rows from Actual's asynchronous query-cache invalidation immediately after an acknowledged edit. The generator now polls the public read until the requested identity/field change is visible, bounded by a monotonic five-second deadline, before recording financial queries. The regenerated fixture reproduces and the full HTTP comparison passes. It compares normalized rows, references, cleared/uncleared and calendar-cutoff balances through restart/replay.

The fixture covers income, expense, refund, uncategorized entries, leap-day/backdated/future dates, edits, duplication, deletion, cleared/uncleared totals, reference names and per-date cutoffs. All-date totals are 28,400 → 28,434 → 27,234 → 27,000 minor units; today's totals differ by the future expense. Comparing these outputs through the replacement public API remains part of acceptance.

## Integration boundaries

The register worker owns ledger/reference operations and register components. The preferences worker owns locale/formatting helpers and settings; the root agent integrates shared application navigation and account views. Both use the shared request helper, including its explicit PATCH/DELETE method option, and the same error/retry semantics. Shared database migrations and OpenAPI integration are coordinated by the backend owner.

The initial integrated register/settings build passed Svelte checks and the five existing ticket02 browser groups, including insecure HTTP, creation recovery after a lost response, mobile layout and cross-tab creation. New register/settings coverage and review fixes are still underway; this is a regression check, not acceptance of those features.

## Pending acceptance

Complete CRUD/reference creation and calculator API/UI, restart/retry verification, baseline comparisons, responsive/keyboard/error recovery, integrated checks and independent review. No completion or deployment claim is made by this preparatory record.

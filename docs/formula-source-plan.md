# Formula source preparation — tickets41/42

Read-only audit,2026-11-10. Pinned Actual26.10.0 commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. No implementation or passing
qualification is claimed.

## Production oracle seam

Report execution lives in desktop-client `hooks/useFormulaExecution.ts`, using
HyperFormula plus loot-core `shared/formulas/customFunctions.ts`. It evaluates
three times: discover named-query/account needs; prefetch those; discover
compositional budget requests; prefetch those; evaluate the final result.
Execute unchanged production functions with real engine DTOs and fixed clock.
The source `formula-card-integration.test.ts` helper performs textual QUERY
replacement and therefore is not a complete production oracle.

Preserve the supported spreadsheet language, not just arithmetic syntax.
Custom functions include BALANCE_OF, FIXED, INTEGER_TO_AMOUNT, QUERY,
QUERY_COUNT, QUERY_EXTRACT_CATEGORIES, QUERY_EXTRACT_TIMEFRAME_START/END,
BUDGET_QUERY, FORMATNUMBER and FORMATCURRENCY. Locale and saved currency
preferences affect formatting. Category extraction can return a range; it
must remain composable with budget queries. Distinguish monetary minor-unit
aggregates from formula/display amounts and counts.

Budget-query dimensions are budgeted, spent, goal, balance_start and
balance_end. They read envelope-budget-month DTOs, including prior-month
carryover and intervening balances. Probe both budget methods rather than
assuming the tracking result. Missing query/account behavior, case sensitivity,
empty results and errors need actual source observations. Source may swallow
query failures as zero; the replacement's failure handling must stay explicit
where returning zero could misrepresent a failed request.

FormulaCard owns formula, queries, queriesVersion, colorFormula, title and
font settings. Conditional color evaluates with RESULT and theme_* named
expressions. Preserve user text and settings through existing report documents
and migration. Keep financial/query evaluation server-owned and bounded by
input size, query work, time, memory and result size. No host filesystem/network
access or arbitrary host code execution should come from formula input.

## Rule extension

loot-core `server/rules/action.ts` uses the same HyperFormula plugin but a
different transaction context and conversion boundary. BALANCE_OF is relative
to the transaction being evaluated, unlike report total balances. Named
expressions include transaction fields, today, account_name and category_name;
null/object values become empty strings. Parent text receives literal escaping
even when it looks like a formula/date/number. Numeric formula output is rounded
and converted to minor units before action type conversion. Formula failures
accumulate rule errors; source templates use Handlebars with separate coercion
rules. Invalid template dates become the source sentinel9999-12-31.

Do not conflate formula and template semantics. Observe ordinary and split
action execution, parent context, rule ordering, date/boolean/string results,
invalid expressions, preview/apply, and idempotent replay through genuine source
workflows. Reuse41's evaluator only after these different boundaries are covered.
Relevant source tests are useful scenario inventories, not a replacement for
the agreed whole-workflow source/API witnesses.

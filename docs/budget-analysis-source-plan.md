# Budget analysis source preparation — ticket36

Read-only source audit,2026-11-10. No implementation or qualification claimed.
Pinned reference: Actual26.10.0, commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`.

## Reusable source boundary

The unchanged frontend function
`packages/desktop-client/src/components/reports/spreadsheets/budget-analysis-spreadsheet.ts`
contains the complete projection, including category filtering and monthly
running balances. Execute that function with genuine engine `get-categories`
and `envelope-budget-month` responses; do not replace its calculation with a
handwritten fixture formula. Ticket34 already established this frontend-DTO
oracle approach. Exact source queries and full source hashes should accompany
the eventual witness.

The source is monthly only. It selects expense categories, optionally hidden,
and uses category/category-group predicates without `customName`. Identity
operators compare IDs; text operators compare category/group names. AND/OR
combine category sets. Other predicates do not filter transaction activity.
Regex is case-insensitive,256-character maximum; invalid patterns match none.

Seed the running balance from the prior month's category carryovers. Positive
balances carry; negative balances carry only when enabled. Uncarried negative
balances become next month's displayed positive overspending adjustment, with
the final adjustment retaining its negative source sign. Initial adjustment is
zero. An all-zero future month preserves the running balance. The projection
uses the actual envelope-month command even though its route has no budget-type
guard: capture genuine tracking and envelope executions before deciding the
replacement behavior; do not infer a tracking formula from its label.

## Interface and saved configuration anchors

`components/reports/reports/BudgetAnalysis.tsx` and `BudgetAnalysisCard.tsx`
use dashboard type `budget-analysis-card`, saved conditions/operator/timeFrame,
Line/Bar, showBalance, balanceOnly and showHiddenCategories. The existing
experimental `budgetAnalysisReport` flag guards routes in `ReportRouter.tsx`.
Date selection extends through next December or the latest transaction month,
whichever is later. Preserve fixed/sliding date semantics through the shared
report clock. User-owned names remain unchanged.

`spreadsheets/budget-analysis-export.ts` owns the source CSV contract; the full
report offers CSV download. Graph/details must retain exact financial values,
including overspending and balance-only presentations. Reuse the accepted
report document/CAS/native conversion, existing locale layer and revision cache
where they fit. No new schema or abstraction is proposed by this preparation.

## Bounded proof priorities

The eventual worker should choose witnesses for carryover enabled/disabled,
prior-month seed, future zero months, hidden/income categories, category/group
AND/OR and text filters, both budget methods, saved configuration and CSV.
Use the running public API, browser interactions and native recovery seams
already agreed with the user. Source observations must precede expected-value
claims; this note is a map of source behavior, not passing test evidence.

# Sankey source preparation — ticket37

Read-only source audit,2026-11-10; no implementation or qualification claimed.
Pinned Actual26.10.0 commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`.

## Oracle boundary

The unchanged frontend module
`packages/desktop-client/src/components/reports/spreadsheets/sankey-spreadsheet.ts`
exports `createBaseGraphSpreadsheet` and `buildSankeyData`. Execute them with
genuine engine responses for `make-filters-from-conditions`, AQL queries and
`api/budget-month`, plus source category ordering. Capture both base graph and
final nodes/links/tooltips; do not substitute a handwritten graph oracle.
Existing report DTO harnesses provide the pattern. Theme/translation adapters
must not replace financial or graph calculations.

## Semantics to preserve

Spent mode aggregates amounts by category/account, also payee for income
groups, before taking absolute values and retaining the original sign.
It iterates supplied category groups: observe uncategorized and split behavior
through genuine queries rather than inferring it. Grouping accounts merges
their identifiers into the source all-accounts node. Optional transfer flows
require both linked on-budget transaction legs inside the filtered date range;
opposing transfers net by account pair. Cross-month/filter-missing peers and
on/off-budget boundaries need witnesses.

Budgeted mode requests every month through `api/budget-month`, accumulates
category fields and overspending, uses the first month's carry-in and last
month's to-budget, and additionally queries the following month to derive
funds held forward. Category/group predicates filter category data, while the
aggregate budget context remains unfiltered. Observe both budget methods.

The final graph transformation order matters: clone, group small nodes,
sort, clean, add hidden nodes, percentage labels/colors, restrict layers,
then convert. Preserve exact link totals and Other-node breakdowns. Negative
allocations/refunds use source structural conventions: the source documents
that a Sankey cannot directly represent negative amounts and totals can
differ from the budget overview. Do not silently convert it into a generic
income/spending chart or claim every flow is a recorded transaction.

## Saved interface and bounded proof

`SankeyCard.tsx`, `Sankey.tsx` and `SankeyGraph.tsx` own saved timeFrame,
conditions/operator, spent/budgeted mode, topNcategories, per-group/global/
budget-order sorting, layerFrom/layerTo, grouped accounts, transfers and
percentage display. The card caps configured nodes by available height;
preserve user configuration separately from responsive presentation. Inspect
the source flag and widget registry before wiring navigation.

Use the existing report document/CAS/native conversion and fixed date resolver.
Calculate bounded aggregates/graphs on the server; return an accessible exact
table with the visual graph. Reject excessive node/link/work requests clearly.
Financial probes should cover splits, refunds/negative allocations, hidden and
income categories, grouped accounts, net/reversed transfers, each budget method,
Other aggregation, layer restriction and filters. Browser proof must include
keyboard access to grouped-node detail, narrow layout and saved recovery.
These are preparation priorities, not passing evidence.

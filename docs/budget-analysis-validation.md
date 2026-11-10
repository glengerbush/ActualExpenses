# Budget analysis36 contract and evidence

Isolated codex/analysis36 from ad0c023 plus35 correction b71d6bb.35 remains frozen separately; root owns its shared47/main composition. Reuse schema22 reporting documents, generic preference conversion and owned document CAS/draft/cancellation; no new migration proposed. Future schema23/24 changes remain other owners’ work.

## Acceptance/API pinned before implementation

- Explicit default-off `experimental.budgetAnalysisReport`; preserve source `budget-analysis-card` IDs/settings/clocks. Deliver a real navigation/tab only with working calculations. Reuse dashboard save/reset/remove/duplicate, stale revision conflict and original epoch/request/cancellation recovery; downloaded-cache reset cannot remove drafts/outboxes.
- Monthly planned allocations, signed recorded spending, running balance and overspending adjustments must match unchanged pinned26.10 `createBudgetAnalysisSpreadsheet` over genuine `get-categories`/`envelope-budget-month` DTOs. Preserve prior-month seed, positive carry, optional negative carry, future zero months, hidden/income filtering, category/group identity/name AND/OR conditions and source invalid-regex behavior. Transaction/account conditions do not filter this analysis and are visibly explained. Both budget methods require genuine observations, not inferred formulas.
- Source Line/Bar, showBalance/balanceOnly, category visibility, fixed/sliding dates, exact accessible table, source CSV and bounded month details. Planned cells, recorded entries and derived carry adjustments must be distinct. Respect privacy/locale, responsive light/dark/keyboard/plain HTTP. Limited disconnected cached views only.
- `POST /api/v1/budgets/{budget}/reports/budget-analysis`: `{version:1,as_of,options:{startDate,endDate,conditions?,conditionsOp?,showHiddenCategories?}}`, explicit calendar dates normalized to monthly intervals. Visual options stay outside the financial query. Response has full source-shaped `result`, selected categories, immutable `epoch`, current `snapshot`, `as_of`, mixed plan/recorded basis, assumptions and cache input revision/hit.
- `POST .../reports/budget-analysis/drilldown`: `{query,snapshot,month,metric:"spent"|"budgeted",offset,limit}` returns bounded underlying recorded entries or planned allocation cells, exact signed totals/count/paging and basis. Recompute selected categories against the supplied immutable snapshot; reject stale snapshots, unsupported metrics/dates and oversized work. Derived balance/overspending are explained rather than represented as purchases. CSV is generated from the bounded exact result using the unchanged pinned exporter as oracle, with translated shipped headers and formula-safe strings.
- Router-owned cache8entries/16MiB, keyed by budget epoch, relevant financial entity revision, canonical financial options and as-of. Check feature/method before hits; reporting-name/layout/display changes must reuse unchanged financial work. A hit updates response snapshot metadata for current inspection without changing financial values. Successful online results survive optional browser-cache storage failure.
- Bounds:64KiB query,20 conditions,5000 active categories,100000 selected posted/plan rows,20000 displayed monthly points and selected category/month detail cells,5m carry steps, existing bounded ECMAScript work budget,8MiB response; no silent truncation or arbitrary elapsed-year cutoff. Reuse budgeting category calculations through a scoped report seam while keeping ordinary budget-page limits/behavior intact.

Agreed seams remain running public API/browser/representative native HTTP, independent pinned26.10 source oracle and public native backup/recovery. No private mirrored tests, direct DB fixture mutation, fabricated old-schema claims or live data. Root reviews the coherent completed slice; no push/release/deployment.

## Initial source observation (not qualification)

Read-only source map `docs/budget-analysis-source-plan.md` reused. `/tmp/analysis36-source-probe.log` executed real26.10 SDK import, supported budget-method switch +export/import reload, and real `envelope-budget-month` in envelope and tracking. Tracking returns populated tracking-sheet category cells under that command (not an empty/unsupported result); envelope and tracking allocations/carry behavior differ. Source analysis must follow those actual method cells, then apply its own unchanged projection. No handwritten expected financial values established yet.

## Backend development checkpoint

UI, complete browser delivery and whole36 acceptance remain pending.

- Independent pinned26.10 generator:10 envelope +4 tracking projections and corresponding exact CSV strings over real commands/unchanged frontend projection/exporter. Genuine archives preserve opted-in source widget IDs/config. Includes prior seed, positive/negative carry, late-month final negative adjustment, future zero months, refund, hidden/income categories, group/name AND/OR/regex/literal punctuation, invalid/over256-character regex and ignored named/other conditions. Supported SDK expense-category/income-group case proves source counts spending but omits that group's allocation/carry cells. No fabricated SQL or old-schema data.
- Two independent real source AQL November detail queries return7 entries totaling-5413 per method. Source projection/ledger totals agree; source DTOs separately supply contributing plan cells. Initial exploratory multi-operator date object ignored the upper bound; changed to separate genuine AQL clauses before recording final details. A one-month food case now proves final adjustment-1600 envelope/-4600 tracking. Source archive bytes and all other projections remained unchanged when tightening that date witness.
- Public tracer `/tmp/analysis36-api-red.log`404→`/tmp/analysis36-api-first-green.log` source envelope PASS. Detail endpoint404 `/tmp/analysis36-details-red.log`→green. `/tmp/analysis36-api-scoped.log` PASS all14 source results, source ledger membership/page totals, planned-cell basis, source opt-in/config, readonly companion/reporting, stale snapshots, reporting name/chart cache reuse with fresh response snapshot, opt-out/re-enable, posted/plan revision invalidation and typed input/work rejection. All honor supplied server; fixture stages use packaging-only isolation and each original is duplicated before mutations.
- Budgeting exposes typed category cells for a scoped report; full budget-page limits/data remain intact. Scope SQL reads only selected category/history rows through the requested end, caps100000 rows, skips unused notes/goals/holds, caps20000 output/category-month cells and5m carry steps. New analysis sums use i128 intermediates and exact safe-integer bounds. Current source-compatible category-name rules own their literal/regex semantics. Router-owned revision cache reuses unchanged math across presentation changes; no financial/clock/receipt writes.
- Served OpenAPI types both reads/errors with exact±9007199254740991 bounds. `/tmp/analysis36-native.log` PASS both methods’ source settings, read/detail/error schemas, public native22 backup/duplicate/restore, immutable reporting clocks, restored epoch and cache invalidation. Version22 is observed from actual manifests; this is current-native round-trip evidence, not migration from a fake old schema. Future23/24 composition can reuse the version-observing consumer.
- `/tmp/analysis36-native-core.log` PASS one focused companion-module plain-HTTP consumer, exact tracking source result, readonly companion and cache. `/tmp/analysis36-budget-regression.log`:10 affected budgeting public integration tests PASS (including genuine15 upgrade, both method oracles, native archives and actual drilldown). `/tmp/analysis36-clippy.log` all targets warnings denied PASS. Targeted Rust/Node/Kotlin formatting/diff checks; shared cache extraction retains forecast cache behavior. No broad device/container/full-suite rerun.

Source reproducibility `/tmp/analysis36-source-final.log` PASS14 projections/CSV plus exact DTO/detail output. Product/backend review checkpoint only; frontend/CSV download, opt-in navigation, privacy/localization/responsive/recovery witnesses and consolidated review remain required. No capability advertised before delivery.


## Bounded backend review corrections

Root identified an unused scoped budget-summary overflow and the distinction between matched-row limits and SQLite scan work. No UI acceptance claimed.

- Genuine SDK archive/source witness adds ten funded months, each income+1e15, expense-1e15 and expense allocation1e15. Unchanged source analysis of October returns budgeted1e15/spent-1e15/balance0. Native ordinary budget read succeeds; old scoped analysis returned400 on the fake, unused cumulative to-budget value (`/tmp/analysis36-large-red.log`). Scoped reads now bypass unused summary arithmetic/validation entirely; per-category historical values, relevant carry and displayed totals retain exact bounds. Initial prior-month budget/spending/reset totals remain i128 because only prior carry seeds the report. Source income-group actual-only cells do not calculate omitted allocation/carry values.
- `/tmp/analysis36-large-source-check.log` PASS reproduces the genuine frozen archive after clean reload. Initial setup logged temporary source safe-number errors while all plans were entered before spending; the final frozen dataset/reload is valid, and the maintained producer now funds/spends each month before continuing. No financial expected value was rewritten from native output.
- `/tmp/analysis36-large-green.log` PASS ordinary budget200, exact final-period analysis and unchanged companion. Existing14 projections/details/cache/errors PASS `/tmp/analysis36-review-api.log`;10 affected budgeting tests PASS `/tmp/analysis36-review-budget.log`; Clippy warnings denied PASS `/tmp/analysis36-review-clippy.log`.
- Both analysis reads now scope a32m SQLite VM-step progress guard, using the public-query evaluator’s existing pattern with guaranteed hook reset, and return typed `analysis_work_limit` refusal. This complements row/carry/regex/output limits; row counts alone do not establish bounded SQL work. Guard exhaustion is not claimed exercised here; final51 volume measurements remain separate. Drilldown uses explicit inclusive ISO date bounds rather than `substr(date)`.
- Read-only EXPLAIN diagnostic on a disposable native22 budget imported through the public API (`/tmp/analysis36-explain.log`) observed `transactions_category_history` category/tombstone/date range seeks for scoped history and spent details; details additionally use a temporary ordering B-tree. This is a plan observation for that fixture, not a financial test seam or performance guarantee. No DB writes, schema/index change or broad performance suite.

## Root composed backend review

2026-11-10: Reviewed878cc96 and corrective678c7f7 against the source contract,
including scoped carry arithmetic, category selection, cache invalidation,
drilldown basis and SQLite progress-hook cleanup. Both reported backend findings
are closed. Integrated at `baef1cb` with reviewed bank/query schema24 changes.
Semantic OpenAPI merge retains both parents' operations/schemas and resolves all
local references:113paths/192schemas. Existing forecast safe-money limits remain.

Combined build PASS `/tmp/analysis36-compose-build.log`; all14 independent source
projections and detail/cache/error checks PASS `/tmp/analysis36-compose-api.log`;
funded-history overflow regression PASS `/tmp/analysis36-compose-large.log`.
Served request/response/error schemas and both methods' current-native24
duplicate/restore, settings/clocks and epoch-cache checks PASS
`/tmp/analysis36-compose-native.log`. The consumer observes the produced schema
version; original Actual26.10 archives remain unchanged. Prior source reproduction,
native companion and budgeting-regression evidence reused. UI and whole36
acceptance remain pending; no broad suite or guard-exhaustion claim.

# Forecast35 contract and evidence

Base: accepted maina5c784c, isolated branch/worktree codex/forecast35.34 remains frozen separately for review/integration. No migration planned; reuse schema22 reporting documents, preferences, schedule/rule/ledger modules. No push, release or deployment.

## Acceptance pinned before implementation

- Explicit opt-in `experimental.balanceForecastReport`, default false; preserve source preference and source `balance-forecast-card` IDs/meta/clocks. Disabled configurations remain preserved and clearly unavailable. Deliver a real navigation command only with the feature.
- Schedule-based projections start from selected, filtered posted history; add future posted entries and simulated schedule occurrences. Preserve single/recurring/approximate/ranged amounts, duplicate-post suppression, transfer legs, accountless combined-only explanation, ordinary/linked rule effects, source account/filter semantics and fixed calendar dates. Simulation performs no ledger/schedule/receipt/clock writes.
- Tracking-budget source is available only for tracking budgets: aggregate on-budget starting balances plus monthly planned income minus planned expenses, monthly only; account/report filters visibly ignored. Explicitly explain its planning horizon/support limits. Forecasts and their assumptions/errors stay distinct from recorded balances.
- Daily/monthly chart, accessible exact values and scheduled-entry explanations; saved widget settings and current-revision CAS/reset/recovery through the reporting registry. Original request/epoch/cancellation and local draft survive interruption/cache reset, as existing reporting operations do. Responsive/light/dark/keyboard/privacy/plain HTTP; no login or full offline budgeting.
- Bound input rows, accounts, expansion, rule/regex work and output; reject oversized/unsupported calculations without silent truncation. Cache by immutable input revision, request and as-of date; invalidate on source/epoch changes. Cached/offline results clearly labeled.

## Public API contract

`POST /api/v1/budgets/{budget}/reports/forecast`: `{version:1,as_of,options:{startDate,endDate,source,granularity,viewStart?,viewEnd?,accountIds?,conditions?,conditionsOp?,includeAccountlessSchedules?}}`. Sources `schedules`/`tracking-budget`; granularity Daily/Monthly. Explicit calendar days; UI resolves saved source month/timeFrame settings through the existing date resolver. Request requires opt-in. Read transaction binds all inputs to one budget snapshot/epoch.

Response `data`: source-shaped `result` (`dataPoints`, `lowestBalance`, `forecastStartDate`, `forecastEndDate`), server-computed displayed `chart`, distinct scheduled-occurrence count, explicit assumptions/warnings, `snapshot`, `epoch`, `as_of`, `recorded:false`, and cache revision/hit. Public served OpenAPI will type all variants/errors. Returning source forecast aggregates is bounded; browser never downloads full ledger history. Source daily low and displayed monthly low must be labeled distinctly if both are exposed.

Provisional actual-work bounds:500 accounts,100000 selected posted rows,10000 generated schedule occurrences,20000 account/day output cells, shared2m recurrence/rule work and existing regex budget,8MiB result and bounded router-owned memory cache. Bounds may be tightened from measured public traces, not silently truncate data. No arbitrary historical limit for otherwise bounded work. Tracking-budget future planning follows the pinned source’s documented12-month limitation, with unsupported imported settings retained for explicit review.

Agreed seams: running public API/browser/representative companion HTTP, independent pinned26.10 financial and migration oracles. No mirrored private tests, raw SQL fixture fabrication or live household data. Pinned `forecast/generate` is available in actual26.10 engine and was probed on a disposable immutable source ZIP copy; source projection/rule/schedule/tracking/chart modules are the reference.

## Evidence

Backend development checkpoint only; UI and complete35 acceptance remain pending.

- Independent pinned26.10 whole `forecast/generate` oracle: basic saved opted-in widget plus extended12-case matrix (basic repeated), whole-month viewport and group-membership allocation:14 distinct calculation requests, two unchanged source chart display variants. Genuine source SDK fixture setup only. Schedule/range/paid/transfer/rule/accountless/filter/closed/future/past/tracking cases retain exact raw source financial outputs.
- Public red→green: missing endpoint404; virtual-account rule50→60; source tracking grouping and viewport/count handling. Running public API verifies full raw result, display/counts, readonly companion/schedules, cache hit then posted-revision/flag invalidation and typed bounds/errors. Final log `/tmp/forecast35-backend-api.log` PASS.
- `/tmp/forecast35-native.log` PASS served request/response/error schemas, original opt-in/widget IDs/settings, native22 backup/duplicate/restore, retained reporting clocks, restore epoch rotation/cache invalidation. No schema migration or old-schema claim.
- `/tmp/forecast35-backend-regressions.log`:24 affected rules/schedules public integration tests PASS. `/tmp/forecast35-backend-clippy.log`: all-target Clippy with warnings denied PASS.
- Backend bounded read transaction, rules/schedule simulation and router-owned cache do not post financial data. UI delivery, interruption/recovery, accessibility/privacy/localization and final consolidated review still required.

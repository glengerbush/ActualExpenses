# Forecast35 contract and evidence

Base: accepted maina5c784c, then merged accepted27/34 maind83c37d at95f9f94, isolated branch/worktree codex/forecast35. No migration planned; reuse schema22 reporting documents, preferences, schedule/rule/ledger modules. No push, release or deployment.

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

### Bounded backend review correction

Root review identified distinct source account selection and simulated LIKE semantics. Added8 whole-handler cases on a disposable source SDK account rename `Checking Mixed %?` and ordinary rule adding literal `?`, `%` and backslash notes. Public native setup mirrors those supported mutations; account point arrays compare stable date/account identity because renaming can reorder source presentation. All original14 calculations remain unchanged. New cases cover lowercase/uppercase anchored regex, literal account `%`/`?`, simulated note/payee `?`, backslash+`%`, and an empty account selection over600 years. Source22 distinct requests total; display2variants remain distinct from that count.

Forecast-only correction: lower account names before case-sensitive regex; literal lowercased substring selection; simulated LIKE uses source anchored regex and `%` expansion with literal `?`; recorded predicate behavior remains unchanged. Empty selections return before ledger/rule/schedule expansion or output-cell checks. `/tmp/forecast35-review-green.log` PASS public source comparison; `/tmp/forecast35-review-source-check.log` PASS reproducible source check; `/tmp/forecast35-review-clippy.log` PASS warnings denied. Initial public red included account-filter400 on the pre-merge binary; mixed-case fixture then exposed presentation ordering, normalized explicitly rather than changing financial expectations. No claim of a financial mismatch red beyond recorded failures. UI not yet accepted.

### Completed UI/native checkpoint; composed47 check pending

- UI reuses dashboard registry CAS, original owned epoch/key/body/cancellation, local draft and layout controls. Delivered Balance forecast tab/navigation command, new/edit/remove widgets, source/date-mode/range/granularity/account/filter controls, and explicit default-off experiment. Source legacy dates and unrecognized metadata remain retained. Tracking-only/monthly/12-month limits produce actionable errors; no silent source/granularity fallback.
- `ForecastWidget` renders combined estimated trend, lowest displayed/ending values, paged exact table and simulated schedule details, unique occurrence count and source assumptions. No ledger download or posting. Revision/epoch-bound downloaded estimates are labeled cached; opt-out hides them. Privacy masks values and removes geometry, conservatively also when preference epoch is unknown. Async prop transitions use generation guards; autofixer’s two state-effect advisories are intentional network reads, not derivable calculations.
- `/tmp/forecast35-browser-final.log` PASS source Nov10/11 values104.00/103.00, save, lost request/cancel before delivery, reload, public downloaded-cache reset preserving draft/fence, exact cancellation retry and unchanged document, tracking-method error/recovery, transport-loss cached estimate/recalculate, navigation/new/remove, real Settings opt-out/re-enable preserving every reporting document, privacy, German controls/user name preservation, responsive keyboard. This public test honors supplied server; packaging-only ZIP comment isolates stages. Initial browser setup mismatches (dashboard selection, implicit select label, inherited cancellation copy) were corrected; the meaningful UI red was busy→ready leaving a new widget unloaded, now fixed with guarded prop-driven recalculation.
- Visually inspected `forecast-desktop.png`, `forecast-mobile.png`390CSS, `forecast-dark-german.png`1280CSS, and `forecast-dark-large-text.png`640CSS with actual200% root font at DPR1 (not browser-menu zoom or DPR2). No page overflow; Tab moves widget name→source/date controls. A cropped detail of the same full render verifies translated selects and readable large text; the crop changes no layout or values. Forecast-only view stacks widgets without changing their saved dashboard coordinates.
- `/tmp/forecast35-dashboard-regression.log` PASS affected dashboard creation/layout/remembered view/cancellation/cache reset/privacy/responsive regression. `/tmp/forecast35-native-core.log` PASS one focused real companion-module plain-HTTP consumer comparing full source forecast and unchanged companion state/cache. Initial default JDK27 launch failed; rerun with installed JDK17 passed. Existing native22 restore/schema evidence remains valid; future schema23/24 composition must update current-version assertions honestly.
- Svelte check0 errors/0 warnings `/tmp/forecast35-ui-check.log`; production build `/tmp/forecast35-ui-build.log`; five autofixers no issues (intentional async-read advisories above); Rust final build `/tmp/forecast35-final-build.log`; targeted formatting/diff checks. Prior source/backend/native proofs reused unchanged; no full Android/device/container rerun claimed.

Root backend reviews e26b6cc/a2c183e PASS. Root identified an inherited shared report-translator null-category rule owned by47: source excludes transfers/parents. Added a23rd genuine full-handler case (`category is null`) on the existing transfer/schedule archive with documented SDK mutations. `/tmp/forecast35-null-source.log` generated independent expectation, `/tmp/forecast35-null-source-check.log` reproduced it; `/tmp/forecast35-null-composition-red.log` intentionally records the current composed mismatch. No shared translator edit here. This single composed public check remains pending root’s47 integration; whole35 acceptance is not claimed. UI checkpoint is ready for consolidated review; no release/push/deployment.

### Root composed review —2026-11-10

Integration branch `codex/integrate-bank-forecast-query` at `5d12801` combines35
through `54d201e`, reviewed47 read corrections through `3d26ae3`, and frozen
banking23 foundation. The23rd whole-source forecast comparison now passes with
the shared report predicate correction: `/tmp/bank-forecast-query-forecast-api.log`.
Rust build and Svelte0/0 pass; OpenAPI parent106/108 operations are preserved in
109 combined paths. This is scoped integration evidence, not complete28/47 or
main acceptance. Earlier native22 evidence is not relabeled as23/24 evidence.

Root reviewed financial/filter/cache code, both UI corrections and the actual
quota-failure browser witness. Desktop/mobile/dark-large-text captures inspected.
Merged ForecastWidget autofixer reports no issues; its two async-effect
suggestions are intentional network reads guarded against stale responses.
Impeccable static scan of the two new Svelte components reports no findings
(`/tmp/bank-forecast-query-forecast-detect.json`); the first invocation used an
invalid CLI flag and was corrected, not counted as a passing scan.

Scoped Impeccable review:17/20 (accessibility3, performance3, theming4,
responsive3, implementation integrity4). Native controls, labeled paged tables,
token colors and privacy geometry removal fit the existing application. Browser
evidence covers keyboard,390CSS width and640CSS with200% root font; physical
touch/screen-reader and whole-system performance qualification remain51 work.
No remaining35 review findings; final main composition is pending other slices.

### Narrow UI review correction

Root found blank forecast names falling back to the internal card identifier, and optional downloaded-cache write failure erasing a successful calculation. Added translated default title and isolated cache-write warning; current online results survive while cached fallback remains reserved for failed calculation. `forecast-cache-browser.mjs` injects quota failure only for downloaded forecast entries on disposable browser data, verifies fresh104.00/table plus visible cache warning (not stale-cache label), then saves a blank name and verifies the default Balance forecast heading and current values. `/tmp/forecast35-review-ui-red.log` missing-table RED → `/tmp/forecast35-review-ui-green.log` PASS. Svelte0/0 and build PASS `/tmp/forecast35-review-ui-{check,build}.log`; two autofixers no issues (same documented async-read advisories); focused formatting/diff checks. Other unchanged source/native/layout proofs reused; shared23rd null-category gate is being composed by root, not silently skipped here.

### Monetary contract boundary correction

During36 schema work, the report worker identified35's four money schemas using
±9000000000000000 instead of the runtime/public ±9007199254740991 range. Root
extended the maintained native consumer with two publicly created budgets at
the actual positive/negative safe-money limits. True RED
`/tmp/forecast-money-schema-red.log`: runtime200 returned the exact negative
boundary, rejected by the served chart schema. All four forecast amount/balance
schema limits now match the existing runtime invariant. No financial algorithm
or source fixture changed. Combined build and both boundary responses/schema
validation plus the existing forecast native24 round trip PASS
`/tmp/forecast-money-schema-{build,green}.log`. Review finding closed.

### Local acceptance

Ticket35 accepted on local main836918a after the documented source, public API,
browser, monetary schema and native24 checks. No remaining feature findings;
final whole-system qualification50–53 remains open. No push or deployment.

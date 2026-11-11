# Final parity audit — accepted

Ticket52 accepted2026-11-11. Production runtime21d2bd9 adds the audited locale-control
correction to229be58. Final test/harness corrections are recorded below. No release
or live deployment. All49 retained tickets are accepted; four provider tickets
were explicitly removed by the user.

## Inventory reconciliation

At9d7f99f, [the ledger](parity-ledger.md) contains all107 ordered unique story IDs
and172 existing evidence links. Stale placeholders for23/24,35–42,47 and50 now
link their accepted source/fixture/public-outcome evidence. Provider exceptions
remain explicit; final51–53 outcomes are recorded below. This checks inventory integrity, not
every linked financial expectation afresh.

`node actualBudget/tools/check-public-api-map.mjs --static` passes:67 pinned SDK
functions and18 query resources explicitly mapped, with pinned source hash.
The existing [caller migration guide](PUBLIC-API.md) includes typed queries,
immutable batch commands, receipt lookup/cancellation and staged manual imports.
Its standard-library Python caller has running public-contract evidence in
[ticket47](public-query-validation.md) and the final production batch below.

## Anonymous production API witness

Read-only requests against the disposable release container used for opening
qualification, image manifest
`sha256:e400473e4eaa0b079edc496196e9996549bb5dde2e44113be3faf828ec51a22e`.
Requests supplied no credentials, cookies or authorization headers. Every
response lacked Set-Cookie and WWW-Authenticate.

| Request | Status |
| --- | ---: |
| GET /api/v1/info |200|
| GET /api/v1/openapi.json |200|
| GET /api/v1/budgets |200|
| POST /account/login |405|
| POST /account/bootstrap |405|
| POST /api/v1/auth/login |404|
| GET /api/v1/session |404|

The served119-path contract has no security schemes, global security requirement
or operation security requirements. Static runtime-source searches found no
password/OAuth/JWT/login implementation; `session` matches concern reconciliation,
and the frontend broad-pattern match was the word “reassigning” in a comment.
Pinned reference code and Android's retained old-server adapter are separate
from the replacement web/server runtime. This witness verifies the sampled
production API surface; it does not claim a fresh complete browser/Android suite.

The final composition review below completes the source/fixture/outcome,
caller/deviation and consolidated acceptance obligations. Resource51 and the
local cutover53 rehearsal are accepted; production cutover remains user-operated.

## Retained experimental capabilities

2026-11-11 read-only reconciliation against the pinned
`packages/loot-core/src/types/prefs.ts`: all14 declared feature flags have an
explicit outcome below. All3088 retained reference-file SHA256 values still
match their manifest. The107 ordered story IDs,172 evidence links and complete
67-method/18-resource SDK map pass again. These checks establish inventory and
source integrity, not a new financial-suite run.

| Pinned flag | Observable replacement and accepted evidence |
| --- | --- |
| newSidebarUI | Account navigation, command search, shortcuts and remembered selection: [45](navigation-validation.md). |
| goalTemplatesEnabled | Note grammar, priorities, targets and progress: [23](budget-automation-validation.md). |
| goalTemplatesUIEnabled | Visual definitions, selective/all application and recovery: [24](budget-automation-validation.md). |
| actionTemplating | Calculated rule actions, splits and real import/schedule/companion workflows: [42](rule-expression-validation.md). |
| formulaMode | Saved formulas, formatting, exact errors and preference changes: [41](formula-validation.md). |
| currency | Currency display/decimal preferences, including independent JPY rounding cases: [43](../actualBudget/web/docs/preferences-validation.md). No exchange-rate accounting claim. |
| balanceForecastReport | Saved schedule/tracking forecasts, filters and unchanged-input caching: [35](forecast-validation.md). |
| customThemes | Built-in/custom slots, retained original CSS and unusable-theme recovery: [44](../actualBudget/web/docs/custom-theme-contract.md). |
| budgetAnalysisReport | Authoritative aggregates, exact details and bounded reads: [36](budget-analysis-validation.md). |
| enableBanking | Removed by the explicit manual-only decision: [29](bank-retirement-validation.md). |
| sankeyReport | Spent/Budgeted source projections and accessible exact details: [37](sankey-validation.md). |
| akahuBankSync | Removed by the explicit manual-only decision: [29](bank-retirement-validation.md). |
| mobileCalculator | Calculator-assisted exact amount entry: [03](register-validation.md). |
| monteCarloReport | Saved plans, strategies, withdrawal rules, cash flows and individual runs: [39/40](monte-carlo-validation.md). |

Monthly cleanup and crossover are also retained independently of that flag union:
[24](budget-automation-validation.md), [38](crossover-validation.md).

## Deliberate architecture and scope differences

The approved [architecture](adr/0005-server-authoritative-web-architecture.md) removes
application authentication/encryption, full offline budgeting and desktop
packaging; limited offline expense capture remains. The later
[manual-only decision](manual-only-scope.md) removes external account providers,
not manual file imports or the application's public API.

The [caller migration guide](PUBLIC-API.md) documents route/field translations,
native budget identities, stable pagination, typed bounded queries and immutable
mutation recovery. Server-authoritative queries replace a downloaded Node engine;
remote arbitrary JavaScript/SQL is not promised. The sole removed SDK operation
is bank refresh. Domain invariants reject contradictory split/transfer writes
atomically, with source counterexamples retained in [rules](rules-validation.md)
and [graph integration](1314151646-validation.md). These are explicit validation
boundaries, not omitted ordinary editing workflows.

Historical WIP statements within feature validation notes describe intermediate
checkpoints; their final acceptance/review sections and later integration notes
own the current result. The complete-story ledger remains the index. Final
consolidated execution passes with the explicit corrected-run accounting below.
[51](resource-qualification.md) and [53](coolify-replacement.md) are accepted.

## Final composition review

Root reviewed all107 ledger rows and their assigned evidence families, retaining
the accepted source/fixture/public-outcome reports. All14 pinned experimental
flags have the explicit dispositions above. The independent3088-file hash check,
107 ordered IDs and172 evidence links pass; no missing source/evidence link was
found. Source oracles retain their pinned expectations; no native output was used
to regenerate a financial expectation during this audit.

The final CI command batch passes46/47 groups directly. Its Rust command exposed
stale migrated-schema22 expectations, now27. A complete no-fail-fast run exercised
458 tests; three older upgrade tests additionally assumed no preference
invalidation. Provider retirement legitimately emits one preferences event.
Corrected tests explicitly require that event, absent provider flags, retained
cursor epoch and unchanged complete financial rows/totals; all three affected
targets then pass. Clippy all targets and rustfmt pass. This is combined passing
coverage after corrections, not a claim that the first full run was green.

All68 Android core tests across six suites pass with zero failures/errors/skips,
including52 real native HTTP tests and the app-first upgrade using the genuine
schema17 executable. Its SHA256 matches the preserved fixture provenance. Android
lint passes. CI now builds that pinned prior source explicitly. Fixture-authoring
helpers are excluded unless their explicit output environment is supplied; actual
acceptance cases remain mandatory. The earlier run's missing-prior-binary skip
is superseded, not counted as passed.

The fresh production image passes plain-HTTP/no-login, assets, exact opening
balance, idempotent replay and container recreation with the same volume. Its
full browser/API batch exercised110 command groups:106 passed initially; all four
failed groups pass in targeted follow-ups. Locale interpolation introduced
insignificant label whitespace; affected regex locators now accept whitespace
without changing their field/value expectations. The three affected checks pass
after correction. The keep-going harness also now accepts the explicit flags
already present in the maintained test command list. No application financial
behavior changed in these harness corrections.

The fourth browser failure was an obsolete custom-report expectation: it assumed
a retired category filter automatically followed the merge target. The later
source-backed47 behavior intentionally keeps that filter's identity literal.
An additional independent Actual26.10 execution of the exact77-case report
fixture confirms old-category payments−6000 before deletion, retired filter0,
replacement filter−6000. The unchanged runtime passes both independently derived
post-merge expectations. The new source fixture/check is in CI; none of the77
earlier financial cases was changed. This correction repairs the test's stale
assumption, not the implementation or the pinned source.

## Acceptance and review

The [hashed result archive](final-acceptance-results/manifest.json) retains the47
CI command results, full/corrected Rust and browser runs, Android XML summary,
independent source check, source inventory and cutover/signing logs. Initial
failures/skips remain visible and are superseded by specific passing evidence.
Combined coverage:458 Rust tests,110 browser/API command groups,68 Android tests,
the47 CI groups and the additional exact retirement source check. No repeated
whole suite was needed after test-only corrections. Web typecheck/build/format,
Rust lint/format and Actions syntax checks pass.

Root completed Standards and Spec review of the final changes; no unresolved
actionable finding remains. Earlier feature-level independent reviews remain in
their acceptance notes. The user-approved consolidated root review replaces new
subagents for this closing phase. Known limits remain explicit: English fallback
for new untranslated prose, the web bundle advisory, measured small-budget Android
regressions, unbounded lifetime undo-lineage storage growth, and no physical-phone
energy claim. None is disguised as a measured improvement or missing capability.

The release eligibility gate is now enabled for actualBudget. Versions remain
unchanged; this does not publish an image/APK or deploy anything. The user's live
Coolify configuration, final backups and device queues must still be checked
using the [operator runbook](coolify-replacement.md) before a separately requested
publication/cutover.

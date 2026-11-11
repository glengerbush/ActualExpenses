# Final parity audit — in progress

Ticket52 remains open. Production application revision229be58; no product
changes, release or live deployment in this audit. Preserve accepted feature
evidence and distinguish it from final whole-system qualification.

## Inventory reconciliation

At9d7f99f, [the ledger](parity-ledger.md) contains all107 ordered unique story IDs
and172 existing evidence links. Stale placeholders for23/24,35–42,47 and50 now
link their accepted source/fixture/public-outcome evidence. Provider exceptions
remain explicit;51–53 gates remain open. This checks inventory integrity, not
every linked financial expectation afresh.

`node actualBudget/tools/check-public-api-map.mjs --static` passes:67 pinned SDK
functions and18 query resources explicitly mapped, with pinned source hash.
The existing [caller migration guide](PUBLIC-API.md) includes typed queries,
immutable batch commands, receipt lookup/cancellation and staged manual imports.
Its standard-library Python caller already has running public-contract evidence
in [ticket47](public-query-validation.md). Final composition review remains open.

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

Remaining: reconcile every story and retained experiment against its complete
source/fixture/outcome obligations, review caller migration/deviations, then run
the final consolidated acceptance checks. Resource51 and cutover53 remain open.

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
consolidated execution and51/53 acceptance are still outstanding.

# Formula cards accepted; calculated rule actions pending

Ticket41 accepted locally2026-11-10;42 remains open. Reference commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`, Actual API26.10.0,
installed/pinned HyperFormula3.5.0.

## Ticket41 acceptance — 2026-11-10

The final display witness resolves the last review question: the unchanged
`FormulaResult` callback and `useFormat` intentionally use comma-dot/before
defaults independently of formula-function locale/currency fallbacks. Native
defaults already match. `generate-formula-display-source.mjs` executes that exact
callback, source amount conversion and source formatter using real engine
preferences; React/Redux lifecycle wiring alone is adapted. Its56 observations
cover absent preferences in English/French, EUR/CHF/JPY, hidden fractions,
explicit spacing/position/apostrophe settings, negative zero and range errors.
Original ledger rows and all earlier fixtures remain unchanged.

The witness exposed two actual display bugs. Negative zero lost its sign;
out-of-range results fell back to unformatted text. Cards now preserve source
negative-zero rendering and the source2^51 financial display bound. Oversized
calculations show an actionable display error without changing the computed
result, recorded balances or app availability. `/tmp/formula-display-red.log`
records `0.00` versus source `-0.00`; `/tmp/formula-display-green3.log` passes all
56 running-browser displays/errors and unchanged financial rows/checkpoints at
390px. Source errors remain verbatim in the fixture, while the UI gives a concise
recovery instruction instead of throwing a component exception.

`/tmp/formula-display-source-check.log` independently reproduces56 source cases;
`/tmp/formula-display-core-source-check.log` reproduces the original40 source
outcomes after the shared harness extension. `/tmp/formula-display-regression.log`
passes the existing complete formula browser scenario: theme bindings,
cache/privacy/quota, drafts, query editing, keyboard and200% text. Typecheck and
production build pass; Svelte analysis has no issues (unchanged guarded
external-request effect advisories). Impeccable edge-case review and visual
inspection of negative-zero/range-error cards pass. French dashboard-label
timeouts were harness localization mistakes, corrected before the full pass.

Acceptance audit against ticket41/spec72,73,80:

| Requirement | Evidence |
| --- | --- |
| Spreadsheet/query/count/extraction, budget queries and balances | Pinned engine/custom functions,77 core/budget/edge/clock outcomes and their public HTTP checks documented below. |
| Formula/named-query import, bounded evaluation, errors and isolation | Genuine saved-source archive; native/served-schema checks; heap/stack/deadline/SQL/query bounds; error/limit recovery; no host execution surface. |
| Financial/statistical fidelity | Exact financial comparisons, stated1e-12 statistical tolerance,78 added formatting/preference observations and56 source display/error cases. |
| Public API, browser and Android | Recorded native contract/recovery and actual companion service tests; complete formula workflow and locale/disabled/display browser cases pass. |
| Complete vertical delivery | Persisted widget/query/style/feature settings, versioned served contract, conversion/original preservation, backup/duplicate/restore, stale-snapshot rejection and reporting change feed verified. |
| UI/HTTP/offline boundaries | Impeccable and Svelte review;390px/200% text, keyboard, loading/error/privacy/cache recovery; plain HTTP, explicit opt-in and disabled mode retain saved work without evaluation. |

Root consolidated standards review from `d27e6d4`: no unresolved finding.
Root consolidated spec review, combining this evidence with reviewed prior41
checkpoints: all six acceptance criteria met; no41 feature work deferred.
Tickets39/40/42 and final qualification50–53 remain open. No push, release or
production deployment; full-suite/container qualification stays at final
integration per the user's credit-saving preference.

## Source seam

`tools/generate-formula-source.mjs` runs the unchanged production
`useFormulaExecution.ts`, including all three discovery/prefetch/evaluate passes,
unchanged custom functions and real Actual engine messages. The React adapter
only supplies state/ref/effect lifecycle and selected `en-US` locale; it does not
calculate expressions, substitute QUERY text, or supply financial results.
Trusted source modules execute in the same realm as HyperFormula because its
cell-array validation uses `instanceof Array`. A separate VM realm initially
caused a harness-only array error; this was corrected before recording results.

The genuine accepted `reports-source-26.10.zip` is reused read-only and checked
against its original digest. Clock is fixed to2026-11-09 UTC. Original account,
category and transaction identities remain unchanged. Source file hashes and
exact expressions/configuration/results are retained in `formula-source.json`.
Forty observations cover spreadsheet functions, sum/count/filtered queries,
case-sensitive/missing queries, balances by real name/ID, category/timeframe
extraction, formatting, named expressions/color, Unicode comparisons, date and
statistical expressions, explicit errors and all five compositional budget
query dimensions. Statistical cases declare a1e-12 tolerance; their original
numeric observations stay intact. Source errors (including extra FIXED argument
and bare FALSE lookup) are retained rather than replaced with guessed results.

Generation and independent `--check` passed:
`/tmp/formula-source-corpus.log`, `/tmp/formula-source-corpus-check.log`.
No replacement implementation is called by the producer.

## Current public tracer

Read-only POST `/api/v1/budgets/{budget}/reports/formula` is the public seam.
First missing-route404: `/tmp/formula-http-first-red.log`.
Initial spreadsheet result6 and unchanged ledger/checkpoint passed:
`/tmp/formula-http-first-green.log`. Query-sum/count then failed before custom
function integration; three initial cases now pass against source1204/19:
`/tmp/formula-query-http-green.log`. Remaining runtime, UI, import, rule-action,
contracts and representative companion coverage are unfinished.

## Evaluator investigation

A disposable prototype loaded the full trusted HyperFormula bundle in
Rust-owned QuickJS (no Node process/server), with64MiB heap, stack and deadline
limits. Approximate debug measurements:8.9MiB allocated after a simple formula;
trusted bytecode initialization60ms after one process-local compilation.
These are local feasibility observations, not a latency/power SLO or release
benchmark. Production work uses ICU collation, not a lexical approximation;
comparison text/work are bounded. Runtime dependencies and source assets remain
under development and are excluded from this source-only review checkpoint.

## Root source review

2026-11-10:abe6be4 reviewed and composed as94e09c8. The adapter invokes the
unchanged full production hook; lifecycle stubs merely capture completion/state.
Actual query, budget, account and preference messages use the genuine engine;
no native replacement or financial callback supplies expected values. Trusted
module hashes, original report archive digest and source errors are preserved.
Independent reproduction `/tmp/formula-root-source-check.log` PASS40. No source
provenance finding remains. This corpus is a reviewed foundation, not complete41:
tracking-mode budget queries and nonzero goal/carry observations still need a
small append-only supplement, alongside runtime/UI/contracts/recovery evidence.

## Nonzero budget supplement

The small budget supplement reuses both genuine `multiline-long-goal` preapply
archives, executes the real `budget/overwrite-goal-template` message, proves no
transaction changes, and retains genuine resulting Actual ZIPs. Ten unchanged
production-hook observations establish envelope budget42.97/spent−15/goal1000/
starting carry70/ending97.97 and tracking42.97/−15/1000/40/67.97.
Generation and independent check passed in `/tmp/formula-budget-source.log` and
`/tmp/formula-budget-source-check.log`; no previous forty observations changed.

Root reviewedb12eaeb and independently reproduced all10 in
`/tmp/formula-budget-root-source-check.log`. Preparation uses genuine public
engine commands over immutable prior archives; exported result archives are
never overwritten. Both method/nonzero-goal/carry source coverage gaps above
are closed. Runtime financial equality, UI and whole41 acceptance remain open.

## Bounded evaluator checkpoint (partial41)

Full trusted HyperFormula3.5.0, enUS dictionary and unchanged Actual custom
functions execute inside Rust-owned QuickJS-ng/rquickjs0.13.0. The standalone
Node producer is test tooling only; production serves no Node process or browser
calculation engine. Bundled asset generation/check is reproducible and records
source/artifact hashes; upstream GPLv3/MIT/Unicode licenses are retained.

Every evaluation has fresh JS state and no filesystem, network, process or
module-loader bindings. Only bounded ICU4X collation is exposed as a host
function. Trusted bytecode is compiled/cached once per process; only successful
initialization is cached, so contention cannot poison subsequent requests.
Limits:64MiB JS heap,512KiB stack,2s monotonic execution deadline;64KiB input,
8192-byte expression,32 named queries/20 conditions,512 named text/numeric values;
32 custom-function requests per family,20000 cumulative prefetched values,
4MiB collation text work,32million shared SQLite VM steps and500000 cumulative
budget/category calendar comparisons. Budget intervals permit240 months and
1000 selected IDs. Refusal never truncates values or returns a fake zero.

Both discovery passes and final evaluation match the production ordering.
Initial speculative budget requests are discarded after query extraction, as
in the source. Query/count/account sums use exact live-leaf predicates and
minor-unit totals before the source's display-unit conversion; categories and
budget data reuse authoritative existing financial tables/calculation paths.
The budget helper adds scoped goal inclusion and a shared work counter without
changing existing unscoped budget/report outputs. Register matching also shares
one regex budget across all formula SQL/extraction passes.

Current public source40 plus budget10 comparisons/guards pass:
`/tmp/formula-core-http-green.log`, `/tmp/formula-core-budget-http-green.log`.
Asset reproduction passes `/tmp/formula-runtime-assets-check.log`; bin Clippy
`-D warnings` passes `/tmp/formula-core-clippy.log`. The large supported matrix
probe fails safely under the configured runtime bounds, retains the upstream
secondary null-error diagnostic with actionable range-size guidance, and then
successfully evaluates a fresh SUM and the original source NAME error. See
`/tmp/formula-limits-http-first.log` (unhelpful diagnostic red) and
`/tmp/formula-limits-http-green.log` (recovery green). No custom unsafe allocator
or guessed OOM classification was added; the stock enforced allocator remains.

Checkpoint exclusions: preferences/volatile evaluation-time integration, card
and named-query editing/rendering/import flows, served contracts, representative
companion checks and all42 rule/template integration are still unfinished.
No whole-ticket or release qualification is claimed here.

## Root continuation: query edges, preferences and volatile clock

2026-11-10: preserved interrupted worker checkpoint b414539 and merged at0e5d174. Root independently reproduced its21 complete source-hook observations, then strengthened the producer to reload each actual exported archive before observation. Immutable ZIPs and expected outputs unchanged. `/tmp/formula-edges-root-reload-source.log` PASS covers named condition metadata, malformed category patterns, imported payee text, duplicate account names, EUR/JPY preference formatting and UTC TODAY/NOW.

Root review found shared report filtering skipped named conditions in formula transaction queries (observed1204 versus source−58). The formula-local adapter now strips only `customName` before shared predicate evaluation; category extraction deliberately retains the source's different named-condition semantics. Invalid category regex syntax produces the source nonmatch; compilation/work limits still refuse explicitly. Account-name lookup retains ID priority and source ordering without preferring open accounts over earlier closed accounts. Imported-payee filtering uses its existing stored text column. Public source comparisons confirm all these changes.

Formula contexts now receive current authoritative shared preferences and configure the unchanged pinned custom-functions plugin. Currency symbols/decimal places come from the pinned currency catalogue; five supported grouping/separator formats map to the source conventions. Currency-disabled formulas retain the source USD fallback. EUR hidden-fraction number formatting, explicit decimal overrides and EUR/JPY currency output match exactly. The tiny trusted bundle entry exposes the pinned currency helper alongside existing functions; generator records its hash and retains existing licenses. No new host bindings or executable user code.

Optional `evaluated_at` is an RFC3339 instant with the caller's UTC offset; omitted uses the server's current UTC time. The response returns that instant. All three evaluation passes share one frozen wall clock for NOW/TODAY, independent of server timezone. `as_of` remains the separately explicit report/query calendar date. Six additional complete unchanged-source-hook observations use the same instant in UTC, Pacific/Kiritimati(+14, following day) and America/New_York(−05). `formula-clock-source.json` records the genuine archive/source hashes and outputs; `/tmp/formula-clock-root-source.log` and `/tmp/formula-clock-root-http.log` PASS exact results. Browser integration must supply its current RFC3339 local offset and label this snapshot.

Focused verification: `/tmp/formula-edges-root-green.log`21/21; `/tmp/formula-preferences-root-core.log`40/40; `/tmp/formula-preferences-root-budget.log`10/10; clock6/6 =77 source comparisons, with exact financial/string/date equality and only the original predeclared statistical tolerances. Public checks preserve financial records/checkpoints, reject stale snapshots, distinguish oversized regex refusal from malformed-pattern nonmatch, and verify subsequent evaluations recover. Rust build/Clippy warnings-denied and reproducible assets pass (`/tmp/formula-preferences-root-{build,clippy,assets-check}.log`). Root consolidated standards/spec review found no remaining issue in this checkpoint.

Checkpoint55c03ac also passes the large-matrix refusal and fresh-context recovery check (`/tmp/formula-preferences-root-limits.log`). No poisoned runtime after a bounded failure.

Still partial41 at55c03ac: card/query UI, named-query editing, import/native/served-contract/Android integration and complete feature acceptance were open. The next checkpoint below advances those areas. No broad suite, push, release or deployment.

## Formula interface and public contract checkpoint — 2026-11-10

- Opt-in Formula dashboard tab/command, creation/editing, server-only previews and cards. Saved query names, reference filters and date modes reuse existing report controls; named transaction conditions remain active. Configuration import/export uses a visible JSON field and works over plain HTTP without clipboard permissions. Formula/query/font/title/color drafts reuse the established reporting draft/receipt machinery. No evaluation on every editor keystroke or idle timer.
- Cards show financial-number formatting or literal string/boolean output, conditional color, static/dynamic type and optional visible title; the accessible article/title remains named. Errors are text, never executable markup. Privacy masks outputs and removes conditional color. Cache binds budget epoch, document revision, preference revision, language and selected theme; an observed epoch replacement refuses cached output. Empty font-size drafts now survive reload as invalid rather than silently reverting. Snapshot-aware color evaluation uses the same instant/query context as its primary result.
- Genuine unchanged-source export/reload fixture `formula-saved-26.10.zip` records complete formula/query/font/title/color settings, enabled feature and actual value/color outcomes. `/tmp/formula-saved-root-source.log` PASS. No direct financial database mutation or manufactured native archive.
- Served OpenAPI now includes formula request/query/output/projection and resource bounds; capabilities advertise formulas. `/tmp/formula-native-root.log` PASS: genuine saved widget/query/style/opt-in import, exact source value/color, successful/error/stale-snapshot schemas, no financial writes, native26 backup/duplicate/restore and old-snapshot rejection.
- `/tmp/formula-native-android.log` PASS one actual companion-module HTTP consumer of the saved source formula; XML confirms1 test,0 skipped/failures/errors and unchanged companion view. Initial harness incorrectly read the checkpoint envelope as its data; corrected that harness, then passed.
- `/tmp/formula-ui-browser.log` PASS: feature opt-in/command, query creation and source-configuration import/export, exact source result/color, durable formula/error and blank-font draft reload, title/font, disconnected cache, changed-epoch cache rejection, privacy,390px no-page-overflow and keyboard preview. Initial font/title assertion raced persistence; it now awaits editor closure. Typecheck/build and all touched Svelte analyzers pass; external-request effects retain the documented generation guards. Impeccable detector empty; root inspected stable desktop and mobile-editor captures in `actualBudget/web/test-results/formula/`. No physical assistive-technology or whole-product claim.

The theme/change-feed checkpoint below resolves more of this list. Remaining41 acceptance work: source fallback formatting when optional imported preferences are absent (confirmed the genuine base fixture has none), corresponding preference edits/native recovery, locale and disabled-feature browser coverage, and final consolidated review. These remain41 work, not deferred qualification.42 and39/40 remain separate unfinished features;50–53 remain open.

## Complete theme bindings and reporting change feed — 2026-11-10

- Formula bindings now follow all228 source theme definitions and the original hook's omission of empty computed values. Reproducible source CSS/name artifacts retain exact custom-property strings, including whitespace and color spelling. Native UI colors remain their own tokens. The browser compares complete bindings against the unchanged source hook and source CSS in isolated frames for light, dark, midnight and a custom override. This caught and corrected sidebar-only aliases being flattened into global formula colors.
- Compiler version2 retains every applicable source color role, including roles without a native UI equivalent. Version1 device records upgrade only when formulas need their bindings; original source, IDs and metadata remain intact. A conditional IndexedDB write prevents an upgrade overwriting a concurrent user save. SHA256 source fingerprints keep formula cache signatures small even for embedded-font themes. Built-in/system/custom appearance changes invalidate formula caches.
- Saved empty formulas use the source card default while editor previews still expose an empty expression. Source query controls now offer imported payee. Long formula help and dashboard actions reflow at390px with200% text; no page overflow. Quota failure preserves the calculated result. Original-source chosen colors are retained rather than silently recolored.
- Browser inspection uncovered an existing integration bug: report commands published `reporting_documents` changes but the feed reader rejected that entity with HTTP500, stranding browser/Android checkpoints. A minimal public native fixture reproduces it in `/tmp/formula-changes-red.log`. The feed now returns the same scoped document/tombstone representation as reporting; its served schema covers the entity. Browser reference refresh advances past these non-reference changes without downloading references again.
- `/tmp/formula-changes-green.log` passes served create/edit/delete feed schemas and cursor advancement, unchanged financial rows, original saved-source formula outcomes and native26 backup/duplicate/restore. `/tmp/formula-changes-android.log` passes the actual Android service refresh after a reporting edit: online, no error, identical expenses. XML confirms1 test,0 skipped/failures/errors. The first native assertion incorrectly compared response checkpoints after intentional metadata writes; corrected to compare financial rows.
- `/tmp/formula-theme-browser.log` passes source bindings, legacy cache upgrade with stable source/ID, quota/privacy, drafts and390px/200% text. No browser exceptions or HTTP5xx. `/tmp/formula-theme-api.log` passes4 theme API cases; existing theme install/recovery/storage-failure browser cases pass in `/tmp/formula-theme-legacy-browser.log`. Reproducible artifacts, typecheck, production build and warnings-denied Clippy pass. All touched Svelte analyzers have no issues; the formula widget's guarded external-request effect retains its documented advisory. Impeccable detector is empty; root inspected desktop/dark and large-text viewport captures under `actualBudget/web/test-results/formula/`.

Root consolidated standards/spec review from4e1db87: no unresolved finding in this checkpoint;41 remains partial for the formatting and final coverage listed above. Existing77 financial/string/date reference outcomes remain valid and were not regenerated. No full-suite/container qualification, push, release or live deployment in this checkpoint.

## Missing formatting preferences and currency migration — 2026-11-10

The importer now preserves absent/null `numberFormat` and
`currencySymbolPosition` internally. Public preferences retain their complete
default-valued contract; unrelated PATCHes preserve absence and explicit writes
override it, including writes equal to displayed defaults. Native backups,
duplicates and restores retain that distinction. Existing explicit native values
are not guessed or rewritten. Locale pattern selection uses ICU4X decimal parts
only when needed; unchanged trusted Actual functions format formula values.

Two append-only source producers record78 new observations:66 across19 locales
and EUR/CHF/JPY defaults, plus12 preference-edit/null observations. Both execute
the unchanged production hook and real Actual preference/export/reload flows,
check original financial rows and retain source/archive hashes. Independent
`--check` passes in `/tmp/formula-formatting-source-check.log` and
`/tmp/formula-formatting-edits-check.log`. The old server failed the first Catalan
case (`1,234.57` versus source `1.234,57`), retained in
`/tmp/formula-formatting-red.log`.

Verification:

- `/tmp/formula-formatting-final.log`: all66 new public formula outcomes pass,
  with unchanged financial rows and checkpoints. Earlier40 core,21 edge and10
  budget cases passed this change; unchanged clock witnesses are reused.
- `/tmp/formula-formatting-recovery-final.log`: source null/absent import,
  unrelated and explicit preference edits, served preference schema, native
  backup/duplicate/restore and unchanged financial rows pass. Null import first
  failed as unsupported, then passed after the source-compatible conversion fix.
- `/tmp/formula-formatting-preferences4.log`:17 genuine schema19/native19 upgrade
  cases and6 preference cases pass. Frozen financial values and immutable
  receipts remain exact. Expectations account explicitly for schema24 note
  identities, schema26 retired flags and their change-feed entries; original
  fixtures were not regenerated. The checks exposed and fixed retired flags
  confusing pre20 currency provenance. Only comparison copies normalize removed
  flags/defaults; ambiguous histories still require explicit currency review.
- `/tmp/formula-formatting-browser-green.log`: browser language changes use the
  exact source formatting result; unrelated settings saves preserve fallback;
  disabling formulas stops evaluation and retains saved work/financial rows.
  No browser exceptions or HTTP5xx. Initial disabled assertion expected a masked
  output, corrected to assert the actual absent output plus disabled controls.
- Warnings-denied Clippy, Rust formatting and changed-file Prettier checks pass.
  Source/API/native/browser scripts are included in existing package commands.

Root standards review from `a3e8a02`: no unresolved finding in this checkpoint.
Spec review: formula-function fallback and recovery are covered;41 remains open.
Final source inspection found a separate numeric-card display question:
`FormulaResult.tsx` formats numeric values through source `useFormat`, whereas
native `FormulaWidget.svelte` uses public default-filled `formatAmount` settings.
Verify absent-preference locale/currency rendering with a genuine source display
witness and fix any mismatch before accepting41. Do not defer it to50–53.
No whole-suite/container run, push, release or deployment in this checkpoint.

## Root runtime review and composition

Reviewed065c94d and79e6ba9 against pinned production prefetch/evaluation code;
composed with reviewed crossover backend as393bfb3. Trusted-only bytecode,
fresh bounded runtime, ICU comparison work, SQL limits, financial prefetch,
snapshot ownership and scoped goal/calendar helper changes reviewed. No new
standards finding remains at this checkpoint.

Independent composed build and asset reproduction pass:
`/tmp/formula-crossover-compose-build.log`, `/tmp/formula-root-assets.log`.
Public forty source outcomes and ten nonzero budget outcomes pass, with unchanged
ledger/checkpoints, in `/tmp/formula-crossover-compose-core.log` and
`/tmp/formula-crossover-compose-budget.log`. Matrix refusal followed by fresh SUM
and original NAME-error recovery passes `/tmp/formula-crossover-compose-limits.log`.
Previously reviewed source producers were reused, not regenerated.

Open spec finding: category extraction currently returns a request error for
invalid regex; pinned source treats that condition as nonmatching. Assigned to
the feature owner with a bounded whole-hook witness. The owner's ongoing query
metadata/imported-payee supplement and preference/volatile-time/UI/contracts
work are not covered by this checkpoint. Final image must retain bundled runtime
notices. No whole41 completion claim.

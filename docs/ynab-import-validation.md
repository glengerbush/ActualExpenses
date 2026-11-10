# YNAB migration — development evidence

Ticket27 implementation is complete on `codex/ynab27` (schema22), pending root
consolidated final review and combined qualification. Initial base `8a49adb`;
accepted main through `a5c784c` merged without runtime conflict.
No live budgets, credentials, deployment, release or version changes. Root owns
consolidated review and qualification; this branch owns the complete migration
slice. Initial design reuses existing staged imports, original-byte retention,
provenance and atomic activation; no schema migration is currently needed.

## Public seams and first source tracer

Delivered public seams: binary POST `/imports/ynab4?filename=...` and
`/imports/nynab?filename=...`, existing staged report/list/read and activation,
and a format-correct generic original-file download. Staging creates no active
budget; activation must be atomic and repeat-safe across independent keys,
restart and lost acknowledgements. The interface must retain the original file
and immutable upload/activation identities before sending. Tests use running
HTTP, actual browser workflows and the companion's real API adapter, not private
conversion helpers.

`tools/generate-ynab-source.mjs` executes `@actual-app/api@26.10.0` public
`importBudget` for the unchanged pinned desktop demo YNAB4 ZIP and nYNAB JSON.
Source commit `2bebdbaae0b701120a8cad9d56aafd64fa04c577`; importer/type file hashes
are recorded. Inputs, their exact digests and genuine post-import Actual ZIPs
are retained under `tests/fixtures/ynab/`; observations are `ynab-source.json`.
The source public accounts, groups, categories, payees, complete account ledgers,
account balances, selected historical budget months, schedules and registered
UI `tags-get` records supply expectations. No replacement parser, converter or
budget calculation supplies oracle values.

Generated UUIDs are labeled by public reference names and ordered ledger/child
positions; integer/fractional `sort_order` values alone are omitted, preserving
relative array order, financial multiplicity and reciprocal graph references.
JSON normalization preserves the actual wire representation (including numeric
negative zero becoming zero). Historical months outside Actual's available
budget horizon retain their actual `No budget exists` result, rather than a
fabricated zero allocation. Independent `--check` reimports both originals and
compares observations while verifying the committed original/export digests.

First source facts: the YNAB4 baseline imports accounts, categories, payees,
transactions and monthly allocations/carry settings; its importer does not
migrate YNAB4 scheduled transactions or edit/undo history. The nYNAB demo imports
15 schedules and6 flag-derived tags in addition to financial history. No source
edit log is represented as new native undo history, and YNAB entity versions are
not relabeled as HLCs. Supported history means the historical transactions,
month allocations and rollover state exposed by the baseline import workflow.

## Required source-model explanations

YNAB credit-card payment reserves and Ready to Assign differ from Actual's
ordinary account transfers and To Budget/hold model. Imports must preserve the
baseline financial records while explaining debt-category rollover, existing
allocation review and optional holds; they must not claim cross-product totals
or credit-card reserve semantics are identical. Duplicate category/group names,
hidden categories, split payee presentation, source goals and any unsupported
source configuration require explicit conversion/reporting. Original exports
remain available unchanged for review and cleanup.

## Initial acceptance checklist (covered below)

- Source-backed public YNAB4 tracer, then complete nYNAB conversion, including
  wrappers, references, milliunit rounding, splits/transfers, flags/locations,
  schedules and historical allocations. Additional independent source fixtures
  answer concrete uncovered behaviors, not copied replacement formulas.
- Malformed/bounded ZIP/JSON parsing, latest-device selection, repeat/restart,
  atomic refusal, original bytes, native archive and companion projection.
- Served contracts, maintained CI/qualification consumers, durable source-picker
  UI, plain HTTP/browser recovery and responsive/privacy/keyboard renders.
- Consolidated review and one integrated qualification after the vertical slice
  is complete. Do not mark ticket27 accepted from the initial oracle alone.

## First YNAB4 public financial slice

The running-HTTP tracer consumes the independent YNAB4 observation and compares
all imported financial rows, account balances, multiplicity, parent/peer edges,
historical category allocations/carry and every selected budget summary field.
Staging creates no active budget, original bytes remain exact, upload receipts
repeat exactly, and repeated activation cannot reinstall history or advance the
budget checkpoint. Initial unavailable-route405 is retained in
`/tmp/ynab-http-first-red.log`; the built implementation passes
`/tmp/ynab4-checkpoint-final-http.log`. An intermediate consumer assumed the
budget view exposed categories directly; adapting it to the existing
`data.months[month]` contract corrected the harness, not financial expectations.
A stale-binary run after a failed compile is not used as checkpoint evidence.

The source adapter emits canonical rows into the existing validation/install
pipeline. A report-owned `source_format` discriminant avoids trusting arbitrary
Actual metadata fields. Source hashes, original IDs/mapping and entity versions
stay in retained provenance; YNAB versions are never minted into HLCs. ZIP paths,
duplicate names, encryption, device knowledge and expanded byte/row limits are
validated. Source-specific reference lookups are indexed. Shared Actual import
regressions12 pass `/tmp/ynab4-existing-imports.log` using a fresh local test
build. This is a financial checkpoint, not ticket acceptance: generated learned
defaults still need a source-backed join, and nYNAB, full failure/restart/archive
coverage, contracts and interface remain in progress.

## YNAB4 learned-default witness

`generate-ynab-learning.mjs` imports the original unchanged YNAB4 demo and runs
the real registered `rules-run` handler for every ordinary source payee. Both
generation and independent `--check` pass (`/tmp/ynab-learning-source.log`,
`/tmp/ynab-learning-source-check.log`). Three distinct category setter effects
are observed. Concurrent source account imports also emit duplicate identical
setters; their original records remain in the genuine Actual archive and the
sample list. Only proved-idempotent single-payee category setters are collapsed
for effective-rule comparison. Every resulting category/amount/note is checked
against actual rule execution; financial multiplicity is never normalized away.

The public consumer initially fails with missing Employer1's income default
(`/tmp/ynab-learning-http-red.log`). Fresh YNAB activation now invokes the existing
explicit-category learner once over newly imported historical evidence; ordinary
Actual/native archive imports never invoke it. Larger candidate sets scan once
instead of constructing an oversized SQL IN clause. Existing per-entry opt-in
learning and native manual override behavior remain unchanged (focused existing
rules regression1 PASS `/tmp/ynab-learning-existing-rules.log`). Source ordinary
transfer-named payees are also retained separately from dedicated account-transfer
payees, matching the real baseline's API behavior rather than the apparent intent
of its importer call.

Financial graphs, historical budget cells/summaries, complete ordinary payees,
read-only source learned-rule effects, receipts and original bytes now pass
`/tmp/ynab-learning-http-final.log`. Bin Clippy `-D warnings` passes
`/tmp/ynab4-checkpoint-clippy-green.log`; its initial empty-string style finding
was corrected without financial changes. The earlier12 Actual import tests are
reused for the unchanged canonical validator split. Main's accepted25/26+33 and
narrow integration harness/docs corrections are merged at `90d8858`.

This checkpoint is ready for bounded YNAB4 financial/source review. nYNAB,
additional source failure/shape cases, restart/native archive/provenance joins,
served contracts, shared import UI and companion/browser qualification remain
active ticket27 work. No ticket acceptance is claimed here.

## nYNAB financial and schedule checkpoint

The new nYNAB JSON adapter uses the same canonical financial validator and staged
activation as YNAB4. Direct exports and `data` plus `plan`/`budget` wrappers are
accepted. Source identities remain in the retained mapping; milliunits use the
pinned divide-by-ten and JavaScript half rounding. Internal/credit-card payment
categories, starting balances, category hiding/name collisions, historical
allocations, split/orphan transfer pairing, flags and manual schedules follow
the source importer. Original JSON stays byte-identical.

`generate-nynab-behavior.mjs` independently imports the unchanged demo through
Actual26.10 and executes actual `rules-run` and registered schedule upcoming-date
calculations. It captures 15 definitions/actions (including split indices),
source default effects and explicit calendar ranges. Current-day initial next
dates are selected from those observed dates; no replacement date formula or
runtime clock override is used. Scheduled payments remain manual. Source flags
retain their names, conflicting-color suffixes and six actual colors.

Evidence:

- `/tmp/nynab-http-third.log`: complete financial graph and historical budgets PASS.
- `/tmp/nynab-behavior-source{,-check}.log`: independent execution and check PASS.
- `/tmp/nynab-http-behavior-second.log`: full public financial/default/schedule/tag
  comparison, staging/repeat/original bytes and read-only probes PASS.
- `/tmp/nynab-clippy-first.log`: bin Clippy `-D warnings` PASS.

True initial reds exposed a zero remaining recurrence-work budget and a retained
parent category; both corrected. Consumer corrections resolve system-transfer
payees through their public account mapping (ordinary `/payees` intentionally
omits them), rather than treating missing ordinary references as null. Source
rule `type` annotations and explicit null action fields are representation-only;
operators, values, order and indexed financial actions remain exact.

This is a reviewed-backend candidate, not ticket completion. Remaining gates:
malformed/restart/archive/duplication, representative companion, complete UI,
served contracts and maintained qualification wiring. No source schedule/edit
history is invented as native undo history; original source-only fields remain
in the downloadable export.

## Corrective source and recovery checkpoint

Root's bounded review found two nYNAB defects, retained as actual source/public
reds (`/tmp/nynab-null-payee-review-red.log`,
`/tmp/nynab-unnamed-review-red.log`). An absent starting-payee ID no longer equals
a null ordinary payee. Three new unchanged source JSONs/genuine Actual exports
prove absent/null/explicit starting-payee handling and exact income/category
values. Schedule naming now uses the source schedule-specific semantics: blank
or whitespace becomes null; normalized nonempty names collide by exact case,
with the source retry suffix. Separate genuine 22-unnamed and trim/case inputs
retain complete definitions/actions and prove no schedule money is posted.
The consumer strips only top-level rule annotations; recurrence pattern `type`
discriminants remain compared exactly.

Recovery exposed another real failure: duplicate/native validation tried to
interpret retained YNAB originals as Actual SQLite ZIPs
(`/tmp/ynab-recovery-fourth.log`). Historical Actual-only enrichment queries now
filter authoritative stage source format. They never reconstruct current YNAB
financial rows or rebase schedule dates from old originals. Source upload
receipts are budget-linked alongside activation receipts; duplicate provenance
uses its actual YNAB kind. Raw originals, immutable original receipts and
financial identities survive same-budget and fresh-server portable restores.

Focused evidence (no broad repeat):

- `generate-nynab-income.mjs` and `--check`: PASS three genuine source cases.
- `generate-nynab-names.mjs` and `--check`: PASS 22 unnamed plus trim/case witness.
- `/tmp/ynab-corrective-http.log`: PASS both demos and all five new cases.
- `/tmp/ynab-recovery-portable.log`: PASS both formats across staged/activated
  process restarts, exact lost-ACK receipt retry, duplicate source-kind/raw bytes,
  public native archive inspection and same-budget/fresh-server restore;
  malformed inputs leave stage list and active budgets unchanged.
- `/tmp/ynab-existing-actual-archive.log`: existing Actual original-byte/clock
  receipt fresh-server archive regression PASS (one focused test).
- `/tmp/ynab-corrective-clippy.log`: bin Clippy `-D warnings` PASS.

Recovery consumer reuses an explicitly supplied isolated server when present;
process restart and a second fresh server are additionally exercised with its
owned debug server. Earlier recovery attempts also corrected harness assumptions
(activation201, canonical owner header, ZIP content type, typed restore name);
those are not claimed as product failures.

## Complete interface/contracts/companion candidate

The incumbent import page now offers Actual ZIP, YNAB4 ZIP and nYNAB JSON, with
a direct YNAB entry and command-search destination. It saves the exact original
file, filename, format, upload key and activation key before sending. Old saved
sessions without format retain the original Actual route. Cross-tab storage
checks prevent changing a captured format/filename under the same operation.
Visible review explains source-model differences before activation. Financial
summaries use the shared privacy-aware saved-format formatter; original files
remain explicit downloads. Shared controls use existing i18n; no catalog or
visual-system replacement. Source requirements now use the established typed
`reason` field; the UI can still display older saved `message` records.

Final scoped evidence:

- `/tmp/ynab-browser-second.log`: both actual browser migrations PASS. Real
  forwarded201 commits are asserted before ACK loss. Reload/retry compares exact
  upload format/path/query/MIME/hash/key and activation body/key, verifies first
  committed stage/budget and unchanged financial rows after retry. Quota failure
  proves zero network submission before durable save.
- Captures: `web/test-results/ynab/{nynab-desktop,nynab-mobile,
  nynab-dark-large,ynab4-desktop,ynab4-mobile}.png`. Inspected desktop,390px phone
  and dark200% text: readable controls/amounts, no horizontal clipping. Impeccable
  detector reports no findings; incumbent DESIGN.md remains authoritative. Root
  supplies consolidated finish verdict. No shipping rasters.
- `/tmp/ynab-existing-actual-browser.log`: existing Actual import recovery,
  blocked/corrupt handling, stale-tab/quota and financial assertions PASS.
- `/tmp/ynab-native-first.log`: real companion API integration PASS both imports:
  exact independent source ordinary expense projection, available references,
  saved offline mutation unchanged on restart and delivered exactly once.
  Android product code unchanged; synthetic server/JVM evidence, no claim of new
  physical-device testing.
- `/tmp/ynab-contract-first.log`: full maintained served OpenAPI consumer PASS,
  including both typed source upload/read/activation/replay/error paths and
  byte-exact original downloads. SQLite integrity is explicitly null for YNAB;
  source adapter is authoritative. Existing schema22 remains unchanged.
- `/tmp/ynab-order-http.log`: complete demos/five edge cases plus source account
  row/allocation ordering PASS; recurring pattern discriminants remain exact.
- `/tmp/ynab-bounds-recovery-http.log`: both recovery paths PASS plus malformed,
  cross-format SHA collision,100001-row,16MiB original/64MiB ZIP expansion and
  unsafe-path refusals, with unchanged stage/budget state.
- `/tmp/ynab-ui-final-{check,format-check}.log`: zero Svelte errors/warnings and
  full formatting PASS; `/tmp/ynab-ui-freeze-build.log` production build PASS
  (existing large-bundle advisory retained). Svelte autofixer both targets reports
  no issues/suggestions. Formatting only was needed in the accepted older
  transaction-import browser test; its assertions are unchanged.

Maintained entry points: `npm --prefix actualBudget/web run test:browser` includes
both new public HTTP consumers and the new browser flow, honoring an explicitly
supplied isolated test URL. `test:ynab-source` independently checks all six
source generators and is wired into the existing release verification workflow.
Native proof joins the maintained NativeApiIntegrationTest class; contracts join
contract-smoke.py. No full suite repeated here; root owns one integrated batch.

Known source differences remain intentional and visible: no synthetic YNAB edit
history, no YNAB4 schedule/autofill configuration execution, no YNAB credit-card
payment reserve/target equivalence. Source originals preserve those records.
No unresolved implementation acceptance gate is declared; root review and
integrated qualification must pass before ticket closure.


## Final YNAB4 sparse carry correction

A source-code check identified a concrete sparse-history gap before final freeze:
Actual's `api/budget-set-carryover` propagates the chosen flag through every
available future month, while YNAB stores only explicitly budgeted months. The
new independent `generate-ynab4-carry.mjs` executes the pinned importer and public
monthly budget reads for two clearly labeled synthetic YNAB4 source inputs
(one−12.34 expense, Jan Confined, optional March AffectsBuffer reset). Both
original ZIPs and genuine post-import Actual ZIPs are retained. Six fixed months
per case expose absent-month carry, reset, subsequent debt handling and carry
beyond the final explicit allocation. No SQLite or HLC fabrication.

True public red `/tmp/ynab4-carry-http-red.log` shows February's lost carry flag.
The adapter now fills missing zero-amount carry cells through the captured
conversion month's source horizon, preserving explicit resets and amounts.
Generated cells and category-month work are bounded at100000; no clock/epoch or
existing financial row is rewritten. Source generation and independent check
PASS (`/tmp/ynab4-carry-source.log`,
`/tmp/ynab4-carry-source-check-green.log`); the first check's only difference was
JavaScript−0 versus serialized JSON0, normalized by the same actual JSON wire
representation used by other source witnesses. Original source/Actual files and
financial observations were not regenerated. All nine focused public YNAB cases
PASS `/tmp/ynab4-carry-http-green.log`; final bin Clippy PASS
`/tmp/ynab-final-clippy.log`. The new generator joins the maintained source-check
command. Prior reviewed interface/contracts/native/recovery evidence is reused;
root owns final combined qualification.

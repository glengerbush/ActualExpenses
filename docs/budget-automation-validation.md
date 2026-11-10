# Budget templates and automations — verification in progress

Tickets23/24 remain open. Backend and browser checkpoint23 and cleanup execution
have focused verification; visual configuration and migration remain in progress. This record
does not claim integrated acceptance, release readiness or production deployment.

## Independent source evidence

All reference observations use Actual26.10.0, commit
`2bebdbaae0b701120a8cad9d56aafd64fa04c577`. Generators call the installed pinned
engine and registered APIs; replacement calculations do not create expectations.
Pre-operation archives preserve original IDs, notes, ledgers and definitions.
Independent `--check` runs compare observations while excluding regenerated IDs.

- `80fe8af`: first genuine simple-template tracer,7299 allocation and goal.
- `5ab2ec2`:70 grammar observations. Descriptions, multiline notes, priorities,
  caps, intervals, history, schedules, targets and malformed directives are
  retained. Calendar strings accepted by the source parser are not claimed to
  execute successfully. Replacement error prose is actionable, not PEG wording.
- `e5475a7`:51 financial states across envelope/tracking. Source category order,
  all monetary cells, per-category projections, notes and ledger observations
  remain exact. Selected categories force application; ordinary apply skips
  nonzero allocations. Per-category projections omit available-funds clamping,
  while full apply previews retain it.
- `ca9b9f2`:44 cleanup states, each observed through two explicit executions.
  Global/scoped pools, weights, overspending, carryover, negative sources,
  missing sinks and UI-source precedence have genuine pre-operation archives.
  Source quirks remain visible: a five-penny scoped pool can allocate3+3;
  negative global availability can produce negative sink allocations; tracking
  global cleanup uses the source's zero/null envelope availability. No idealized
  redistribution substitutes for these observations.
- `12a71b2`:40 visual-definition/rendered-note observations,20 paired states.
  These exercise standalone limits/refills, fixed entries, goals, history,
  targets, income IDs, schedule IDs and weighted remainders. The actual engine
  renders the notes; its UI's income-ID-to-name sanitation is explicit.
  Eighteen pairs have identical financial/goal cells. Both methods' capped
  remainder pair keeps allocation unchanged but changes goal null to0 after
  rendering. These are recorded source differences, not a claim of lossless
  migration.

All five sets passed independent provenance review, including archive
digests, integrity, raw notes, definitions, complete ledgers and seed cells.
`2712430` adds eight independently checked visual observations: goal-only plus
standalone cap, and refill without a cap, in both methods and both sources.
All four new UI/rendered-note pairs differ. The complete visual set is48 states,
24 pairs, with six observed differences; migration must refuse these conversions.

## Genuine previous-version state

`80d3f3a` captures a real schema19 database and native backup using immutable
executable `.scratch/actualbudget-schema19-94bd0c2-server`, SHA256
`eb1b47e56443f805c5ae828916329d1501ad5f1f67481e2a91c29a28b5c6b135`.
There is no database-version relabeling or direct SQLite mutation.

The source engine first creates7299 allocation,100000 goal and long-goal1, then
disables stored definitions through its real command while retaining original
template notes. Schema19 imports this supported state; public writes add an
expense, owned edit, manual schedule and month note. Two separate imports stay
blocked on template/cleanup support. The native backup covers the active budget;
the database additionally retains those staged originals and receipts.

Provenance review verifies schema19, integrity/FKs, four exact receipts/history
actions, original goals/notes, blocked archives and every native manifest entry.
Both upgrade/restore public tests pass in
`/tmp/budget-automation-upgrade-corrected.log`. They preserve money, definitions,
notes, history and immutable old receipts; current staged reports become ready
without rewriting original blocked response receipts. Restore rotates epoch.
Only operational epoch/snapshot and real-current-date schedule projections are
excluded where appropriate; schedule definitions and next dates remain exact.

Earlier red runs remain recorded. They caught the cleanup-table support
whitelist and native report version omissions. A test initially compared restored
operational schedule cursors directly; that harness comparison was corrected.

Seven additional genuine schema19 currency histories cover untouched JPY,
explicit USD, display disable, unrelated preference edits, two explicit reset
paths, and ambiguous history created through public backup/restore branches.
Fourteen database/native cases plus one current-version reset case pass across
`/tmp/budget-currency-upgrade-green.log`, `/tmp/budget-currency-reset-green.log`
and `/tmp/budget-currency-ambiguous-http.log`. Together with the two original
upgrade cases this is17 distinct tests. Older receipts and ledgers remain exact.
Ambiguous provenance refuses calculation without changes until an explicit
Settings choice. Genuine fixture provenance and the bounded lineage resolver,
including explicit-null reset handling, passed consolidated review.

## Focused runtime evidence

- `/tmp/budget-templates-projection-first.log`:51 fresh financial cases pass,
  including exact independent per-category projections, full previews,
  transactional failures, unchanged complete ledgers and restart receipt replay.
  Earlier financial coverage was47 initial passes plus four currency fixes.
- JPY calculation retains source zero-decimal behavior even when currency UI is
  disabled; KWD retains the source's two-decimal fallback. Raw calculator currency
  is stored separately from display opt-in; no three-decimal support is claimed.
- `/tmp/budget-templates-visual-config-green.log`: configure/read/apply tracer
  preserves original notes and refuses a stale snapshot atomically.
- `/tmp/budget-templates-history-first.log`: owned definition/allocation undo and
  redo, exact receipts, older/equal-clock refusal and newer-edit undo protection.
- `/tmp/budget-cleanup-http-red.log`: first cleanup consumer correctly fails at
  unsupported preview action. `/tmp/budget-cleanup-http-final.log` passes44
  source cases and one new-pool preview/cancel/undo/redo guard. Deterministic
  preview creates no writes; owned cleanup applies exactly once. Backend and
  contracts at `9b9c376` passed consolidated review.
- `/tmp/budget-visual-http-first.log`:40 visual/source-note states pass, including
  effective imported definitions, exact previews/applied cells, unchanged notes
  and ledgers, and receipt replay after restart. This tests independent states;
  it does not substitute for the pending interactive migration workflow.
- `/tmp/budget-visual-edge-http-green.log`: eight additional visual cases pass
  after `7652eb9` fixes goal-only cap and uncapped refill handling. The first run
  had four passing rendered-note cases and four failing UI cases. The unchanged
  original40 are reused. Source fixtures, consumer and fixes passed review.
- `/tmp/budget-templates-browser-final23.log`: goal-template UI passes including
  exact preview, durable lost-200-ack replay, retained review and quota failures.
  Desktop/mobile, large-text and dark captures were reviewed with Impeccable.
  Source-authority copy correction `6d10200` passes its browser assertion and
  Svelte analysis.
- `/tmp/budget-automation-native-first.log`: Android preserves expenses and
  pending delivery across automation, pool and settings events, advances its
  cursor and makes no idle cache writes. Regression passed independent review.
- `/tmp/budget-templates-contract-second.log`: served OpenAPI validates real
  template parse, read, preview, projection, configure and apply responses along
  with existing contract workflows. The additional change-feed schemas pass in
  `/tmp/budget-templates-contract-feed-corrected.log`; the first harness request
  omitted the required cursor and remains recorded as a failed run.
- `/tmp/budget-cleanup-contract-first.log`: real cleanup preview/commit and feed
  responses satisfy served schemas.
- `/tmp/budget-migration-http-final.log`:48 public migration cases pass against
  checkpoint `7da1aaf`:18 compatible UI-to-notes conversions, six refused source
  differences, and24 notes-to-UI conversions. Every preview leaves stored state
  unchanged; commits change definitions only. Subsequent application preserves
  independent source cells and receipts survive restart.
- `/tmp/budget-migration-invalid-guard.log`: focused edited-note refusal,
  cancellation fencing, stale-snapshot rejection and atomic source/notes
  undo/redo pass. `/tmp/budget-migration-pool-guard.log` verifies newly visible
  note-derived provisional pools retain identity through preview and undo/redo.
- `/tmp/budget-migration-contract-first.log` and
  `/tmp/budget-pool-contract-first.log`: real migration and pool command
  responses satisfy served schemas. `/tmp/budget-named-pool-http-first.log`
  passes creation, case-insensitive reuse, single-action history and restart
  receipts, without changing financial cells or transactions.

Consolidated migration review found one bounded-work gap in pool lookup during
reads. `7320095` closes it with a shared indexed resolver. The affected48 migration
and44 cleanup cases pass again in `/tmp/budget-migration-index-green.log` and
`/tmp/budget-cleanup-index-green.log`. The600-pool stable read/migration guard and
named-pool history/restart case pass in `/tmp/budget-pool-index-guard.log` and
`/tmp/budget-named-pool-index-green.log`. Review confirms the finding is closed.
Typed visual configuration and its browser acceptance remain open. No timing or
whole-system resource conclusion is inferred from these checks.

## Integrated backend qualification in progress

All-target Clippy passes (`/tmp/budget-automation-clippy-all.log`), the complete43
native Android API cases pass with zero skips (`/tmp/budget-automation-native-full.log`),
and the final served-schema contract passes (`/tmp/budget-automation-contract-final.log`).
The initial full HTTP run passes10 account cases, then catches old expected schema19
assertions in the schema17 upgrade consumer. Current-result assertions were updated
to20; original fixture versions and contents remain unchanged. The remaining
integration binaries are running without fail-fast; completed account evidence is reused.

Public test consumers received independent review. Cleanup's44 cases check exact
warnings/counts, goal null/zero distinctions, two executions and restart receipts.
The visual consumer also passed independent review.

## Remaining acceptance

Complete reviewed visual UI and migration behavior, resolve consolidated review,
then run one integrated batch qualification. Reuse valid unchanged source and
test evidence; rerun affected checks after corrections. No version bump, push,
release, live deployment or live-volume mutation is authorized for this rewrite.
Whole-system resource measurements remain ticket51 work.

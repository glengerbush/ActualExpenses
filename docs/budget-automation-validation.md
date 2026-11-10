# Budget templates and automations — verification in progress

Tickets23/24 remain open. Backend checkpoint `f211ff5` implements23; browser work,
cleanup execution and migration workflows are still being completed. This record
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

The first four sets passed independent provenance review, including archive
digests, integrity, raw notes, definitions, complete ledgers and seed cells.
The visual set is awaiting its consolidated review.

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
  unsupported preview action. Its import, original definitions and seed checks
  pass; cleanup execution is not yet accepted.
- `/tmp/budget-visual-http-first.log`:40 visual/source-note states pass, including
  effective imported definitions, exact previews/applied cells, unchanged notes
  and ledgers, and receipt replay after restart. This tests independent states;
  it does not substitute for the pending interactive migration workflow.
- `/tmp/budget-templates-contract-second.log`: served OpenAPI validates real
  template parse, read, preview, projection, configure and apply responses along
  with existing contract workflows. The additional change-feed schemas pass in
  `/tmp/budget-templates-contract-feed-corrected.log`; the first harness request
  omitted the required cursor and remains recorded as a failed run.

Public test consumers received independent review. Cleanup's44 cases check exact
warnings/counts, goal null/zero distinctions, two executions and restart receipts.
The new visual consumer awaits independent review.

## Remaining acceptance

Complete reviewed UI and cleanup/migration behavior, resolve consolidated review,
then run one integrated batch qualification. Reuse valid unchanged source and
test evidence; rerun affected checks after corrections. No version bump, push,
release, live deployment or live-volume mutation is authorized for this rewrite.
Whole-system resource measurements remain ticket51 work.

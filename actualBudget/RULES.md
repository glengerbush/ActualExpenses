# Transaction rules

Implementation checkpoint; ticket19 remains in progress. UI, broader action/contract edge cases, and final qualification are pending.

Rules execute pre/default/post, then ascending pinned specificity and ID. Conditions inspect the evolving transaction. Preview reads a captured snapshot and never saves ledger rows, payees, or learning. Selective apply uses one command-wide regex/recurrence work budget, one receipt, and one optional owned history action. Protected connected entries require unlock and a new preview.

Category learning requires an explicit manual category-write flag, global learning enabled, and per-payee learning enabled. Inferred categories, previews, replay, native manual writes, and undo do not train. Native clients cache only unambiguous default-stage single payee-equality/category-set associations; explicit categories remain unchanged.

Schema17 retains saved rules, per-payee flags, and global learning settings through Actual conversion and native archives. Genuine schema16 upgrade retains owned history and original receipts. Native restoration rotates the history epoch. Template/formula actions retain raw content with explicit ticket42 requirements; schedule links require ticket21. These capabilities cannot execute or silently disappear during import.

## Financial invariant exception

`tools/generate-rules.mjs` independently executes pinned `@actual-app/api@26.10.0` (source `2bebdbaae0b701120a8cad9d56aafd64fa04c577`). Financial cases use public `rule-apply-actions`, then read settled persisted transactions and balances. Original inputs are exported as `tests/fixtures/rules-financial-{0..5}-26.10.0.zip`.

In fixture1, selecting both reciprocal transfer legs and setting each amount to -77 returns no Actual errors but persists +77/+77, violating reciprocal sum zero. Replacement refuses this contradictory command atomically: no changed rows, balances, cursor, or history. The original observed result remains in `rules-26.10.0.json`. Single-leg amounts, both-leg notes/delete, and split-transfer notes match the persisted baseline.

## Focused evidence (2026-11-09)

- `cargo test --manifest-path actualBudget/server/Cargo.toml --test rules`: 7 PASS, 4.06s. Includes 43 executed condition/action cases, 13 learning cases, six settled financial cases, shared-work refusal, exact replay/owned undo, genuine schema16 upgrade, Actual import/native archive restoration.
- Focused `NativeApiIntegrationTest` simple-association cache/manual-override test PASS: initial/incremental projection, offline restart, explicit queued category survives changed rules, no implicit native learning, ambiguity removes suggestion.
- Reviewer null-ID regex and ECMAScript tag-whitespace regressions are included in the executed oracle/public HTTP tracer.

These are development checks, not final ticket acceptance or a full-suite claim.

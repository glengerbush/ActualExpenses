# Transaction rules

Ticket19 accepted through `f486a7b`; [qualification and independent reviews](../docs/rules-validation.md) cover the complete slice. Schedule execution and expressions retain their separately assigned tickets.

Rules execute pre/default/post, then ascending pinned specificity and ID. Conditions inspect the evolving transaction. Preview reads a captured snapshot and never saves ledger rows, payees, or learning. Selective apply uses one command-wide regex/recurrence work budget, one receipt, and one optional owned history action. Protected connected entries require unlock and a new preview.

Category learning requires an explicit manual category-write flag, global learning enabled, and per-payee learning enabled. Inferred categories, previews, replay, native manual writes, and undo do not train. Native clients cache only unambiguous default-stage single payee-equality/category-set associations; explicit categories remain unchanged.

Schema17 retains saved rules, per-payee flags, and global learning settings through Actual conversion and native archives. Genuine schema16 upgrade retains owned history and original receipts. Native restoration rotates the history epoch. Template/formula actions retain raw content with explicit ticket42 requirements; schedule links require ticket21. These capabilities cannot execute or silently disappear during import.

## Financial invariant exception

`tools/generate-rules.mjs` independently executes pinned `@actual-app/api@26.10.0` (source `2bebdbaae0b701120a8cad9d56aafd64fa04c577`). Financial cases use public `rule-apply-actions`, then read settled persisted transactions and balances. Original inputs are exported as `tests/fixtures/rules-financial-{0..12}-26.10.0.zip`.

In fixture1, selecting both reciprocal transfer legs and setting each amount to -77 returns no Actual errors but persists +77/+77, violating reciprocal sum zero. Replacement refuses this contradictory command atomically: no changed rows, balances, cursor, or history. The original observed result remains in `rules-26.10.0.json`. Single-leg amounts, both-leg notes/delete, split-transfer notes, paired unlink, and conversion to two reciprocal split allocations match the persisted baseline. Existing-parent child-target amount/date/cleared cases persist an unbalanced total or a child whose date/cleared disagrees with its aggregate; replacement refuses the entire command without silently normalizing those results. Original observations remain in the fixture.

The pinned manual apply parser discards split options for prepend/append/delete actions; live rules-run keeps them. Preview/application uses the corresponding context rather than silently treating the two public pipelines as identical.

Automatic browser entry opts into `POST /rules/transactions`; legacy/native transaction routes retain manual behavior. Entry runs one captured rules snapshot inside its original receipt/history action. The exact pinned desktop `shouldApplyRuleChange` function is extracted by TypeScript AST and executed independently: empty/null/zero/false fields can fill; nonempty manual values remain; notes allow only preserved prefix/suffix additions with duplicate-addition protection. Existing browser edits bypass this entry engine, so clearing remains explicit; new-entry null categories remain fillable. Inferred fills never become manual learning evidence.

## Focused evidence (2026-11-09)

- `cargo test --manifest-path actualBudget/server/Cargo.toml --test rules`: 7 PASS, 4.06s. Includes 43 executed condition/action cases, 13 learning cases, 12 settled financial cases, shared-work refusal, exact replay/owned undo, genuine schema16 upgrade, Actual import/native archive restoration.
- Focused `NativeApiIntegrationTest` simple-association cache/manual-override test PASS: initial/incremental projection, offline restart, explicit queued category survives changed rules, no implicit native learning, ambiguity removes suggestion.
- Reviewer null-ID regex and ECMAScript tag-whitespace regressions are included in the executed oracle/public HTTP tracer.

These are development checks, not final ticket acceptance or a full-suite claim.

Second backend checkpoint (UI still in progress): 12 persisted financial cases PASS `/tmp/rules-financial-edges-http.log`; compatible selected-peer notes+cleared and one matched amount are covered. Positive splitIndex setters do not project native suggestions; focused native invalidation PASS `/tmp/rules-native-invalidation.log`. Full focused rules tracer rerun recorded separately before commit.

Generated counterparts execute the complete saved-rule chain but persist only notes/cleared, matching transfer.addTransfer. They share the original command work budget and cannot recursively mutate the graph. Detailed candidate evidence: `../docs/rules-validation.md`.

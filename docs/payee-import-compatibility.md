# Actual payee-name import compatibility

2026-11-13. Reported symptom: importing a desktop ZIP returns `Invalid source payee name`.

## Reproduction and cause

Generated a genuine archive through the pinned `@actual-app/api@26.10.0`, using only public budget/payee/transaction/export operations. Actual preserves blank, whitespace-only, tab-containing, multiline and longer-than-120-character payee names. A deleted payee with a blank name is also retained in the archive. No source SQLite rows were fabricated for this regression fixture.

The replacement's public import endpoint returned HTTP422 with the exact reported error. Separate diagnostic copies proved that blank, 121-character and newline-containing names each trigger that same error, while an ordinary name stages successfully. The importer incorrectly reused the validation rules for newly entered names.

The fix preserves imported payee text exactly, including native backup/restore. The new regression first exposed the source-import rejection, then the same inappropriate name restriction in native restore validation; both boundaries now accept the same source values. String/type checks, identity/reference checks, financial validation and bounded archive processing remain. New-entry validation remains unchanged. No source names are trimmed, replaced or dropped.

## Regression coverage

`genuine_actual_payee_names_survive_import_without_normalization_or_financial_changes` exercises real HTTP upload, staging, activation, restart, payee/transaction queries and native backup/restore. It checks exact names and IDs, the deleted payee, transaction references/amounts/notes, balances and byte-identical original archive download. Before the fix, this test failed with the exact reported HTTP422 message.

Fixture generation: `node actualBudget/tools/generate-import-payee-names.mjs`. The fixture contains only synthetic merchant text and expenses created for this test. The user's private archive was not accessed; which rejected name occurs in their file remains unknown.

This adds compatibility coverage to the existing suite. No release checks or financial assertions were removed.

Local validation passed all 36 import, payee/tag, budget-management and recovery
tests, including the new complete import/native-restore regression. Patch version:
actualBudget 0.2.5; Android remains unchanged. Hosted qualification and publication
must succeed before the image is available for Coolify's manual pull/restart.

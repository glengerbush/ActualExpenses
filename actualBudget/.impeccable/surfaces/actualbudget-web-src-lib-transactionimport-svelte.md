---
version: 1
slug: "actualbudget-web-src-lib-transactionimport-svelte"
primary_target: "actualBudget/web/src/lib/TransactionImport.svelte"
related_targets: ["actualBudget/web/src/lib/BulkActions.svelte"]
---

# Transaction files
Mode: Operate. Existing actualBudget controls, typography and surfaces.
Scope: account-scoped CSV/TSV/OFX/QFX/QIF/CAMT upload, mapping, options, review and selective import; CSV download; two-entry merge review.
Visitor: budget owner moving bank records into a register without losing manual edits or creating duplicate purchases.
Task: choose account/file, inspect parsed columns, review explicit add/match/skip outcomes, commit once. Recovery retains the original file, captured context and exact pending request across reload and connection loss.
Direction: code-led incumbent extension. First viewport: heading, close control, durable recovery feedback, account/file controls; later steps appear in reading order. Signature interaction: matched and added records are reviewed before a single atomic commit, with original request retry/cancel after interruption.
Proof: source-backed public API and real browser flows. Desktop, narrow, dark and 200% text remain readable; long previews paginate and tables have named keyboard scroll regions.
Constraints: no JSON editor, no background polling, no silent epoch rebasing, no content-hash purchase IDs. Retain existing privacy masking and complete native backup navigation.
Unresolved: none; financial behavior follows pinned source and approved invariant refusals.

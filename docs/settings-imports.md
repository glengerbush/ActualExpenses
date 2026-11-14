# Settings imports and recovery

Settings contains General, Import, Backups and Recovery. Transaction files and Recovery tools no longer occupy sidebar entries. Settings remains available during connection recovery. The original URLs and recovered transaction-import drafts still open the corresponding Settings section.

Import accepts Actual ZIP, YNAB4 ZIP and nYNAB JSON budget exports, plus CSV/TSV/OFX/QFX/QIF/XML transaction files. Both flows target the single current budget; there is no destination chooser or creation form. Archive imports initialize an empty budget or merge into its existing data. Transaction files retain mapping, preview, matching and retry behavior in the current transaction history.

Archive merging is additive and atomic: financial records and their payee/category references are imported; destination plans and settings stay unchanged. Matching named metadata is reused. Stable source identities and persisted provenance mappings prevent repeat-import duplicates, including after metadata renaming. Existing transaction edits and deletions are preserved. Split/transfer references are remapped together and validated before commit. Existing source-account boundaries remain internal so transfers and historical financial totals keep their meaning.

The merge endpoint requires the reviewed destination epoch. Browser recovery saves the destination and original idempotency key before sending. The receipt, imported rows and provenance commit together; receipts and provenance participate in native backup/restore. Source archives remain staged and downloadable.

Native backup restore is separate from additive import: Settings → Backups explicitly replaces current data after confirmation, even when the archive came from a former budget. A recovery backup is saved first. See [single-budget behavior](single-budget.md) for upgrade consolidation and stale-device recovery.

Relevant coverage: `settings-import-browser.mjs`, `single-budget-browser.mjs` and `single-budget-migration.mjs`.

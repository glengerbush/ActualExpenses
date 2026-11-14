# One budget

The app automatically opens Transactions in one budget. No first-run naming, budget chooser, switcher, or normal budget rename/delete/duplicate actions remain. The installed iPhone companion opens the same data automatically. Settings contains General, Import, Backups and Recovery.

## Upgrade consolidation

Schema migration 037 merges existing active budgets into the oldest active budget (database insertion order). That budget retains its settings and preferences. Source transactions and categories remain distinct, even when names, dates or amounts match. Plans, recurrence, colors and financial relationships follow remapped entity identities; schedule names gain source labels where needed. Historical source accounts remain internal to preserve splits, transfers and totals.

The schema runner first saves a complete, consistent SQLite file under `ACTUALBUDGET_DATA_DIR/backups/before-v<old-schema>-<uuid>.sqlite3`, including committed WAL data. When several active budgets need consolidation, each receives a native recovery snapshot labeled `Before merging <name>`. Source snapshots are attached to the retained budget’s Backups list. Already-deleted budgets are not reactivated by this migration. The full SQLite copy retains the database as it existed before the upgrade.

A source snapshot restores that source’s contents as the current budget; it does not recreate a second selectable budget. Keep the standalone SQLite file for whole-database recovery. It is not an import ZIP.

## Saved device work

Merged source identities are retired. The retained budget’s epoch/generation changes, so pre-merge queued edits cannot silently replay against the merged history. Devices must discover the current budget online before using its cache; an old multi-budget cache is not sufficient.

Review saved work in Settings → Recovery and recovered-operation screens. Download available saved request/expense details before explicit cancellation or removal. Keep unresolved local drafts and browser storage until reviewed. Do not rewrite IDs or epochs to force stale work through; re-enter needed changes against the current data after checking whether the original operation already completed.

## Import and restore

- **Budget exports:** Actual ZIP, YNAB4 ZIP and nYNAB JSON initialize an empty current budget or merge into existing data. Additive archive merge retains current plans/settings and uses source provenance to avoid repeat-import duplicates. This import policy differs from upgrade consolidation, which preserves distinct source entities and their plans.
- **Transaction files:** CSV/TSV/OFX/QFX/QIF/XML use the current transaction history, retaining mapping, preview, matching and retry controls.
- **Native backups:** Settings → Backups requires explicit replacement confirmation. Restore replaces the singleton’s data even when the archive has a different source budget ID. A pre-restore recovery backup is saved first; review saved device edits afterward.

The migration implementation is `actualBudget/server/src/single_budget.rs` and `single_budget/merge.rs`; recovery and import behavior remain in their existing modules. Focused checks live in `single-budget-browser.mjs` and `single-budget-migration.mjs`. Historical multi-budget specifications and acceptance artifacts remain unchanged as evidence of earlier behavior.

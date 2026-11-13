# Settings imports and recovery

Settings contains General, Import, and Recovery. Transaction files and Recovery tools no longer occupy sidebar entries. Settings remains available without an open budget. The original URLs and recovered transaction-import drafts still open the corresponding Settings section.

Import accepts Actual ZIP, YNAB4 ZIP and nYNAB JSON budget exports, plus CSV/TSV/OFX/QFX/QIF/XML transaction files. Both flows offer a new budget or an existing destination. Transaction files retain mapping, preview, matching and retry behavior. New transaction budgets use the existing recoverable creation form and unified history.

Archive merging is additive and atomic: financial records and their payee/category references are imported; destination plans and settings stay unchanged. Matching named metadata is reused. Stable source identities and persisted provenance mappings prevent repeat-import duplicates, including after metadata renaming. Existing transaction edits and deletions are preserved. Split/transfer references are remapped together and validated before commit. Existing source-account boundaries remain internal so transfers and historical financial totals keep their meaning.

The merge endpoint requires the reviewed destination epoch. Browser recovery saves the destination and original idempotency key before sending. The receipt, imported rows and provenance commit together; receipts and provenance participate in native backup/restore. Source archives remain staged and downloadable.

Validation: `settings-import-browser.mjs` exercises archive normal/split/transfer fixtures, same-source/repeated/replayed imports, stale destination rejection, settings preservation, Settings navigation, CSV into new/existing budgets, merge retry after a committed response is lost, and narrow-screen layout.

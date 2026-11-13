# Tracking-only budgeting

Tracking is the only active budgeting method. Creation has no method selector; planning has no method switch, income holds, money moves, overspending coverage, Clear allocations action, or month/budget notes. Category metadata and transaction notes remain available.

Schema 31 converts an existing envelope budget's active allocation amounts and carryover flags to tracking plans. Previously inactive tracking plans are replaced, not combined. Transactions are unchanged. The normal pre-migration SQLite backup retains the original state. Converted budgets receive a new epoch and clear undo history so old method-specific actions cannot change the converted plans.

Actual imports and native restores use the same conversion boundary. Legacy archive columns and source parsing remain for compatibility, but no envelope calculation or writable month-note API remains. Removed actions reject new requests; exact historical receipts retain their original bytes. Generic notes commands reject reserved month and budget identities. Tracking budgets retain their current plans; retired month/method undo entries are removed.

Validation: tracking-only-browser covers schema 30→31 conversion, conflicting inactive plans, carryover, unchanged transactions, rejected actions and note writes, Actual envelope imports, native archive staging, creation, row editing, and desktop/mobile layout. Tracking reference fixtures cover supported allocation and carryover actions; budget API tests cover concurrency, receipts, history and portable restore.

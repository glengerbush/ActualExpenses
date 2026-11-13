# Transaction recurrence

The web transaction editor owns recurrence setup. Its Recurring switch reveals interval, frequency, start, and optional end date/count. New entries default off. Editing a linked entry loads its series; changes affect future repeats, and switching off completes the series without deleting historical entries. The Schedules navigation item and command are removed. Existing advanced schedule/recovery review remains available from recurring-entry review links.

Ordinary transaction POST/PATCH and automatic-rule entry accept an optional `recurrence: { enabled, config }`. The transaction and schedule share one receipt, rollback and undo/redo history entry. The existing outbox stores both together, including offline capture. Ordinary requests retain their original serialized receipt identity. Explicit new recurrence is independent of automatic schedule matching.

The existing schedule engine generates dates and posts due repeats during foreground catch-up. It copies amount, payee, category and notes; the saved transaction and already handled occurrences are not reposted. Existing imported recurrence patterns and extra conditions/actions are preserved. Splits, transfers and starting balances retain their existing specialized workflows.

Verification: `npm run test:recurrence-editor` from `actualBudget/web` with the built Rust server and web bundle. Covers atomic create/rollback, replay, catch-up, rule entry, stop/reopen, undo/redo, mobile layout and offline reload/delivery.

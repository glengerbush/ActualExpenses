# Compact editing and notes retirement

Budget category rows have a pencil button for the full editor. Planned amounts edit directly in the table and save on blur or after one second of inactivity. Actual and remaining amounts remain calculated. Drafts and immutable requests survive reloads and failed delivery; continued typing during a save queues the latest amount after the first receipt resolves. Canceling an uncertain request never automatically resubmits it.

The month control uses arrows, `Nov-26` text entry and an embedded reset button titled “This month”. Transaction editors fit date, signed direction, amount, payee, category, recurrence and save/cancel controls in one row on wide screens. Direction keeps accessible Expense/Income names while showing red minus/green plus. Recurrence configuration expands only when enabled. Refresh buttons use Firefox's default reload artwork, under its original MPL-2.0 notice in `RefreshIcon.svelte`.

Automatic category shades now span 50% darker through 75% lighter, with stronger row tints. The server and browser use the same stable identity calculation; manual color overrides remain unchanged.

Budget, category/group and transaction notes are retired. Schema 32 clears stored values and prevents archive restores, history or old clients from recreating them. Inputs, displays, bulk note operations, rule actions and note filtering/query fields are removed. Android no longer offers or sends notes. Legacy SQLite columns and empty transaction wire/draft fields remain only to decode old archives and recover previously saved operations. Original import archives and pre-migration backups are retained.

Verification: `compact-editing-browser.mjs` exercises quick edits, in-flight typing, failed-request/reload recovery, recurrence, month navigation and responsive layout. `notes-retirement-api.mjs` verifies schema31 upgrade, financial preservation, Actual import and portable restore. The budgeting Rust suite checks financial results, history, clocks and archive conversion.

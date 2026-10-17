# ActualExpenses design interview

Status: Approved and implemented on 2026-10-16. See [validation evidence and remaining qualification limits](validation.md) and [implementation review](review.md).

Published spec: [ActualExpenses Android Expense Entry Spec](</home/ggg/Documents/pileOfNotes/Work/Issues/ActualExpenses Android Expense Entry Spec.md>).

## Confirmed scope

- Android companion app for recording expenses at the point of purchase in Actual Budget.
- The user uses Actual's tracking budget method rather than envelope budgeting.
- The user selects an Actual account, such as Spending.
- Each installation connects to one Actual budget, chosen during setup. Daily navigation switches accounts within that budget.
- Setup is server address → sign in with the existing Actual password → choose budget → choose account. Store the session securely on Android so daily use needs no login; request authentication again when the saved session stops working.
- The app remembers the last selected account and opens directly into its expense screen. Returning to account selection remains one tap away.
- The transaction entry form requires an amount. Payee and category are optional; the date defaults to today. Optional notes are tucked away from the primary entry fields.
- Payee entry searches existing payees as the user types and allows a new name when no match exists. This works offline; sync matches an existing payee or creates the new payee for the expense.
- Category selection uses existing Actual categories only, with a searchable list cached for offline use. Leaving the category blank is allowed; creating categories is done in Actual.
- Automatic categorization uses Actual's existing simple payee-to-category associations, downloaded and cached for offline use. Selecting a payee autofills its category; the user may override it before saving. Broader rule behavior is outside v1 and can be added later if needed.
- A manual category override applies only to that expense. It does not create or update the payee's default association; defaults are managed in Actual.
- The focused account interface allows returning to account selection, adding an expense, and viewing/editing/deleting ordinary unreconciled expenses in that account.
- The list excludes transfers, split transactions, reconciled entries, and income. These are managed in Actual.
- New expenses are marked cleared automatically, avoiding a separate manual clearing step. Cleared unreconciled expenses remain visible and editable; cleared and reconciled are distinct states. No clearing/reconciliation controls are planned in the app.
- The recent expense list includes qualifying entries from all sources in the selected account, including Actual Desktop and bank imports. Pending entries appear immediately and are marked unsynced.
- The main list contains the 10 qualifying expenses with the newest transaction dates. Editing an older expense does not promote it merely because it was edited.
- Pending changes stay visible until synced, even when this temporarily exceeds 10 entries.
- Exception: a newly added transaction backdated outside the latest 10 remains visible at the bottom of the list, so the user can edit or delete a mistaken entry. It stays until the user adds another transaction in that account; unsynced entries remain visible regardless.
- Cache the selected account and recent transactions for offline use.
- Allow creating expenses while disconnected, with a small pending-sync indicator.
- While offline, allow editing only transactions created offline that have not yet synced. Transactions already in Actual are not editable offline.
- When connected, allow editing eligible expenses and syncing changes.
- Offline deletion follows editing eligibility: only transactions created offline that have not synced may be deleted offline. Deleting an existing server transaction requires a connection.
- Deleting an expense requires one confirmation showing its amount, payee, and date. No separate undo workflow is planned.
- If editing begins online and the connection drops, retain the edit and sync it later. Follow Actual's newest-edit-wins behavior per field, regardless of device: independent amount/category changes can both survive; competing changes to the same field use Actual's native sync ordering. The earlier server-precedence interpretation is superseded. Retain a brief notice when a queued phone edit is superseded.
- Pending-deletion exception: if a phone deletion predates an edit on another device that is found before the deletion is published, cancel the pending deletion and retain the edited expense. Compare original deletion/edit ordering, not arrival or retry time. Once a deletion has synced, leave subsequent behavior to Actual's native synchronization; do not automatically restore the expense. Actual's native tombstone-field merge does not itself provide the pending cross-field check.
- When the Actual server is reachable, send changes immediately. Retry pending changes automatically when connectivity returns.
- Background syncing is best-effort and only serves pending expense changes: additions, modifications, and deletions. The user's term "edits" includes deletions. Do not schedule ongoing background polling, refresh-only syncing, or background work when there are no pending changes. Resource usage should stay minimal.
- If Android prevents background work, keep pending changes saved locally and retry in the foreground when the app opens. Background scheduling restrictions must not make the app fail. A deletion interrupted by connectivity loss stays pending, can retry in the background, and follows the pending-deletion conflict check.
- The phone will use a tailnet to reach the server. Reachability matters; the app must not depend on being connected to home Wi-Fi or distinguish LAN from tailnet access.
- The existing Actual deployment uses HTTP. Its persistent volume is named `actualbudget-data`, mounted at `/data`.
- Record the agreed design and work in Obsidian after the interview is complete.

## Integration decision

- Use direct Android-to-Actual integration with the existing server endpoints. No companion service or embedded Node.js runtime is planned.
- This is the selected path for now; revisit if compatibility testing exposes impractical requirements.
- After design approval, the first implementation milestone is a compatibility prototype against a disposable budget: authenticate, read accounts, add an expense, edit/delete it, and verify the results in Actual Desktop before building the production UI.
- The prototype must also test the custom deletion-conflict rule, including concurrent sync and interrupted requests; do not assume native sync provides it.
- Maintain compatibility with Actual's sync protocol and relevant budget schema. See [ADR 0001](adr/0001-direct-actual-integration.md).
- Build a native Android app with Kotlin and Jetpack Compose. The user delegated routine technology choices to sensible defaults; no alternative has an established performance advantage for this workload. Use standard Android local-storage and background-work facilities, with detailed persistence boundaries validated by the compatibility prototype.
- Distribute v1 as a signed, directly installable APK for the user and trusted users. Google Play publication is outside v1.
- V1 supports unencrypted Actual budgets only; the user will not enable Actual's optional budget encryption. Do not implement its key exchange or encrypted budget/message handling in v1. Detect an encrypted budget and explain that it is unsupported. This does not remove ordinary credential protection on Android.

## Confirmed defaults

- Enter a positive expense amount in the budget's configured currency; store it using Actual's money-out convention. Reject zero or negative input. Use exact monetary arithmetic, not floating-point calculations.
- Persist changes before showing them as saved. Retain pending work across app restarts, authentication failures, server errors, and interrupted requests. Show a brief actionable error when user intervention is needed.
- Give each new expense a stable identity and each mutation its original sync ordering. Retry the same operation after a timeout; never create another expense or make an old edit appear newer merely because it was retried.
- Reconcile uncertain acknowledgements before deciding a deletion is still unpublished. A pull-before-publish check cannot guarantee protection from an edit that races with publication; once published, Actual's native behavior applies. Validate this boundary in the prototype.

## Acceptance checks

- Complete setup against an unencrypted disposable budget over HTTP; reopen without signing in again and return to the remembered account. Verify equivalent access through LAN and tailnet routing.
- Create an expense with amount only and with payee, category, date, and notes. Verify the account, sign, exact amount, date, and cleared state in Actual Desktop.
- Search cached payees/categories offline, create a new payee, autofill a simple category association, and override it without changing the association.
- Show the newest 10 eligible expenses by transaction date across sources. Exclude income, transfers, splits, and reconciled entries. Keep pending changes visible and retain the newest backdated addition below the list until the next addition in that account.
- Offline: create and modify/delete an unsynced offline-created expense; prevent starting edits/deletions of synced entries. Lose connectivity during an online edit and verify that the saved change survives restart and later syncs.
- Interrupt requests before sending, during sending, and after server acceptance but before acknowledgement. Retry without duplicate expenses, lost changes, or refreshed mutation timestamps.
- Concurrent devices: merge independent field edits; use Actual's ordering for same-field edits. Notify when a queued phone edit is superseded.
- Cancel an unpublished phone deletion when a newer remote edit is discovered. Exercise publication races and lost acknowledgements; verify that already published deletions follow native behavior without automatic restoration.
- Attempt background work only while additions, modifications, or deletions are pending. Deny Android background execution and verify that foreground reopening resumes retained work. With no pending work, schedule no background sync.
- Authentication expiry or unsupported encrypted/schema data produces a clear explanation and preserves pending work. Do not mutate a budget whose compatibility has not been established.
- Install the signed APK and update it without losing local pending work or setup. Validate the supported Android versions and signing-key handling before distribution.

## Delivery sequence

1. Completed: confirm this design and record the project and spec in Obsidian.
2. Completed: prove direct protocol compatibility against a disposable budget, including retries and conflict boundaries.
3. Completed: build the focused Android interface and durable pending-change handling on the validated integration.
4. Completed: deliver the signed APK with installation instructions and recorded acceptance evidence; deployment-specific checks remain explicit in the validation record.

## Evidence

- The repository initially contains only an empty ReadMe.md.
- Actual's official programmatic integration is `@actual-app/api`; direct protocol integration was selected after inspecting its implementation.
- Inspected Actual v26.10.0 source: direct HTTP integration is possible without Node.js. `/account/login` authenticates; `/sync/list-user-files` and `/sync/download-user-file` provide budget discovery/bootstrap; `/sync/sync` exchanges binary Protobuf database changes and sync state. These are synchronization endpoints, not account/transaction CRUD endpoints.
- A direct Android client needs compatible budget snapshot reading, relevant database schema handling, change serialization, logical clocks, and synchronization/retry state. Encryption is excluded from v1; detailed compatibility must be validated by the prototype.
- Source: https://github.com/actualbudget/actual/blob/v26.10.0/packages/sync-server/src/app-sync.ts and https://github.com/actualbudget/actual/blob/v26.10.0/packages/crdt/src/proto/sync.proto
- Payee/category associations are represented by rules in the synced budget data. Actual's automatic category learning creates/updates categorization rules. Android can obtain and cache this data through budget download/sync; executing arbitrary rules is a separate compatibility requirement. Sources: https://actualbudget.org/docs/budgeting/rules/ and https://github.com/actualbudget/actual/blob/v26.10.0/packages/loot-core/src/server/transactions/transaction-rules.ts
- Actual v26.10.0 compares sync messages for each dataset/row/column and suppresses older values for that cell. It orders changes with a hybrid logical timestamp. These mechanisms need to guide the final newest-edit-wins design: https://github.com/actualbudget/actual/blob/v26.10.0/packages/loot-core/src/server/sync/index.ts and https://github.com/actualbudget/actual/blob/v26.10.0/packages/crdt/src/crdt/timestamp.ts

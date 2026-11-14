# actualExpenses and actualBudget

Two applications sharing a budget: focused expense tracking and full budgeting.

## Language

**actualExpenses**:
The Android companion for recording and managing expenses in a selected budget. Its scope is expense tracking.

**actualBudget**:
The full web application for managing budgets, including features beyond expense tracking. A desktop application is deferred.

**Budget**:
An independent financial dataset containing accounts, transactions, categories and a budgeting plan. actualBudget can manage multiple budgets; actualExpenses selects one during setup.
_Avoid_: Account

**Account**:
An internal compatibility reference retained for imported history and queued operations. Users work with one transaction history per budget; new transactions receive an automatic destination. Account creation, selection and transfers between accounts are no longer product workflows.
_Avoid_: Login account, user identity

**Transaction**:
A financial entry in the budget-wide history. Existing internal account references remain intact. ActualExpenses lets the user add and edit these entries.

**Expense**:
An ordinary money-out transaction in the selected budget. ActualExpenses shows ordinary expenses, excluding transfers and split transactions. Entries are complete when recorded; sync delivery is separate.

**Payee**:
The person or business receiving an expense payment. It may be selected from existing payees or named when recording an expense.

**Category**:
An existing Actual budget classification assigned to an expense. An expense may be left uncategorized.

**Completed entry**:
Every manually recorded transaction. There is no cleared, uncleared, pending-payment or reconciliation workflow. Historical status columns are compatibility storage only.

**Pending change**:
A transaction addition, modification, or deletion saved on a device that has not yet been confirmed by the budget server. “Edits” includes deletions when discussing sync behavior. Delivery is silent; pending is an internal queue state, not a transaction status.
_Avoid_: Synced transaction

**Cached view**:
A previously retrieved view of budget information available on a device while disconnected. It can be older than the current budget and is not a separate authoritative budget.

**Offline expense capture**:
Recording an expense while disconnected, with correction or deletion allowed before that offline-created expense is delivered. This does not imply full offline budgeting.

**Released-state decoder**:
A read-only Android migration adapter for the original companion’s durable cache and outbox. It projects saved rows and original queued changes without connecting to the old server or rewriting saved work. The old network client remains test-only as a compatibility oracle.

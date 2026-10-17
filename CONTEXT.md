# ActualExpenses

A focused expense-entry companion to Actual Budget.

## Language

**Budget**:
The Actual budget containing the accounts and transactions used by an installation of ActualExpenses. The user chooses one budget during setup.
_Avoid_: Account

**Account**:
An account within an Actual budget, such as Spending, whose transactions the user records and reviews.
_Avoid_: Login account, user identity

**Transaction**:
A financial entry belonging to an account. ActualExpenses lets the user add and edit these entries.

**Expense**:
An ordinary money-out transaction in the selected account. ActualExpenses shows only unreconciled expenses, excluding transfers and split transactions.

**Payee**:
The person or business receiving an expense payment. It may be selected from existing payees or named when recording an expense.

**Category**:
An existing Actual budget classification assigned to an expense. An expense may be left uncategorized.

**Cleared expense**:
An expense marked cleared in Actual. New expenses from ActualExpenses are marked cleared automatically; being cleared does not make an expense reconciled.

**Reconciled entry**:
A transaction marked reconciled in Actual. These entries are excluded from ActualExpenses.

**Pending change**:
A transaction addition, modification, or deletion saved on the phone that has not yet been confirmed by Actual. “Edits” includes deletions when discussing sync behavior.
_Avoid_: Synced transaction

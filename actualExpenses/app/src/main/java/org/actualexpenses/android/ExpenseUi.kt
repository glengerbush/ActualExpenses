package org.actualexpenses.android

import java.time.LocalDate
import org.actualexpenses.core.EditSession
import org.actualexpenses.core.ExpenseContext

enum class Screen { SERVER, BUDGETS, ACCOUNTS, EXPENSES, RECOVERY }

data class Choice(val id: String, val name: String)

data class ExpenseRow(
    val id: String,
    val amountMinor: Long,
    val date: LocalDate,
    val payee: String,
    val categoryId: String?,
    val category: String,
    val notes: String,
    val pending: Boolean,
    val pendingDeletion: Boolean,
    val editable: Boolean,
)

data class ExpenseUiState(
    val screen: Screen = Screen.SERVER,
    val server: String = "",
    val budgets: List<Choice> = emptyList(),
    val accounts: List<Choice> = emptyList(),
    val selectedAccount: Choice? = null,
    val expenses: List<ExpenseRow> = emptyList(),
    val payees: List<Choice> = emptyList(),
    val categories: List<Choice> = emptyList(),
    val associations: Map<String, String> = emptyMap(),
    val currency: String = "USD",
    val decimalPlaces: Int = 2,
    val online: Boolean = false,
    val busy: Boolean = false,
    val pendingCount: Int = 0,
    val splitRecovery: Boolean = false,
    val error: String? = null,
    val notice: String? = null,
    val savingOperationId: String? = null,
    val completedOperationId: String? = null,
    val recoveryMessage: String? = null,
    val canCancelConnection: Boolean = false,
    val expenseContext: ExpenseContext? = null,
)

data class ExpenseForm(
    val id: String?,
    val amount: String,
    val date: LocalDate,
    val payee: String,
    val categoryId: String?,
    val notes: String,
    val operationId: String,
    val context: ExpenseContext? = null, val edit: EditSession? = null,
)

interface ExpenseUiActions {
    fun showServer()
    fun cancelConnection()
    fun beginEdit(id: String, completed: (EditSession) -> Unit)
    fun connect(server: String)
    fun chooseBudget(id: String)
    fun chooseAccount(id: String)
    fun showBudgets()
    fun showAccounts()
    fun refresh()
    fun save(form: ExpenseForm)
    fun delete(session: EditSession, operationId: String)
    fun discardRejectedSplitEdit()
    fun dismissMessage()
}

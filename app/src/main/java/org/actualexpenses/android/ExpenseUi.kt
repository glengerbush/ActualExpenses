package org.actualexpenses.android

import java.time.LocalDate

enum class Screen { LOGIN, BUDGETS, ACCOUNTS, EXPENSES }

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
    val screen: Screen = Screen.LOGIN,
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
    val error: String? = null,
    val notice: String? = null,
    val savingOperationId: String? = null,
    val completedOperationId: String? = null,
    val requiresSignIn: Boolean = false,
)

data class ExpenseForm(
    val id: String?,
    val amount: String,
    val date: LocalDate,
    val payee: String,
    val categoryId: String?,
    val notes: String,
    val operationId: String,
)

interface ExpenseUiActions {
    fun backToLogin()
    fun beginEdit(id: String, completed: () -> Unit)
    fun login(server: String, password: String)
    fun chooseBudget(id: String)
    fun chooseAccount(id: String)
    fun showAccounts()
    fun refresh()
    fun save(form: ExpenseForm, completed: () -> Unit)
    fun delete(id: String, operationId: String, completed: () -> Unit)
    fun dismissMessage()
}

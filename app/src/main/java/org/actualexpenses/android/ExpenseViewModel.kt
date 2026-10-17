package org.actualexpenses.android

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import org.actualexpenses.core.AppState
import org.actualexpenses.core.EditSession
import org.actualexpenses.core.ExpenseDraft
import java.time.LocalDate

class ExpenseViewModel(application: Application) : AndroidViewModel(application), ExpenseUiActions {
    private val graph = (application as ExpensesApplication).graph
    private val pickingAccount = MutableStateFlow(false)
    private val showingLogin = MutableStateFlow(false)
    private val busy = MutableStateFlow(false)
    private val localError = MutableStateFlow<String?>(null)
    private data class SaveState(val saving: String? = null, val completed: String? = null)
    private val saveState = MutableStateFlow(SaveState())
    private var editSession: EditSession? = null
    private var refreshJob: Job? = null
    private var recoveredPending = false
    val state = combine(graph.state, pickingAccount, showingLogin, busy, localError) { state, choose, login, busy, error ->
        state.toUi(choose, busy, error).let { if (login) it.copy(screen = Screen.LOGIN) else it }
    }.combine(graph.loading) { state, loading -> state.copy(busy = state.busy || loading) }
        .combine(graph.fatalError) { state, fatal -> state.copy(error = fatal ?: state.error, busy = state.busy || fatal != null) }
        .combine(saveState) { state, operation -> state.copy(savingOperationId = operation.saving, completedOperationId = operation.completed) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, ExpenseUiState(busy = true))

    private fun task(action: suspend () -> Unit) {
        viewModelScope.launch {
            busy.value = true
            localError.value = null
            try { withContext(Dispatchers.IO) { action() } }
            catch (error: Exception) { localError.value = error.message ?: "Something went wrong. Your saved changes are retained." }
            finally { busy.value = false }
        }
    }

    override fun login(server: String, password: String) = task {
        graph.access { it.login(server, password) }
        showingLogin.value = false
        graph.schedulePending()
        if (graph.state.value.budgetId != null) withContext(Dispatchers.Main) { refresh() }
    }
    override fun chooseBudget(id: String) = task { graph.access { it.chooseBudget(id) }; pickingAccount.value = true }
    override fun chooseAccount(id: String) = task { graph.access { it.chooseAccount(id) }; pickingAccount.value = false }
    override fun showAccounts() { pickingAccount.value = true }
    override fun backToLogin() { showingLogin.value = true }
    override fun cancelSignIn() { showingLogin.value = false }
    override fun refresh() {
        if (refreshJob?.isActive == true) return
        refreshJob = viewModelScope.launch {
            busy.value = true
            try { withContext(Dispatchers.IO) {
                graph.refresh()
                if (!recoveredPending) { recoveredPending = true; graph.schedulePending() }
            } }
            catch (error: Exception) { localError.value = error.message ?: "Unable to connect. Saved changes are retained." }
            finally { busy.value = false }
        }
    }
    fun markOffline() = task { graph.access { it.markOffline() } }
    override fun beginEdit(id: String, completed: () -> Unit) = task {
        editSession = graph.access { it.beginEdit(id) }
        withContext(Dispatchers.Main) { completed() }
    }
    override fun save(form: ExpenseForm) = saveOperation(form.operationId) {
        graph.access { service ->
            val matchingPayee = service.state().payees.find { it.name.equals(form.payee.trim(), ignoreCase = true) }
            service.saveExpense(ExpenseDraft(form.amount, form.date.toString(), form.payee, matchingPayee?.id, form.categoryId, form.notes,
                creationId = form.operationId.takeIf { form.id == null }),
                if (form.id == null) null else editSession?.takeIf { it.transactionId == form.id }
                    ?: error("Reopen this expense before editing it."))
        }
    }
    override fun delete(id: String, operationId: String) = saveOperation(operationId) {
        val session = editSession?.takeIf { it.transactionId == id } ?: error("Reopen this expense before deleting it.")
        graph.access { it.deleteExpense(session) }
    }
    private fun saveOperation(id: String, action: suspend () -> Unit) {
        if (saveState.value.saving != null) return
        saveState.value = SaveState(saving = id)
        task {
            try {
                action()
                saveState.value = SaveState(completed = id)
                graph.schedulePending()
                // UI observes completion by operation id; never invoke a disposed editor's closure.
                withContext(Dispatchers.Main) { refresh() }
            } finally {
                if (saveState.value.saving == id) saveState.value = SaveState()
            }
        }
    }
    override fun dismissMessage() {
        localError.value = null
        viewModelScope.launch(Dispatchers.IO) { runCatching { graph.access { it.dismissNotice() } } }
    }

    private fun AppState.toUi(chooseAccount: Boolean, busy: Boolean, localError: String?): ExpenseUiState = ExpenseUiState(
        screen = when {
            budgetId == null && !authenticated -> Screen.LOGIN
            budgetId == null -> Screen.BUDGETS
            selectedAccountId == null || chooseAccount -> Screen.ACCOUNTS
            else -> Screen.EXPENSES
        }, server = server,
        budgets = budgets.map { Choice(it.fileId, it.name) },
        accounts = accounts.map { Choice(it.id, it.name) },
        selectedAccount = accounts.find { it.id == selectedAccountId }?.let { Choice(it.id, it.name) },
        expenses = expenses.map { ExpenseRow(it.id, it.amountMinor, LocalDate.parse(it.date), it.payee, it.categoryId, it.category, it.notes, it.pending, it.pendingDeletion, it.editable) },
        payees = payees.map { Choice(it.id, it.name) }, categories = categories.map { Choice(it.id, it.name) },
        associations = payeeCategoryAssociations, currency = currency, decimalPlaces = decimalPlaces,
        online = online, busy = busy, pendingCount = pendingCount, error = localError ?: error, notice = notice,
        requiresSignIn = budgetId != null && !authenticated,
        canCancelSignIn = budgetId != null,
    )
}

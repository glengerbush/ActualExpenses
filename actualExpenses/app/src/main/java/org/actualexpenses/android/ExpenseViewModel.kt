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
    private val selection = MutableStateFlow<Screen?>(null)
    private val busy = MutableStateFlow(false)
    private val localError = MutableStateFlow<String?>(null)
    private data class SaveState(val saving: String? = null, val completed: String? = null)
    private val saveState = MutableStateFlow(SaveState())
    private var refreshJob: Job? = null
    private var recoveredPending = false
    private var exportedRecovery: String? = null
    val state = combine(graph.state, selection, busy, localError) { state, selection, busy, error ->
        state.toUi(busy, error).let { if (selection != null && !state.legacyRecovery && state.scopeRecovery == null) it.copy(screen = selection) else it }
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

    override fun connect(server: String) = task {
        graph.access { it.connect(server) }
        selection.value = null
        graph.schedulePending()
        if (graph.state.value.budgetId != null) withContext(Dispatchers.Main) { refresh() }
    }
    override fun recoverLegacyBudget(id: String) = task {
        graph.access { it.recoverLegacyBudget(id) }
        selection.value = null
        graph.schedulePending()
        // Legacy identity recovery resumes the existing expense flow.
        withContext(Dispatchers.Main) { refresh() }
    }
    override fun exportRecovery(destination: String, completed: () -> Unit) = task {
        val snapshot = graph.access { it.exportScopeRecovery() }
        val resolver = getApplication<Application>().contentResolver
        requireNotNull(resolver.openOutputStream(android.net.Uri.parse(destination), "wt")) {
            "The recovery file could not be opened. Saved work remains on this phone."
        }.use { it.write(snapshot.toByteArray(Charsets.UTF_8)); it.flush() }
        exportedRecovery = snapshot
        withContext(Dispatchers.Main) { completed() }
    }
    override fun openCurrentAfterExport() = task {
        val snapshot = requireNotNull(exportedRecovery) { "Save a recovery file before continuing." }
        graph.access { it.openCurrentAfterExport(snapshot) }
        exportedRecovery = null
        selection.value = null
    }
    override fun showServer() { selection.value = Screen.SERVER }
    override fun cancelConnection() { selection.value = null }
    override fun retryScheduledWork()=task{graph.access{it.retryScheduledWork()}}
    override fun cancelScheduledWork()=task{graph.access{it.cancelScheduledWork()}}
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
    override fun beginEdit(id: String, completed: (EditSession) -> Unit) = task {
        val session = graph.access { it.beginEdit(id) }
        withContext(Dispatchers.Main) { completed(session) }
    }
    override fun save(form: ExpenseForm) = saveOperation(form.operationId) {
        graph.access { service ->
            val matchingPayee = service.state().payees.find { it.name.equals(form.payee.trim(), ignoreCase = true) }
            service.saveExpense(ExpenseDraft(form.amount, form.date.toString(), form.payee, matchingPayee?.id, form.categoryId, form.notes,
                creationId = form.operationId.takeIf { form.id == null },context=form.context), form.edit)
        }
    }
    override fun delete(session: EditSession, operationId: String) = saveOperation(operationId) {
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
    override fun discardRejectedSplitEdit() = task { graph.access { it.discardRejectedSplitEdit() }; withContext(Dispatchers.Main) { refresh() } }
    override fun dismissMessage() {
        localError.value = null
        viewModelScope.launch(Dispatchers.IO) { runCatching { graph.access { it.dismissNotice() } } }
    }

    private fun AppState.toUi(busy: Boolean, localError: String?): ExpenseUiState = ExpenseUiState(
        screen = when {
            legacyRecovery -> Screen.RECOVERY
            scopeRecovery != null -> Screen.SAVED_WORK
            budgetId == null -> Screen.SERVER
            else -> Screen.EXPENSES
        }, server = server,
        budgets = budgets.map { Choice(it.fileId, it.name) },
        expenses = expenses.map { ExpenseRow(it.id, it.amountMinor, LocalDate.parse(it.date), it.payee, it.categoryId, it.category, it.notes, it.pending, it.pendingDeletion, it.editable) },
        payees = payees.map { Choice(it.id, it.name) }, categories = categories.map { Choice(it.id, it.name) },
        associations = payeeCategoryAssociations, currency = currency, decimalPlaces = decimalPlaces,
        online = online, busy = busy, pendingCount = pendingCount, error = if (legacyRecovery) null else localError ?: error, notice = notice,
        recoveryMessage = if (legacyRecovery) localError ?: error else scopeRecovery,
        canCancelConnection = budgetId != null, expenseContext=expenseContext, splitRecovery=splitRecovery,
        scheduledWorkPending=scheduledWorkPending,scheduledWorkError=scheduledWorkError,
    )
}

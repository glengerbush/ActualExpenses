package org.actualexpenses.core

import org.actualexpenses.core.protocol.RemoteBudget
import java.time.LocalDate

/** Store must replace its contents atomically; Android encrypts this payload with Keystore. */
interface StateStore { fun read(): String?; fun write(value: String) }
data class Choice(val id: String, val name: String)
data class Expense(
    val id: String, val accountId: String, val amountMinor: Long, val date: String,
    val payeeId: String?, val payee: String, val categoryId: String?, val category: String,
    val notes: String, val pending: Boolean, val pendingDeletion: Boolean, val editable: Boolean,
)
data class ExpenseDraft(
    val amount: String, val date: String = LocalDate.now().toString(),
    val payee: String = "", val payeeId: String? = null, val categoryId: String? = null,
    val notes: String = "",
    val creationId: String? = null,
)
/** Issued when editing begins; preserves permission if the connection drops during editing. */
data class EditSession(val transactionId: String, val beganOnline: Boolean, val original: ExpenseDraft? = null)
data class AppState(
    val server: String = "", val authenticated: Boolean = false,
    val budgets: List<RemoteBudget> = emptyList(), val budgetId: String? = null,
    val accounts: List<Choice> = emptyList(), val selectedAccountId: String? = null,
    val expenses: List<Expense> = emptyList(), val payees: List<Choice> = emptyList(),
    val categories: List<Choice> = emptyList(), val currency: String = "", val decimalPlaces: Int = 2,
    val payeeCategoryAssociations: Map<String, String> = emptyMap(),
    val online: Boolean = false, val pendingCount: Int = 0, val notice: String? = null,
    val error: String? = null,
    val syncBlocked: Boolean = false,
)

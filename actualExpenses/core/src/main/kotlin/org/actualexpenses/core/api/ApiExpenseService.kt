package org.actualexpenses.core.api

import com.google.gson.*
import java.math.BigDecimal
import java.time.LocalDate
import java.util.UUID
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import org.actualexpenses.core.*
import org.actualexpenses.core.protocol.*

private data class BudgetIdentity(val id: String, val epoch: String)

private class UnavailablePendingAccount(message: String) : IllegalArgumentException(message)

private data class NativeOperation(
    val id: String = UUID.randomUUID().toString(),
    val transactionId: String = "",
    val accountId: String = "",
    val method: String = "",
    val path: String = "",
    val body: String? = null,
    val clock: String = ZERO_TIMESTAMP,
    val epoch: String = "",
    val budgetId: String = "",
    val creates: Boolean = false,
    val deletes: Boolean = false,
    val createdOffline: Boolean = false,
    var attempted: Boolean = false,
    var splitRejected: Boolean = false,
)

private data class NativeState(
    val format: String = "actualBudget-v1",
    var server: String = "",
    var budgetId: String? = null,
    var epoch: String = "",
    var accountId: String? = null,
    var ledgerInitialized: Boolean = false,
    var checkpoint: String? = null,
    var node: String = UUID.randomUUID().toString().replace("-", "").takeLast(16),
    var clock: String = ZERO_TIMESTAMP,
    val accounts: MutableMap<String, JsonObject> = mutableMapOf(),
    val payees: MutableMap<String, JsonObject> = mutableMapOf(),
    val categories: MutableMap<String, JsonObject> = mutableMapOf(),
    val associations: MutableMap<String, String> = mutableMapOf(),
    var preferences: JsonObject = JsonObject(),
    val rows: MutableMap<String, JsonObject> = mutableMapOf(),
    val recent: MutableMap<String, List<String>> = mutableMapOf(),
    val retained: MutableMap<String, String> = mutableMapOf(),
    val pending: MutableList<NativeOperation> = mutableListOf(),
    var scheduledCommand: ScheduledCommand? = null,
    var scopeRecovery: String? = null,
    var recoveryArchive: String? = null,
    var notice: String? = null,
)

/** Current native expense workflow with durable offline delivery. */
class ApiExpenseService(
    private val store: StateStore,
    private val transport: HttpTransport = BudgetHttpTransport(),
) {
    private val gson = GsonBuilder().serializeNulls().create()
    @Volatile
    private var saved =
        store.read()?.let {
            require(
                JsonParser.parseString(it).asJsonObject["format"]?.asString == "actualBudget-v1"
            ) {
                "Saved API data is incompatible. Keep app data intact."
            }
            gson.fromJson(it, NativeState::class.java)
        } ?: NativeState()
    init { saved.scheduledCommand?.validate() }
    @Volatile private var scheduleError:String? = null
    @Volatile private var online = false
    @Volatile private var error: String? = null
    @Volatile private var blocked = false
    private val syncLock = ReentrantLock()

    private fun ready() {
        check(saved.scopeRecovery == null) { saved.scopeRecovery.orEmpty() }
    }

    private fun client() = BudgetApi(saved.server, transport)

    @Synchronized
    private fun change(update: (NativeState) -> Unit) {
        val previous = gson.toJson(saved)
        val next = gson.fromJson(previous, NativeState::class.java)
        update(next)
        val serialized = gson.toJson(next)
        if (serialized != previous) {
            store.write(serialized)
            saved = next
        }
    }

    @Synchronized
    fun state(): AppState {
        val rows = projected()
        val eligible =
            rows.values
                .filter { eligible(it) }
                .sortedWith(
                    compareByDescending<JsonObject> { it.text("date") }
                        .thenByDescending { orderKey(it) }
                        .thenBy { it.text("id") }
                )
        val pendingIds = saved.pending.map { it.transactionId }.toSet()
        val recent = eligible.take(10)
        val visible = recent.toMutableList()
        eligible.filter { it.text("id") in pendingIds && it !in visible }.forEach(visible::add)
        val retainedIds = retainedIds(saved, saved.budgetId)
        eligible.filter { it.text("id") in retainedIds && it !in visible }.forEach(visible::add)
        saved.pending
            .filter { it.deletes }
            .forEach { op ->
                saved.rows[op.transactionId]
                    ?.takeIf { row -> visible.none { it.text("id") == row.text("id") } }
                    ?.let(visible::add)
            }
        val payees = projectedPayees()
        val currency = saved.preferences.text("currency_code").orEmpty()
        return AppState(
            server = saved.server,
            serverSelected = saved.server.isNotEmpty(),
            expenseContext = if (saved.accountId != null && saved.scopeRecovery == null) captureContext() else null,
            scopeRecovery = saved.scopeRecovery,
            budgetId = saved.budgetId,
            expenses =
                visible.map { row ->
                    val id = row.text("id")!!
                    val payee = row.text("payee_id")
                    val category = row.text("category_id")
                    Expense(
                        id,
                        row.text("account_id").orEmpty(),
                        -row.long("amount"),
                        row.text("date").orEmpty(),
                        payee,
                        payees[payee]?.text("name") ?: row.text("payee_name").orEmpty(),
                        category,
                        saved.categories[category]?.text("name")
                            ?: row.text("category_name").orEmpty(),
                        row.text("notes").orEmpty(),
                        id in pendingIds,
                        saved.pending.any { it.transactionId == id && it.deletes },
                        canEdit(id),
                    )
                },
            payees =
                choices(payees).filter {
                    payees[it.id]?.let { row ->
                        row["deleted"]?.asBoolean != true && row.text("transfer_account_id") == null
                    } == true
                },
            categories =
                choices(saved.categories).filter {
                    saved.categories[it.id]?.let { row ->
                        listOf("is_income", "deleted").none {
                            row[it]?.asBoolean == true
                        }
                    } == true
                },
            payeeCategoryAssociations = saved.associations.filter { (payee, category) ->
                payees[payee]?.let { it["deleted"]?.asBoolean != true && it.text("transfer_account_id") == null } == true &&
                saved.categories[category]?.let { listOf("is_income", "deleted").none { key -> it[key]?.asBoolean == true } } == true
            },
            currency = currency,
            decimalPlaces = if (currency in setOf("IRR", "JPY", "KRW")) 0 else 2,
            online = online,
            pendingCount = saved.pending.map { it.transactionId }.distinct().size,
            notice = saved.notice,
            error = pendingAccountError() ?: error,
            syncBlocked = blocked || saved.scopeRecovery != null || pendingAccountError() != null,
            splitRecovery = saved.pending.firstOrNull()?.splitRejected == true,
            scheduledWorkPending=saved.scheduledCommand!=null,
            scheduledWorkError=saved.scheduledCommand?.reviewMessage?:scheduleError,
        )
    }

    fun connect(server: String) = syncLock.withLock {
        ready()
        val normalized = BudgetApi.normalized(server)
        require(saved.scheduledCommand==null||saved.scheduledCommand?.server==normalized){"Confirm or cancel saved schedule work before switching servers."}
        require(saved.pending.isEmpty() || normalized == saved.server) {
            "Keep this server address while a budget is selected. Saved work belongs to its original server."
        }
        val api = BudgetApi(normalized, transport)
        api.requireSupported()
        val current = currentBudget(api)
        change { next ->
            if (next.server != normalized) {
                require(next.pending.isEmpty()) { "Deliver pending changes before switching servers." }
                clearCache(next)
                next.budgetId = null
                next.accountId = null
                next.epoch = ""
                next.ledgerInitialized = false
            }
            next.server = normalized
        }
        openCurrent(current)
        markOnline()
    }

    private fun currentBudget(api: BudgetApi): BudgetIdentity {
        val row = try {
            api.request("/budget").getAsJsonObject("data")
        } catch (failure: BudgetApiException) {
            if (failure.status == 404) throw IllegalArgumentException("Update actualBudget to open its single budget. Saved work is retained.")
            throw failure
        }
        return BudgetIdentity(row["id"].asString, row["epoch"].asString)
    }

    private fun openCurrent(current: BudgetIdentity) {
        val changed = saved.budgetId != null &&
            (saved.budgetId != current.id || saved.epoch != current.epoch)
        if (changed && (saved.pending.isNotEmpty() || saved.scheduledCommand != null)) {
            val message = "The server budget changed. Saved expenses and schedule work still belong to the previous version. Save a recovery file before opening the current budget."
            change { it.scopeRecovery = message }
            blocked = true
            throw IllegalArgumentException(message)
        }
        change { it.scopeRecovery = null }
        if (saved.budgetId == null || changed || saved.accountId == null)
            bootstrap(current.id)
    }

    /** Complete original state, including immutable requests and earlier recovery archives. */
    @Synchronized
    fun exportScopeRecovery(): String {
        check(saved.scopeRecovery != null) { "Refresh to check whether saved work needs recovery." }
        return gson.toJson(JsonObject().apply {
            addProperty("format", "actualExpenses-recovery-v1")
            add("saved_state", gson.toJsonTree(saved))
        })
    }

    /** Called only after a successful user-directed export and explicit confirmation. */
    fun openCurrentAfterExport(exported: String) = syncLock.withLock {
        check(exportScopeRecovery() == exported) { "Saved work changed. Save a new recovery file before continuing." }
        val api = client()
        api.requireSupported()
        val current = currentBudget(api)
        change { next ->
            next.recoveryArchive = exported
            next.pending.clear()
            next.scheduledCommand = null
            next.scopeRecovery = null
            clearCache(next)
            next.budgetId = null
            next.accountId = null
            next.epoch = ""
            next.ledgerInitialized = false
        }
        // The full archive remains in encrypted app storage even if bootstrap fails.
        openCurrent(current)
        markOnline()
    }


    @Synchronized
    fun beginExpense(): ExpenseContext {
        ready()
        return captureContext()
    }

    private fun captureContext(accountId: String? = saved.accountId): ExpenseContext {
        val currency = saved.preferences.text("currency_code").orEmpty()
        return ExpenseContext(
            saved.server,
            saved.budgetId ?: error("Connect to prepare expenses."),
            accountId ?: error("Connect to prepare this budget."),
            saved.epoch,
            currency,
            if (currency in setOf("IRR", "JPY", "KRW")) 0 else 2,
            saved.preferences.text("number_format") ?: "comma-dot",
        )
    }

    private fun validateContext(context: ExpenseContext) {
        require(
            context.apiFamily == "actualBudget-v1" &&
                context.server == saved.server &&
                context.budgetId == saved.budgetId &&
                (context.accountId == saved.accountId || saved.accounts.containsKey(context.accountId))
        ) {
            "This form belongs to a different budget or account. Its values are retained; return to the original selection before saving."
        }
        require(context.epoch == saved.epoch) {
            "This budget was restored after the form opened. Keep these values and review the restored budget before preparing a new expense."
        }
        require(context.decimalPlaces in setOf(0, 2)) { "Unsupported saved amount format." }
    }

    private fun scopeKey(server: String, budget: String?) =
        "$server\u001factualBudget-v1\u001f$budget"

    private fun retentionKey(budget: String?, account: String?) =
        "${scopeKey(saved.server,budget)}\u001f$account"

    private fun retainedIds(state: NativeState, budget: String?): Collection<String> =
        state.retained.filterKeys { it.startsWith(scopeKey(state.server, budget) + "\u001f") }.values

    private fun clearCache(next: NativeState) {
        next.checkpoint = null
        next.accounts.clear()
        next.payees.clear()
        next.categories.clear()
        next.rows.clear()
        next.recent.clear()
        next.preferences = JsonObject()
    }

    @Synchronized
    fun markOffline() {
        online = false
    }

    @Synchronized
    fun dismissNotice() {
        change { it.notice = null }
        error = null
    }

    @Synchronized
    fun beginEdit(id: String): EditSession {
        require(canEdit(id)) { "Connect before editing this expense." }
        val view = state()
        val expense =
            view.expenses.singleOrNull { it.id == id && !it.pendingDeletion }
                ?: error("This expense is no longer editable.")
        return EditSession(
            id,
            online,
            ExpenseDraft(
                BigDecimal.valueOf(expense.amountMinor)
                    .movePointLeft(view.decimalPlaces)
                    .toPlainString(),
                expense.date,
                expense.payee,
                expense.payeeId,
                expense.categoryId,
                expense.notes,
            ),
            captureContext(expense.accountId),
        )
    }

    @Synchronized
    fun saveExpense(draft: ExpenseDraft, edit: EditSession? = null): String {
        ready()
        val view = state()
        val account = edit?.context?.accountId ?: draft.context?.accountId ?: saved.accountId ?: error("Connect to prepare this budget.")
        val budget = saved.budgetId ?: error("Connect to prepare expenses.")
        require(saved.accounts[account]?.let(::writableAccount) == true) {
            "This account is unavailable. Saved work is retained."
        }
        val context =
            if (edit != null)
                requireNotNull(edit.context) {
                    "Reopen this expense to capture its original budget and amount format."
                }
            else draft.context ?: captureContext()
        validateContext(context)
        val amount = parseExpenseAmount(draft.amount, context.decimalPlaces)
        require(amount <= 9_007_199_254_740_991L) { "Amount exceeds the supported exact range." }
        val date = LocalDate.parse(draft.date).toString()
        require(date.length == 10 && !date.startsWith("0000")) { "Choose a valid date." }
        val id = edit?.transactionId ?: draft.creationId ?: UUID.randomUUID().toString()
        require(UUID.fromString(id).toString() == id) { "Invalid expense identity." }
        if (edit == null && projected().containsKey(id)) return id
        if (edit != null)
            require(
                (edit.beganOnline || canEdit(id)) &&
                    projected()[id]?.let { eligible(it) } ==
                        true
            ) {
                "This expense cannot be edited here."
            }
        require(
            draft.categoryId == null ||
                view.categories.any { it.id == draft.categoryId } ||
                edit?.original?.categoryId == draft.categoryId
        ) {
            "Choose an available category."
        }
        require(draft.notes.codePointCount(0, draft.notes.length) <= 10000) { "Notes must contain at most 10000 characters." }
        val original = edit?.original
        val body = JsonObject()
        if (original == null || original.notes != draft.notes) body.addProperty("notes", draft.notes)
        if (edit == null) {
            body.addProperty("id", id)
            body.addProperty("account_id", account)
            body.addProperty("cleared", true)
        }
        if (
            original == null || parseExpenseAmount(original.amount, context.decimalPlaces) != amount
        )
            body.addProperty("amount", -amount)
        if (original == null || original.date != date) body.addProperty("date", date)
        if (original == null || original.categoryId != draft.categoryId)
            body.add("category_id", nullable(draft.categoryId))
        if (original == null || original.payee != draft.payee.trim()) {
            val name = draft.payee.trim()
            require(name.length <= 120 && name.none { it.isISOControl() }) {
                "Enter a payee of at most 120 characters."
            }
            val matching = view.payees.find { it.name.equals(name, true) }
            if (name.isEmpty() || matching != null) body.add("payee_id", nullable(matching?.id))
            else
                body.add(
                    "new_payee",
                    JsonObject().apply {
                        addProperty("id", UUID.randomUUID().toString())
                        addProperty("name", name)
                    },
                )
        }
        if (body.size() == 0) return id
        change { next ->
            val clock = LogicalClock(next.node, next.clock).next()
            next.pending +=
                NativeOperation(
                    transactionId = id,
                    accountId = account,
                    method = if (edit == null) "POST" else "PATCH",
                    path = "/budgets/$budget/transactions" + if (edit == null) "" else "/$id",
                    body = body.toString(),
                    clock = clock,
                    epoch = context.epoch,
                    budgetId = budget,
                    creates = edit == null,
                    createdOffline = edit == null && !online,
                )
            next.clock = clock
            if (edit == null) {
                next.retained.keys.removeAll { it.startsWith(scopeKey(next.server, budget) + "\u001f") }
                next.retained[retentionKey(budget, account)] = id
            }
        }
        return id
    }

    @Synchronized
    fun deleteExpense(edit: EditSession) {
        ready()
        validateContext(requireNotNull(edit.context) { "Reopen this expense before deleting it." })
        val id = edit.transactionId
        require(edit.beganOnline || canEdit(id)) { "Connect before deleting this expense." }
        require(projected()[id]?.let(::eligible) == true) { "This expense is no longer editable." }
        change { next ->
            val related = next.pending.filter { it.transactionId == id }
            if (related.any { it.creates } && related.none { it.attempted }) {
                val references = related.mapNotNull {
                    it.body?.let(JsonParser::parseString)?.asJsonObject
                        ?.getAsJsonObject("new_payee")
                }
                next.pending.removeAll { it.transactionId == id }
                // An unsent dependent expense takes ownership of an unsent inline
                // payee when the original offline expense is explicitly removed.
                references.forEach { reference ->
                    val index =
                        next.pending.indexOfFirst { op ->
                            !op.attempted &&
                                op.body?.let {
                                    JsonParser.parseString(it).asJsonObject["payee_id"]
                                } == reference["id"]
                        }
                    if (index >= 0) {
                        val op = next.pending[index]
                        val body = JsonParser.parseString(op.body).asJsonObject
                        body.remove("payee_id")
                        body.add("new_payee", reference)
                        next.pending[index] =
                            op.copy(id = UUID.randomUUID().toString(), body = body.toString())
                    }
                }
            } else {
                val clock = LogicalClock(next.node, next.clock).next()
                next.pending +=
                    NativeOperation(
                        transactionId = id,
                        accountId = edit.context!!.accountId,
                        method = "DELETE",
                        path = "/budgets/${next.budgetId}/transactions/$id",
                        clock = clock,
                        epoch = next.epoch,
                        budgetId = next.budgetId!!,
                        deletes = true,
                    )
                next.clock = clock
            }
        }
    }

    fun refresh(foreground: Boolean = false) {
        if (saved.server.isEmpty() || !syncLock.tryLock()) return
        try {
            val api = client()
            // A selected compatible API is checked without login. Compatibility
            // failures never touch durable outbox contents.
            api.requireSupported()
            var scheduleResolved=false
            if(foreground && saved.scheduledCommand!=null){
                try{scheduleResolved=reconcileScheduledReceipt(api)}catch(failure:Exception){scheduleFailure(failure)}
            }
            val current = currentBudget(api)
            if (saved.budgetId == current.id) {
                try { reconcileReceipts(api) }
                catch (failure: BudgetApiException) {
                    // A retired generation may no longer offer the old receipt. Keep
                    // the immutable request and expose export recovery below.
                    if (saved.epoch == current.epoch || failure.status !in 400..499) throw failure
                }
            }
            openCurrent(current)
            // Refresh reference availability before delivering captured work so known
            // deleted accounts hold their expenses for explicit recovery.
            val budget = saved.budgetId!!
            if (!saved.ledgerInitialized)
                bootstrap(budget, includeView = saved.pending.isEmpty())
            refreshBudget(api, budget, includeView = saved.pending.isEmpty())
            requirePendingAccounts()
            var delivered = false
            while (true) {
                val op =
                    synchronized(this) {
                        saved.pending.firstOrNull()?.also { first ->
                            change { next ->
                                next.pending.first { it.id == first.id }.attempted = true
                            }
                        }
                    } ?: break
                try {
                    val result = api.request(op.path, op.method, op.body, op.id, op.clock, op.epoch)
                    acknowledge(op, result)
                } catch (failure: BudgetApiException) {
                    if (failure.code == "deletion_superseded") {
                        change {
                            it.pending.removeAll { pending -> pending.id == op.id }
                            it.notice = "Deletion cancelled: a newer edit kept this expense."
                        }
                    } else {
                        if (failure.code in setOf("split_transaction", "transfer_transaction")) change { it.pending.first { pending -> pending.id == op.id }.splitRejected = true }
                        throw failure
                    }
                }
                delivered = true
            }
            if (delivered) refreshBudget(api, budget)
            if(foreground && !scheduleResolved && saved.pending.isEmpty()){
                try{if(runScheduledForeground(api,budget))refreshBudget(api,budget)}catch(failure:Exception){scheduleFailure(failure)}
            }
            markOnline()
        } catch (failure: Exception) {
            online = failure is UnavailablePendingAccount
            error = failure.message ?: "Could not connect. Saved work is retained."
            blocked =
                failure is BudgetApiException && failure.status in 400..499 ||
                    failure is IllegalArgumentException
        } finally {
            syncLock.unlock()
        }
    }


    private fun scheduledScope(command: ScheduledCommand) {
        require(command.server == saved.server && command.budget == saved.budgetId) {
            "Saved schedule work belongs to its original server and budget."
        }
    }

    private fun finishScheduled(command: ScheduledCommand, cancelled: Boolean = false) {
        change { next ->
            check(next.scheduledCommand?.id == command.id)
            next.scheduledCommand = null
            if (cancelled)
                next.notice =
                    "The saved schedule request was cancelled. Review automatic schedules in actualBudget."
        }
        scheduleError = null
    }

    /** Proof lookup precedes generation bootstrap, and can never run unknown work. */
    private fun reconcileScheduledReceipt(api: BudgetApi): Boolean {
        val command = saved.scheduledCommand ?: return false
        scheduledScope(command)
        if (!command.attempted) return false
        return try {
            api.request(
                command.path,
                "POST",
                command.body,
                command.id,
                command.clock,
                command.epoch,
                receiptOnly = true,
            )
            finishScheduled(command)
            true
        } catch (failure: BudgetApiException) {
            when {
                failure.status == 404 && failure.code == "operation_uncommitted" -> false
                failure.status == 409 && failure.code == "operation_canceled" -> {
                    finishScheduled(command, true)
                    true
                }
                else -> throw failure
            }
        }
    }

    private fun scheduleFailure(failure: Exception) {
        val message =
            failure.message ?: "Scheduled entries could not be confirmed. Refresh to retry."
        scheduleError = message
        if (
            failure is IllegalArgumentException ||
                failure is BudgetApiException && failure.status in 400..499
        ) {
            change { next -> next.scheduledCommand?.reviewMessage = message }
        }
    }

    /** Only explicit foreground calls may prepare or deliver this separate command. */
    private fun runScheduledForeground(api: BudgetApi, budget: String): Boolean {
        if (saved.scheduledCommand == null) {
            val server = saved.server
            val epoch = saved.epoch
            val due = api.request("/budgets/$budget/schedules/due").getAsJsonObject("data")
            scheduleError = null
            if (!due["due"].asBoolean) return false
            require(due["epoch"].asString == epoch) {
                "The budget changed. Refresh before checking scheduled entries."
            }
            val body =
                JsonObject()
                    .apply {
                        addProperty("action", "catch-up")
                        addProperty("through", due["today"].asString)
                    }
                    .toString()
            change { next ->
                check(
                    next.server == server &&
                        next.budgetId == budget &&
                        next.epoch == epoch &&
                        next.scheduledCommand == null
                )
                val clock = LogicalClock(next.node, next.clock).next()
                next.scheduledCommand =
                    ScheduledCommand(
                        UUID.randomUUID().toString(),
                        server,
                        budget,
                        "/budgets/$budget/schedules/commands",
                        body,
                        clock,
                        epoch,
                    )
                next.clock = clock
            }
        }
        val command = requireNotNull(saved.scheduledCommand)
        scheduledScope(command)
        if (command.reviewMessage != null) return false
        require(saved.epoch == command.epoch) {
            "This budget was restored. Cancel the saved schedule request before reviewing the restored schedules in actualBudget."
        }
        change { next ->
            check(next.scheduledCommand?.id == command.id)
            next.scheduledCommand?.attempted = true
        }
        try {
            api.request(
                command.path,
                "POST",
                command.body,
                command.id,
                command.clock,
                command.epoch,
            )
            finishScheduled(command)
        } catch (failure: BudgetApiException) {
            if (failure.code == "operation_canceled") finishScheduled(command, true)
            else throw failure
        }
        return true
    }

    fun retryScheduledWork() {
        syncLock.withLock {
            ready()
            scheduleError = null
            change { next -> next.scheduledCommand?.reviewMessage = null }
        }
        refresh(foreground = true)
    }

    /** A receipt or an exact durable refusal is required before local cancellation. */
    fun cancelScheduledWork() {
        syncLock.withLock {
            ready()
            val command = saved.scheduledCommand ?: return@withLock
            val api = client()
            api.requireSupported()
                scheduledScope(command)
            if (reconcileScheduledReceipt(api)) return@withLock
            try {
                api.request(
                    command.path,
                    "POST",
                    command.body,
                    command.id,
                    command.clock,
                    command.epoch,
                    cancelIfUncommitted = true,
                )
                finishScheduled(command)
            } catch (failure: BudgetApiException) {
                if (failure.code == "operation_canceled") finishScheduled(command, true)
                else throw failure
            }
        }
        refresh()
    }

    private fun reconcileReceipts(api: BudgetApi) {
        // A receipt proves an earlier outcome; probing must never apply or cancel
        // an unknown request, even if its account or generation has since changed.
        for (op in saved.pending.filter { it.attempted }) {
            try {
                acknowledge(
                    op,
                    api.request(
                        op.path,
                        op.method,
                        op.body,
                        op.id,
                        op.clock,
                        op.epoch,
                        receiptOnly = true,
                    ),
                )
            } catch (failure: BudgetApiException) {
                when {
                    failure.status == 404 && failure.code == "operation_uncommitted" -> Unit
                    failure.code == "deletion_superseded" ->
                        change {
                            it.pending.removeAll { pending -> pending.id == op.id }
                            it.notice = "Deletion cancelled: a newer edit kept this expense."
                        }
                    else -> {
                        if (failure.code in setOf("split_transaction", "transfer_transaction")) change { it.pending.first { pending -> pending.id == op.id }.splitRejected = true }
                        throw failure
                    }
                }
            }
        }
    }

    /** Only a durable refusal for this exact original request permits discarding it. */
    fun discardRejectedSplitEdit() = syncLock.withLock {
        ready()
        val op = saved.pending.firstOrNull() ?: return@withLock
        require(op.splitRejected) { "Refresh to verify the rejected expense change first." }
        val api = BudgetApi(saved.server, transport)
        try {
            acknowledge(op, api.request(op.path, op.method, op.body, op.id, op.clock, op.epoch, receiptOnly = true))
        } catch (failure: BudgetApiException) {
            if (failure.code !in setOf("split_transaction", "transfer_transaction")) throw failure
            change {
                it.pending.removeAll { pending -> pending.id == op.id }
                it.rows.remove(op.transactionId)
                it.notice = "Rejected expense change discarded. Review the transaction in actualBudget."
            }
        }
        blocked = false
        error = null
    }

    private fun refreshBudget(api: BudgetApi, budget: String, includeView: Boolean = true) {
        if (saved.checkpoint == null || saved.accounts[saved.accountId]?.let(::availableAccount) != true)
            bootstrap(budget, includeView)
        else
            try {
                refreshChanges(api, budget, includeView)
                if (saved.accounts[saved.accountId]?.let(::availableAccount) != true)
                    bootstrap(budget, includeView)
            } catch (failure: BudgetApiException) {
                if (failure.code == "cursor_invalid" && saved.pending.isEmpty())
                    bootstrap(budget, includeView)
                else throw failure
            }
    }

    private fun requirePendingAccounts() {
        pendingAccountError()?.let { throw UnavailablePendingAccount(it) }
    }

    private fun pendingAccountError(): String? =
        saved.pending
            .firstOrNull { saved.accounts[it.accountId]?.let(::writableAccount) != true }
            ?.let { op ->
                val account = saved.accounts[op.accountId]
                "Account ${account?.text("name") ?: op.accountId} is deleted. Pending expenses retain their original account and delivery identity. Keep app data intact and reconcile this saved work with the original budget in actualBudget before continuing."
            }

    private fun acknowledge(op: NativeOperation, result: JsonObject) {
        val row = result.getAsJsonObject("data")
        change { next ->
            next.pending.removeAll { it.id == op.id }
            if (row?.text("id") == op.transactionId) {
                if (row["deleted"]?.asBoolean == true || op.deletes)
                    next.rows.remove(op.transactionId)
                else next.rows[op.transactionId] = row.deepCopy()
                observe(next, row)
            }
            val conflicts = result.getAsJsonArray("conflicts")
            if (conflicts?.any { it.asJsonObject["outcome"]?.asString == "deletion_superseded" } == true)
                next.notice = "Deletion cancelled: a newer edit kept this expense."
            else if (conflicts?.size()?.let { it > 0 } == true)
                next.notice = "A newer edit from another device replaced a field."
            if (row?.get("deleted")?.asBoolean == true && !op.deletes)
                next.notice = "This expense remains deleted on the server."
        }
    }

    private fun bootstrap(budget: String, includeView: Boolean = true) {
        val api = client()
        val epoch = api.request("/budgets/$budget/checkpoint").getAsJsonObject("data")["epoch"].asString
        val destination = api.request("/budgets/$budget/ledger", "POST", "{}", epoch = epoch).getAsJsonObject("data")["id"].asString
        repeat(3) {
            try {
                val checkpoint = api.request("/budgets/$budget/checkpoint").getAsJsonObject("data")
                val snapshot = checkpoint["snapshot"].asString
                val epoch = checkpoint["epoch"].asString
                val suffix = "snapshot=${BudgetApi.query(snapshot)}"
                val accounts =
                    api.request("/budgets/$budget/accounts?$suffix").getAsJsonArray("data").map {
                        it.asJsonObject
                    }
                val payees = references(api, budget, "payees", suffix)
                val categories = references(api, budget, "categories", suffix)
                val associations = associations(api, budget, snapshot)
                val prefs =
                    api.request("/budgets/$budget/preferences?$suffix").getAsJsonObject("data")
                val selected = destination.takeIf { id ->
                    accounts.any { it.text("id") == id && availableAccount(it) }
                }
                val view =
                    if (includeView) selected?.let { companion(api, budget, snapshot) }
                    else null
                change { next ->
                    require(
                        next.pending.isEmpty() || (next.epoch == epoch && next.budgetId == budget)
                    ) {
                        "This budget was restored. Saved work needs reconciliation before continuing."
                    }
                    if (next.budgetId != budget || next.epoch != epoch) {
                        next.rows.clear()
                        next.recent.clear()
                    }
                    next.budgetId = budget
                    next.ledgerInitialized = true
                    next.epoch = epoch
                    next.accountId =
                        selected
                            ?: next.accountId?.takeIf { id -> next.pending.any { it.accountId == id } }
                    next.checkpoint = snapshot
                    val unavailable =
                        next.accounts.filterKeys { id -> next.pending.any { it.accountId == id } }
                    next.accounts.clear()
                    unavailable.forEach { (id, row) ->
                        next.accounts[id] = row.deepCopy().apply { addProperty("deleted", true) }
                    }
                    accounts.forEach { next.accounts[it.text("id")!!] = it }
                    next.payees.clear()
                    payees.forEach { next.payees[it.text("id")!!] = it }
                    next.categories.clear()
                    categories.forEach { next.categories[it.text("id")!!] = it }
                    next.associations.clear()
                    next.associations.putAll(associations)
                    next.preferences = preferences(prefs)
                    if (view != null) installView(next, selected!!, view)
                }
                markOnline()
                return
            } catch (failure: BudgetApiException) {
                if (failure.code != "snapshot_changed") throw failure
            }
        }
        throw IllegalStateException(
            "The budget is changing. Retry when it settles; saved work is retained."
        )
    }

    private fun refreshChanges(api: BudgetApi, budget: String, includeView: Boolean = true) {
        repeat(3) {
            val start = saved.checkpoint!!
            var cursor = start
            val changes = mutableListOf<JsonObject>()
            do {
                val result =
                    api.request(
                        "/budgets/$budget/changes?cursor=${BudgetApi.query(cursor)}&limit=1000"
                    )
                changes += result.getAsJsonArray("data").map { it.asJsonObject }
                cursor = result["next_cursor"].asString
            } while (result["has_more"].asBoolean)
            val accounts = saved.accounts.toMutableMap()
            changes
                .filter { it["entity"].asString == "accounts" }
                .forEach {
                    accounts[it["id"].asString] = it.getAsJsonObject("data")
                }
            val account = saved.accountId?.takeIf { accounts[it]?.let(::availableAccount) == true }
            try {
                // Category responses include inherited group visibility. Refresh only
                // this reference set at the same guarded cursor when its parent changes.
                val categories =
                    if (changes.any { it["entity"].asString == "category_groups" })
                        references(api, budget, "categories", "snapshot=${BudgetApi.query(cursor)}")
                    else null
                val associations = if (changes.any { it["entity"].asString in setOf("rules", "rule_settings", "payees", "categories", "category_groups") }) associations(api,budget,cursor) else null
                val view =
                    if (
                        includeView &&
                            account != null &&
                            (changes.isNotEmpty() || !saved.recent.containsKey(account))
                    )
                        companion(api, budget, cursor)
                    else null
                val clearSelection =
                    saved.accountId != null &&
                        account == null &&
                        saved.pending.none { it.accountId == saved.accountId }
                if (changes.isEmpty() && view == null && !clearSelection) return
                change { next ->
                    changes.forEach { item ->
                        val id = item["id"].asString
                        val row = item.getAsJsonObject("data")
                        when (item["entity"].asString) {
                            "accounts" -> next.accounts[id] = row
                            "payees" -> next.payees[id] = row
                            "categories" -> next.categories[id] = row
                            "preferences" -> next.preferences = preferences(row)
                            "transactions" -> {
                                observe(next, row)
                                if (row["deleted"]?.asBoolean == true) {
                                    if (next.pending.none { it.transactionId == id })
                                        next.rows.remove(id)
                                } else if (next.rows.containsKey(id)) next.rows[id] = row
                            }
                        }
                    }
                    if (associations != null) {
                        next.associations.clear()
                        next.associations.putAll(associations)
                    }
                    if (categories != null) {
                        next.categories.clear()
                        categories.forEach { next.categories[it.text("id")!!] = it }
                    }
                    if (account == null && next.pending.none { it.accountId == next.accountId }) {
                        next.accountId = null
                    }
                    if (view != null) installView(next, account!!, view)
                    next.checkpoint = cursor
                }
                return
            } catch (failure: BudgetApiException) {
                if (failure.code != "snapshot_changed") throw failure
            }
        }
        throw IllegalStateException(
            "The budget is changing. Refresh again; saved work is retained."
        )
    }

    private fun companion(
        api: BudgetApi,
        budget: String,
        snapshot: String,
    ): JsonObject {
        val retained =
            synchronized(this) {
                (saved.pending
                        .filter { it.budgetId == budget }
                        .map { it.transactionId } +
                        retainedIds(saved, budget))
                    .distinct()
            }
        require(retained.size <= 100) {
            "Deliver pending expenses before loading more than 100 retained entries."
        }
        return api.request(
                "/budgets/$budget/companion?retain=${BudgetApi.query(retained.joinToString(","))}&snapshot=${BudgetApi.query(snapshot)}"
            )
            .getAsJsonObject("data")
    }

    private fun installView(next: NativeState, account: String, view: JsonObject) {
        val keep =
            next.pending.map { it.transactionId }.toSet() +
                retainedIds(next, next.budgetId)
        next.rows.entries.removeAll { it.key !in keep }
        view.getAsJsonArray("unavailable").forEach {
            if (it.asString !in next.pending.map { op -> op.transactionId })
                next.rows.remove(it.asString)
        }
        val recent = view.getAsJsonArray("recent").map { it.asJsonObject }
        (recent + view.getAsJsonArray("retained").map { it.asJsonObject }).forEach { row ->
            next.rows[row.text("id")!!] = row
            observe(next, row)
        }
        next.recent[account] = recent.map { it.text("id")!! }
    }

    private fun associations(api:BudgetApi,budget:String,snapshot:String):Map<String,String> {
        val data=api.request("/budgets/$budget/rules/associations?snapshot=${BudgetApi.query(snapshot)}").getAsJsonObject("data")
        require(data.size()<=100000) { "Too many category associations for this device." }
        return data.entrySet().associate { (payee,value) ->
            val category=value.asString
            require(UUID.fromString(payee).toString()==payee && UUID.fromString(category).toString()==category) { "Invalid cached category association." }
            payee to category
        }
    }

    private fun references(
        api: BudgetApi,
        budget: String,
        kind: String,
        suffix: String,
    ): List<JsonObject> {
        val rows = mutableListOf<JsonObject>()
        var offset = 0
        do {
            val page = api.request("/budgets/$budget/$kind?$suffix&limit=1000&offset=$offset")
            rows += page.getAsJsonArray("data").map { it.asJsonObject }
            require(rows.size <= 100000) { "Too many reference entries for this device." }
            val next = page["next_offset"]
            offset = if (next == null || next.isJsonNull) -1 else next.asInt
        } while (offset >= 0)
        return rows
    }

    // Historical closed accounts are still editable in a unified history.
    // Deleted destinations remain blocked so queued writes cannot resurrect them.
    private fun writableAccount(row: JsonObject) =
        row["deleted"]?.asBoolean != true

    private fun availableAccount(row: JsonObject) =
        row["closed"]?.asBoolean != true && row["deleted"]?.asBoolean != true

    private fun preferences(value: JsonObject) = value.getAsJsonObject("settings") ?: value

    private fun markOnline() {
        online = true
        error = null
        blocked = false
    }

    private fun observe(next: NativeState, row: JsonObject) {
        val newest =
            row.getAsJsonObject("field_versions")?.entrySet()?.maxOfOrNull { it.value.asString }
        if (newest != null && newest > next.clock) {
            val clock = LogicalClock(next.node, next.clock)
            clock.observe(newest)
            next.clock = clock.state
        }
    }

    private fun canEdit(id: String) =
        online || saved.pending.any { it.transactionId == id && it.creates && it.createdOffline }

    private fun projectedPayees(): MutableMap<String, JsonObject> =
        saved.payees.toMutableMap().apply {
            saved.pending
                .mapNotNull {
                    it.body?.let(JsonParser::parseString)?.asJsonObject
                        ?.getAsJsonObject("new_payee")
                }
                .forEach { put(it["id"].asString, it) }
        }

    private fun projected(): MutableMap<String, JsonObject> =
        saved.rows
            .mapValues { it.value.deepCopy() }
            .toMutableMap()
            .apply {
                saved.pending.forEach { op ->
                    if (!op.deletes) {
                        val row =
                            getOrPut(op.transactionId) {
                                JsonObject().apply {
                                    addProperty("id", op.transactionId)
                                    addProperty("account_id", op.accountId)
                                    addProperty("sort_order", Long.MAX_VALUE)
                                    addProperty("reconciled", false)
                                }
                            }
                        op.body?.let(JsonParser::parseString)?.asJsonObject?.let {
                            it.entrySet().forEach {
                                (field, value) ->
                                if (field == "new_payee") {
                                    row.add("payee_id", value.asJsonObject["id"])
                                    row.add("payee_name", value.asJsonObject["name"])
                                } else row.add(field, value)
                            }
                        }
                    }
                }
            }

    private fun choices(rows: Map<String, JsonObject>) =
        rows
            .map { Choice(it.key, it.value.text("name").orEmpty()) }
            .sortedBy { it.name.lowercase() }

    private fun eligible(row: JsonObject) =
        row.long("amount") < 0 &&
            row["is_parent"]?.asBoolean != true &&
            row.text("parent_id") == null &&
            row["reconciled"]?.asBoolean != true &&
            row.text("transfer_id") == null &&
            row["deleted"]?.asBoolean != true

    private fun nativeOrderKey(rank: Long) =
        java.lang.Long.toUnsignedString(rank xor Long.MIN_VALUE, 16).padStart(16, '0')

    private fun orderKey(row: JsonObject) =
        row.text("order_key") ?: nativeOrderKey(row.long("sort_order"))

    private fun JsonObject.text(key: String) = get(key)?.takeUnless { it.isJsonNull }?.asString

    private fun JsonObject.long(key: String) = get(key)?.takeUnless { it.isJsonNull }?.asLong ?: 0L

    private fun nullable(value: String?): JsonElement =
        value?.let(::JsonPrimitive) ?: JsonNull.INSTANCE

}

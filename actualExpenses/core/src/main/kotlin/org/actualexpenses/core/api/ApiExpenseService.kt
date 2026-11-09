package org.actualexpenses.core.api

import com.google.gson.*
import java.math.BigDecimal
import java.time.LocalDate
import java.util.UUID
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import org.actualexpenses.core.*
import org.actualexpenses.core.protocol.*

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
)

private data class NativeState(
    val format: String = "actualBudget-v1",
    var server: String = "",
    var budgets: List<RemoteBudget> = emptyList(),
    var budgetId: String? = null,
    var epoch: String = "",
    var accountId: String? = null,
    var checkpoint: String? = null,
    val node: String = UUID.randomUUID().toString().replace("-", "").takeLast(16),
    var clock: String = ZERO_TIMESTAMP,
    val accounts: MutableMap<String, JsonObject> = mutableMapOf(),
    val payees: MutableMap<String, JsonObject> = mutableMapOf(),
    val categories: MutableMap<String, JsonObject> = mutableMapOf(),
    var preferences: JsonObject = JsonObject(),
    val rows: MutableMap<String, JsonObject> = mutableMapOf(),
    val rememberedAccounts: MutableMap<String, String> = mutableMapOf(),
    val recent: MutableMap<String, List<String>> = mutableMapOf(),
    val retained: MutableMap<String, String> = mutableMapOf(),
    val pending: MutableList<NativeOperation> = mutableListOf(),
    var notice: String? = null,
)

/** Native v1 application boundary. Legacy storage is read only and never converted here. */
class ApiExpenseService(
    private val store: StateStore,
    private val legacyStore: StateStore? = null,
    private val transport: HttpTransport = BudgetHttpTransport(),
) {
    private val gson = GsonBuilder().serializeNulls().create()
    private val legacyPresent = legacyStore?.read()?.isNotBlank() == true
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
    @Volatile private var online = false
    @Volatile private var error: String? = null
    @Volatile private var blocked = false
    private val syncLock = ReentrantLock()

    private fun ready() {
        check(!legacyPresent) { LEGACY_RECOVERY }
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
        if (legacyPresent)
            return AppState(
                server = saved.server,
                error = LEGACY_RECOVERY,
                syncBlocked = true,
                legacyRecovery = true,
            )
        val rows = projected()
        val account = saved.accountId
        val eligible =
            rows.values
                .filter { eligible(it) && it.text("account_id") == account }
                .sortedWith(
                    compareByDescending<JsonObject> { it.text("date") }
                        .thenByDescending { it.long("sort_order") }
                        .thenBy { it.text("id") }
                )
        val pendingIds = saved.pending.map { it.transactionId }.toSet()
        val recent = eligible.take(10)
        val visible = recent.toMutableList()
        eligible.filter { it.text("id") in pendingIds && it !in visible }.forEach(visible::add)
        eligible
            .find {
                it.text("id") == saved.retained[retentionKey(saved.budgetId, account)] &&
                    it !in visible
            }
            ?.let(visible::add)
        saved.pending
            .filter { it.deletes && it.accountId == account }
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
            expenseContext = if (saved.accountId != null) captureContext() else null,
            budgets = saved.budgets,
            budgetId = saved.budgetId,
            accounts = accountChoices(),
            selectedAccountId = account,
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
            payees = choices(payees),
            categories =
                choices(saved.categories).filter {
                    saved.categories[it.id]?.let { row ->
                        listOf("is_income", "hidden", "group_hidden", "deleted").none {
                            row[it]?.asBoolean == true
                        }
                    } == true
                },
            currency = currency,
            decimalPlaces = if (currency in setOf("IRR", "JPY", "KRW")) 0 else 2,
            online = online,
            pendingCount = saved.pending.map { it.transactionId }.distinct().size,
            notice = saved.notice,
            error = pendingAccountError() ?: error,
            syncBlocked = blocked || pendingAccountError() != null,
        )
    }

    fun connect(server: String) = syncLock.withLock {
        ready()
        val normalized = BudgetApi.normalized(server)
        require(saved.pending.isEmpty() || normalized == saved.server) {
            "Keep this server address while a budget is selected. Saved work belongs to its original server."
        }
        val api = BudgetApi(normalized, transport)
        api.requireSupported()
        val budgets = budgets(api)
        require(
            normalized != saved.server ||
                saved.budgetId == null ||
                budgets.any { it.fileId == saved.budgetId }
        ) {
            "The selected budget is unavailable. Saved work is retained."
        }
        change { next ->
            if (next.server != normalized) {
                require(next.pending.isEmpty()) {
                    "Deliver pending changes before switching servers."
                }
                clearCache(next)
                next.budgetId = null
                next.accountId = null
                next.epoch = ""
            }
            next.server = normalized
            next.budgets = budgets
        }
        online = true
        error = null
        blocked = false
    }

    fun chooseBudget(id: String) = syncLock.withLock {
        ready()
        synchronized(this) {
            require(saved.pending.isEmpty() || saved.budgetId == id) {
                "Deliver pending changes before switching budgets."
            }
            require(saved.budgets.any { it.fileId == id }) { "Choose an available budget." }
        }
        bootstrap(id, saved.rememberedAccounts[scopeKey(saved.server, id)])
    }

    fun chooseAccount(id: String) = syncLock.withLock {
        ready()
        require(state().accounts.any { it.id == id }) { "Choose an open account." }
        val budget = saved.budgetId ?: error("Choose a budget first.")
        if (saved.accounts[id]?.let(::availableAccount) != true) {
            require(saved.pending.any { it.accountId == id }) { "Choose an open account." }
            change {
                it.accountId = id
                it.rememberedAccounts[scopeKey(it.server, budget)] = id
            }
        } else if (online) loadAccount(budget, id)
        else {
            require(saved.recent.containsKey(id)) {
                "Connect once to load this account before selecting it offline."
            }
            change {
                it.accountId = id
                it.rememberedAccounts[scopeKey(it.server, budget)] = id
            }
        }
    }

    fun refreshBudgets() {
        ready()
        val list = budgets(client())
        change { it.budgets = list }
    }

    @Synchronized
    fun beginExpense(): ExpenseContext {
        ready()
        return captureContext()
    }

    private fun captureContext(): ExpenseContext {
        val currency = saved.preferences.text("currency_code").orEmpty()
        return ExpenseContext(
            saved.server,
            saved.budgetId ?: error("Choose a budget first."),
            saved.accountId ?: error("Choose an account first."),
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
                context.accountId == saved.accountId
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
            captureContext(),
        )
    }

    @Synchronized
    fun saveExpense(draft: ExpenseDraft, edit: EditSession? = null): String {
        ready()
        val view = state()
        val account = saved.accountId ?: error("Choose an account first.")
        val budget = saved.budgetId ?: error("Choose a budget first.")
        require(saved.accounts[account]?.let(::availableAccount) == true) {
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
                    projected()[id]?.let { eligible(it) && it.text("account_id") == account } ==
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
        require(draft.notes.length <= 10000) { "Notes are too long." }
        val original = edit?.original
        val body = JsonObject()
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
        if (original == null || original.notes != draft.notes)
            body.addProperty("notes", draft.notes)
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
            if (edit == null) next.retained[retentionKey(budget, account)] = id
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
                    it.body
                        ?.let(JsonParser::parseString)
                        ?.asJsonObject
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
                        accountId = saved.accountId!!,
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

    fun refresh() {
        if (legacyPresent || saved.server.isEmpty() || !syncLock.tryLock()) return
        try {
            val api = client()
            // A selected compatible API is checked without login. Compatibility
            // failures never touch durable outbox contents or legacy storage.
            api.requireSupported()
            if (saved.budgetId == null) {
                val list = budgets(api)
                change { it.budgets = list }
                markOnline()
                return
            }
            // Refresh reference availability before delivering captured work so known
            // closed or deleted accounts hold their expenses for explicit recovery.
            val budget = saved.budgetId!!
            refreshBudget(api, budget)
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
                    } else throw failure
                }
                delivered = true
            }
            if (delivered) refreshBudget(api, budget)
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

    private fun refreshBudget(api: BudgetApi, budget: String) {
        if (saved.checkpoint == null) bootstrap(budget, saved.accountId)
        else
            try {
                refreshChanges(api, budget)
            } catch (failure: BudgetApiException) {
                if (failure.code == "cursor_invalid" && saved.pending.isEmpty())
                    bootstrap(budget, saved.accountId)
                else throw failure
            }
    }

    private fun requirePendingAccounts() {
        pendingAccountError()?.let { throw UnavailablePendingAccount(it) }
    }

    private fun accountChoices(): List<Choice> =
        choices(saved.accounts).mapNotNull { choice ->
            val row = saved.accounts.getValue(choice.id)
            when {
                availableAccount(row) -> choice
                saved.pending.any { it.accountId == choice.id } ->
                    Choice(
                        choice.id,
                        "${choice.name} (${if (row["deleted"]?.asBoolean == true) "deleted" else "closed"}; pending recovery)",
                    )
                else -> null
            }
        }

    private fun pendingAccountError(): String? =
        saved.pending
            .firstOrNull { saved.accounts[it.accountId]?.let(::availableAccount) != true }
            ?.let { op ->
                val account = saved.accounts[op.accountId]
                val reason =
                    if (account?.get("deleted")?.asBoolean == true || account == null) "deleted"
                    else "closed"
                val recovery =
                    if (reason == "closed") "Reopen it in actualBudget, then refresh to retry."
                    else
                        "Keep app data intact and reconcile this saved work with the original budget in actualBudget before continuing."
                "Account ${account?.text("name") ?: op.accountId} is $reason. Pending expenses retain their original account and delivery identity. $recovery"
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
            if (result.getAsJsonArray("conflicts")?.size()?.let { it > 0 } == true)
                next.notice = "A newer edit from another device replaced a field."
            if (row?.get("deleted")?.asBoolean == true && !op.deletes)
                next.notice = "This expense remains deleted on the server."
        }
    }

    private fun loadAccount(budget: String, account: String) {
        val api = client()
        repeat(3) {
            try {
                refreshChanges(api, budget)
                val view = companion(api, budget, account, saved.checkpoint!!)
                change { next ->
                    next.accountId = account
                    next.rememberedAccounts[scopeKey(next.server, budget)] = account
                    installView(next, account, view)
                }
                return
            } catch (failure: BudgetApiException) {
                if (failure.code == "cursor_invalid" && saved.pending.isEmpty()) {
                    bootstrap(budget, account)
                    return
                }
                if (failure.code != "snapshot_changed") throw failure
            }
        }
        throw IllegalStateException("The budget is changing. Retry account selection.")
    }

    private fun bootstrap(budget: String, account: String?) {
        val api = client()
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
                val prefs =
                    api.request("/budgets/$budget/preferences?$suffix").getAsJsonObject("data")
                val selected = account?.takeIf { id ->
                    accounts.any { it.text("id") == id && availableAccount(it) }
                }
                val view = selected?.let { companion(api, budget, it, snapshot) }
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
                    next.epoch = epoch
                    next.accountId =
                        selected
                            ?: account?.takeIf { id -> next.pending.any { it.accountId == id } }
                    next.checkpoint = snapshot
                    if (selected != null)
                        next.rememberedAccounts[scopeKey(next.server, budget)] = selected
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

    private fun refreshChanges(api: BudgetApi, budget: String) {
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
                val view =
                    if (
                        account != null &&
                            (changes.isNotEmpty() || !saved.recent.containsKey(account))
                    )
                        companion(api, budget, account, cursor)
                    else null
                if (changes.isEmpty() && view == null) return
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
                    if (categories != null) {
                        next.categories.clear()
                        categories.forEach { next.categories[it.text("id")!!] = it }
                    }
                    if (account == null && next.pending.none { it.accountId == next.accountId }) {
                        next.accountId = null
                        next.rememberedAccounts.remove(scopeKey(next.server, budget))
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
        account: String,
        snapshot: String,
    ): JsonObject {
        val retained =
            synchronized(this) {
                (saved.pending
                        .filter { it.accountId == account && it.budgetId == budget }
                        .map { it.transactionId } +
                        listOfNotNull(saved.retained[retentionKey(budget, account)]))
                    .distinct()
            }
        require(retained.size <= 100) {
            "Deliver pending expenses before loading more than 100 retained entries."
        }
        return api.request(
                "/budgets/$budget/companion?account_id=$account&retain=${BudgetApi.query(retained.joinToString(","))}&snapshot=${BudgetApi.query(snapshot)}"
            )
            .getAsJsonObject("data")
    }

    private fun installView(next: NativeState, account: String, view: JsonObject) {
        val keep =
            next.pending.map { it.transactionId }.toSet() +
                listOfNotNull(next.retained[retentionKey(next.budgetId, account)])
        next.rows.entries.removeAll { it.value.text("account_id") == account && it.key !in keep }
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

    private fun availableAccount(row: JsonObject) =
        row["closed"]?.asBoolean != true && row["deleted"]?.asBoolean != true

    private fun preferences(value: JsonObject) = value.getAsJsonObject("settings") ?: value

    private fun budgets(api: BudgetApi) =
        api.request("/budgets")
            .getAsJsonArray("data")
            .map { it.asJsonObject }
            .map {
                RemoteBudget(it["id"].asString, it.text("epoch").orEmpty(), it["name"].asString)
            }

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
                    it.body
                        ?.let(JsonParser::parseString)
                        ?.asJsonObject
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
                        op.body?.let {
                            JsonParser.parseString(it).asJsonObject.entrySet().forEach {
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
            row["reconciled"]?.asBoolean != true &&
            row["deleted"]?.asBoolean != true

    private fun JsonObject.text(key: String) = get(key)?.takeUnless { it.isJsonNull }?.asString

    private fun JsonObject.long(key: String) = get(key)?.takeUnless { it.isJsonNull }?.asLong ?: 0L

    private fun nullable(value: String?): JsonElement =
        value?.let(::JsonPrimitive) ?: JsonNull.INSTANCE

    companion object {
        const val LEGACY_RECOVERY =
            "Your previous Actual budget and pending changes are preserved on this phone. This update cannot migrate them yet. Keep app data intact; use the migration update before connecting to the new server."
    }
}

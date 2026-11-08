package org.actualexpenses.core

import com.google.gson.*
import org.actualexpenses.core.protocol.*
import java.math.BigDecimal
import java.time.LocalDate
import java.util.UUID
import java.util.concurrent.locks.ReentrantLock

private data class PendingOperation(
    val id: String = UUID.randomUUID().toString(), val transactionId: String = "",
    val changes: MutableList<ProtocolChange> = mutableListOf(), val createdOffline: Boolean = false,
    val creates: Boolean = false, val deletes: Boolean = false, var attempted: Boolean = false,
)
private data class SyncCheckpoint(val since: String, val merkle: JsonObject)
private data class SavedState(
    var server: String = "", var token: String? = null,
    var budgets: List<RemoteBudget> = emptyList(), var budget: RemoteBudget? = null,
    var accountId: String? = null, var clock: String = ZERO_TIMESTAMP,
    val node: String = UUID.randomUUID().toString().replace("-", "").takeLast(16),
    val rows: MutableMap<String, MutableMap<String, JsonObject>> = mutableMapOf(),
    val versions: MutableMap<String, String> = mutableMapOf(),
    val pending: MutableList<PendingOperation> = mutableListOf(),
    val retained: MutableMap<String, String> = mutableMapOf(),
    var notice: String? = null,
    var checkpoint: SyncCheckpoint? = null,
)

/** Single application boundary shared by foreground UI and pending-only Android worker. */
class ExpenseService(
    private val store: StateStore,
    private val decoder: SnapshotDecoder,
    private val transport: HttpTransport = JdkHttpTransport(),
) {
    // Gson otherwise maps a JSON null mutation value to a Kotlin null after restart.
    private val gson = GsonBuilder().serializeNulls().registerTypeAdapter(ProtocolChange::class.java,
        JsonDeserializer<ProtocolChange> { element, _, _ ->
            val obj = element.asJsonObject
            ProtocolChange(obj["timestamp"].asString, obj["dataset"].asString, obj["row"].asString,
                obj["column"].asString, obj["value"] ?: JsonNull.INSTANCE)
        }).create()
    @Volatile private var saved: SavedState = store.read()?.let { gson.fromJson(it, SavedState::class.java) } ?: SavedState()
    @Volatile private var online = false
    @Volatile private var error: String? = null
    @Volatile private var needsLogin = false
    @Volatile private var syncBlocked = false
    private val syncLock = ReentrantLock()

    @Synchronized fun state(): AppState {
        val rows = projected()
        val payees = choices(rows, "payees").filter { rows["payees"]?.get(it.id)?.text("transfer_acct") == null }
        val categories = choices(rows, "categories").filter { rows["categories"]?.get(it.id)?.flag("hidden") != true && rows["categories"]?.get(it.id)?.flag("is_income") != true }
        val accounts = choices(rows, "accounts").filter { rows["accounts"]?.get(it.id)?.flag("closed") != true }
        val currency = currency(rows)
        val eligible = rows["transactions"].orEmpty().values.filter { eligible(it) && it.text("acct") == saved.accountId }
            .sortedWith(compareByDescending<JsonObject> { it.number("date") }.thenByDescending { it.number("sort_order") }.thenBy { it.text("id") })
        val ordinary = eligible.take(10).mapNotNull { it.text("id") }.toSet()
        val pendingIds = saved.pending.map { it.transactionId }.toSet()
        val retained = saved.retained[saved.accountId]
        val visible = eligible.filter { it.text("id") in ordinary || it.text("id") in pendingIds }
            .toMutableList().apply { eligible.find { it.text("id") == retained && it !in this }?.let(::add) }
        // A pending deletion is still visible until its outcome is known.
        saved.pending.filter { it.deletes }.map { it.transactionId }.distinct().forEach { id ->
            val row = saved.rows["transactions"]?.get(id)
            if (row != null && row.text("acct") == saved.accountId && visible.none { it.text("id") == id }) visible.add(row)
        }
        val expenses = visible.map { row ->
            val id = row.text("id")!!
            val payeeId = resolveMapping(rows, "payee_mapping", row.text("description"), "targetId")
                ?.takeIf { rows["payees"]?.get(it)?.flag("tombstone") != true }
            val categoryId = resolveMapping(rows, "category_mapping", row.text("category"), "transferId")
                ?.takeIf { rows["categories"]?.get(it)?.flag("tombstone") != true }
            Expense(id, row.text("acct").orEmpty(), -row.number("amount"), formatDate(row.number("date")),
                payeeId, rows["payees"]?.get(payeeId)?.text("name").orEmpty(), categoryId,
                rows["categories"]?.get(categoryId)?.text("name").orEmpty(), row.text("notes").orEmpty(),
                id in pendingIds, saved.pending.any { it.transactionId == id && it.deletes }, canEdit(id))
        }
        return AppState(server = saved.server, authenticated = saved.token != null && !needsLogin,
            budgets = saved.budgets, budgetId = saved.budget?.fileId, accounts = accounts,
            selectedAccountId = saved.accountId, expenses = expenses, payees = payees, categories = categories,
            currency = currency, decimalPlaces = precision(currency), payeeCategoryAssociations = associations(rows, categories.map { it.id }.toSet()),
            online = online, pendingCount = pendingIds.size, notice = saved.notice, error = error, syncBlocked = syncBlocked)
    }

    @Synchronized fun login(server: String, password: String) {
        val normalized = server.trim().trimEnd('/')
        require(saved.budget == null || normalized == saved.server) { "This installation is linked to ${saved.server}. Sign in there to preserve your budget and pending work." }
        val client = ActualClient(normalized, transport = transport)
        client.requireSupportedServer()
        val session = client.login(password)
        val budgets = client.listBudgets()
        require(saved.budget == null || budgets.any { it.fileId == saved.budget?.fileId }) { "This sign-in cannot access the saved budget. Pending changes are retained." }
        change { it.server = normalized; it.token = session.token; it.budgets = budgets }
        needsLogin = false; online = true; error = null; syncBlocked = false
    }

    @Synchronized fun chooseBudget(id: String) {
        require(saved.budget == null || saved.budget?.fileId == id) { "This installation already has a budget." }
        val budget = saved.budgets.single { it.fileId == id }
        client().requireSupportedServer()
        val snapshot = decoder.decode(client().downloadBudget(budget))
        change { next ->
            next.budget = budget
            next.checkpoint = null
            next.rows.clear(); next.versions.clear()
            snapshot.rows.filterKeys { it !in setOf("messages_crdt", "messages_clock", "__migrations__") }.forEach { (table, records) ->
                next.rows[table] = records.mapNotNull { row -> row.text("id")?.let { it to row.deepCopy() } }.toMap().toMutableMap()
            }
            // Snapshot rows already contain the winning values. History establishes field ordering.
            snapshot.changes.forEach { message ->
                val key = cell(message)
                if (message.timestamp > (next.versions[key] ?: "")) next.versions[key] = message.timestamp
            }
            val clock = LogicalClock(next.node, next.clock)
            (snapshot.changes.maxOfOrNull { it.timestamp } ?: snapshot.clock)?.let(clock::observe)
            next.clock = clock.state
        }
        refresh()
    }

    @Synchronized fun chooseAccount(id: String) {
        require(state().accounts.any { it.id == id }) { "Choose an open account." }
        change { it.accountId = id }
    }

    @Synchronized fun markOffline() { online = false }
    @Synchronized fun dismissNotice() { change { it.notice = null }; error = null }

    @Synchronized fun beginEdit(id: String): EditSession {
        require(canEdit(id)) { "Connect to Actual before editing this expense." }
        require(projected()["transactions"]?.get(id)?.let(::eligible) == true) { "This expense can no longer be edited here." }
        val view = state()
        val expense = view.expenses.find { it.id == id } ?: throw IllegalArgumentException("This expense is no longer visible.")
        return EditSession(id, online, ExpenseDraft(BigDecimal.valueOf(expense.amountMinor).movePointLeft(view.decimalPlaces).toPlainString(),
            expense.date, expense.payee, expense.payeeId, expense.categoryId, expense.notes))
    }

    @Synchronized fun saveExpense(draft: ExpenseDraft, edit: EditSession? = null): String {
        val account = saved.accountId ?: throw IllegalStateException("Choose an account first.")
        if (edit == null && draft.creationId != null) {
            require(runCatching { UUID.fromString(draft.creationId).toString() == draft.creationId }.getOrDefault(false)) { "Invalid expense identity. Reopen the expense form." }
            val existing = projected()["transactions"]?.get(draft.creationId)
            if (existing != null) {
                require(existing.text("acct") == account) { "This expense belongs to another account." }
                return draft.creationId
            }
        }
        val view = state()
        require(view.accounts.any { it.id == account }) { "This account is unavailable. Your pending work is retained." }
        val amount = parseExpenseAmount(draft.amount, view.decimalPlaces)
        val date = try { LocalDate.parse(draft.date).toString().replace("-", "").toInt() } catch (_: Exception) { throw IllegalArgumentException("Enter a valid date (YYYY-MM-DD).") }
        require(date in 10000101..99991231) { "Choose a date between years 1000 and 9999." }
        if (edit != null) require(edit.beganOnline || canEdit(edit.transactionId)) { "Connect to Actual before editing this expense." }
        val old = edit?.let { projected()["transactions"]?.get(it.transactionId) }
        if (edit != null) require(old != null && eligible(old) && old.text("acct") == account) { "This expense can no longer be edited here." }
        require(draft.categoryId == null || view.categories.any { it.id == draft.categoryId } || edit?.original?.categoryId == draft.categoryId) { "Choose an available category." }
        val id = edit?.transactionId ?: draft.creationId ?: UUID.randomUUID().toString()
        change { next ->
            val clock = LogicalClock(next.node, next.clock)
            val messages = mutableListOf<ProtocolChange>()
            fun set(table: String, row: String, column: String, value: JsonElement) { messages += ProtocolChange(clock.next(), table, row, column, value) }
            val name = draft.payee.trim()
            val original = edit?.original
            val payeeChanged = original == null || name != original.payee
            val existing = view.payees.firstOrNull { (it.id == draft.payeeId && it.name == name) || it.name.equals(name, ignoreCase = true) }
            val payee = if (!payeeChanged) original?.payeeId else if (name.isEmpty()) null else existing?.id ?: UUID.randomUUID().toString().also { newId ->
                set("payees", newId, "name", JsonPrimitive(name))
                set("payees", newId, "tombstone", JsonPrimitive(0))
                set("payees", newId, "transfer_acct", JsonNull.INSTANCE)
                set("payee_mapping", newId, "targetId", JsonPrimitive(newId))
            }
            val fields = linkedMapOf<String, JsonElement>()
            if (original == null || parseExpenseAmount(original.amount, view.decimalPlaces) != amount) fields["amount"] = JsonPrimitive(-amount)
            if (original == null || original.date != draft.date) fields["date"] = JsonPrimitive(date)
            if (payeeChanged) fields["description"] = json(payee)
            if (original == null || original.categoryId != draft.categoryId) fields["category"] = json(draft.categoryId)
            if (original == null || original.notes != draft.notes) fields["notes"] = json(draft.notes.ifEmpty { null })
            if (edit == null) fields.putAll(mapOf("acct" to JsonPrimitive(account), "cleared" to JsonPrimitive(1), "reconciled" to JsonPrimitive(0),
                "tombstone" to JsonPrimitive(0), "isParent" to JsonPrimitive(0), "isChild" to JsonPrimitive(0),
                "parent_id" to JsonNull.INSTANCE, "transferred_id" to JsonNull.INSTANCE, "sort_order" to JsonPrimitive(System.currentTimeMillis().toDouble())))
            fields.forEach { (column, value) -> if (old == null || old[column] != value) set("transactions", id, column, value) }
            if (messages.isNotEmpty()) next.pending += PendingOperation(transactionId = id, changes = messages, creates = edit == null, createdOffline = edit == null && !online)
            if (edit == null) next.retained[account] = id
            next.clock = clock.state
        }
        return id
    }

    @Synchronized fun deleteExpense(edit: EditSession) {
        require(edit.beganOnline || canEdit(edit.transactionId)) { "Connect to Actual before deleting this expense." }
        require(projected()["transactions"]?.get(edit.transactionId)?.let(::eligible) == true) { "This expense can no longer be deleted here." }
        change { next ->
            val created = next.pending.find { it.transactionId == edit.transactionId && it.creates }
            val related = next.pending.filter { it.transactionId == edit.transactionId }
            if (created != null && related.none { it.attempted }) {
                next.pending.removeAll { it.transactionId == edit.transactionId }
                val metadata = related.flatMap { it.changes }.filter { it.dataset in setOf("payees", "payee_mapping") }
                metadata.groupBy { it.row }.forEach { (payeeId, changes) ->
                    next.pending.firstOrNull { op -> op.changes.any { it.dataset == "transactions" && it.column == "description" && it.value == JsonPrimitive(payeeId) } }
                        ?.changes?.addAll(0, changes)
                }
            } else {
                val clock = LogicalClock(next.node, next.clock)
                val mutation = ProtocolChange(clock.next(), "transactions", edit.transactionId, "tombstone", JsonPrimitive(1))
                next.pending += PendingOperation(transactionId = edit.transactionId, changes = mutableListOf(mutation), deletes = true)
                next.clock = clock.state
            }
        }
    }

    /** Network errors become observable state; durable operations remain queued. */
    fun refresh() {
        if (!syncLock.tryLock()) return
        try { refreshOnce() } finally { syncLock.unlock() }
    }

    private fun refreshOnce() {
        if (saved.token == null) return
        val budget = saved.budget
        try {
            val client = client()
            client.requireSupportedServer()
            val remoteBudgets = client.listBudgets()
            if (budget == null) {
                change { it.budgets = remoteBudgets }
                online = true; needsLogin = false; error = null; syncBlocked = false
                return
            }
            val remote = remoteBudgets.find { it.fileId == budget.fileId }
                ?: throw ActualException(ActualException.Kind.UNSUPPORTED, "This budget is no longer available. Pending changes are retained.")
            if (remote.groupId != budget.groupId || remote.encryptKeyId != budget.encryptKeyId) throw ActualException(ActualException.Kind.UNSUPPORTED, "The budget sync group changed. Pending changes are retained; restore the original budget before retrying.")
            val checkpoint = synchronized(this) { saved.checkpoint.takeIf { saved.pending.isEmpty() } }
            if (checkpoint != null) {
                val probe = client.pull(budget, checkpoint.since)
                if (probe.messages.isEmpty() && probe.merkle == checkpoint.merkle) {
                    // Saves can arrive while the network request is in flight. A queued
                    // operation still needs the complete-history acknowledgement/conflict path.
                    synchronized(this) {
                        if (saved.pending.isEmpty()) {
                            markSynced()
                            return
                        }
                    }
                }
            }
            // A changed tree may represent a late-arriving mutation OLDER than since.
            // A timestamp cursor alone would miss it; always replay when the tree differs.
            val pull = client.pull(budget)
            val pulled = pull.messages
            change { next ->
                applyCompletePull(next, pull)
                val observed = pulled.map { it.timestamp }.toSet()
                next.pending.removeAll { op -> op.changes.all { it.timestamp in observed } }
                next.pending.removeAll { op ->
                    // A complete pull reconciles an uncertain acknowledgement first: observed
                    // operations were removed above. An absent deletion remains unpublished at
                    // this observation; a concurrent later publication follows native semantics.
                    val cancel = op.deletes && pulled.any { it.dataset == "transactions" && it.row == op.transactionId && it.timestamp > op.changes.single().timestamp }
                    if (cancel) next.notice = "Deletion cancelled: this expense was edited more recently on another device."
                    cancel
                }
                reconcilePayees(next)
            }
            var operations: List<PendingOperation> = emptyList()
            // Capture and mark one batch atomically with respect to local saves/deletions.
            synchronized(this) {
                if (saved.pending.isNotEmpty()) change { next ->
                    next.pending.forEach { it.attempted = true }
                    operations = next.pending.toList()
                }
            }
            if (operations.isNotEmpty()) {
                val batch = client.exchange(budget, operations.flatMap { it.changes })
                change { next ->
                    applyRemote(next, batch.messages)
                    operations.flatMap { it.changes }.forEach { apply(next.rows, next.versions, it) }
                    next.pending.removeAll { op -> operations.any { it.id == op.id } }
                }
                val confirmation = client.pull(budget)
                change { applyCompletePull(it, confirmation) }
            }
            markSynced()
        } catch (e: Exception) {
            online = false; error = e.message ?: "Could not sync. Pending changes are retained."
            if (e is ActualException && e.kind == ActualException.Kind.AUTHENTICATION) needsLogin = true
            syncBlocked = e !is ActualException || e.kind !in setOf(ActualException.Kind.NETWORK, ActualException.Kind.SERVER)
        }
    }

    private fun markSynced() {
        online = true; needsLogin = false; error = null; syncBlocked = false
    }
    private fun applyCompletePull(next: SavedState, batch: SyncBatch) {
        applyRemote(next, batch.messages)
        // Commit rows and checkpoint atomically. Never use an upload response here:
        // its tree includes uploaded messages that its returned history may omit.
        next.checkpoint = SyncCheckpoint(batch.messages.maxOfOrNull { it.timestamp } ?: ZERO_TIMESTAMP, batch.merkle.deepCopy())
    }
    private fun client() = ActualClient(saved.server, saved.token, transport)
    @Synchronized private fun change(block: (SavedState) -> Unit) {
        val next = gson.fromJson(gson.toJson(saved), SavedState::class.java)
        block(next)
        store.write(gson.toJson(next))
        saved = next
    }
    private fun canEdit(id: String): Boolean = online || saved.pending.any { it.transactionId == id && it.creates && it.createdOffline }
    private fun projected(): MutableMap<String, MutableMap<String, JsonObject>> {
        val rows = saved.rows.mapValues { (_, records) -> records.mapValues { it.value.deepCopy() }.toMutableMap() }.toMutableMap()
        val versions = saved.versions.toMutableMap()
        saved.pending.flatMap { it.changes }.forEach { apply(rows, versions, it) }
        return rows
    }
    private fun applyRemote(next: SavedState, messages: List<ProtocolChange>) {
        val latest = messages.maxOfOrNull { it.timestamp }
        if (latest != null && latest > next.clock) {
            val clock = LogicalClock(next.node, next.clock); clock.observe(latest); next.clock = clock.state
        }
        messages.forEach { incoming ->
            if (incoming.dataset == "__migrations__" && incoming.row.toLongOrNull()?.let { it > SnapshotCompatibility.SUPPORTED_MIGRATION } == true) throw ActualException(ActualException.Kind.UNSUPPORTED, "Actual's schema changed. Pending changes are retained.")
            if (!incoming.timestamp.endsWith(next.node) && next.pending.flatMap { it.changes }.any { local -> cell(local) == cell(incoming) && incoming.timestamp > local.timestamp }) next.notice = "A newer edit from another device replaced a pending field change."
            apply(next.rows, next.versions, incoming)
        }
    }
    private fun apply(rows: MutableMap<String, MutableMap<String, JsonObject>>, versions: MutableMap<String, String>, message: ProtocolChange) {
        val key = cell(message)
        if (message.timestamp <= (versions[key] ?: "")) return
        val row = rows.getOrPut(message.dataset) { mutableMapOf() }.getOrPut(message.row) { JsonObject().apply { addProperty("id", message.row) } }
        row.add(message.column, message.value.deepCopy())
        versions[key] = message.timestamp
    }
    private fun reconcilePayees(next: SavedState) {
        // Never rewrite an attempted operation: its original payload may already be published.
        val replacements = mutableMapOf<String, String>()
        next.pending.filter { !it.attempted }.flatMap { it.changes }.filter { it.dataset == "payees" && it.column == "name" }.forEach { change ->
            val match = next.rows["payees"].orEmpty().values.firstOrNull { !it.flag("tombstone") && it.text("transfer_acct") == null && it.text("name").equals(change.value.asString, ignoreCase = true) }
            if (match != null && match.text("id") != change.row) replacements[change.row] = match.text("id")!!
        }
        next.pending.filter { !it.attempted }.forEach { op ->
            op.changes.removeAll { it.dataset in setOf("payees", "payee_mapping") && it.row in replacements }
            op.changes.replaceAll { if (it.dataset == "transactions" && it.column == "description" && !it.value.isJsonNull && it.value.asString in replacements) it.copy(value = JsonPrimitive(replacements[it.value.asString])) else it }
        }
    }
    private fun associations(rows: Map<String, Map<String, JsonObject>>, categories: Set<String>): Map<String, String> {
        val found = mutableMapOf<String, MutableSet<String>>()
        rows["rules"].orEmpty().values.filter { !it.flag("tombstone") && it.text("stage") == null }.forEach { rule ->
            runCatching {
                val conditions = JsonParser.parseString(rule.text("conditions")).asJsonArray
                val actions = JsonParser.parseString(rule.text("actions")).asJsonArray
                if (conditions.size() == 1 && actions.size() == 1) {
                    val c = conditions[0].asJsonObject; val a = actions[0].asJsonObject
                    if (c.text("field") in setOf("description", "payee") && c.text("op") == "is" && a.text("field") == "category" && a.text("op") == "set") {
                        val payee = resolveMapping(rows, "payee_mapping", c.text("value"), "targetId")
                        val category = resolveMapping(rows, "category_mapping", a.text("value"), "transferId")
                        if (payee != null && category in categories) found.getOrPut(payee) { mutableSetOf() }.add(category!!)
                    }
                }
            }
        }
        return found.mapNotNull { (payee, values) -> values.singleOrNull()?.let { payee to it } }.toMap()
    }
    private fun choices(rows: Map<String, Map<String, JsonObject>>, table: String) = rows[table].orEmpty().values.filter { !it.flag("tombstone") }.mapNotNull { row -> row.text("id")?.let { Choice(it, row.text("name").orEmpty()) } }.sortedBy { it.name.lowercase() }
    private fun eligible(row: JsonObject) = !row.flag("tombstone") && !row.flag("reconciled") && !row.flag("isParent") && !row.flag("isChild") && row.text("parent_id") == null && row.text("transferred_id") == null && row.number("amount") < 0
    private fun resolveMapping(rows: Map<String, Map<String, JsonObject>>, table: String, id: String?, column: String): String? {
        var current = id ?: return null; val visited = mutableSetOf<String>()
        while (visited.add(current)) { val next = rows[table]?.get(current)?.text(column) ?: return current; if (next == current) return current; current = next }
        return id
    }
    private fun currency(rows: Map<String, Map<String, JsonObject>>): String {
        val value = rows["preferences"]?.get("defaultCurrencyCode")?.text("value").orEmpty()
        return runCatching { JsonParser.parseString(value).asString }.getOrDefault(value).takeIf { it.matches(Regex("[A-Z]{3}")) }.orEmpty()
    }
    private fun precision(currency: String) = if (currency in setOf("IRR", "JPY", "KRW")) 0 else 2
    private fun formatDate(value: Long): String = value.toString().padStart(8, '0').let { "${it.take(4)}-${it.substring(4, 6)}-${it.takeLast(2)}" }
    private fun cell(m: ProtocolChange) = "${m.dataset}\u001f${m.row}\u001f${m.column}"
    private fun json(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull.INSTANCE
    private fun JsonObject.text(key: String): String? = get(key)?.takeUnless { it.isJsonNull }?.asString?.takeIf { it.isNotEmpty() }
    private fun JsonObject.number(key: String): Long = get(key)?.takeUnless { it.isJsonNull }?.asLong ?: 0
    private fun JsonObject.flag(key: String): Boolean = number(key) == 1L
}

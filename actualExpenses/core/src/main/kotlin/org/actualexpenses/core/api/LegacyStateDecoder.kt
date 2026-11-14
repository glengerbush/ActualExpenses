package org.actualexpenses.core.api

import com.google.gson.*
import org.actualexpenses.core.Choice
import org.actualexpenses.core.Expense

internal data class LegacyView(
    val expenses: List<Expense>,
    val payees: List<Choice>,
    val categories: List<Choice>,
    val currency: String,
    val payeeCategoryAssociations: Map<String, String>,
)

private data class LegacyChange(val timestamp: String, val dataset: String, val row: String, val column: String, val value: JsonElement)
private data class LegacyOperation(
    val transactionId: String = "", val changes: List<LegacyChange> = emptyList(),
    val creates: Boolean = false, val createdOffline: Boolean = false, val deletes: Boolean = false,
)
private data class LegacyState(
    val accountId: String? = null,
    val rows: Map<String, Map<String, JsonObject>> = emptyMap(),
    val versions: Map<String, String> = emptyMap(),
    val pending: List<LegacyOperation> = emptyList(),
    val retained: Map<String, String> = emptyMap(),
)

/** Pure released-state decoding. No network, password, database bootstrap, or state writes. */
internal class LegacyStateDecoder(raw: String) {
    private val saved = GsonBuilder().registerTypeAdapter(LegacyChange::class.java,
        JsonDeserializer<LegacyChange> { element, _, _ ->
            val obj = element.asJsonObject
            LegacyChange(obj["timestamp"].asString, obj["dataset"].asString, obj["row"].asString,
                obj["column"].asString, obj["value"] ?: JsonNull.INSTANCE)
        }).create().fromJson(raw, LegacyState::class.java)
    private fun canEdit(id: String) = saved.pending.any { it.transactionId == id && it.creates && it.createdOffline }

    fun view(): LegacyView {
        val rows = projected()
        val payees = choices(rows, "payees").filter { rows["payees"]?.get(it.id)?.text("transfer_acct") == null }
        val categories = choices(rows, "categories").filter { rows["categories"]?.get(it.id)?.flag("is_income") != true }
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
        return LegacyView(expenses, payees, categories, currency, associations(rows, categories.map { it.id }.toSet()))
    }
    private fun projected(): MutableMap<String, MutableMap<String, JsonObject>> {
        val rows = saved.rows.mapValues { (_, records) -> records.mapValues { it.value.deepCopy() }.toMutableMap() }.toMutableMap()
        val versions = saved.versions.toMutableMap()
        saved.pending.flatMap { it.changes }.forEach { apply(rows, versions, it) }
        return rows
    }
    private fun apply(rows: MutableMap<String, MutableMap<String, JsonObject>>, versions: MutableMap<String, String>, message: LegacyChange) {
        val key = cell(message)
        if (message.timestamp <= (versions[key] ?: "")) return
        val row = rows.getOrPut(message.dataset) { mutableMapOf() }.getOrPut(message.row) { JsonObject().apply { addProperty("id", message.row) } }
        row.add(message.column, message.value.deepCopy())
        versions[key] = message.timestamp
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
    private fun formatDate(value: Long): String = value.toString().padStart(8, '0').let { "${it.take(4)}-${it.substring(4, 6)}-${it.takeLast(2)}" }
    private fun cell(m: LegacyChange) = "${m.dataset}\u001f${m.row}\u001f${m.column}"
    private fun JsonObject.text(key: String): String? = get(key)?.takeUnless { it.isJsonNull }?.asString?.takeIf { it.isNotEmpty() }
    private fun JsonObject.number(key: String): Long = get(key)?.takeUnless { it.isJsonNull }?.asLong ?: 0
    private fun JsonObject.flag(key: String): Boolean = number(key) == 1L
}

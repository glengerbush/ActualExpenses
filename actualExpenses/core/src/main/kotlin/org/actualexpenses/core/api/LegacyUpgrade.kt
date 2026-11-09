package org.actualexpenses.core.api

import com.google.gson.*
import java.security.MessageDigest
import org.actualexpenses.core.*
import org.actualexpenses.core.protocol.*

/** Read-only interpretation of released expenses.state. The original remains recoverable. */
internal class LegacyUpgrade(val raw: String) {
    val json = JsonParser.parseString(raw).asJsonObject
    val budget =
        requireNotNull(json.getAsJsonObject("budget")) {
            "The previous installation has no selected budget. Keep its app data for recovery."
        }

    init {
        check(listOf("fileId", "groupId").all { budget[it]?.isJsonPrimitive == true }) {
            "The original budget identity is incomplete. Keep app data intact for recovery."
        }
    }

    val source =
        JsonObject().apply {
            add("file_id", budget["fileId"])
            add("group_id", budget["groupId"])
        }
    val operations = json.getAsJsonArray("pending") ?: JsonArray()
    val fingerprint = fingerprint(raw)
    val view: AppState by lazy {
        ExpenseService(
                object : StateStore {
                    override fun read() = raw

                    override fun write(value: String): Unit = error("Original state is read only")
                },
                SnapshotDecoder { error("Migration never downloads a source snapshot") },
            )
            .state()
    }

    fun rows(table: String): List<JsonObject> =
        json
            .getAsJsonObject("rows")
            ?.getAsJsonObject(table)
            ?.entrySet()
            ?.map { it.value.asJsonObject.deepCopy() }
            .orEmpty()

    fun row(table: String, id: String): JsonObject? =
        json.getAsJsonObject("rows")?.getAsJsonObject(table)?.getAsJsonObject(id)?.deepCopy()

    fun body(operation: JsonObject, epoch: String): String =
        JsonObject()
            .apply {
                add("source", source.deepCopy())
                addProperty("epoch", epoch)
                add("operation", operation.deepCopy())
            }
            .toString()

    companion object {
        fun fingerprint(raw: String): String =
            MessageDigest.getInstance("SHA-256")
                .digest(raw.toByteArray(Charsets.UTF_8))
                .joinToString("") { "%02x".format(it.toInt() and 255) }

        fun cachedRow(raw: JsonObject): JsonObject =
            JsonObject().apply {
                add("id", raw["id"])
                for ((old, new) in
                    mapOf(
                        "acct" to "account_id",
                        "amount" to "amount",
                        "description" to "payee_id",
                        "category" to "category_id",
                        "notes" to "notes",
                        "parent_id" to "parent_id",
                        "transferred_id" to "transfer_id",
                        "sort_order" to "sort_order",
                    )) add(new, raw[old] ?: JsonNull.INSTANCE)
                for ((old, new) in
                    mapOf(
                        "tombstone" to "deleted",
                        "cleared" to "cleared",
                        "reconciled" to "reconciled",
                        "isParent" to "is_parent",
                        "isChild" to "is_child",
                    )) addProperty(new, raw[old]?.asInt == 1)
                raw["date"]?.asLong?.toString()?.padStart(8, '0')?.let {
                    addProperty("date", "${it.take(4)}-${it.substring(4,6)}-${it.takeLast(2)}")
                }
            }

        /** Cached projection only. The wire body always retains all original messages verbatim. */
        fun projection(operation: JsonObject): JsonObject =
            JsonObject().apply {
                val changes = operation.getAsJsonArray("changes")
                changes.forEach { item ->
                    val m = item.asJsonObject
                    if (m["dataset"].asString == "transactions") {
                        val field =
                            when (val column = m["column"].asString) {
                                "acct" -> "account_id"
                                "description" -> "payee_id"
                                "category" -> "category_id"
                                "isParent" -> "is_parent"
                                "isChild" -> "is_child"
                                "transferred_id" -> "transfer_id"
                                "tombstone" -> "deleted"
                                else -> column
                            }
                        val value = m["value"] ?: JsonNull.INSTANCE
                        when (field) {
                            "cleared",
                            "reconciled",
                            "is_parent",
                            "is_child",
                            "deleted" -> addProperty(field, value.asInt != 0)
                            "date" ->
                                value.asLong.toString().padStart(8, '0').let {
                                    addProperty(
                                        field,
                                        "${it.take(4)}-${it.substring(4, 6)}-${it.takeLast(2)}",
                                    )
                                }
                            "notes" ->
                                addProperty(
                                    field,
                                    value.takeUnless { it.isJsonNull }?.asString.orEmpty(),
                                )
                            else -> add(field, value.deepCopy())
                        }
                    }
                }
                val payee =
                    changes
                        .map { it.asJsonObject }
                        .firstOrNull {
                            it["dataset"].asString == "payees" && it["column"].asString == "name"
                        }
                if (payee != null)
                    add(
                        "new_payee",
                        JsonObject().apply {
                            add("id", payee["row"])
                            add("name", payee["value"])
                        },
                    )
            }
    }
}

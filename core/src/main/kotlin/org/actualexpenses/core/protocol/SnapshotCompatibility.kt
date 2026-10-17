package org.actualexpenses.core.protocol

import com.google.gson.JsonObject
import com.google.gson.JsonParser

/** Validation shared by Android's read-only SQLite reader and the independent JVM test adapter. */
object SnapshotCompatibility {
    const val SUPPORTED_MIGRATION = 1788468782000L
    val requiredColumns: Map<String, Set<String>> = mapOf(
        "__migrations__" to setOf("id"),
        "accounts" to setOf("id", "name", "closed", "tombstone"),
        "transactions" to setOf("id", "acct", "amount", "date", "description", "category", "notes", "cleared", "reconciled", "isParent", "isChild", "parent_id", "transferred_id", "tombstone"),
        "payees" to setOf("id", "name", "transfer_acct", "tombstone"),
        "payee_mapping" to setOf("id", "targetId"),
        "categories" to setOf("id", "name", "hidden", "is_income", "tombstone"),
        "category_mapping" to setOf("id", "transferId"),
        "rules" to setOf("id", "stage", "conditions", "actions", "conditions_op", "tombstone"),
        "preferences" to setOf("id", "value"),
        "messages_clock" to setOf("clock"),
        "messages_crdt" to setOf("timestamp", "dataset", "row", "column", "value"),
    )

    fun validate(rows: Map<String, List<JsonObject>>, columns: Map<String, Set<String>>) {
        val migrations = rows["__migrations__"]?.mapNotNull { it["id"]?.asLong }.orEmpty()
        if (migrations.maxOrNull() != SUPPORTED_MIGRATION || !migrations.containsAll(listOf(1597756566448L, 1697046240000L, 1723665565000L))) {
            throw ActualException(ActualException.Kind.UNSUPPORTED, "This budget schema is unsupported. ActualExpenses currently supports Actual 26.10.0; open and update the budget in that version first.")
        }
        for ((table, required) in requiredColumns) {
            if (!columns[table].orEmpty().containsAll(required)) {
                throw ActualException(ActualException.Kind.UNSUPPORTED, "Budget schema is incompatible ($table). Pending changes were retained.")
            }
        }
    }

    fun decodeChange(row: JsonObject): ProtocolChange = try {
        ProtocolChange(row["timestamp"].asString.also(LogicalClock::validate), row["dataset"].asString, row["row"].asString, row["column"].asString, Protobuf.decodeValue(row["value"].asString))
    } catch (e: ActualException) { throw e }
    catch (e: RuntimeException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Invalid budget sync history", e) }

    fun clock(rows: Map<String, List<JsonObject>>): String? = rows["messages_clock"]?.firstOrNull()?.get("clock")?.asString?.let { raw ->
        try { JsonParser.parseString(raw).asJsonObject["timestamp"].asString.also(LogicalClock::validate) }
        catch (e: RuntimeException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Invalid budget clock", e) }
    }
}

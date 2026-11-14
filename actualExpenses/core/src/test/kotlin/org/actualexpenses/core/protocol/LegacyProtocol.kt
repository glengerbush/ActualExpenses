package org.actualexpenses.core.protocol

import com.google.gson.JsonElement
import com.google.gson.JsonObject

/** Frozen wire shapes used only by released-client fixtures and protocol oracles. */
data class Session(val token: String)
data class BudgetArchive(val metadata: JsonObject, val sqlite: ByteArray)
data class ProtocolChange(val timestamp: String, val dataset: String, val row: String, val column: String, val value: JsonElement)
data class SyncBatch(val messages: List<ProtocolChange>, val merkle: JsonObject)
/** Read-only bootstrap. Never migrate or write the upstream SQLite database. */
data class SnapshotTables(val rows: Map<String, List<JsonObject>>, val clock: String? = null, val changes: List<ProtocolChange> = emptyList())
fun interface SnapshotDecoder { fun decode(archive: BudgetArchive): SnapshotTables }

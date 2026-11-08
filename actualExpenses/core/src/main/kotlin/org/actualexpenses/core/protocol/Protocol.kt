package org.actualexpenses.core.protocol

import com.google.gson.JsonElement
import com.google.gson.JsonObject

const val ZERO_TIMESTAMP = "1970-01-01T00:00:00.000Z-0000-0000000000000000"

data class Session(val token: String)
data class RemoteBudget(val fileId: String, val groupId: String, val name: String, val encryptKeyId: String? = null)
data class BudgetArchive(val metadata: JsonObject, val sqlite: ByteArray)
data class ProtocolChange(val timestamp: String, val dataset: String, val row: String, val column: String, val value: JsonElement)
data class SyncBatch(val messages: List<ProtocolChange>, val merkle: JsonObject)
/** Read-only bootstrap. Never migrate or write the upstream SQLite database. */
data class SnapshotTables(val rows: Map<String, List<JsonObject>>, val clock: String? = null, val changes: List<ProtocolChange> = emptyList())
fun interface SnapshotDecoder { fun decode(archive: BudgetArchive): SnapshotTables }

class ActualException(val kind: Kind, message: String, cause: Throwable? = null) : Exception(message, cause) {
    enum class Kind { AUTHENTICATION, UNSUPPORTED, NETWORK, SERVER, INVALID_DATA, CLOCK }
}

data class HttpResponse(val status: Int, val contentType: String?, val body: ByteArray)
fun interface HttpTransport {
    fun request(url: String, method: String, headers: Map<String, String>, body: ByteArray?): HttpResponse
}

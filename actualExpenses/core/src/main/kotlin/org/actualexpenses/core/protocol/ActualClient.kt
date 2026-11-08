package org.actualexpenses.core.protocol

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URI
import java.util.zip.ZipInputStream

/** Blocking protocol boundary. Call on an IO dispatcher, never the Android UI thread. */
class ActualClient(baseUrl: String, token: String? = null, private val transport: HttpTransport = JdkHttpTransport()) {
    private val baseUrl = baseUrl.trim().trimEnd('/').also {
        val uri = try { URI(it) } catch (_: Exception) { throw IllegalArgumentException("Enter a valid Actual server address") }
        require(uri.scheme in listOf("http", "https") && uri.host != null && uri.userInfo == null && uri.query == null && uri.fragment == null) { "Enter an HTTP or HTTPS Actual server address" }
    }
    private var token = token

    fun serverVersion(): String {
        val response = request("/info")
        return try {
            JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject
                .getAsJsonObject("build")?.get("version")?.takeUnless { it.isJsonNull }?.asString
                ?: unsupported("Cannot identify this Actual server version. ActualExpenses supports Actual 26.10.0.")
        } catch (e: RuntimeException) {
            throw ActualException(ActualException.Kind.UNSUPPORTED, "Cannot identify this Actual server version. ActualExpenses supports Actual 26.10.0.", e)
        }
    }

    /** Call on bootstrap and before publishing queued work, including after a server upgrade. */
    fun requireSupportedServer() {
        if (serverVersion() != "26.10.0") unsupported("This Actual server version is unsupported. ActualExpenses currently supports Actual 26.10.0. Pending changes are retained.")
    }

    fun login(password: String): Session {
        val body = JsonObject().apply { addProperty("password", password); addProperty("loginMethod", "password") }
        val data = json(request("/account/login", "POST", body.toString().toByteArray(), "application/json"))
        val session = data.getAsJsonObject("data")?.get("token")?.takeUnless { it.isJsonNull }?.asString
            ?: throw ActualException(ActualException.Kind.AUTHENTICATION, "Password sign-in is unavailable on this server")
        if (session.isBlank()) throw ActualException(ActualException.Kind.AUTHENTICATION, "Actual returned an empty session")
        token = session
        return Session(session)
    }

    fun listBudgets(): List<RemoteBudget> {
        val result = json(request("/sync/list-user-files"))
        val rows = result.getAsJsonArray("data") ?: throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned an invalid budget list")
        return rows.map { it.asJsonObject }.filter { it.get("deleted")?.asInt != 1 }.map { row ->
            RemoteBudget(row.required("fileId"), row.text("groupId") ?: "", row.required("name"), row.text("encryptKeyId"))
        }
    }

    fun downloadBudget(budget: RemoteBudget): BudgetArchive {
        checkBudget(budget)
        val bytes = request("/sync/download-user-file", extraHeaders = mapOf("X-ACTUAL-FILE-ID" to budget.fileId)).body
        val entries = linkedMapOf<String, ByteArray>()
        var totalSize = 0L
        try {
            ZipInputStream(ByteArrayInputStream(bytes)).use { zip ->
                var count = 0
                while (true) {
                    val entry = zip.nextEntry ?: break
                    if (++count > 10_000) unsupported("Budget archive contains too many files")
                    val output = ByteArrayOutputStream()
                    val relevant = entry.name == "metadata.json" || entry.name.endsWith("/metadata.json") || entry.name == "db.sqlite" || entry.name.endsWith("/db.sqlite")
                    val buffer = ByteArray(8192)
                    while (true) {
                        val length = zip.read(buffer)
                        if (length < 0) break
                        totalSize += length
                        if (totalSize > MAX_ARCHIVE_BYTES) unsupported("Budget archive exceeds the supported 256 MiB limit")
                        if (relevant) output.write(buffer, 0, length)
                    }
                    if (relevant && entries.put(entry.name, output.toByteArray()) != null) invalid("Budget archive contains duplicate files")
                    zip.closeEntry()
                }
            }
        } catch (e: IOException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned an invalid budget archive", e) }
        val directories = entries.keys.filter { it == "db.sqlite" || it.endsWith("/db.sqlite") }.map { it.removeSuffix("db.sqlite") }
            .filter { entries.containsKey(it + "metadata.json") }
        val directory = if ("" in directories) "" else directories.singleOrNull() ?: invalid("Budget archive must contain one SQLite database and its metadata")
        val sqlite = entries.getValue(directory + "db.sqlite")
        if (sqlite.size < 16 || sqlite.copyOfRange(0, 16).toString(Charsets.UTF_8) != "SQLite format 3\u0000") invalid("Unsupported budget database format")
        val metadata = try { JsonParser.parseString(entries.getValue(directory + "metadata.json").toString(Charsets.UTF_8)).asJsonObject }
            catch (e: RuntimeException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned invalid budget metadata", e) }
        if (!metadata.text("encryptKeyId").isNullOrEmpty()) unsupported("Encrypted budgets are not supported")
        return BudgetArchive(metadata, sqlite)
    }

    /** Epoch pulls deliberately include late-arriving old mutations, unlike a timestamp high-water mark. */
    fun pull(budget: RemoteBudget, since: String = ZERO_TIMESTAMP): SyncBatch = exchange(budget, emptyList(), since)

    /** Callers persist messages before this call and must reuse their original timestamps on retry. */
    fun exchange(budget: RemoteBudget, messages: List<ProtocolChange>, since: String = ZERO_TIMESTAMP): SyncBatch {
        checkBudget(budget)
        LogicalClock.validate(since)
        val body = Protobuf.request(budget, messages, since)
        if (body.size > 19 * 1024 * 1024) unsupported("Pending sync exceeds the supported request size; changes remain saved locally")
        val response = request("/sync/sync", "POST", body, "application/actual-sync")
        if (response.contentType?.substringBefore(';') != "application/actual-sync") {
            json(response)
            invalid("Actual returned an unexpected sync response")
        }
        return Protobuf.response(response.body)
    }

    private fun checkBudget(budget: RemoteBudget) {
        if (!budget.encryptKeyId.isNullOrEmpty()) unsupported("Encrypted budgets are not supported")
        if (budget.groupId.isBlank()) unsupported("This budget has no active sync group. Open and sync it in Actual first.")
    }

    private fun request(path: String, method: String = "GET", body: ByteArray? = null, contentType: String? = null, extraHeaders: Map<String, String> = emptyMap()): HttpResponse {
        val headers = mutableMapOf("Accept" to "application/json, application/actual-sync")
        token?.let { headers["X-ACTUAL-TOKEN"] = it }
        contentType?.let { headers["Content-Type"] = it }
        headers.putAll(extraHeaders)
        val response = try { transport.request(baseUrl + path, method, headers, body) }
        catch (e: IOException) { throw ActualException(ActualException.Kind.NETWORK, "Cannot reach Actual. Changes remain saved; check the server address or tailnet connection.", e) }
        if (response.status !in 200..299) {
            val reason = runCatching { JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject.get("reason")?.asString }.getOrNull()
                ?: response.body.toString(Charsets.UTF_8).take(120)
            val kind = when {
                response.status in listOf(401, 403) || reason in listOf("invalid-password", "unauthorized", "invalid-token", "token-expired") -> ActualException.Kind.AUTHENTICATION
                reason in listOf("file-has-reset", "file-old-version", "file-needs-upload") || reason.contains("group") || reason.contains("encrypt") || reason.contains("key") -> ActualException.Kind.UNSUPPORTED
                else -> ActualException.Kind.SERVER
            }
            val message = when (kind) {
                ActualException.Kind.AUTHENTICATION -> "Actual requires sign-in. Pending changes are retained."
                ActualException.Kind.UNSUPPORTED -> "Actual's sync group or encryption changed. Pending changes are retained; reconnect this budget."
                else -> "Actual could not complete the request (HTTP ${response.status}). Pending changes are retained."
            }
            throw ActualException(kind, message)
        }
        return response
    }

    private fun json(response: HttpResponse): JsonObject {
        val result = try { JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject }
        catch (e: RuntimeException) { throw ActualException(ActualException.Kind.INVALID_DATA, "Actual returned invalid JSON", e) }
        if (result.get("status")?.asString != "ok") {
            val reason = result.text("reason") ?: "unknown"
            val kind = if (reason in listOf("unauthorized", "invalid-token", "invalid-password", "token-expired")) ActualException.Kind.AUTHENTICATION else ActualException.Kind.SERVER
            throw ActualException(kind, if (kind == ActualException.Kind.AUTHENTICATION) "Actual requires sign-in. Pending changes are retained." else "Actual could not complete the request. Pending changes are retained.")
        }
        return result
    }

    private fun JsonObject.text(key: String) = get(key)?.takeUnless { it.isJsonNull }?.asString
    private fun JsonObject.required(key: String) = text(key)?.takeIf { it.isNotBlank() } ?: invalid("Actual returned an incomplete budget record")
    private fun invalid(message: String): Nothing = throw ActualException(ActualException.Kind.INVALID_DATA, message)
    private fun unsupported(message: String): Nothing = throw ActualException(ActualException.Kind.UNSUPPORTED, message)
    private companion object { const val MAX_ARCHIVE_BYTES = 256L * 1024 * 1024 }
}

/** Redirects are refused so an Actual token is never forwarded to an unexpected host. */
class JdkHttpTransport : HttpTransport {
    override fun request(url: String, method: String, headers: Map<String, String>, body: ByteArray?): HttpResponse {
        val connection = URI(url).toURL().openConnection() as HttpURLConnection
        try {
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 15_000
            connection.readTimeout = 30_000
            connection.requestMethod = method
            headers.forEach { (name, value) -> connection.setRequestProperty(name, value) }
            if (body != null) {
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(body.size)
                connection.outputStream.use { it.write(body) }
            }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val bytes = stream?.use { readLimited(it) } ?: byteArrayOf()
            return HttpResponse(status, connection.contentType, bytes)
        } finally { connection.disconnect() }
    }

    private fun readLimited(stream: InputStream): ByteArray {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        while (true) {
            val length = stream.read(buffer)
            if (length < 0) break
            if (output.size().toLong() + length > 128L * 1024 * 1024) throw ActualException(ActualException.Kind.UNSUPPORTED, "Actual response exceeds the supported 128 MiB limit. Pending changes are retained.")
            output.write(buffer, 0, length)
        }
        return output.toByteArray()
    }
}

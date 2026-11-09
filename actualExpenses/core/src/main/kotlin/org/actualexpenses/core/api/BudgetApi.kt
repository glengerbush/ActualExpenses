package org.actualexpenses.core.api

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import java.net.URI
import java.net.URLEncoder
import java.util.concurrent.TimeUnit
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.actualexpenses.core.protocol.HttpResponse
import org.actualexpenses.core.protocol.HttpTransport

/** One pooled client, bounded responses, no redirects or invisible mutation retries. */
class BudgetHttpTransport : HttpTransport {
    private val client =
        OkHttpClient.Builder()
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(20, TimeUnit.SECONDS)
            .callTimeout(30, TimeUnit.SECONDS)
            .build()

    override fun request(
        url: String,
        method: String,
        headers: Map<String, String>,
        body: ByteArray?,
    ): HttpResponse {
        val request = Request.Builder().url(url)
        headers.forEach { (key, value) -> request.header(key, value) }
        request.method(method, body?.toRequestBody("application/json".toMediaType()))
        return client.newCall(request.build()).execute().use { response ->
            val source = response.body.source()
            require(!source.request(8L * 1024 * 1024 + 1)) {
                "Budget response exceeds 8 MiB. Saved work is retained."
            }
            HttpResponse(response.code, response.header("Content-Type"), source.readByteArray())
        }
    }
}

class BudgetApiException(
    val status: Int,
    val code: String,
    message: String,
    val response: JsonObject,
) : Exception(message)

internal class BudgetApi(private val base: String, private val transport: HttpTransport) {
    fun request(
        path: String,
        method: String = "GET",
        body: String? = null,
        key: String? = null,
        clock: String? = null,
        epoch: String? = null,
        receiptOnly: Boolean = false,
    ): JsonObject {
        val headers = mutableMapOf("Accept" to "application/json")
        if (body != null) headers["Content-Type"] = "application/json"
        key?.let { headers["Idempotency-Key"] = it }
        clock?.let { headers["X-Actual-Edit-Clock"] = it }
        epoch?.let { headers["X-Actual-Budget-Epoch"] = it }
        if (receiptOnly) headers["X-Actual-Receipt-Only"] = "true"
        // A captured expense operation cannot gain split/transfer semantics later.
        if (key != null && method in listOf("PATCH", "DELETE") && path.contains("/transactions/")) {
            headers["X-Actual-Expense-Only"] = "true"
        }
        val result =
            transport.request(
                "$base/api/v1$path",
                method,
                headers,
                body?.toByteArray(Charsets.UTF_8),
            )
        val json = runCatching {
            JsonParser.parseString(result.body.toString(Charsets.UTF_8)).asJsonObject
        }
            .getOrElse {
                if (result.status >= 500)
                    throw java.io.IOException("The server is unavailable. Saved work is retained.")
                throw IllegalArgumentException(
                    "This server did not return the actualBudget API. Saved work is retained."
                )
            }
        if (result.status !in 200..299) {
            val error = json.getAsJsonObject("error")
            throw BudgetApiException(
                result.status,
                error?.get("code")?.asString ?: "invalid_response",
                error?.get("message")?.asString
                    ?: "Server returned HTTP ${result.status}. Saved work is retained.",
                json,
            )
        }
        return json
    }

    fun requireSupported(): Set<String> {
        val info =
            request("/info").getAsJsonObject("data")
                ?: throw IllegalArgumentException(
                    "This server did not return actualBudget API information. Saved work is retained."
                )
        require(
            info["api_version"]?.asInt == 1 &&
                info["name"]?.asString == "actualBudget" &&
                info["authentication"]?.asString == "none"
        ) {
            "This server is incompatible. Use actualBudget API v1. Saved work is retained."
        }
        val capabilities = info.getAsJsonArray("capabilities").map { it.asString }.toSet()
        require(
            capabilities.containsAll(
                setOf(
                    "companion_expenses",
                    "field_ordering",
                    "change_cursors",
                    "budget_epoch_preconditions",
                )
            )
        ) {
            "This server cannot support the companion safely. Saved work is retained."
        }
        require("transfers" !in capabilities || "expense_kind_precondition" in capabilities) {
            "This server needs companion transfer protection. Update actualBudget; saved work is retained."
        }
        return capabilities
    }

    companion object {
        fun normalized(value: String): String {
            val uri = URI(value.trim())
            require(
                uri.scheme?.lowercase() in setOf("http", "https") &&
                    uri.host != null &&
                    uri.userInfo == null &&
                    uri.query == null &&
                    uri.fragment == null
            ) {
                "Enter an HTTP or HTTPS actualBudget server address."
            }
            val scheme = uri.scheme.lowercase()
            val port =
                uri.port.takeUnless {
                    (scheme == "http" && it == 80) || (scheme == "https" && it == 443)
                } ?: -1
            return URI(scheme, null, uri.host.lowercase(), port, uri.path.trimEnd('/'), null, null)
                .toASCIIString()
        }

        fun query(value: String): String = URLEncoder.encode(value, "UTF-8")
    }
}

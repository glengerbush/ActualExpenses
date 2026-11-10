package org.actualexpenses.core

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.file.Files
import java.nio.file.Path
import java.util.Base64
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.*
import org.actualexpenses.core.api.ApiExpenseService
import org.actualexpenses.core.api.BudgetHttpTransport
import org.actualexpenses.core.protocol.HttpTransport
import org.junit.jupiter.api.Test

class BankApiIntegrationTest {
    private class MemoryStore : StateStore {
        var raw: String? = null
        var writes = 0
        override fun read() = raw
        override fun write(value: String) { raw = value; writes++ }
    }

    @Test
    fun `bank refresh preserves queued expenses and booked identities without native provider polling`() {
        val provider = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        val booked = AtomicBoolean(false)
        val requests = AtomicInteger(0)
        val origin = "http://127.0.0.1:${provider.address.port}"
        provider.createContext("/") { exchange ->
            val body = if (exchange.requestURI.path == "/claim") {
                "http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address.port}/access"
            } else {
                requests.incrementAndGet()
                val paid = booked.get()
                """{"errors":[],"accounts":[{"id":"remote","name":"Checking","org":{"id":"bank","name":"Synthetic bank"},"currency":"USD","balance":"100.00","balance-date":1794225600,"transactions":[{"id":"posted","posted":1793491200,"transacted_at":1793491200,"amount":"-12.34","payee":"CAFE","description":"Bank #note"},{"id":"pending","posted":${if (paid) 1793491200 else 0},"transacted_at":1793491200,"pending":${!paid},"amount":"-2.00","payee":"SHOP","description":"Pending"}]}]}"""
            }
            val bytes = body.toByteArray()
            exchange.sendResponseHeaders(200, bytes.size.toLong())
            exchange.responseBody.use { it.write(bytes) }
        }
        provider.start()
        val directory = Files.createTempDirectory("actualexpenses-bank-")
        var child: Process? = null
        try {
            val binary = System.getenv("ACTUALBUDGET_BINARY") ?: Path.of("../../actualBudget/server/target/debug/actualbudget-server").toAbsolutePath().normalize().toString()
            child = ProcessBuilder(binary).apply {
                environment().putAll(mapOf("ACTUALBUDGET_HOST" to "127.0.0.1", "ACTUALBUDGET_PORT" to "0", "ACTUALBUDGET_DATA_DIR" to directory.toString(), "ACTUALBUDGET_WEB_DIR" to directory.toString(), "ACTUALBUDGET_BANK_TEST_ORIGIN" to origin))
                redirectError(ProcessBuilder.Redirect.INHERIT)
            }.start()
            val line = child.inputStream.bufferedReader().readLine()
            assertTrue(line.startsWith("LISTENING "), line)
            val url = "http://${line.removePrefix("LISTENING ")}"
            val http = HttpClient.newHttpClient()
            val budget = UUID.randomUUID().toString()
            val account = UUID.randomUUID().toString()
            var epoch: String? = null
            fun call(path: String, body: String? = null, status: Int = 200): JsonObject {
                val builder = HttpRequest.newBuilder(URI("$url/api/v1$path"))
                if (body != null) {
                    builder.header("Content-Type", "application/json").header("Idempotency-Key", UUID.randomUUID().toString())
                    epoch?.let { builder.header("X-Actual-Budget-Epoch", it) }
                    builder.POST(HttpRequest.BodyPublishers.ofString(body))
                } else builder.GET()
                val response = http.send(builder.build(), HttpResponse.BodyHandlers.ofString())
                assertEquals(status, response.statusCode(), response.body())
                return JsonParser.parseString(response.body()).asJsonObject
            }
            call("/budgets", """{"id":"$budget","name":"Native bank"}""", 201)
            val root = "/budgets/$budget"
            call("$root/accounts", """{"id":"$account","name":"Spending","opening_balance":0,"opening_date":"2026-01-01"}""", 201)
            epoch = call("$root/checkpoint").getAsJsonObject("data")["epoch"].asString
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val entities = mutableSetOf<String>()
            val transport = HttpTransport { requestUrl, method, headers, body ->
                val response = real.request(requestUrl, method, headers, body)
                if (requestUrl.contains("/changes?") && response.status == 200) {
                    JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject.getAsJsonArray("data").forEach { entities += it.asJsonObject["entity"].asString }
                }
                response
            }
            val app = ApiExpenseService(store, transport = transport)
            app.connect(url); app.chooseBudget(budget); app.chooseAccount(account)
            val manual = app.saveExpense(ExpenseDraft("4.56", notes = "Native manual"))
            app.refresh()
            app.markOffline()
            val pending = app.saveExpense(ExpenseDraft("1.23", notes = "Queued native"))
            assertEquals(1, app.state().pendingCount)
            val id = UUID.randomUUID().toString()
            val token = Base64.getEncoder().encodeToString("$origin/claim".toByteArray())
            val staged = call("$root/banking/connections", """{"id":"$id","provider":"simplefin","name":"Native bank","setup_token":"$token"}""", 201).getAsJsonObject("data")
            val generation = staged["generation"].asString
            call("$root/banking/connections/$id/commands", """{"action":"claim","generation":"$generation"}""")
            val discovery = call("$root/banking/connections/$id/commands", """{"action":"discover","generation":"$generation"}""")
            val remote = discovery.getAsJsonObject("data").getAsJsonArray("accounts").single().asJsonObject["id"].asString
            var snapshot = call("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            val link = call("$root/accounts/$account/banking/commands", """{"action":"link","snapshot":"$snapshot","connection_id":"$id","generation":"$generation","remote_account_id":"$remote","opening_balance_mode":"auto"}""").getAsJsonObject("data")
            val revision = link["revision"].asLong
            fun refresh() {
                snapshot = call("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
                val result = call("$root/banking/connections/$id/commands", """{"action":"refresh","generation":"$generation","snapshot":"$snapshot","start_date":"2026-11-01","end_date":"2026-11-10","accounts":[{"account_id":"$account","revision":$revision}]}""")
                assertEquals("complete", result.getAsJsonObject("data")["status"].asString)
            }
            refresh()
            val initial = call("$root/transactions").getAsJsonArray("data").filter { it.asJsonObject.has("imported_id") }.associate { it.asJsonObject["imported_id"].asString to it.asJsonObject["id"].asString }
            assertEquals(2, initial.size)
            app.refresh(); assertNull(app.state().error); assertEquals(0, app.state().pendingCount)
            assertEquals(setOf(manual, pending) + initial.values, app.state().expenses.map { it.id }.toSet())
            assertEquals(1234L, app.state().expenses.single { it.id == initial["posted"] }.amountMinor)
            assertEquals(200L, app.state().expenses.single { it.id == initial["pending"] }.amountMinor)
            booked.set(true); refresh(); app.refresh()
            val updated = call("$root/transactions").getAsJsonArray("data").filter { it.asJsonObject.has("imported_id") }.associate { it.asJsonObject["imported_id"].asString to it.asJsonObject }
            assertEquals(initial["pending"], updated.getValue("pending")["id"].asString)
            assertTrue(updated.getValue("pending")["cleared"].asBoolean)
            assertTrue(entities.containsAll(setOf("bank_connections", "bank_remote_accounts", "bank_links", "bank_transaction_origins", "transaction_import_metadata", "transactions")), entities.toString())
            val count = requests.get(); val writes = store.writes
            app.refresh(); assertEquals(writes, store.writes, "Unchanged feed must not rewrite native cache")
            val restarted = ApiExpenseService(store, transport = transport)
            restarted.refresh(); assertNull(restarted.state().error)
            assertEquals(setOf(manual, pending) + initial.values, restarted.state().expenses.map { it.id }.toSet())
            assertEquals(count, requests.get(), "Native refresh/restart never polls a provider")
            assertFalse(store.raw!!.contains(token)); assertFalse(store.raw!!.contains("NOT-A-REAL-SECRET"))
            assertEquals(4, call("$root/transactions").getAsJsonArray("data").size(), "No duplicate pending/booked rows")
        } finally {
            child?.destroy(); child?.waitFor()
            directory.toFile().deleteRecursively()
            provider.stop(0)
        }
    }
}

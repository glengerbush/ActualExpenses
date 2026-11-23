package org.actualexpenses.core

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import java.io.IOException
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.file.Files
import java.nio.file.Path
import java.time.Instant
import java.time.format.DateTimeFormatterBuilder
import java.util.UUID
import kotlin.test.*
import org.actualexpenses.core.api.ApiExpenseService
import org.actualexpenses.core.api.BudgetHttpTransport
import org.actualexpenses.core.protocol.HttpTransport
import org.junit.jupiter.api.Test

class NativeApiIntegrationTest {
    @Test
    fun `transaction notes survive offline edits restart and clearing`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.markOffline()
            val id = app.saveExpense(ExpenseDraft("2.50", notes = "Receipt\nCafé ☕"))
            val edit = app.beginEdit(id)
            app.saveExpense(edit.original!!.copy(notes = "Shared groceries"), edit)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals("Shared groceries", restarted.state().expenses.single { it.id == id }.notes)
            val onlineEdit = restarted.beginEdit(id)
            restarted.saveExpense(onlineEdit.original!!.copy(notes = ""), onlineEdit)
            restarted.refresh()
            assertEquals("", ApiExpenseService(store).apply { refresh() }.state().expenses.single { it.id == id }.notes)
        }
    }

    @Test
    fun `connect automatically prepares the single ledger without budget or account selection`() {
        Server().use { server ->
            val budget = server.budget
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            assertNotNull(app.state().expenseContext)
            val id = app.saveExpense(ExpenseDraft("1.23", notes = "No account setup"))
            app.refresh()
            assertEquals(0, app.state().pendingCount)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals(id, restarted.state().expenses.single().id)
            repeat(12) { number ->
                server.post("/budgets/$budget/transactions", """{"id":"${UUID.randomUUID()}","account_id":"${restarted.state().expenseContext?.accountId}","amount":-100,"date":"2026-10-${(number + 1).toString().padStart(2, '0')}"}""")
            }
            restarted.refresh()
            restarted.markOffline()
            val backdated = restarted.saveExpense(ExpenseDraft("2.34", date = "2000-01-01", notes = "Keep visible"))
            restarted.refresh()
            val reopened = ApiExpenseService(store)
            reopened.refresh()
            assertEquals(11, reopened.state().expenses.size)
            assertEquals(backdated, reopened.state().expenses.last().id)
        }
    }

    @Test
    fun `unified recent expenses edit and delete imported destinations after reconnect`() {
        Server().use { server ->
            val other = UUID.randomUUID().toString()
            server.post("/budgets/${server.budget}/accounts", """{"id":"$other","name":"Legacy","opening_balance":0,"opening_date":"2026-01-01"}""")
            val first = UUID.randomUUID().toString()
            val second = UUID.randomUUID().toString()
            server.post("/budgets/${server.budget}/transactions", """{"id":"$first","account_id":"${server.account}","amount":-100,"date":"2026-01-01"}""")
            server.post("/budgets/${server.budget}/transactions", """{"id":"$second","account_id":"$other","amount":-200,"date":"2026-01-02"}""")
            server.post("/budgets/${server.budget}/transactions", """{"id":"${UUID.randomUUID()}","account_id":"$other","amount":200,"date":"2026-01-02"}""")
            server.post("/budgets/${server.budget}/accounts/$other/close", "{}", 200)
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            assertEquals(setOf(first, second), app.state().expenses.map { it.id }.toSet())
            val edit = app.beginEdit(second)
            assertEquals(other, edit.context!!.accountId)
            app.markOffline()
            app.saveExpense(edit.original!!.copy(date = "2026-02-03"), edit)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount)
            assertEquals("2026-02-03", restarted.state().expenses.single { it.id == second }.date)
            assertEquals(other, restarted.state().expenses.single { it.id == second }.accountId)
            restarted.deleteExpense(restarted.beginEdit(second))
            restarted.refresh()
            assertEquals(listOf(first), restarted.state().expenses.map { it.id })
        }
    }
    @Test
    fun `empty server connects and prepares expenses without a named budget`() {
        Server(seed = false).use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            val context = assertNotNull(app.state().expenseContext)
            assertEquals(server.get("/budget").getAsJsonObject("data")["id"].asString, context.budgetId)
            assertEquals(1, server.get("/budgets").getAsJsonArray("data").size())
            val reopened = ApiExpenseService(store)
            reopened.refresh()
            assertEquals(context, reopened.state().expenseContext)
            assertNull(reopened.state().error)
        }
    }

    @Test
    fun `retired budget without queued work automatically opens the current ledger`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            val expense = app.saveExpense(ExpenseDraft("1.23"))
            app.refresh()
            val old = JsonParser.parseString(store.raw).asJsonObject
            old.addProperty("budgetId", UUID.randomUUID().toString())
            store.raw = old.toString()
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertNull(restarted.state().error)
            assertNull(restarted.state().scopeRecovery)
            assertEquals(server.budget, restarted.state().expenseContext!!.budgetId)
            assertEquals(expense, restarted.state().expenses.single().id)
        }
    }

    @Test
    fun `retired budget queue stays immutable until exported and explicitly archived`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.markOffline()
            app.saveExpense(ExpenseDraft("7.89", payee = "Retired purchase"))
            val old = JsonParser.parseString(store.raw).asJsonObject
            val retired = UUID.randomUUID().toString()
            old.addProperty("budgetId", retired)
            old.getAsJsonArray("pending").forEach { entry ->
                entry.asJsonObject.addProperty("budgetId", retired)
                entry.asJsonObject.addProperty("path", "/budgets/$retired/transactions")
            }
            store.raw = old.toString()
            val pending = old["pending"].deepCopy()
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertNotNull(restarted.state().scopeRecovery)
            assertNull(restarted.state().expenseContext)
            assertTrue(restarted.state().syncBlocked)
            assertEquals(pending, JsonParser.parseString(store.raw).asJsonObject["pending"])
            val exported = restarted.exportScopeRecovery()
            assertEquals(pending, JsonParser.parseString(exported).asJsonObject.getAsJsonObject("saved_state")["pending"])
            assertFailsWith<IllegalStateException> { restarted.openCurrentAfterExport("changed export") }
            assertEquals(1, restarted.state().pendingCount)
            restarted.openCurrentAfterExport(exported)
            assertEquals(0, restarted.state().pendingCount)
            assertEquals(server.budget, restarted.state().budgetId)
            assertNotNull(restarted.state().expenseContext)
            assertEquals(exported, JsonParser.parseString(store.raw).asJsonObject["recoveryArchive"].asString)
            assertEquals(0, server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data").size())
        }
    }

    @Test
    fun `changed epoch preserves unsent expense and recovery across restart`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.markOffline()
            app.saveExpense(ExpenseDraft("3.21"))
            val captured = JsonParser.parseString(store.raw).asJsonObject
            captured.addProperty("epoch", "00000000000000000000000000000000")
            captured.getAsJsonArray("pending")[0].asJsonObject.addProperty("epoch", "00000000000000000000000000000000")
            store.raw = captured.toString()
            val pending = captured["pending"].deepCopy()
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertNotNull(restarted.state().scopeRecovery)
            val reopen = ApiExpenseService(store)
            assertNotNull(reopen.state().scopeRecovery)
            assertEquals(pending, JsonParser.parseString(reopen.exportScopeRecovery()).asJsonObject.getAsJsonObject("saved_state")["pending"])
            assertEquals(0, server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data").size())
        }
    }

    private class MemoryStore(var raw: String? = null) : StateStore {
        var writes = 0
        var reads = 0

        override fun read() = raw.also { reads++ }

        override fun write(value: String) {
            raw = value
            writes++
        }
    }

    private class Server(private val binaryOverride: String? = null, private val seed: Boolean = true) : AutoCloseable {
        private val directory = Files.createTempDirectory("actualexpenses-v1-")
        private val client = HttpClient.newHttpClient()
        private val child: Process
        val url: String
        val budget = UUID.randomUUID().toString()
        val account = UUID.randomUUID().toString()

        init {
            val web = Files.createDirectory(directory.resolve("web"))
            Files.writeString(web.resolve("index.html"), "<title>Disposable fixture</title>")
            val binary =
                binaryOverride ?: System.getenv("ACTUALBUDGET_BINARY")
                    ?: Path.of("../../actualBudget/server/target/debug/actualbudget-server")
                        .toAbsolutePath()
                        .normalize()
                        .toString()
            child =
                ProcessBuilder(binary)
                    .apply {
                        environment()
                            .putAll(
                                mapOf(
                                    "ACTUALBUDGET_HOST" to "127.0.0.1",
                                    "ACTUALBUDGET_PORT" to "0",
                                    "ACTUALBUDGET_DATA_DIR" to directory.toString(),
                                    "ACTUALBUDGET_WEB_DIR" to web.toString(),
                                )
                            )
                        redirectError(ProcessBuilder.Redirect.INHERIT)
                    }
                    .start()
            val line = child.inputStream.bufferedReader().readLine()
            check(line.startsWith("LISTENING ")) { line }
            url = "http://${line.removePrefix("LISTENING ")}"
            if (seed) {
            post("/budgets", """{"id":"$budget","name":"Native fixture"}""")
            post(
                "/budgets/$budget/accounts",
                """{"id":"$account","name":"Spending","opening_balance":0,"opening_date":"2026-01-01"}""",
            )
            }
        }

        fun post(path: String, body: String, status: Int = 201, headers: Map<String, String> = emptyMap()): JsonObject {
            val response =
                client.send(
                    HttpRequest.newBuilder(URI("$url/api/v1$path"))
                        .header("Content-Type", "application/json")
                        .header("Idempotency-Key", UUID.randomUUID().toString())
                        .apply {
                            if (path.startsWith("/budgets/") && "X-Actual-Budget-Epoch" !in headers) {
                                val budgetId = path.split('/')[2]
                                get("/budgets/$budgetId/checkpoint").getAsJsonObject("data")?.get("epoch")?.asString?.let {
                                    header("X-Actual-Budget-Epoch", it)
                                }
                            }
                            headers.forEach { (key, value) -> header(key, value) }
                        }
                        .POST(HttpRequest.BodyPublishers.ofString(body))
                        .build(),
                    HttpResponse.BodyHandlers.ofString(),
                )
            assertEquals(status, response.statusCode(), response.body())
            return JsonParser.parseString(response.body()).asJsonObject.getAsJsonObject("data")
        }

        fun patch(path: String, body: String, clock: String) {
            val response =
                client.send(
                    HttpRequest.newBuilder(URI("$url/api/v1$path"))
                        .header("Content-Type", "application/json")
                        .header("Idempotency-Key", UUID.randomUUID().toString())
                        .header("X-Actual-Edit-Clock", clock)
                        .method("PATCH", HttpRequest.BodyPublishers.ofString(body))
                        .build(),
                    HttpResponse.BodyHandlers.ofString(),
                )
            assertEquals(200, response.statusCode(), response.body())
        }

        fun delete(path: String, body: String = "{}") {
            val response =
                client.send(
                    HttpRequest.newBuilder(URI("$url/api/v1$path"))
                        .header("Content-Type", "application/json")
                        .header("Idempotency-Key", UUID.randomUUID().toString())
                        .method("DELETE", HttpRequest.BodyPublishers.ofString(body))
                        .build(),
                    HttpResponse.BodyHandlers.ofString(),
                )
            assertEquals(200, response.statusCode(), response.body())
        }

        fun upload(bytes: ByteArray, path: String = "/imports/actual"): JsonObject {
            val response =
                client.send(
                    HttpRequest.newBuilder(URI("$url/api/v1$path"))
                        .header("Content-Type", "application/zip")
                        .header("Idempotency-Key", UUID.randomUUID().toString())
                        .POST(HttpRequest.BodyPublishers.ofByteArray(bytes))
                        .build(),
                    HttpResponse.BodyHandlers.ofString(),
                )
            assertEquals(201, response.statusCode(), response.body())
            return JsonParser.parseString(response.body()).asJsonObject.getAsJsonObject("data")
        }

        fun get(path: String) =
            JsonParser.parseString(
                    client
                        .send(
                            HttpRequest.newBuilder(URI("$url/api/v1$path")).GET().build(),
                            HttpResponse.BodyHandlers.ofString(),
                        )
                        .body()
                )
                .asJsonObject

        override fun close() {
            child.destroy()
            child.waitFor()
            directory.toFile().deleteRecursively()
        }
    }

    @Test
    fun `imported ordering keys preserve public companion order in cached native restart`() {
        Server().use { server ->
            val directory = Path.of("../../actualBudget/tests/fixtures")
            val expected =
                JsonParser.parseString(Files.readString(directory.resolve("android-order.json")))
                    .asJsonObject
            val stage =
                server.upload(Files.readAllBytes(directory.resolve("android-order-source.zip")))
            server.post("/imports/${stage["id"].asString}/activate", "{}")
            val store = MemoryStore()
            var app = ApiExpenseService(store)
            app.connect(server.url)

            val ids =
                expected.getAsJsonArray("snapshots")[0].asJsonObject.getAsJsonArray("ids").map {
                    it.asString
                }
            assertEquals(ids, app.state().expenses.map { it.id })
            val bytes = store.raw
            app = ApiExpenseService(store)
            app.markOffline()
            assertEquals(ids, app.state().expenses.map { it.id })
            assertEquals(bytes, store.raw)
            app.refresh()
            assertEquals(ids, app.state().expenses.map { it.id })
        }
    }

    @Test
    fun `simple rule associations cache while native overrides never rerun rules or teach defaults`() {
        Server().use { server ->
            val base = "/budgets/${server.budget}"
            val food = UUID.randomUUID().toString()
            val travel = UUID.randomUUID().toString()
            val utilities = UUID.randomUUID().toString()
            val payee = UUID.randomUUID().toString()
            val rule = UUID.randomUUID().toString()
            for ((id, name) in
                listOf(food to "Food", travel to "Travel", utilities to "Utilities")) server.post(
                "$base/categories",
                """{"id":"$id","name":"$name","is_income":false}""",
            )
            server.post("$base/payees", """{"id":"$payee","name":"Shop"}""")
            val snapshot =
                server.get("$base/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post(
                "$base/rules",
                """{"id":"$rule","stage":null,"conditionsOp":"and","conditions":[{"field":"payee","op":"is","value":"$payee"}],"actions":[{"op":"set","field":"category","value":"$food"}],"snapshot":"$snapshot"}""",
            )
            for (days in 1L..2L) {
                val date = java.time.LocalDate.now().minusDays(days)
                server.post(
                    "$base/transactions",
                    """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":-100,"date":"$date","category_id":"$travel","payee_id":"$payee"}""",
                )
            }
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            assertEquals(food, app.state().payeeCategoryAssociations[payee])
            app.markOffline()
            val expense =
                app.saveExpense(
                    ExpenseDraft("4.56", payee = "Shop", payeeId = payee, categoryId = travel)
                )
            val pending =
                JsonParser.parseString(store.raw)
                    .asJsonObject
                    .getAsJsonArray("pending")[0]
                    .asJsonObject
            val body = pending["body"].asString
            assertFalse(JsonParser.parseString(body).asJsonObject.has("learn_categories"))
            val restarted = ApiExpenseService(store)
            assertEquals(food, restarted.state().payeeCategoryAssociations[payee])
            val now = server.get("$base/checkpoint").getAsJsonObject("data")["snapshot"].asString
            val clock =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusMillis(1)) + "-0000-0000000000000001"
            server.patch(
                "$base/rules/$rule",
                """{"snapshot":"$now","actions":[{"op":"set","field":"category","value":"$utilities"}]}""",
                clock,
            )
            restarted.refresh()
            assertTrue(restarted.state().online, restarted.state().error)
            assertEquals(0, restarted.state().pendingCount)
            assertEquals(utilities, restarted.state().payeeCategoryAssociations[payee])
            assertEquals(travel, restarted.state().expenses.single { it.id == expense }.categoryId)
            val clearing = restarted.beginEdit(expense)
            restarted.saveExpense(clearing.original!!.copy(categoryId = null), clearing)
            restarted.refresh()
            assertNull(restarted.state().expenses.single { it.id == expense }.categoryId)
            val rules = server.get("$base/rules").getAsJsonArray("data")
            assertEquals(1, rules.size())
            assertEquals(
                utilities,
                rules[0].asJsonObject.getAsJsonArray("actions")[0].asJsonObject["value"].asString,
            )
            val splitOnly =
                server.get("$base/checkpoint").getAsJsonObject("data")["snapshot"].asString
            val splitClock =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusMillis(2)) + "-0000-0000000000000001"
            server.patch(
                "$base/rules/$rule",
                """{"snapshot":"$splitOnly","actions":[{"op":"set","field":"category","value":"$utilities","options":{"splitIndex":1}}]}""",
                splitClock,
            )
            assertEquals(0, server.get("$base/rules/associations").getAsJsonObject("data").size())
            restarted.refresh()
            assertFalse(restarted.state().payeeCategoryAssociations.containsKey(payee))
            val ordinary =
                server.get("$base/checkpoint").getAsJsonObject("data")["snapshot"].asString
            val ordinaryClock =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusMillis(3)) + "-0000-0000000000000001"
            server.patch(
                "$base/rules/$rule",
                """{"snapshot":"$ordinary","actions":[{"op":"set","field":"category","value":"$utilities"}]}""",
                ordinaryClock,
            )
            restarted.refresh()
            assertEquals(utilities, restarted.state().payeeCategoryAssociations[payee])
            val current =
                server.get("$base/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post(
                "$base/rules",
                """{"id":"${UUID.randomUUID()}","stage":null,"conditionsOp":"and","conditions":[{"field":"payee","op":"is","value":"$payee"}],"actions":[{"op":"set","field":"category","value":"$food"}],"snapshot":"$current"}""",
            )
            restarted.refresh()
            assertFalse(restarted.state().payeeCategoryAssociations.containsKey(payee))
            assertNull(restarted.state().expenses.single { it.id == expense }.categoryId)
        }
    }

    @Test
    fun rulesRespectManualCompanionWritesAndRefresh() {
        Server().use { server ->
            val root = "/budgets/${server.budget}"
            val rule = UUID.randomUUID().toString()
            val snapshot = server.get("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post("$root/rules", """{"id":"$rule","stage":null,"conditionsOp":"and","conditions":[{"field":"account","op":"is","value":"${server.account}"}],"actions":[{"op":"set","field":"amount","value":-556}],"snapshot":"$snapshot"}""")
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            assertTrue(app.state().payeeCategoryAssociations.isEmpty())
            app.markOffline()
            val expense = app.saveExpense(ExpenseDraft("4.56", notes = "Manual"))
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertTrue(restarted.state().online, restarted.state().error)
            assertEquals(0, restarted.state().pendingCount)
            val first = server.get("$root/transactions/$expense").getAsJsonObject("data")
            assertEquals(-456, first["amount"].asInt)
            assertEquals("Manual", first["notes"].asString)
            assertTrue(first["cleared"].asBoolean)
            val edit = restarted.beginEdit(expense)
            restarted.saveExpense(edit.original!!.copy(date = "2026-02-03"), edit)
            restarted.refresh()
            val edited = server.get("$root/transactions/$expense").getAsJsonObject("data")
            assertEquals(-456, edited["amount"].asInt)
            assertEquals("2026-02-03", edited["date"].asString)
            val current = server.get("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post("$root/rules/apply", """{"snapshot":"$current","ids":["$expense"],"rule_ids":["$rule"]}""", 200)
            restarted.refresh()
            assertTrue(restarted.state().online, restarted.state().error)
            val view = restarted.state().expenses.single { it.id == expense }
            assertEquals(-556, server.get("$root/transactions/$expense").getAsJsonObject("data")["amount"].asInt)
            assertEquals(556L, view.amountMinor)
            assertEquals("Manual", view.notes)
            assertEquals(0, restarted.state().pendingCount)
        }
    }

    @Test
    fun externalAutomationPreservesCompanionProjection() {
        Server().use { server ->
            val root = "/budgets/${server.budget}"
            val category = UUID.randomUUID().toString()
            server.post("$root/categories", """{"id":"$category","name":"Manual category","is_income":false}""")
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val entities = mutableSetOf<String>()
            val requests = mutableListOf<Pair<String, String>>()
            val transport = HttpTransport { url, method, headers, body ->
                requests += method to url
                val response = real.request(url, method, headers, body)
                if (url.contains("/changes?") && response.status == 200) {
                    JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject
                        .getAsJsonArray("data").forEach { entities += it.asJsonObject["entity"].asString }
                }
                response
            }
            val app = ApiExpenseService(store, transport = transport)
            app.connect(server.url)

            val original = app.saveExpense(ExpenseDraft("12.34", categoryId = category, notes = "Manual entry"))
            val removed = app.saveExpense(ExpenseDraft("1.00", notes = "Removed externally"))
            app.refresh()
            val cursor = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            fun snapshot() = server.get("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post("$root/notes/commands", """{"id":"remote:annotation","note":"External API note","snapshot":"${snapshot()}"}""", 200)
            val added = UUID.randomUUID().toString()
            val result = server.post("$root/transactions/batch", """{"snapshot":"${snapshot()}","max_graph_members":2,"operations":[{"action":"create","transaction":{"id":"$added","account_id":"${server.account}","amount":-333,"date":"2026-11-09","notes":"External purchase","cleared":true}},{"action":"update","id":"$original","fields":{"amount":-1300,"notes":"External correction"}},{"action":"delete","id":"$removed"}]}""", 200)
            assertEquals(3, result["operation_count"].asInt)
            requests.clear()
            app.refresh()
            assertNull(app.state().error)
            assertTrue(app.state().online)
            assertFalse(app.state().syncBlocked)
            assertEquals(0, app.state().pendingCount)
            assertEquals(setOf(original, added), app.state().expenses.map { it.id }.toSet())
            val corrected = app.state().expenses.single { it.id == original }
            assertEquals(1300L, corrected.amountMinor)
            assertEquals("External correction", corrected.notes)
            assertEquals(category, corrected.categoryId, "Unedited manual category survives external updates")
            val incoming = app.state().expenses.single { it.id == added }
            assertEquals(333L, incoming.amountMinor)
            assertEquals("External purchase", incoming.notes)
            assertNull(incoming.categoryId, "Plain API batches never infer native categories")
            assertTrue(entities.containsAll(setOf("notes", "transactions")), entities.toString())
            assertTrue(requests.all { it.first == "GET" }, "Refresh observes external work without automation writes")
            val next = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            assertNotEquals(cursor, next)
            assertEquals(snapshot(), next)
            val writes = store.writes
            app.refresh()
            assertEquals(writes, store.writes, "Unchanged foreign events do not rewrite the cache")
            val restarted = ApiExpenseService(store, transport = transport)
            restarted.refresh()
            assertNull(restarted.state().error)
            assertEquals(setOf(original, added), restarted.state().expenses.map { it.id }.toSet())
            assertEquals(2, server.get("$root/transactions").getAsJsonArray("data").size())
        }
    }

    @Test
    fun `planning changes advance native checkpoint without losing expenses`() {
        Server().use { server ->
            val base = "/budgets/${server.budget}"
            val category = UUID.randomUUID().toString()
            server.post(
                "$base/categories",
                """{"id":"$category","name":"Food","is_income":false}""",
            )
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val expense = app.saveExpense(ExpenseDraft("4.56", notes = "Before planning"))
            val checkpoint = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            fun plan(action: String) {
                val snapshot =
                    server
                        .get("$base/budget?month=2026-01")
                        .getAsJsonObject("data")["snapshot"]
                        .asString
                server.post(
                    "$base/budget",
                    """{"month":"2026-01","snapshot":"$snapshot",$action}""",
                    200,
                )
            }
            plan("\"action\":\"set\",\"category\":\"$category\",\"amount\":500")
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertEquals(expense, app.state().expenses.single().id)
            assertEquals(456, app.state().expenses.single().amountMinor)
            val next = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            assertNotEquals(checkpoint, next)
            val clock =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusMillis(1)) + "-0000-0000000000000001"
            server.patch("$base/transactions/$expense", """{"date":"2026-02-03"}""", clock)
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertEquals("2026-02-03", app.state().expenses.single().date)
            assertEquals(expense, app.state().expenses.single().id)
            assertEquals(
                server
                    .get("$base/budget?month=2026-01")
                    .getAsJsonObject("data")["snapshot"]
                    .asString,
                JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString,
            )
        }
    }

    @Test
    fun `file import metadata and remembered options preserve native incremental expenses and pending delivery`() {
        Server().use { server ->
            val root = "/budgets/${server.budget}"
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val entities = mutableSetOf<String>()
            val transport = HttpTransport { url, method, headers, body ->
                val response = real.request(url, method, headers, body)
                if (url.contains("/changes?") && response.status == 200) {
                    JsonParser.parseString(response.body.toString(Charsets.UTF_8))
                        .asJsonObject
                        .getAsJsonArray("data")
                        .forEach { entities += it.asJsonObject["entity"].asString }
                }
                response
            }
            val app = ApiExpenseService(store, transport = transport)
            app.connect(server.url)

            val first = app.saveExpense(ExpenseDraft("4.56", notes = "Native manual"))
            app.refresh()
            val checkpoint = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            app.markOffline()
            val pending = app.saveExpense(ExpenseDraft("1.23", notes = "Queued native"))
            val client = HttpClient.newHttpClient()
            val fixture =
                JsonParser.parseString(
                        Files.readString(
                            Path.of(
                                "../../actualBudget/tests/fixtures/transaction-format-flows.json"
                            )
                        )
                    )
                    .asJsonObject
                    .getAsJsonArray("cases")
                    .first { it.asJsonObject["id"].asString == "qfx" }
                    .asJsonObject
            val bytes = java.util.Base64.getDecoder().decode(fixture["bytes_base64"].asString)
            val uploaded =
                client.send(
                    HttpRequest.newBuilder(
                            URI(
                                "${server.url}/api/v1$root/accounts/${server.account}/transaction-imports?filename=bank.qfx"
                            )
                        )
                        .header("Idempotency-Key", UUID.randomUUID().toString())
                        .POST(HttpRequest.BodyPublishers.ofByteArray(bytes))
                        .build(),
                    HttpResponse.BodyHandlers.ofString(),
                )
            assertEquals(201, uploaded.statusCode(), uploaded.body())
            val stage =
                JsonParser.parseString(uploaded.body())
                    .asJsonObject
                    .getAsJsonObject("data")["id"]
                    .asString
            val cursor = server.get("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            val result =
                server.post(
                    "$root/transaction-imports/$stage/commit",
                    """{"snapshot":"$cursor","options":{"date_format":"yyyy mm dd"},"remember":true,"rows":[{"row":0,"action":"auto"}]}""",
                    200,
                )
            val imported = result.getAsJsonArray("added").single().asString
            app.refresh()
            assertNull(app.state().error)
            assertFalse(app.state().syncBlocked)
            assertEquals(0, app.state().pendingCount)
            assertEquals(
                setOf(first, pending, imported),
                app.state().expenses.map { it.id }.toSet(),
            )
            assertEquals(2000L, app.state().expenses.single { it.id == imported }.amountMinor)
            assertNull(
                app.state().expenses.single { it.id == first }.categoryId,
                "File rules must not rewrite a manual native category",
            )
            assertTrue(
                entities.containsAll(
                    setOf(
                        "transaction_import_settings",
                        "transaction_import_metadata",
                        "transactions",
                    )
                ),
                entities.toString(),
            )
            assertNotEquals(
                checkpoint,
                JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString,
            )
            val clock =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusMillis(1)) + "-0000-0000000000000001"
            server.patch(
                "$root/transactions/$first",
                """{"date":"2026-02-03"}""",
                clock,
            )
            app.refresh()
            assertNull(app.state().error)
            assertEquals(
                "2026-02-03",
                app.state().expenses.single { it.id == first }.date,
            )
            val writes = store.writes
            app.refresh()
            assertEquals(
                writes,
                store.writes,
                "An unchanged import feed must not rewrite the native cache",
            )
            val restarted = ApiExpenseService(store, transport = transport)
            restarted.refresh()
            assertNull(restarted.state().error)
            assertEquals(
                setOf(first, pending, imported),
                restarted.state().expenses.map { it.id }.toSet(),
            )
        }
    }

    @Test
    fun `budget allocation events preserve expenses and pending delivery without idle cache writes`() {
        Server().use { server ->
            val root = "/budgets/${server.budget}"
            val category = UUID.randomUUID().toString()
            val reserve = UUID.randomUUID().toString()
            server.post("$root/categories", """{"id":"$category","name":"Food","is_income":false}""")
            server.post("$root/categories", """{"id":"$reserve","name":"Reserve","is_income":false}""")
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val entities = mutableSetOf<String>()
            val requests = mutableListOf<Pair<String, String>>()
            val transport = HttpTransport { url, method, headers, body ->
                requests += method to url
                val response = real.request(url, method, headers, body)
                if (url.contains("/changes?") && response.status == 200) {
                    JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject
                        .getAsJsonArray("data").forEach { entities += it.asJsonObject["entity"].asString }
                }
                response
            }
            val app = ApiExpenseService(store, transport = transport)
            app.connect(server.url)

            val first = app.saveExpense(ExpenseDraft("4.56", categoryId = category, notes = "Existing expense"))
            app.refresh()
            val cursor = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            app.markOffline()
            val pending = app.saveExpense(ExpenseDraft("1.23", categoryId = category, notes = "Offline expense"))
            assertEquals(1, app.state().pendingCount)
            fun snapshot() = server.get("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post("$root/budget", """{"action":"set","month":"2026-11","snapshot":"${snapshot()}","category":"$category","amount":2500}""", 200)
            val before = server.get("$root/transactions").getAsJsonArray("data")
            assertEquals(1, before.size(), "Planning must not create a transaction")
            assertEquals(first, before.single().asJsonObject["id"].asString)
            assertEquals(-456L, before.single().asJsonObject["amount"].asLong)
            app.refresh()
            assertNull(app.state().error)
            assertFalse(app.state().syncBlocked)
            assertEquals(0, app.state().pendingCount)
            assertEquals(setOf(first, pending), app.state().expenses.map { it.id }.toSet())
            assertEquals(setOf(456L, 123L), app.state().expenses.map { it.amountMinor }.toSet())
            assertTrue(app.state().categories.any { it.id == category })
            assertTrue(entities.containsAll(setOf("budget_cells", "transactions")), entities.toString())
            assertFalse(requests.any { it.second.contains("/budget/templates") }, "The expense companion never executes budget automations")
            val savedCursor = JsonParser.parseString(store.raw).asJsonObject["checkpoint"].asString
            assertNotEquals(cursor, savedCursor)
            assertEquals(snapshot(), savedCursor)
            val writes = store.writes
            app.refresh()
            assertEquals(writes, store.writes, "An unchanged automation feed must not rewrite the cache")
            val restarted = ApiExpenseService(store, transport = transport)
            restarted.refresh()
            assertNull(restarted.state().error)
            assertEquals(setOf(first, pending), restarted.state().expenses.map { it.id }.toSet())
            assertEquals(0, restarted.state().pendingCount)
            assertEquals(2, server.get("$root/transactions").getAsJsonArray("data").size())
        }
    }

    @Test
    fun `location-only changes preserve native expense projection and pending offline edits`() {
        Server().use { server ->
            val base = "/budgets/${server.budget}"
            val payee = UUID.randomUUID().toString()
            val location = UUID.randomUUID().toString()
            server.post("$base/payees", """{"id":"$payee","name":"Local shop"}""")
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            app.markOffline()
            val expense = app.saveExpense(ExpenseDraft("4.56", payee = "Local shop"))
            server.post("$base/payee-locations", """{"id":"$location","payee_id":"$payee","latitude":40.123,"longitude":-74.456}""")
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertTrue(restarted.state().online, restarted.state().error)
            assertEquals(0, restarted.state().pendingCount)
            assertEquals(expense, restarted.state().expenses.single().id)
            assertEquals(listOf(payee), restarted.state().payees.map { it.id })
            server.patch("$base/payee-locations/$location", """{"deleted":true}""", DateTimeFormatterBuilder().appendInstant(3).toFormatter().format(Instant.now().plusMillis(1)) + "-0000-0000000000000001")
            restarted.refresh()
            assertTrue(restarted.state().online, restarted.state().error)
            assertEquals(456, restarted.state().expenses.single().amountMinor)
            assertFalse(store.raw!!.contains("latitude"))
            assertFalse(store.raw!!.contains("longitude"))
        }
    }

    @Test
    fun `genuine incremental transfers never become merchant choices or expenses`() {
        Server().use { server ->
            val account = UUID.randomUUID().toString()
            val first = UUID.randomUUID().toString()
            val second = UUID.randomUUID().toString()
            val base = "/budgets/${server.budget}"
            server.post("$base/accounts", """{"id":"$account","name":"Credit card","opening_balance":0,"opening_date":"2026-01-01"}""")
            val store = MemoryStore()
            val reads = mutableListOf<String>()
            val real = BudgetHttpTransport()
            val network = HttpTransport { url, method, headers, body ->
                if (method == "GET") reads += url
                real.request(url, method, headers, body)
            }
            val app = ApiExpenseService(store, transport = network)
            app.connect(server.url)

            assertTrue(app.state().payees.isEmpty())
            reads.clear()
            server.post("$base/transfers", """{"id":"$first","counterpart_id":"$second","account_id":"${server.account}","to_account_id":"$account","amount":-2500,"date":"2026-01-01"}""")
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertTrue(reads.any { it.contains("/changes?") })
            assertFalse(reads.any { it.contains("/payees?") }, "Transfer identities must arrive through genuine incremental delivery")
            assertTrue(app.state().payees.isEmpty())
            assertTrue(app.state().expenses.isEmpty())
            val retained = server.get("$base/companion?account_id=${server.account}&retain=$first").getAsJsonObject("data")
            assertEquals(first, retained.getAsJsonArray("unavailable").single().asString)
            assertTrue(retained.getAsJsonArray("retained").isEmpty)
            val restarted = ApiExpenseService(store, transport = network)
            assertTrue(restarted.state().payees.isEmpty())
            restarted.refresh()
            assertTrue(restarted.state().expenses.isEmpty())
            val expense = restarted.saveExpense(ExpenseDraft("3.00", payee = "Merchant"))
            assertNotNull(expense)
            assertEquals(listOf("Merchant"), restarted.state().payees.map { it.name })
        }
    }

    @Test
    fun `pair edits never restore native expense eligibility`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("12.34", notes = "Converted payment"))
            app.refresh()
            val base = "/budgets/${server.budget}"
            val account = UUID.randomUUID().toString()
            val peer = UUID.randomUUID().toString()
            server.post("$base/accounts", """{"id":"$account","name":"Card","opening_balance":0,"opening_date":"2026-01-01"}""")
            server.post("$base/transactions", """{"id":"$peer","account_id":"$account","amount":1234,"date":"2026-01-01","cleared":true}""")
            server.post("$base/transfers/link", """{"id":"$id","counterpart_id":"$peer"}""", 200)
            val clock = DateTimeFormatterBuilder().appendInstant(3).toFormatter().format(Instant.now().plusSeconds(1)) + "-0000-ffffffffffffffff"
            server.patch("$base/transactions/$id", """{"amount":-1300}""", clock)
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertTrue(app.state().expenses.isEmpty())
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertTrue(restarted.state().expenses.isEmpty())
            assertTrue(restarted.state().payees.isEmpty())
            val retained = server.get("$base/companion?account_id=${server.account}&retain=$id").getAsJsonObject("data")
            assertEquals(id, retained.getAsJsonArray("unavailable").single().asString)
        }
    }

    @Test
    fun `saved expenses stay eligible after edits and restart without settlement`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("12.34"))
            app.refresh()
            val path = "/budgets/${server.budget}/transactions/$id"
            server.patch(path, """{"notes":"Complete immediately","cleared":false}""", DateTimeFormatterBuilder().appendInstant(3).toFormatter().format(Instant.now().plusMillis(1)) + "-0000-ffffffffffffffff")
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals(id, restarted.state().expenses.single().id)
            assertEquals(1234, restarted.state().expenses.single().amountMinor)
            assertEquals("Complete immediately", restarted.state().expenses.single().notes)
            assertTrue(server.get(path).getAsJsonObject("data")["cleared"].asBoolean)
        }
    }

    @Test
    fun `recombined split becomes an ordinary editable expense immediately`() {
        Server().use { server ->
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("3.00", notes = "Native split"))
            app.refresh()
            val base = "/budgets/${server.budget}"
            val snapshot = server.get("$base/transactions")["snapshot"].asString
            server.post("$base/transactions/$id/splits", """{"snapshot":"$snapshot","children":[{"id":"${UUID.randomUUID()}","amount":-100},{"id":"${UUID.randomUUID()}","amount":-200}]}""", 200)
            app.refresh()
            assertTrue(app.state().expenses.isEmpty())
            val current = server.get("$base/transactions/$id/splits")
            server.post("$base/transactions/$id/splits", """{"action":"recombine","snapshot":${current["snapshot"]}}""", 200)
            app.refresh()
            assertEquals(id, app.state().expenses.single().id)
            assertTrue(app.state().expenses.single().editable)
        }
    }

    @Test
    fun `converted split preserves pending edit across restart until confirmed discard`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("7.89", notes = "Original"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            app.saveExpense(edit.original!!.copy(amount = "8.90"), edit)
            val pending = JsonParser.parseString(store.raw).asJsonObject.getAsJsonArray("pending")[0].asJsonObject.deepCopy()
            val snapshot = server.get("/budgets/${server.budget}/transactions")["snapshot"].asString
            server.post("/budgets/${server.budget}/transactions/$id/splits", """{"snapshot":"$snapshot","children":[{"id":"${UUID.randomUUID()}","amount":-789}]}""", 200)
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().splitRecovery)
            val restarted = ApiExpenseService(store)
            assertTrue(restarted.state().splitRecovery)
            val retained = JsonParser.parseString(store.raw).asJsonObject.getAsJsonArray("pending")[0].asJsonObject
            for (field in listOf("id", "path", "body", "clock", "epoch")) assertEquals(pending[field], retained[field])
            restarted.discardRejectedSplitEdit()
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount)
            assertTrue(restarted.state().expenses.isEmpty())
            assertEquals("Original", server.get("/budgets/${server.budget}/transactions/$id").getAsJsonObject("data")["notes"].asString)
        }
    }

    @Test
    fun `expense edit and delete captured before transfer conversion retain exact intent until discard`() {
        for (delete in listOf(false, true)) for (queueFirst in listOf(false, true)) Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("7.89", notes = "Original"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            fun queue() {
                if (delete) app.deleteExpense(edit) else app.saveExpense(edit.original!!.copy(amount = "8.90"), edit)
            }
            if (queueFirst) queue()
            val account = UUID.randomUUID().toString()
            val peer = UUID.randomUUID().toString()
            val base = "/budgets/${server.budget}"
            server.post("$base/accounts", """{"id":"$account","name":"Other","opening_balance":0,"opening_date":"2026-01-01"}""")
            server.post("$base/transactions", """{"id":"$peer","account_id":"$account","amount":789,"date":"2026-01-01"}""")
            server.post("$base/transfers/link", """{"id":"$id","counterpart_id":"$peer"}""", 200)
            if (!queueFirst) queue()
            val pending = JsonParser.parseString(store.raw).asJsonObject.getAsJsonArray("pending")[0].asJsonObject.deepCopy()
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().splitRecovery, app.state().toString())
            val restarted = ApiExpenseService(store)
            assertTrue(restarted.state().splitRecovery)
            val retained = JsonParser.parseString(store.raw).asJsonObject.getAsJsonArray("pending")[0].asJsonObject
            for (field in listOf("id", "path", "body", "clock", "epoch")) assertEquals(pending[field], retained[field])
            restarted.discardRejectedSplitEdit()
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount)
            assertTrue(restarted.state().expenses.isEmpty())
            val row = server.get("$base/transactions/$id").getAsJsonObject("data")
            assertEquals("Original", row["notes"].asString)
            assertEquals(peer, row["transfer_id"].asString)
            assertEquals(id, server.get("$base/transactions/$peer").getAsJsonObject("data")["transfer_id"].asString)
        }
    }

    @Test
    fun `no credentials select account create edit delete through real API and preserve remembered selection`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("12.34"))
            assertEquals(1, app.state().pendingCount)
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            val row =
                server.get("/budgets/${server.budget}/transactions/$id").getAsJsonObject("data")
            assertEquals(-1234, row["amount"].asLong)
            assertTrue(row["cleared"].asBoolean)
            val restarted = ApiExpenseService(store)
            assertEquals(server.account, restarted.state().expenseContext?.accountId)
            restarted.refresh()
            val edit = restarted.beginEdit(id)
            restarted.saveExpense(edit.original!!.copy(amount = "15.00", notes = "Updated"), edit)
            restarted.refresh()
            assertEquals(1500, restarted.state().expenses.single().amountMinor)
            restarted.deleteExpense(restarted.beginEdit(id))
            restarted.refresh()
            assertTrue(restarted.state().expenses.isEmpty(), restarted.state().error)
        }
    }

    @Test
    fun `remote close and deletion preserve unified history and automatic entry`() {
        Server().use { server ->
            for (amount in listOf(-100, 100)) server.post(
                "/budgets/${server.budget}/transactions",
                """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":$amount,"date":"2026-01-01"}""",
            )
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            server.post("/budgets/${server.budget}/accounts/${server.account}/close", "{}", 200)
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertNotNull(app.state().expenseContext)
            assertNotEquals(server.account, app.state().expenseContext?.accountId)
            server.post("/budgets/${server.budget}/accounts/${server.account}/reopen", "{}", 200)
            app.refresh()
            assertNotNull(app.state().expenseContext)

            server.delete(
                "/budgets/${server.budget}/accounts/${server.account}",
                """{"confirm_name":"Spending"}""",
            )
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertNotNull(app.state().expenseContext)
            assertNotEquals(server.account, app.state().expenseContext?.accountId)
        }
    }

    @Test
    fun `more than one hundred offline expenses drain after remote metadata changes`() {
        Server().use { server ->
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            app.markOffline()
            val ids = (1..101).map { app.saveExpense(ExpenseDraft("1.00", notes = "Offline $it")) }
            assertEquals(101, app.state().pendingCount)
            server.post(
                "/budgets/${server.budget}/accounts",
                """{"id":"${UUID.randomUUID()}","name":"Remote account","opening_balance":0,"opening_date":"2026-01-01"}""",
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertTrue(app.state().online, app.state().error)
            val rows =
                server
                    .get("/budgets/${server.budget}/transactions?limit=1000")
                    .getAsJsonArray("data")
            assertEquals(ids.toSet(), rows.map { it.asJsonObject["id"].asString }.toSet())
            assertEquals(-10100, rows.sumOf { it.asJsonObject["amount"].asLong })
        }
    }

    @Test
    fun `committed lost acknowledgement reconciles before restored cursor recovery`() =
        committedReceiptSurvives("restore")

    @Test
    fun `committed lost acknowledgement reconciles while original account stays closed`() =
        committedReceiptSurvives("close")

    @Test
    fun `committed lost acknowledgement reconciles after original account deletion`() =
        committedReceiptSurvives("delete")

    private fun committedReceiptSurvives(lifecycle: String) {
        Server().use { server ->
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val requests = mutableListOf<Triple<String, Map<String, String>, String>>()
            var lose = true
            val network = HttpTransport { url, method, headers, body ->
                val response = real.request(url, method, headers, body)
                if (method == "POST" && url.endsWith("/transactions")) {
                    requests += Triple(url, headers, body!!.toString(Charsets.UTF_8))
                    if (lose) {
                        lose = false
                        throw IOException("Committed acknowledgement lost")
                    }
                }
                response
            }
            val app = ApiExpenseService(store, transport = network)
            app.connect(server.url)

            val point = server.post("/budgets/${server.budget}/backups", "{}")
            val id =
                app.saveExpense(ExpenseDraft("8.91", notes = "Receipt survives lifecycle change"))
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            assertEquals(
                -891,
                server
                    .get("/budgets/${server.budget}/transactions/$id")
                    .getAsJsonObject("data")["amount"]
                    .asLong,
            )
            when (lifecycle) {
                "restore" ->
                    server.post(
                        "/budgets/${server.budget}/backups/${point["id"].asString}/restore",
                        """{"confirm_name":"Native fixture"}""",
                        200,
                    )
                "close" -> {
                    server.post(
                        "/budgets/${server.budget}/transactions",
                        """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":891,"date":"2026-01-01"}""",
                    )
                    server.post(
                        "/budgets/${server.budget}/accounts/${server.account}/close",
                        "{}",
                        200,
                    )
                }
                "delete" ->
                    server.delete(
                        "/budgets/${server.budget}/accounts/${server.account}",
                        """{"confirm_name":"Spending"}""",
                    )
            }
            val restarted = ApiExpenseService(store, transport = network)
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount, restarted.state().error)
            assertTrue(restarted.state().online, restarted.state().error)
            assertFalse(restarted.state().syncBlocked)
            assertEquals(2, requests.size)
            assertEquals(requests[0].first, requests[1].first)
            assertEquals(requests[0].second, requests[1].second - "X-Actual-Receipt-Only")
            assertEquals("true", requests[1].second["X-Actual-Receipt-Only"])
            assertEquals(requests[0].third, requests[1].third)
            val current =
                server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data")
            assertEquals(
                if (lifecycle == "close") 1 else 0,
                current.count { it.asJsonObject["id"].asString == id },
            )
            assertEquals(
                if (lifecycle == "close") listOf(id) else emptyList(),
                restarted.state().expenses.map { it.id },
                "Unified history keeps closed-account entries but never resurrects restored or deleted entries",
            )
            if (lifecycle != "restore") assertNotNull(restarted.state().expenseContext)
            restarted.refresh()
            assertEquals(2, requests.size, "Acknowledged receipt must leave the durable queue")
        }
    }


    @Test
    fun `closed historical account delivers queued expense with identical delivery identity`() {
        Server().use { server ->
            for (amount in listOf(-100, 100)) server.post(
                "/budgets/${server.budget}/transactions",
                """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":$amount,"date":"2026-01-01"}""",
            )
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val requests = mutableListOf<Triple<String, Map<String, String>, String>>()
            var fail = true
            val network = HttpTransport { url, method, headers, body ->
                if (
                    method == "POST" &&
                        url.endsWith("/transactions") &&
                        headers["X-Actual-Receipt-Only"] == null
                ) {
                    requests += Triple(url, headers, body!!.toString(Charsets.UTF_8))
                    if (fail) throw IOException("Connection lost before sending")
                }
                real.request(url, method, headers, body)
            }
            val app = ApiExpenseService(store, transport = network)
            app.connect(server.url)

            app.markOffline()
            val id = app.saveExpense(ExpenseDraft("4.56", notes = "Keep this purchase"))
            app.refresh()
            assertEquals(1, requests.size)
            server.post("/budgets/${server.budget}/accounts/${server.account}/close", "{}", 200)
            fail = false
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertFalse(app.state().syncBlocked)
            assertTrue(app.state().expenses.any { it.id == id && !it.pending })
            val restarted = ApiExpenseService(store, transport = network)
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount, restarted.state().error)
            assertEquals(2, requests.size)
            assertEquals(requests[0], requests[1])
            assertEquals(
                1,
                server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data").count {
                    it.asJsonObject["id"].asString == id
                },
            )
            assertEquals(
                -456,
                server
                    .get("/budgets/${server.budget}/transactions/$id")
                    .getAsJsonObject("data")["amount"]
                    .asLong,
            )
        }
    }

    @Test
    fun `deleted historical account preserves pending edit while automatic destination advances`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("7.89", notes = "Original"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            app.saveExpense(edit.original!!.copy(amount = "8.90"), edit)
            val second = UUID.randomUUID().toString()
            server.post(
                "/budgets/${server.budget}/accounts",
                """{"id":"$second","name":"Other","opening_balance":0,"opening_date":"2026-01-01"}""",
            )
            server.delete(
                "/budgets/${server.budget}/accounts/${server.account}",
                """{"confirm_name":"Spending"}""",
            )
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            assertTrue(
                app.state().online,
                "Reachable recovery is distinct from network disconnection",
            )
            assertTrue(app.state().syncBlocked)
            assertTrue(app.state().error.orEmpty().contains("deleted"), app.state().error)
            assertEquals(second, app.state().expenseContext?.accountId)
            assertEquals(890L, app.state().expenses.single().amountMinor)
            assertTrue(app.state().expenses.single().pending)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals(id, restarted.state().expenses.single().id)
            restarted.refresh()
            assertEquals(id, restarted.state().expenses.single().id)

            assertEquals(second, restarted.state().expenseContext?.accountId)
            assertEquals(1, restarted.state().pendingCount)
            restarted.refresh()
            assertTrue(restarted.state().syncBlocked)
            assertTrue(restarted.state().error.orEmpty().contains("Spending"))

            assertEquals(id, restarted.state().expenses.single().id)
            assertFailsWith<IllegalArgumentException> {
                restarted.saveExpense(ExpenseDraft("1.00", context = edit.context))
            }
            assertTrue(
                server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data").isEmpty
            )
            assertEquals(1, restarted.state().pendingCount)
        }
    }

    @Test
    fun `payee choices follow merge removal and unrelated expense edits`() {
        Server().use { server ->
            val base = "/budgets/${server.budget}"
            val source = UUID.randomUUID().toString()
            val target = UUID.randomUUID().toString()
            server.post("$base/payees", """{"id":"$source","name":"Old merchant"}""")
            server.post("$base/payees", """{"id":"$target","name":"Current merchant"}""")
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("4.56", payee = "Old merchant"))
            app.refresh()
            val beforeMerge = app.beginEdit(id)
            server.post(
                "$base/payees/batch",
                """{"action":"merge","ids":["$source"],"target_id":"$target"}""",
                200,
            )
            app.refresh()
            assertEquals(listOf(target), app.state().payees.map { it.id })
            assertEquals(target, app.state().expenses.single().payeeId)
            assertEquals("Current merchant", app.state().expenses.single().payee)
            app.saveExpense(beforeMerge.original!!.copy(notes = "Edited after merge"), beforeMerge)
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertEquals(
                target,
                server.get("$base/transactions/$id").getAsJsonObject("data")["payee_id"].asString,
            )
            val beforeRemoval = app.beginEdit(id)
            server.post("$base/payees/batch", """{"action":"remove","ids":["$target"]}""", 200)
            app.refresh()
            assertTrue(app.state().payees.isEmpty())
            assertNull(app.state().expenses.single().payeeId)
            assertEquals("", app.state().expenses.single().payee)
            app.saveExpense(
                beforeRemoval.original!!.copy(notes = "Edited after removal"),
                beforeRemoval,
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            val row = server.get("$base/transactions/$id").getAsJsonObject("data")
            assertTrue(row["payee_id"].isJsonNull)
            assertEquals("Edited after removal", row["notes"].asString)
            val recreated = app.saveExpense(ExpenseDraft("1.00", payee = "Current merchant"))
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            val newPayee =
                server
                    .get("$base/transactions/$recreated")
                    .getAsJsonObject("data")["payee_id"]
                    .asString
            assertNotEquals(target, newPayee)
            assertEquals(listOf(newPayee), app.state().payees.map { it.id })
        }
    }

    @Test
    fun `pending merged payee alias retries original bytes and preserves inline payee dependencies`() {
        Server().use { server ->
            val base = "/budgets/${server.budget}"
            val source = UUID.randomUUID().toString()
            val target = UUID.randomUUID().toString()
            server.post("$base/payees", """{"id":"$source","name":"Offline alias"}""")
            server.post("$base/payees", """{"id":"$target","name":"Canonical merchant"}""")
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val requests = mutableListOf<Triple<String, Map<String, String>, String>>()
            var fail = true
            val network = HttpTransport { url, method, headers, body ->
                if (
                    method == "POST" &&
                        url.endsWith("/transactions") &&
                        headers["X-Actual-Receipt-Only"] == null
                ) {
                    requests += Triple(url, headers, body!!.toString(Charsets.UTF_8))
                    if (fail) {
                        fail = false
                        throw IOException("Offline before sending")
                    }
                }
                real.request(url, method, headers, body)
            }
            val app = ApiExpenseService(store, transport = network)
            app.connect(server.url)

            app.markOffline()
            val aliasExpense = app.saveExpense(ExpenseDraft("1.11", payee = "Offline alias"))
            val inlineExpense = app.saveExpense(ExpenseDraft("2.22", payee = "Queued shop"))
            val dependentExpense = app.saveExpense(ExpenseDraft("3.33", payee = "Queued shop"))
            assertEquals(1, app.state().payees.count { it.name == "Queued shop" })
            app.refresh()
            assertEquals(3, app.state().pendingCount)
            assertEquals(
                source,
                JsonParser.parseString(requests.single().third).asJsonObject["payee_id"].asString,
            )
            server.post(
                "$base/payees/batch",
                """{"action":"merge","ids":["$source"],"target_id":"$target"}""",
                200,
            )
            val restarted = ApiExpenseService(store, transport = network)
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount, restarted.state().error)
            assertEquals(4, requests.size)
            assertEquals(requests[0], requests[1])
            assertEquals(
                target,
                server
                    .get("$base/transactions/$aliasExpense")
                    .getAsJsonObject("data")["payee_id"]
                    .asString,
            )
            val inlinePayee =
                server
                    .get("$base/transactions/$inlineExpense")
                    .getAsJsonObject("data")["payee_id"]
                    .asString
            assertEquals(
                inlinePayee,
                server
                    .get("$base/transactions/$dependentExpense")
                    .getAsJsonObject("data")["payee_id"]
                    .asString,
            )
            assertEquals(setOf(target, inlinePayee), restarted.state().payees.map { it.id }.toSet())
            assertEquals(
                "Canonical merchant",
                restarted.state().expenses.single { it.id == aliasExpense }.payee,
            )
            assertEquals(3, server.get("$base/transactions").getAsJsonArray("data").size())
        }
    }

    @Test
    fun `YNAB migrations retain native ordinary expenses references and offline delivery`() {
        val directory = Path.of("../../actualBudget/tests/fixtures")
        val oracle =
            JsonParser.parseString(Files.readString(directory.resolve("ynab-source.json")))
                .asJsonObject
        for (case in oracle.getAsJsonArray("cases")) {
            Server().use { server ->
                val source = case.asJsonObject
                val ynab4 = source["type"].asString == "ynab4"
                val file = source.getAsJsonObject("input")["filename"].asString
                val path =
                    if (ynab4) "/imports/ynab4?filename=companion.ynab4.zip"
                    else "/imports/nynab?filename=companion.json"
                val stage = server.upload(Files.readAllBytes(directory.resolve("ynab/$file")), path)
                val budget = server.budget
                server.post("/imports/${stage["id"].asString}/activate", "{}")
                val snapshot = source.getAsJsonObject("snapshot")
                val expected =
                    snapshot.getAsJsonArray("ledgers")
                        .flatMap { it.asJsonObject.getAsJsonArray("transactions").map { row -> row.asJsonObject } }
                        .filter {
                            it["amount"].asLong < 0 &&
                                !it["is_parent"].asBoolean &&
                                it["transfer_id"].isJsonNull
                        }
                        .map {
                            Triple(
                                -it["amount"].asLong,
                                it["date"].asString,
                                "", // Legacy file import omits source notes.
                            )
                        }
                        .sortedBy { it.toString() }
                val store = MemoryStore()
                val app = ApiExpenseService(store)
                app.connect(server.url)

                assertNull(app.state().error)
                assertEquals(
                    expected,
                    app.state()
                        .expenses
                        .map { Triple(it.amountMinor, it.date, it.notes) }
                        .sortedBy { it.toString() },
                )
                assertTrue(app.state().categories.isNotEmpty())
                assertTrue(app.state().payees.isNotEmpty())
                val oldIds = app.state().expenses.map { it.id }.toSet()
                app.markOffline()
                val id = app.saveExpense(ExpenseDraft("1.23", notes = "YNAB companion offline"))
                assertEquals(1, app.state().pendingCount)
                val raw = store.raw
                val restarted = ApiExpenseService(store)
                assertEquals(raw, store.raw, "Restart does not recapture pending request")
                assertEquals(1, restarted.state().pendingCount)
                restarted.refresh()
                assertNull(restarted.state().error)
                assertEquals(0, restarted.state().pendingCount)
                assertEquals(oldIds + id, restarted.state().expenses.map { it.id }.toSet())
                assertEquals(123L, restarted.state().expenses.single { it.id == id }.amountMinor)
                restarted.refresh()
                val rows =
                    server
                        .get("/budgets/$budget/transactions?limit=1000")
                        .getAsJsonArray("data")
                        .map { it.asJsonObject }
                assertEquals(1, rows.count { it["id"].asString == id })
                assertEquals(-123L, rows.single { it["id"].asString == id }["amount"].asLong)
            }
        }
    }

    @Test
    fun `imported transfer payees stay outside native merchant choices`() {
        Server().use { server ->
            val fixtures =
                Path.of(
                    System.getenv("ACTUALBUDGET_FIXTURE_DIR") ?: "../../actualBudget/tests/fixtures"
                )
            val fixture =
                JsonParser.parseString(
                        Files.readString(fixtures.resolve("payees-tags-import.json"))
                    )
                    .asJsonObject
            val stage =
                server.upload(Files.readAllBytes(fixtures.resolve("payees-tags-import.zip")))
            server.post("/imports/${stage["id"].asString}/activate", "{}")
            val budget = server.budget
            val transfer = fixture["transfer"].asString
            val publicPayees =
                server
                    .get("/budgets/$budget/payees?include_deleted=true&include_transfers=true")
                    .getAsJsonArray("data")
            assertTrue(
                publicPayees.any {
                    it.asJsonObject["id"].asString == transfer &&
                        !it.asJsonObject["transfer_account_id"].isJsonNull
                }
            )
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            assertEquals(3, publicPayees.size())
            assertTrue(app.state().payees.isEmpty())
            assertFalse(app.state().payees.any { it.id == transfer })
            app.refresh()
            assertTrue(app.state().payees.isEmpty())
        }
    }

    @Test
    fun `category choices remain visible until deletion`() {
        Server().use { server ->
            val group = UUID.randomUUID().toString()
            val category = UUID.randomUUID().toString()
            val base = "/budgets/${server.budget}"
            server.post("$base/category-groups", """{"id":"$group","name":"Everyday"}""")
            server.post(
                "$base/categories",
                """{"id":"$category","name":"Food","group_id":"$group"}""",
            )
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            assertEquals(listOf(category), app.state().categories.map { it.id })
            val expense = app.saveExpense(ExpenseDraft("1.23", categoryId = category))
            app.refresh()
            val originalEdit = app.beginEdit(expense)
            assertFalse(server.get("$base/categories/$category").getAsJsonObject("data")["hidden"].asBoolean)
            assertFalse(server.get("$base/categories/$category").getAsJsonObject("data")["group_hidden"].asBoolean)
            app.saveExpense(originalEdit.original!!.copy(notes = "Keep existing category"), originalEdit)
            app.refresh()
            assertEquals(listOf(category), app.state().categories.map { it.id })
            assertEquals(0, app.state().pendingCount, app.state().error)
            server.delete("$base/categories/$category", """{"replacement_id":null}""")
            app.refresh()
            assertTrue(app.state().categories.isEmpty())
            assertNull(app.state().expenses.single().categoryId)
        }
    }

    @Test
    fun `offline capture survives restart and later has one financial effect with original retry bytes`() {
        Server().use { server ->
            val store = MemoryStore()
            val requests = mutableListOf<List<String>>()
            var lose = true
            val transport = BudgetHttpTransport()
            val network = HttpTransport { url, method, headers, body ->
                val response = transport.request(url, method, headers, body)
                if (method == "POST" && url.endsWith("/transactions")) {
                    requests +=
                        listOf(
                            headers["Idempotency-Key"].orEmpty(),
                            headers["X-Actual-Edit-Clock"].orEmpty(),
                            headers["X-Actual-Budget-Epoch"].orEmpty(),
                            body!!.toString(Charsets.UTF_8),
                        )
                    if (lose) {
                        lose = false
                        throw IOException("Acknowledgement lost")
                    }
                }
                response
            }
            val app = ApiExpenseService(store, transport = network)
            app.connect(server.url)

            app.markOffline()
            val id = app.saveExpense(ExpenseDraft("8.91", date = "2001-01-01", notes = "offline"))
            assertTrue(app.state().expenses.single().editable)
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            val restored = ApiExpenseService(store, transport = network)
            assertEquals(id, restored.state().expenses.single().id)
            restored.refresh()
            assertEquals(0, restored.state().pendingCount, restored.state().error)
            assertEquals(2, requests.size)
            assertEquals(requests[0], requests[1])
            assertEquals(
                1,
                server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data").size(),
            )
            val writes = store.writes
            restored.refresh()
            assertEquals(
                writes,
                store.writes,
                "Unchanged foreground refresh must not rewrite durable state",
            )
            restored.markOffline()
            assertFailsWith<IllegalArgumentException> { restored.beginEdit(id) }
        }
    }

    @Test
    fun `latest ten across sources retain only last backdated addition and pending expenses`() {
        Server().use { server ->
            repeat(15) { index ->
                server.post(
                    "/budgets/${server.budget}/transactions",
                    """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":-100,"date":"2026-01-01","notes":"Browser $index"}""",
                )
            }
            server.post(
                "/budgets/${server.budget}/transactions",
                """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":100,"date":"2027-01-01","notes":"Income excluded"}""",
            )
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            assertEquals(10, app.state().expenses.size)
            assertFalse(app.state().expenses.any { it.notes == "Income excluded" })
            val first = app.saveExpense(ExpenseDraft("1.00", date = "2000-01-01"))
            app.refresh()
            assertEquals(11, app.state().expenses.size)
            assertEquals(first, app.state().expenses.last().id)
            val editing = app.beginEdit(first)
            app.saveExpense(editing.original!!.copy(notes = "still retained"), editing)
            app.refresh()
            assertEquals(first, app.state().expenses.last().id)
            val second = app.saveExpense(ExpenseDraft("2.00", date = "2000-01-02"))
            app.refresh()
            assertEquals(11, app.state().expenses.size)
            assertEquals(second, app.state().expenses.last().id)
            assertFalse(app.state().expenses.any { it.id == first })
            app.markOffline()
            val third =
                app.saveExpense(ExpenseDraft("3.00", date = "1999-01-01", payee = "Offline shop"))
            val fourth =
                app.saveExpense(ExpenseDraft("4.00", date = "1999-01-02", payee = "Offline shop"))
            app.deleteExpense(app.beginEdit(third))
            assertFalse(app.state().expenses.any { it.id == third })
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertEquals(fourth, app.state().expenses.last().id)
            assertEquals(
                1,
                server.get("/budgets/${server.budget}/payees").getAsJsonArray("data").size(),
            )
        }
    }

    @Test
    fun `interrupted online edit merges independent fields and unpublished deletion loses to newer edit`() {
        Server().use { server ->
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("1.00"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            app.saveExpense(edit.original!!.copy(amount = "2.00"), edit)
            fun clock(seconds: Long) =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusSeconds(seconds)) + "-0000-ffffffffffffffff"
            server.patch(
                "/budgets/${server.budget}/transactions/$id",
                """{"date":"2026-02-01"}""",
                clock(1),
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertEquals(200, app.state().expenses.single().amountMinor)
            assertEquals("2026-02-01", app.state().expenses.single().date)
            app.deleteExpense(app.beginEdit(id))
            server.patch(
                "/budgets/${server.budget}/transactions/$id",
                """{"date":"2026-02-02"}""",
                clock(2),
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertEquals("2026-02-02", app.state().expenses.single().date)
            assertTrue(app.state().notice!!.contains("Deletion cancelled"))
        }
    }

    @Test
    fun `pinned Actual export retains eligible identities and excludes income refunds and tombstone`() {
        Server().use { server ->
            val archive =
                Files.readAllBytes(Path.of("../../actualBudget/tests/fixtures/import-26.10.0.zip"))
            val stage = server.upload(archive)
            val imported = server.budget
            server.post("/imports/${stage["id"].asString}/activate", "{}")
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)

            assertEquals(
                listOf(
                    "04000000-0000-4000-8000-000000000005",
                    "b958973e-223a-4a68-8651-8d3d6c47c74e",
                    "04000000-0000-4000-8000-000000000002",
                ),
                app.state().expenses.map { it.id },
            )
            assertEquals(listOf(100L, 5000L, 1200L), app.state().expenses.map { it.amountMinor })
        }
    }

    @Test
    fun `incompatible response preserves pending body and automatic ledger reuses cached references`() {
        Server().use { server ->
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val reads = mutableListOf<String>()
            var incompatible = false
            var missingUnifiedLedger = false
            val transport = HttpTransport { url, method, headers, body ->
                assertFalse(headers.keys.any { it.equals("Authorization", true) })
                if (method == "GET") reads += url
                if (incompatible && url.endsWith("/info"))
                    org.actualexpenses.core.protocol.HttpResponse(
                        200,
                        "application/json",
                        """{"data":{"api_version":99}}""".toByteArray(),
                    )
                else {
                    val response = real.request(url, method, headers, body)
                    if (missingUnifiedLedger && url.endsWith("/info")) {
                        val info = JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject
                        info.getAsJsonObject("data").getAsJsonArray("capabilities").remove(com.google.gson.JsonPrimitive("unified_ledger"))
                        response.copy(body = info.toString().toByteArray())
                    } else response
                }
            }
            val app = ApiExpenseService(store, transport = transport)
            app.connect(server.url)
            reads.clear()
            app.refresh()
            assertFalse(
                reads.any { it.contains("/payees?") || it.contains("/categories?") },
                "Unchanged refresh must reuse coherent cached references",
            )
            app.markOffline()
            app.saveExpense(ExpenseDraft("5.00"))
            val original = store.raw
            incompatible = true
            app.refresh()
            assertEquals(original, store.raw)
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().syncBlocked)
            incompatible = false
            missingUnifiedLedger = true
            app.refresh()
            assertEquals(original, store.raw)
            assertTrue(app.state().syncBlocked)
            assertEquals(1, app.state().pendingCount)
            assertFailsWith<IllegalArgumentException> { app.connect("http://127.0.0.1:1") }
            assertEquals(original, store.raw)
        }
    }

    @Test
    fun `form captures precision and restore identity without silently rebasing saved intent`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)

            val id = app.saveExpense(ExpenseDraft("1.00"))
            app.refresh()
            val edit = app.beginEdit(id)
            val context = app.beginExpense()
            server.patch(
                "/budgets/${server.budget}/preferences",
                """{"hide_fraction":true}""",
                "2020-01-01T00:00:00.000Z-0000-ffffffffffffffff",
            )
            app.refresh()
            assertEquals(2, app.state().decimalPlaces)
            val purchase = app.saveExpense(ExpenseDraft("12.34", context = context))
            app.refresh()
            assertEquals(
                -1234,
                server
                    .get("/budgets/${server.budget}/transactions/$purchase")
                    .getAsJsonObject("data")["amount"]
                    .asLong,
            )
            app.saveExpense(edit.original!!.copy(date = "2026-02-03"), edit)
            app.refresh()
            assertEquals(
                -100,
                server
                    .get("/budgets/${server.budget}/transactions/$id")
                    .getAsJsonObject("data")["amount"]
                    .asLong,
            )
            val point = server.post("/budgets/${server.budget}/backups", "{}")
            val beforeRestore = app.beginExpense()
            server.post(
                "/budgets/${server.budget}/backups/${point["id"].asString}/restore",
                """{"confirm_name":"Native fixture"}""",
                200,
            )
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertFailsWith<IllegalArgumentException> {
                app.saveExpense(ExpenseDraft("1", context = beforeRestore))
            }
            assertEquals(0, app.state().pendingCount)
            app.markOffline()
            val queued = app.saveExpense(ExpenseDraft("2", context = app.beginExpense()))
            val original =
                JsonParser.parseString(store.raw)
                    .asJsonObject
                    .getAsJsonArray("pending")[0]
                    .asJsonObject
            // A second restore requires the current generation, provided explicitly.
            val epoch =
                server
                    .get("/budgets/${server.budget}/checkpoint")
                    .getAsJsonObject("data")["epoch"]
                    .asString
            val response =
                BudgetHttpTransport()
                    .request(
                        "${server.url}/api/v1/budgets/${server.budget}/backups/${point["id"].asString}/restore",
                        "POST",
                        mapOf(
                            "Content-Type" to "application/json",
                            "Idempotency-Key" to UUID.randomUUID().toString(),
                            "X-Actual-Budget-Epoch" to epoch,
                        ),
                        """{"confirm_name":"Native fixture"}""".toByteArray(),
                    )
            assertEquals(200, response.status, response.body.toString(Charsets.UTF_8))
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().syncBlocked)
            val retained =
                JsonParser.parseString(store.raw)
                    .asJsonObject
                    .getAsJsonArray("pending")[0]
                    .asJsonObject
            for (field in listOf("id", "body", "clock", "epoch", "budgetId", "path")) assertEquals(
                original[field],
                retained[field],
            )
            assertFalse(
                server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data").any {
                    it.asJsonObject["id"].asString == queued
                }
            )
        }
    }

    @Test
    fun `server changes preserve captured context and prevent queued work crossing origins`() {
        Server().use { first ->
            Server().use { second ->
                val app = ApiExpenseService(MemoryStore())
                app.connect(first.url)

                val context = app.beginExpense()
                app.connect(second.url)
                assertEquals(second.budget, app.state().budgetId)

                assertFailsWith<IllegalArgumentException> {
                    app.saveExpense(ExpenseDraft("1.00", context = context))
                }
                app.connect(first.url)

                assertEquals(first.account, app.state().expenseContext?.accountId)
                app.markOffline()
                app.saveExpense(ExpenseDraft("3.00"))
                assertFailsWith<IllegalArgumentException> { app.connect(second.url) }
                assertEquals(first.budget, app.state().budgetId)
                assertEquals(1, app.state().pendingCount)
            }
        }
    }
    @Test
    fun `foreground schedules post once while pending delivery mode never runs schedules`() {
        Server().use { server ->
            val store=MemoryStore()
            val requests=mutableListOf<Pair<String,String>>()
            val real=BudgetHttpTransport()
            val transport=HttpTransport { url,method,headers,body ->
                if(url.contains("/schedules")) requests += method to url
                real.request(url,method,headers,body)
            }
            val app=ApiExpenseService(store,transport=transport)
            app.connect(server.url)
            val root="/budgets/${server.budget}"
            val schedule=UUID.randomUUID().toString()
            val today=server.get("$root/schedules").getAsJsonObject("data")["today"].asString
            val snapshot=server.get("$root/checkpoint").getAsJsonObject("data")["snapshot"].asString
            server.post("$root/schedules/commands", """{"action":"create","id":"$schedule","snapshot":"$snapshot","posts_transaction":true,"conditions":[{"field":"account","op":"is","value":"${server.account}"},{"field":"amount","op":"is","value":-1234},{"field":"date","op":"is","value":"$today"}]}""")
            app.refresh()
            assertTrue(requests.isEmpty(),"Pending-expense delivery may not probe or run schedules")
            assertTrue(app.state().expenses.isEmpty())
            app.refresh(foreground=true)
            assertNull(app.state().error)
            assertEquals(1234L,app.state().expenses.single().amountMinor)
            assertEquals(0,app.state().pendingCount)
            assertEquals(1,requests.count{it.first=="POST"})
            val writes=store.writes
            app.refresh(foreground=true)
            assertEquals(1,requests.count{it.first=="POST"})
            assertEquals(writes,store.writes,"No due work must not rewrite the native cache")
        }
    }

    private fun dueSchedule(server:Server):String {
        val root="/budgets/${server.budget}"
        val schedule=UUID.randomUUID().toString()
        val view=server.get("$root/schedules").getAsJsonObject("data")
        val body=JsonObject().apply{
            addProperty("action","create");addProperty("id",schedule);addProperty("posts_transaction",true)
            addProperty("snapshot",view["snapshot"].asString)
            add("conditions",JsonParser.parseString("""[{"field":"account","op":"is","value":"${server.account}"},{"field":"amount","op":"is","value":-1234},{"field":"date","op":"is","value":"${view["today"].asString}"}]"""))
        }
        server.post("$root/schedules/commands",body.toString())
        return schedule
    }

    @Test
    fun `scheduled foreground lost ack retains separate original command through restart and background refresh`() {
        Server().use { server ->Server().use { other ->
            val store=MemoryStore();val real=BudgetHttpTransport()
            val deliveries=mutableListOf<Pair<Map<String,String>,String>>()
            var lose=true
            val transport=HttpTransport{url,method,headers,body->
                val scheduled=method=="POST"&&url.endsWith("/schedules/commands")
                if(scheduled)deliveries+=headers.toMap() to requireNotNull(body).toString(Charsets.UTF_8)
                val response=real.request(url,method,headers,body)
                if(scheduled&&lose){lose=false;throw java.io.IOException("Deliberately lost committed schedule acknowledgement")}
                response
            }
            var app=ApiExpenseService(store,transport=transport)
            app.connect(server.url)
            dueSchedule(server)
            app.refresh(foreground=true)
            assertNull(app.state().error);assertTrue(app.state().scheduledWorkPending)
            assertEquals(0,app.state().pendingCount);assertFalse(app.state().syncBlocked)
            val original=JsonParser.parseString(store.raw).asJsonObject["scheduledCommand"].deepCopy()
            assertEquals(deliveries[0].second,original.asJsonObject["body"].asString)
            assertEquals(deliveries[0].first["X-Actual-Edit-Clock"],original.asJsonObject["clock"].asString)
            assertEquals(deliveries[0].first["X-Actual-Budget-Epoch"],original.asJsonObject["epoch"].asString)
            assertFailsWith<IllegalArgumentException>{app.connect(other.url)}
            app=ApiExpenseService(store,transport=transport)
            app.refresh()
            assertEquals(1,deliveries.size,"Background/expense mode must not even probe the schedule command receipt")
            assertEquals(original,JsonParser.parseString(store.raw).asJsonObject["scheduledCommand"])
            app.refresh(foreground=true)
            assertNull(app.state().error);assertFalse(app.state().scheduledWorkPending)
            assertEquals(0,app.state().pendingCount);assertEquals(1234L,app.state().expenses.single().amountMinor)
            assertEquals(2,deliveries.size)
            val first=deliveries[0];val proof=deliveries[1]
            assertEquals("true",proof.first["X-Actual-Receipt-Only"])
            for(header in listOf("Idempotency-Key","X-Actual-Edit-Clock","X-Actual-Budget-Epoch"))assertEquals(first.first[header],proof.first[header])
            assertEquals(first.second,proof.second)
            val rows=server.get("/budgets/${server.budget}/transactions").getAsJsonArray("data")
            assertEquals(1,rows.size());assertEquals(-1234L,rows[0].asJsonObject["amount"].asLong)
        }}
    }

    @Test
    fun `saved native state without scheduled command still decodes without an idle rewrite`() {
        Server().use { server ->
            val saved=MemoryStore();val app=ApiExpenseService(saved)
            app.connect(server.url)
            val old=JsonParser.parseString(saved.raw).asJsonObject.apply{remove("scheduledCommand")}.toString()
            val store=MemoryStore(old);val restarted=ApiExpenseService(store)
            assertFalse(restarted.state().scheduledWorkPending)
            restarted.refresh(foreground=true)
            assertNull(restarted.state().error);assertEquals(0,store.writes);assertEquals(old,store.raw)
        }
    }

    @Test
    fun `uncommitted schedule command keeps original epoch after restore until explicit exported recovery`() {
        Server().use { server ->
            val saved=MemoryStore();val real=BudgetHttpTransport();var intercept=true
            val attempts=mutableListOf<Triple<String,Map<String,String>,ByteArray?>>()
            val transport=HttpTransport{url,method,headers,body->
                if(method=="POST"&&url.endsWith("/schedules/commands")){
                    attempts+=Triple(url,headers.toMap(),body?.clone())
                    if(intercept&&!headers.containsKey("X-Actual-Receipt-Only"))throw java.io.IOException("Disconnected before original schedule delivery")
                }
                real.request(url,method,headers,body)
            }
            val app=ApiExpenseService(saved,transport=transport)
            app.connect(server.url)
            dueSchedule(server);val root="/budgets/${server.budget}"
            val backup=server.post("$root/backups","""{"label":"Before schedule delivery"}""")
            app.refresh(foreground=true);assertTrue(app.state().scheduledWorkPending)
            val original=JsonParser.parseString(saved.raw).asJsonObject.getAsJsonObject("scheduledCommand").deepCopy()
            val wire=attempts.first();intercept=false
            server.post("$root/backups/${backup["id"].asString}/restore","""{"confirm_name":"Native fixture"}""",200)
            app.refresh(foreground=true)
            assertTrue(app.state().scheduledWorkPending);assertTrue(app.state().syncBlocked)
            assertNotNull(app.state().scopeRecovery)
            val retained=JsonParser.parseString(saved.raw).asJsonObject.getAsJsonObject("scheduledCommand")
            for(field in listOf("id","server","budget","path","body","clock","epoch"))assertEquals(original[field],retained[field])
            assertNotEquals(original["epoch"].asString,server.get("$root/checkpoint").getAsJsonObject("data")["epoch"].asString)
            val sends=attempts.count{!it.second.containsKey("X-Actual-Receipt-Only")}
            app.refresh(foreground=true)
            assertEquals(sends,attempts.count{!it.second.containsKey("X-Actual-Receipt-Only")},"A known review refusal must not spin on foreground")
            val exported = app.exportScopeRecovery()
            assertEquals(retained, JsonParser.parseString(exported).asJsonObject.getAsJsonObject("saved_state")["scheduledCommand"])
            app.openCurrentAfterExport(exported)
            assertFalse(app.state().scheduledWorkPending)
            assertEquals(exported, JsonParser.parseString(saved.raw).asJsonObject["recoveryArchive"].asString)
            val late=real.request(wire.first,"POST",wire.second,wire.third)
            assertEquals(409,late.status);assertEquals("budget_restored",JsonParser.parseString(late.body.toString(Charsets.UTF_8)).asJsonObject.getAsJsonObject("error")["code"].asString)
            assertEquals(0,server.get("$root/transactions").getAsJsonArray("data").size())
            app.refresh(foreground=true)
            assertFalse(app.state().scheduledWorkPending);assertEquals(1,app.state().expenses.size)
            assertNotEquals(original["id"].asString,attempts.last().second["Idempotency-Key"])
        }
    }

    @Test
    fun `overdue scheduled work holds for review without spinning or blocking ordinary pending expenses`() {
        Server().use { server ->
            val source=Files.readAllBytes(Path.of("../../actualBudget/tests/fixtures/schedules-overdue-26.10.zip"))
            val stage=server.upload(source);val budget=server.budget
            server.post("/imports/${stage["id"].asString}/activate","{}")
            val account=server.get("/budgets/$budget/accounts").getAsJsonArray("data")[0].asJsonObject["id"].asString
            val real=BudgetHttpTransport();var sends=0;var scheduleTraffic=0
            val workloadClient = HttpClient.newHttpClient()
            val network=HttpTransport{url,method,headers,body->
                if(url.contains("/schedules"))scheduleTraffic++
                if(method=="POST"&&url.endsWith("/schedules/commands")&&!headers.containsKey("X-Actual-Receipt-Only"))sends++
                if (method == "POST" && url.endsWith("/schedules/commands")) {
                    // This fixture executes and rolls back 500 occurrences. Let the
                    // debug server finish the cap check; mobile timeout/retry has separate coverage.
                    val response = workloadClient.send(
                        HttpRequest.newBuilder(URI(url)).timeout(java.time.Duration.ofMinutes(2))
                            .apply { headers.forEach { (key, value) -> header(key, value) } }
                            .POST(HttpRequest.BodyPublishers.ofByteArray(body ?: byteArrayOf())).build(),
                        HttpResponse.BodyHandlers.ofByteArray(),
                    )
                    org.actualexpenses.core.protocol.HttpResponse(response.statusCode(), response.headers().firstValue("Content-Type").orElse(null), response.body())
                } else real.request(url,method,headers,body)
            }
            val store=MemoryStore();val app=ApiExpenseService(store,transport=network)
            app.connect(server.url)
            app.refresh(foreground=true)
            assertEquals(1,sends);assertTrue(app.state().scheduledWorkPending)
            assertTrue(app.state().scheduledWorkError.orEmpty().contains("500"),app.state().scheduledWorkError)
            assertEquals(0,app.state().pendingCount);assertFalse(app.state().syncBlocked);assertNull(app.state().error)
            assertEquals(0,server.get("/budgets/$budget/transactions").getAsJsonArray("data").size())
            app.markOffline();val expense=app.saveExpense(ExpenseDraft("2.34",notes="Capture remains available"))
            assertEquals(1,app.state().pendingCount)
            val traffic=scheduleTraffic;app.refresh()
            assertEquals(traffic,scheduleTraffic,"Pending-only refresh may not deliver or probe held schedule work")
            assertEquals(0,app.state().pendingCount);assertTrue(app.state().scheduledWorkPending)
            assertEquals(expense,app.state().expenses.single().id)
            val restarted=ApiExpenseService(store,transport=network)
            restarted.refresh(foreground=true)
            assertEquals(1,sends,"Known work-cap refusal requires explicit review instead of another identical foreground POST")
            assertTrue(restarted.state().scheduledWorkPending)
            assertNull(restarted.state().error);assertEquals(expense,restarted.state().expenses.single().id)
        }
    }

    @Test
    fun `crossover preserves source estimate and companion state over plain HTTP`() {
        Server().use { server ->
            val fixture = Path.of("../../actualBudget/tests/fixtures/crossover-source-26.10")
            val source =
                JsonParser.parseString(Files.readString(Path.of("$fixture.json"))).asJsonObject
            val stage = server.upload(Files.readAllBytes(Path.of("$fixture.zip")))
            if (stage["status"].asString == "ready")
                server.post("/imports/${stage["id"].asString}/activate", "{}")
            val root = "/budgets/${server.budget}"
            val case = source.getAsJsonArray("cases")[0].asJsonObject
            val account = source.getAsJsonObject("ids")["checking"].asString
            val before = server.get("$root/companion?account_id=$account")
            val query =
                JsonObject().apply {
                    addProperty("version", 1)
                    addProperty("as_of", "2026-11-09")
                    add("options", case["options"])
                }
            val projection = server.post("$root/reports/crossover", query.toString(), 200)
            assertFalse(projection["recorded"].asBoolean)
            assertEquals(case["result"], projection["result"])
            assertEquals(before, server.get("$root/companion?account_id=$account"))
            assertTrue(
                server
                    .post("$root/reports/crossover", query.toString(), 200)
                    .getAsJsonObject("cache")["hit"]
                    .asBoolean
            )
        }
    }

}

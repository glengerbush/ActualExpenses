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
    private class MemoryStore(var raw: String? = null) : StateStore {
        var writes = 0

        override fun read() = raw

        override fun write(value: String) {
            raw = value
            writes++
        }
    }

    private class Server : AutoCloseable {
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
                System.getenv("ACTUALBUDGET_BINARY")
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
            post("/budgets", """{"id":"$budget","name":"Native fixture"}""")
            post(
                "/budgets/$budget/accounts",
                """{"id":"$account","name":"Spending","opening_balance":0,"opening_date":"2026-01-01"}""",
            )
        }

        fun post(path: String, body: String, status: Int = 201): JsonObject {
            val response =
                client.send(
                    HttpRequest.newBuilder(URI("$url/api/v1$path"))
                        .header("Content-Type", "application/json")
                        .header("Idempotency-Key", UUID.randomUUID().toString())
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

        fun upload(bytes: ByteArray): JsonObject {
            val response =
                client.send(
                    HttpRequest.newBuilder(URI("$url/api/v1/imports/actual"))
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            plan("\"action\":\"method\",\"method\":\"envelope\"")
            plan("\"action\":\"hold\",\"amount\":0")
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
            server.patch("$base/transactions/$expense", """{"notes":"After planning"}""", clock)
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertEquals("After planning", app.state().expenses.single().notes)
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
    fun `location-only changes preserve native expense projection and pending offline edits`() {
        Server().use { server ->
            val base = "/budgets/${server.budget}"
            val payee = UUID.randomUUID().toString()
            val location = UUID.randomUUID().toString()
            server.post("$base/payees", """{"id":"$payee","name":"Local shop"}""")
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
    fun `confirmed protected pair edits and unlock never restore native expense eligibility`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("12.34", notes = "Converted payment"))
            app.refresh()
            val base = "/budgets/${server.budget}"
            val account = UUID.randomUUID().toString()
            val peer = UUID.randomUUID().toString()
            server.post("$base/accounts", """{"id":"$account","name":"Card","opening_balance":0,"opening_date":"2026-01-01"}""")
            server.post("$base/transactions", """{"id":"$peer","account_id":"$account","amount":1234,"date":"2026-01-01","cleared":true}""")
            server.post("$base/transfers/link", """{"id":"$id","counterpart_id":"$peer"}""", 200)
            for ((owner, balance) in listOf(server.account to -1234, account to 1234)) {
                val path = "$base/accounts/$owner/reconciliation"
                val progress = server.get(path).getAsJsonObject("data")
                server.post(path, """{"action":"lock","statement_balance":$balance,"snapshot":${progress["snapshot"]},"date":"2026-02-28"}""", 200)
            }
            val current = server.get("$base/transactions/$id/splits")
            val clock = DateTimeFormatterBuilder().appendInstant(3).toFormatter().format(Instant.now().plusSeconds(1)) + "-0000-ffffffffffffffff"
            server.patch("$base/transactions/$id", """{"amount":-1300,"confirm_reconciled":true,"transfer_protection_token":${current["transfer_protection_token"]}}""", clock)
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertTrue(app.state().expenses.isEmpty())
            assertTrue(server.get("$base/transactions/$id").getAsJsonObject("data")["reconciled"].asBoolean)
            assertEquals(1300, server.get("$base/transactions/$peer").getAsJsonObject("data")["amount"].asInt)
            val proof = server.get("$base/transactions/$id/splits")
            server.post("$base/transactions/$id/unlock", """{"reconciliation_token":${proof["reconciliation_token"]}}""", 200)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertTrue(restarted.state().expenses.isEmpty())
            assertTrue(restarted.state().payees.isEmpty())
            val retained = server.get("$base/companion?account_id=${server.account}&retain=$id").getAsJsonObject("data")
            assertEquals(id, retained.getAsJsonArray("unavailable").single().asString)
        }
    }

    @Test
    fun `reconciliation hides settled expenses and unlocking restores native eligibility`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("12.34"))
            app.refresh()
            assertEquals(id, app.state().expenses.single().id)
            val path = "/budgets/${server.budget}/accounts/${server.account}/reconciliation"
            val progress = server.get(path + "?statement_balance=-1234").getAsJsonObject("data")
            val snapshot = progress["snapshot"].asString
            server.post(path, """{"action":"lock","statement_balance":-1234,"snapshot":"$snapshot","date":"2026-02-28"}""", 200)
            app.refresh()
            assertTrue(app.state().expenses.isEmpty(), app.state().error)
            val row = server.get("/budgets/${server.budget}/transactions/$id").getAsJsonObject("data")
            val version = row.getAsJsonObject("field_versions")["reconciled"].asString
            server.post("/budgets/${server.budget}/transactions/$id/unlock", """{"reconciled_version":"$version"}""", 200)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals(id, restarted.state().expenses.single().id)
            assertEquals(1234, restarted.state().expenses.single().amountMinor)
        }
    }

    @Test
    fun `reconciled split stays ineligible through confirmed recombination until unlocked`() {
        Server().use { server ->
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("3.00", notes = "Native split"))
            app.refresh()
            val base = "/budgets/${server.budget}"
            val snapshot = server.get("$base/transactions")["snapshot"].asString
            server.post("$base/transactions/$id/splits", """{"snapshot":"$snapshot","children":[{"id":"${UUID.randomUUID()}","amount":-100},{"id":"${UUID.randomUUID()}","amount":-200}]}""", 200)
            val path = "$base/accounts/${server.account}/reconciliation"
            val progress = server.get(path).getAsJsonObject("data")
            assertEquals(1, progress["ready_count"].asInt)
            server.post(path, """{"action":"lock","statement_balance":-300,"snapshot":${progress["snapshot"]},"date":"2026-02-28"}""", 200)
            app.refresh()
            assertTrue(app.state().expenses.isEmpty())
            val current = server.get("$base/transactions/$id/splits")
            val row = current.getAsJsonObject("data")
            val proof = JsonObject()
            proof.add(id, row.getAsJsonObject("field_versions")["reconciled"])
            for (child in row.getAsJsonArray("children")) {
                val value = child.asJsonObject
                assertTrue(value["reconciled"].asBoolean)
                proof.add(value["id"].asString, value.getAsJsonObject("field_versions")["reconciled"])
            }
            server.post("$base/transactions/$id/splits", """{"snapshot":${current["snapshot"]},"action":"recombine","confirm_reconciled":true,"reconciled_versions":$proof}""", 200)
            app.refresh()
            assertTrue(app.state().expenses.isEmpty())
            val recombined = server.get("$base/transactions/$id").getAsJsonObject("data")
            assertTrue(recombined["reconciled"].asBoolean)
            server.post("$base/transactions/$id/unlock", """{"reconciled_version":${recombined.getAsJsonObject("field_versions")["reconciled"]}}""", 200)
            app.refresh()
            assertEquals(300, app.state().expenses.single().amountMinor)
        }
    }

    @Test
    fun `converted split preserves pending edit across restart until confirmed discard`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("7.89", notes = "Original"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            app.saveExpense(edit.original!!.copy(notes = "Pending correction"), edit)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("7.89", notes = "Original"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            fun queue() {
                if (delete) app.deleteExpense(edit) else app.saveExpense(edit.original!!.copy(notes = "Pending correction"), edit)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("12.34"))
            assertEquals(1, app.state().pendingCount)
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            val row =
                server.get("/budgets/${server.budget}/transactions/$id").getAsJsonObject("data")
            assertEquals(-1234, row["amount"].asLong)
            assertTrue(row["cleared"].asBoolean)
            val restarted = ApiExpenseService(store)
            assertEquals(server.account, restarted.state().selectedAccountId)
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
    fun `remote close and deletion refresh account choices and clear unavailable selection`() {
        Server().use { server ->
            for (amount in listOf(-100, 100)) server.post(
                "/budgets/${server.budget}/transactions",
                """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":$amount,"date":"2026-01-01"}""",
            )
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            server.post("/budgets/${server.budget}/accounts/${server.account}/close", "{}", 200)
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertTrue(app.state().accounts.isEmpty())
            assertNull(app.state().selectedAccountId)
            server.post("/budgets/${server.budget}/accounts/${server.account}/reopen", "{}", 200)
            app.refresh()
            assertEquals(listOf(server.account), app.state().accounts.map { it.id })
            app.chooseAccount(server.account)
            server.delete(
                "/budgets/${server.budget}/accounts/${server.account}",
                """{"confirm_name":"Spending"}""",
            )
            app.refresh()
            assertTrue(app.state().online, app.state().error)
            assertTrue(app.state().accounts.isEmpty())
            assertNull(app.state().selectedAccountId)
        }
    }

    @Test
    fun `more than one hundred offline expenses drain after remote metadata changes`() {
        Server().use { server ->
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            assertTrue(
                restarted.state().expenses.isEmpty(),
                "Historical receipt must not resurrect current expenses",
            )
            if (lifecycle != "restore") assertNull(restarted.state().selectedAccountId)
            restarted.refresh()
            assertEquals(2, requests.size, "Acknowledged receipt must leave the durable queue")
        }
    }

    @Test
    fun `older API without receipt lookup retains uncertain closed account work without unsafe probe`() {
        Server().use { server ->
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            var advertiseReceiptLookup = false
            var lose = true
            var mutationRequests = 0
            val network = HttpTransport { url, method, headers, body ->
                if (!advertiseReceiptLookup) assertNull(headers["X-Actual-Receipt-Only"])
                val response = real.request(url, method, headers, body)
                if (url.endsWith("/info") && !advertiseReceiptLookup) {
                    val info =
                        JsonParser.parseString(response.body.toString(Charsets.UTF_8)).asJsonObject
                    info
                        .getAsJsonObject("data")
                        .getAsJsonArray("capabilities")
                        .remove(com.google.gson.JsonPrimitive("transaction_receipt_lookup"))
                    org.actualexpenses.core.protocol.HttpResponse(
                        response.status,
                        response.contentType,
                        info.toString().toByteArray(),
                    )
                } else {
                    if (method == "POST" && url.endsWith("/transactions")) {
                        mutationRequests++
                        if (lose) {
                            lose = false
                            throw IOException("Committed acknowledgement lost")
                        }
                    }
                    response
                }
            }
            val app = ApiExpenseService(store, transport = network)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("1.00"))
            app.refresh()
            server.post(
                "/budgets/${server.budget}/transactions",
                """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":100,"date":"2026-01-01"}""",
            )
            server.post("/budgets/${server.budget}/accounts/${server.account}/close", "{}", 200)
            val restarted = ApiExpenseService(store, transport = network)
            restarted.refresh()
            assertEquals(1, restarted.state().pendingCount)
            assertTrue(restarted.state().expenses.any { it.id == id && it.pending })
            assertTrue(restarted.state().syncBlocked)
            assertEquals(1, mutationRequests)
            advertiseReceiptLookup = true
            restarted.refresh()
            assertEquals(0, restarted.state().pendingCount, restarted.state().error)
            assertEquals(2, mutationRequests)
            assertNull(restarted.state().selectedAccountId)
        }
    }

    @Test
    fun `closed account retains queued expense and reopens with identical delivery identity`() {
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            app.markOffline()
            val id = app.saveExpense(ExpenseDraft("4.56", notes = "Keep this purchase"))
            app.refresh()
            assertEquals(1, requests.size)
            server.post("/budgets/${server.budget}/accounts/${server.account}/close", "{}", 200)
            fail = false
            app.refresh()
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().syncBlocked)
            assertTrue(app.state().error.orEmpty().contains("closed"), app.state().error)
            assertTrue(app.state().accounts.single().name.contains("closed; pending recovery"))
            assertEquals(server.account, app.state().selectedAccountId)
            assertTrue(app.state().expenses.any { it.id == id && it.pending })
            assertEquals(1, requests.size, "Unavailable accounts must retain unsent work")
            val restarted = ApiExpenseService(store, transport = network)
            restarted.refresh()
            assertEquals(1, restarted.state().pendingCount)
            assertTrue(restarted.state().expenses.any { it.id == id && it.pending })
            server.post("/budgets/${server.budget}/accounts/${server.account}/reopen", "{}", 200)
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
    fun `deleted account preserves pending edit and other accounts remain selectable`() {
        Server().use { server ->
            val store = MemoryStore()
            val app = ApiExpenseService(store)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("7.89", notes = "Original"))
            app.refresh()
            val edit = app.beginEdit(id)
            app.markOffline()
            app.saveExpense(edit.original!!.copy(notes = "Keep pending correction"), edit)
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
            assertEquals(setOf(second, server.account), app.state().accounts.map { it.id }.toSet())
            assertTrue(
                app.state()
                    .accounts
                    .single { it.id == server.account }
                    .name
                    .contains("deleted; pending recovery")
            )
            assertEquals("Keep pending correction", app.state().expenses.single().notes)
            assertTrue(app.state().expenses.single().pending)
            val restarted = ApiExpenseService(store)
            restarted.refresh()
            assertEquals(id, restarted.state().expenses.single().id)
            restarted.chooseBudget(server.budget)
            assertEquals(id, restarted.state().expenses.single().id)
            restarted.chooseAccount(second)
            assertEquals(second, restarted.state().selectedAccountId)
            assertEquals(1, restarted.state().pendingCount)
            restarted.refresh()
            assertTrue(restarted.state().syncBlocked)
            assertTrue(restarted.state().error.orEmpty().contains("Spending"))
            restarted.chooseAccount(server.account)
            assertEquals(id, restarted.state().expenses.single().id)
            assertFailsWith<IllegalArgumentException> {
                restarted.saveExpense(ExpenseDraft("1.00"))
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            val budget = stage["budget_id"].asString
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
            app.chooseBudget(budget)
            app.chooseAccount(fixture["account"].asString)
            assertEquals(3, publicPayees.size())
            assertTrue(app.state().payees.isEmpty())
            assertFalse(app.state().payees.any { it.id == transfer })
            app.refresh()
            assertTrue(app.state().payees.isEmpty())
        }
    }

    @Test
    fun `category choices follow hidden parents category visibility and deletion`() {
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            assertEquals(listOf(category), app.state().categories.map { it.id })
            val expense = app.saveExpense(ExpenseDraft("1.23", categoryId = category))
            app.refresh()
            val originalEdit = app.beginEdit(expense)
            fun clock(seconds: Long) =
                DateTimeFormatterBuilder()
                    .appendInstant(3)
                    .toFormatter()
                    .format(Instant.now().plusSeconds(seconds)) + "-0000-ffffffffffffffff"
            server.patch("$base/category-groups/$group", """{"hidden":true}""", clock(1))
            assertTrue(
                server
                    .get("$base/categories/$category")
                    .getAsJsonObject("data")["group_hidden"]
                    .asBoolean
            )
            app.refresh()
            assertTrue(app.state().categories.isEmpty(), "Hidden group must hide category choices")
            assertEquals("Food", app.state().expenses.single().category)
            app.saveExpense(
                originalEdit.original!!.copy(notes = "Keep existing category"),
                originalEdit,
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertFailsWith<IllegalArgumentException> {
                app.saveExpense(ExpenseDraft("1.00", categoryId = category))
            }
            server.patch("$base/category-groups/$group", """{"hidden":false}""", clock(2))
            app.refresh()
            assertEquals(listOf(category), app.state().categories.map { it.id })
            server.patch("$base/categories/$category", """{"hidden":true}""", clock(3))
            app.refresh()
            assertTrue(app.state().categories.isEmpty())
            server.patch("$base/categories/$category", """{"hidden":false}""", clock(4))
            app.refresh()
            assertEquals(listOf(category), app.state().categories.map { it.id })
            server.delete("$base/categories/$category", """{"replacement_id":null}""")
            app.refresh()
            assertTrue(app.state().categories.isEmpty())
            assertNull(app.state().expenses.single().categoryId)
        }
    }

    @Test
    fun `legacy state remains untouched behind explicit recovery`() {
        val legacy =
            MemoryStore("""{"budget":{"fileId":"old"},"pending":[{"id":"retained-original"}]}""")
        val native = MemoryStore()
        val app =
            ApiExpenseService(
                native,
                legacy,
                HttpTransport { _, _, _, _ -> error("Legacy recovery must not send") },
            )
        assertTrue(app.state().legacyRecovery)
        assertFailsWith<IllegalStateException> { app.connect("http://127.0.0.1:1") }
        app.refresh()
        assertEquals(0, legacy.writes)
        assertEquals(0, native.writes)
        assertTrue(legacy.raw!!.contains("retained-original"))
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
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
                """{"notes":"Browser notes"}""",
                clock(1),
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertEquals(200, app.state().expenses.single().amountMinor)
            assertEquals("Browser notes", app.state().expenses.single().notes)
            app.deleteExpense(app.beginEdit(id))
            server.patch(
                "/budgets/${server.budget}/transactions/$id",
                """{"notes":"Newer than deletion"}""",
                clock(2),
            )
            app.refresh()
            assertEquals(0, app.state().pendingCount, app.state().error)
            assertEquals("Newer than deletion", app.state().expenses.single().notes)
            assertTrue(app.state().notice!!.contains("Deletion cancelled"))
        }
    }

    @Test
    fun `pinned Actual export retains eligible identities and excludes income refunds and tombstone`() {
        Server().use { server ->
            val archive =
                Files.readAllBytes(Path.of("../../actualBudget/tests/fixtures/import-26.10.0.zip"))
            val stage = server.upload(archive)
            val imported = stage["budget_id"].asString
            server.post("/imports/${stage["id"].asString}/activate", "{}")
            val app = ApiExpenseService(MemoryStore())
            app.connect(server.url)
            app.chooseBudget(imported)
            app.chooseAccount("0165d0a7-99e2-4719-a102-c007b7a8daaa")
            assertEquals(
                listOf(
                    "04000000-0000-4000-8000-000000000005",
                    "04000000-0000-4000-8000-000000000002",
                ),
                app.state().expenses.map { it.id },
            )
            assertEquals(listOf(100L, 1200L), app.state().expenses.map { it.amountMinor })
        }
    }

    @Test
    fun `incompatible response preserves pending body and selecting account reuses cached references`() {
        Server().use { server ->
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val reads = mutableListOf<String>()
            var incompatible = false
            val transport = HttpTransport { url, method, headers, body ->
                assertFalse(headers.keys.any { it.equals("Authorization", true) })
                if (method == "GET") reads += url
                if (incompatible && url.endsWith("/info"))
                    org.actualexpenses.core.protocol.HttpResponse(
                        200,
                        "application/json",
                        """{"data":{"api_version":99}}""".toByteArray(),
                    )
                else real.request(url, method, headers, body)
            }
            val app = ApiExpenseService(store, transport = transport)
            app.connect(server.url)
            app.chooseBudget(server.budget)
            reads.clear()
            app.chooseAccount(server.account)
            assertFalse(
                reads.any { it.contains("/payees?") || it.contains("/categories?") },
                "Account selection must reuse coherent cached references",
            )
            app.markOffline()
            app.saveExpense(ExpenseDraft("5.00"))
            val original = store.raw
            incompatible = true
            app.refresh()
            assertEquals(original, store.raw)
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().syncBlocked)
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
            app.chooseBudget(server.budget)
            app.chooseAccount(server.account)
            val id = app.saveExpense(ExpenseDraft("1.00"))
            app.refresh()
            val edit = app.beginEdit(id)
            val context = app.beginExpense()
            server.patch(
                "/budgets/${server.budget}/preferences",
                """{"experimental":{"currency":true},"currency_code":"JPY"}""",
                "2020-01-01T00:00:00.000Z-0000-ffffffffffffffff",
            )
            app.refresh()
            assertEquals(0, app.state().decimalPlaces)
            val purchase = app.saveExpense(ExpenseDraft("12.34", context = context))
            app.refresh()
            assertEquals(
                -1234,
                server
                    .get("/budgets/${server.budget}/transactions/$purchase")
                    .getAsJsonObject("data")["amount"]
                    .asLong,
            )
            app.saveExpense(edit.original!!.copy(notes = "format frozen"), edit)
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
    fun `explicit selection changes preserve remembered account and prevent queued work crossing origins`() {
        Server().use { first ->
            Server().use { second ->
                val app = ApiExpenseService(MemoryStore())
                app.connect(first.url)
                app.chooseBudget(first.budget)
                app.chooseAccount(first.account)
                val context = app.beginExpense()
                app.connect(second.url)
                assertNull(app.state().budgetId)
                app.chooseBudget(second.budget)
                app.chooseAccount(second.account)
                assertFailsWith<IllegalArgumentException> {
                    app.saveExpense(ExpenseDraft("1.00", context = context))
                }
                app.connect(first.url)
                app.chooseBudget(first.budget)
                assertEquals(first.account, app.state().selectedAccountId)
                app.markOffline()
                app.saveExpense(ExpenseDraft("3.00"))
                assertFailsWith<IllegalArgumentException> { app.connect(second.url) }
                assertEquals(first.budget, app.state().budgetId)
                assertEquals(1, app.state().pendingCount)
            }
        }
    }
}

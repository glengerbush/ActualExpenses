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
    fun `closed account retains queued expense and reopens with identical delivery identity`() {
        for (committed in listOf(false, true)) Server().use { server ->
            for (amount in listOf(-100, 100)) server.post(
                "/budgets/${server.budget}/transactions",
                """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":$amount,"date":"2026-01-01"}""",
            )
            val store = MemoryStore()
            val real = BudgetHttpTransport()
            val requests = mutableListOf<Triple<String, Map<String, String>, String>>()
            var fail = true
            val network = HttpTransport { url, method, headers, body ->
                if (method == "POST" && url.endsWith("/transactions")) {
                    requests += Triple(url, headers, body!!.toString(Charsets.UTF_8))
                    if (fail) {
                        if (committed) real.request(url, method, headers, body)
                        throw IOException("Connection lost")
                    }
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
            if (committed)
                server.post(
                    "/budgets/${server.budget}/transactions",
                    """{"id":"${UUID.randomUUID()}","account_id":"${server.account}","amount":456,"date":"2026-01-01"}""",
                )
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

package org.actualexpenses.core

import com.google.gson.JsonArray
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.JsonPrimitive
import org.actualexpenses.core.protocol.*
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.Test
import java.io.File
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.util.UUID
import kotlin.system.measureTimeMillis

class StartupSyncTest {
    private class Store : StateStore {
        var data: String? = null
        var writes = 0
        override fun read() = data
        override fun write(value: String) { data = value; writes++ }
    }

    @Test fun `reopening an unchanged budget avoids replaying history and rewriting storage`() {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        val fixture = JsonParser.parseString(File(path!!).readText()).asJsonObject
        val transactions = JsonArray().apply {
            repeat(128) { add(JsonObject().apply {
                addProperty("id", UUID.randomUUID().toString())
                addProperty("date", "2026-01-01")
                addProperty("amount", -100)
                addProperty("notes", "History only: " + "x".repeat(4096))
            }) }
        }
        val oracle = HttpClient.newHttpClient()
        val stateRequest = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + "/state")).build()
        val state = JsonParser.parseString(oracle.send(stateRequest, HttpResponse.BodyHandlers.ofString()).body()).asJsonObject
        val seeded = state["transactions"].asJsonArray.any { it.asJsonObject["notes"]?.takeUnless { note -> note.isJsonNull }?.asString?.startsWith("History only: ") == true }
        val request = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + "/transactions"))
            .header("Content-Type", "application/json")
            .POST(HttpRequest.BodyPublishers.ofString(JsonObject().apply { add("transactions", transactions) }.toString())).build()
        if (!seeded) assertEquals(200, oracle.send(request, HttpResponse.BodyHandlers.ofString()).statusCode())

        var measuring = false
        var syncBytes = 0
        var requests = 0
        val real = JdkHttpTransport()
        // A fixed 128 KiB/s connection makes redundant history transfer observable,
        // without relying on the host's network speed or the user's budget data.
        val transport = HttpTransport { url, method, headers, body ->
            real.request(url, method, headers, body).also { response ->
                if (measuring) {
                    requests++
                    if (url.endsWith("/sync/sync")) syncBytes += response.body.size
                    Thread.sleep(response.body.size * 1000L / (128 * 1024))
                }
            }
        }
        val store = Store()
        var app = ExpenseService(store, JdbcSnapshotDecoder(), transport)
        app.login(fixture["serverUrl"].asString, fixture["password"].asString)
        app.chooseBudget(fixture["budgetId"].asString)
        app.chooseAccount(fixture["accountId"].asString)
        app.refresh()
        assertNull(app.state().error)
        app = ExpenseService(store, JdbcSnapshotDecoder(), transport)
        val writesBefore = store.writes
        measuring = true
        val elapsed = measureTimeMillis { app.refresh() }
        println("Unchanged startup: ${elapsed}ms; $syncBytes sync bytes; $requests requests; ${store.writes - writesBefore} storage writes")
        assertNull(app.state().error)
        assertTrue(app.state().online)
        assertAll(
            { assertTrue(elapsed < 2000, "No-change sync took ${elapsed}ms on fixed-throughput link") },
            { assertTrue(syncBytes < 16 * 1024, "No-change sync replayed $syncBytes bytes of history") },
            { assertEquals(writesBefore, store.writes, "No-change sync rewrote the entire saved budget") },
        )
    }

    @Test fun `changed history detects an older offline edit arriving after the checkpoint`() {
        val fixture = fixture()
        val store = Store()
        var app = open(fixture, store)
        val id = app.saveExpense(ExpenseDraft("3.00", notes = "Before offline edit"))
        app.refresh()
        val client = ActualClient(fixture["serverUrl"].asString, fixture["token"].asString)
        val budget = client.listBudgets().single { it.fileId == fixture["budgetId"].asString }
        val clock = LogicalClock()
        client.pull(budget).messages.forEach { clock.observe(it.timestamp) }
        val delayed = ProtocolChange(clock.next(), "transactions", id, "notes", JsonPrimitive("Delayed offline edit"))
        // A different device publishes a later independent field, advancing the checkpoint.
        Thread.sleep(10)
        oracle(fixture, "/transactions/$id", "PATCH", """{"amount":-345}""")
        app.refresh()
        val since = client.pull(budget).messages.maxOf { it.timestamp }
        assertTrue(since > delayed.timestamp)
        app = ExpenseService(store, JdbcSnapshotDecoder())
        client.exchange(budget, listOf(delayed))
        assertTrue(client.pull(budget, since).messages.isEmpty(), "A timestamp-only cursor must miss this delayed mutation")

        app.refresh()
        assertNull(app.state().error)
        val expense = app.state().expenses.single { it.id == id }
        assertEquals("Delayed offline edit", expense.notes)
        assertEquals(345L, expense.amountMinor)
        val observed = oracle(fixture, "/state")["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == id }.asJsonObject
        assertEquals(expense.notes, observed["notes"].asString)
    }

    @Test fun `local save during an unchanged probe is still sent in the same refresh`() {
        val fixture = fixture()
        val store = Store()
        open(fixture, store)
        val real = JdkHttpTransport()
        lateinit var app: ExpenseService
        var created: String? = null
        val transport = HttpTransport { url, method, headers, body ->
            real.request(url, method, headers, body).also {
                if (url.endsWith("/sync/sync") && created == null) {
                    created = app.saveExpense(ExpenseDraft("1.23", notes = "Saved during startup"))
                }
            }
        }
        app = ExpenseService(store, JdbcSnapshotDecoder(), transport)
        app.refresh()
        assertNull(app.state().error)
        assertNotNull(created)
        assertEquals(0, app.state().pendingCount)
        val rows = oracle(fixture, "/state")["transactions"].asJsonArray.filter { it.asJsonObject["id"].asString == created }
        assertEquals(1, rows.size)
        assertEquals(-123, rows.single().asJsonObject["amount"].asInt)
    }

    @Test fun `old persisted state bootstraps checkpoint without losing pending expenses`() {
        val fixture = fixture()
        val store = Store()
        var app = open(fixture, store)
        app.markOffline()
        val id = app.saveExpense(ExpenseDraft("2.34"))
        // The 0.1.1 persisted format has no checkpoint field.
        store.data = JsonParser.parseString(store.data).asJsonObject.apply { remove("checkpoint") }.toString()
        app = ExpenseService(store, JdbcSnapshotDecoder())
        app.refresh()
        assertNull(app.state().error)
        assertEquals(0, app.state().pendingCount)
        assertEquals(234L, app.state().expenses.single { it.id == id }.amountMinor)
        val writesBefore = store.writes
        app = ExpenseService(store, JdbcSnapshotDecoder())
        app.refresh()
        assertNull(app.state().error)
        assertEquals(writesBefore, store.writes)
        assertEquals(1, oracle(fixture, "/state")["transactions"].asJsonArray.count { it.asJsonObject["id"].asString == id })
    }

    private fun fixture(): JsonObject {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        return JsonParser.parseString(File(path!!).readText()).asJsonObject
    }

    private fun open(fixture: JsonObject, store: Store): ExpenseService {
        val app = ExpenseService(store, JdbcSnapshotDecoder())
        app.login(fixture["serverUrl"].asString, fixture["password"].asString)
        app.chooseBudget(fixture["budgetId"].asString)
        app.chooseAccount(fixture["accountId"].asString)
        return app
    }

    private fun oracle(fixture: JsonObject, path: String, method: String = "GET", body: String? = null): JsonObject {
        val request = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + path))
            .header("Content-Type", "application/json")
            .method(method, body?.let(HttpRequest.BodyPublishers::ofString) ?: HttpRequest.BodyPublishers.noBody()).build()
        val response = HttpClient.newHttpClient().send(request, HttpResponse.BodyHandlers.ofString())
        assertEquals(200, response.statusCode(), response.body())
        return JsonParser.parseString(response.body()).asJsonObject
    }
}

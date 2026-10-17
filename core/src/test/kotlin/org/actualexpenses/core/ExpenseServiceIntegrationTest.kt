package org.actualexpenses.core

import com.google.gson.JsonParser
import org.actualexpenses.core.protocol.JdbcSnapshotDecoder
import org.actualexpenses.core.protocol.HttpTransport
import org.actualexpenses.core.protocol.JdkHttpTransport
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.Test
import java.io.File
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse

class ExpenseServiceIntegrationTest {
    class MemoryStore : StateStore {
        var contents: String? = null
        override fun read() = contents
        override fun write(value: String) { contents = value }
    }
    @Test fun `offline expense survives restart then appears once in Actual`() {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        val fixture = JsonParser.parseString(File(path!!).readText()).asJsonObject
        val store = MemoryStore()
        var app = ExpenseService(store, JdbcSnapshotDecoder())
        app.login(fixture["serverUrl"].asString, fixture["password"].asString)
        app.chooseBudget(fixture["budgetId"].asString)
        app.chooseAccount(fixture["accountId"].asString)
        app.markOffline()
        val draft = ExpenseDraft("12.34", "2020-01-02", "Corner shop", creationId = java.util.UUID.randomUUID().toString())
        val id = app.saveExpense(draft)
        assertEquals(1234, app.state().expenses.single { it.id == id }.amountMinor)
        assertEquals(1, app.state().pendingCount)
        app = ExpenseService(store, JdbcSnapshotDecoder())
        assertTrue(app.state().expenses.single { it.id == id }.pending)
        assertEquals(id, app.saveExpense(draft), "Restored form must reuse the already saved expense")
        app.refresh()
        assertNull(app.state().error)
        assertEquals(0, app.state().pendingCount)
        assertFalse(app.state().expenses.single { it.id == id }.pending)
        val state = JsonParser.parseString(java.net.URI(fixture["oracleUrl"].asString + "/state").toURL().readText()).asJsonObject
        val recorded = state["transactions"].asJsonArray.filter { it.asJsonObject["id"].asString == id }
        assertEquals(1, recorded.size)
        assertEquals(-1234, recorded.single().asJsonObject["amount"].asInt)
        assertTrue(recorded.single().asJsonObject["cleared"].asBoolean)
    }
    @Test fun `unpublished deletion loses to a newer edit after interrupted send`() {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        val fixture = JsonParser.parseString(File(path!!).readText()).asJsonObject
        val app = ExpenseService(MemoryStore(), JdbcSnapshotDecoder())
        app.login(fixture["serverUrl"].asString, fixture["password"].asString)
        app.chooseBudget(fixture["budgetId"].asString)
        app.chooseAccount(fixture["accountId"].asString)
        val id = app.saveExpense(ExpenseDraft("9.90"))
        app.refresh()
        app.deleteExpense(app.beginEdit(id))
        oracle(fixture, "/fault", "POST", """{"mode":"before-send","onlyWrites":true}""")
        app.refresh()
        assertEquals(1, app.state().pendingCount)
        oracle(fixture, "/transactions/$id", "PATCH", """{"notes":"Edited after deletion"}""")
        app.refresh()
        assertNull(app.state().error)
        assertEquals(0, app.state().pendingCount)
        assertTrue(app.state().notice.orEmpty().contains("Deletion cancelled"))
        val tx = oracle(fixture, "/state")["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == id }.asJsonObject
        assertEquals("Edited after deletion", tx["notes"].asString)
    }
    @Test fun `saving an amount does not overwrite a remote notes change while editor is open`() {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        val fixture = JsonParser.parseString(File(path!!).readText()).asJsonObject
        val app = ExpenseService(MemoryStore(), JdbcSnapshotDecoder())
        app.login(fixture["serverUrl"].asString, fixture["password"].asString)
        app.chooseBudget(fixture["budgetId"].asString)
        app.chooseAccount(fixture["accountId"].asString)
        val id = app.saveExpense(ExpenseDraft("5.00", notes = "Original note"))
        app.refresh()
        val editor = app.beginEdit(id)
        oracle(fixture, "/transactions/$id", "PATCH", """{"notes":"New desktop note"}""")
        app.refresh()
        app.saveExpense(ExpenseDraft("6.00", notes = "Original note"), editor)
        app.refresh()
        val tx = oracle(fixture, "/state")["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == id }.asJsonObject
        assertEquals(-600, tx["amount"].asInt)
        assertEquals("New desktop note", tx["notes"].asString)
    }
    private fun oracle(fixture: com.google.gson.JsonObject, path: String, method: String = "GET", body: String? = null): com.google.gson.JsonObject {
        val request = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + path))
            .header("Content-Type", "application/json")
            .method(method, body?.let(HttpRequest.BodyPublishers::ofString) ?: HttpRequest.BodyPublishers.noBody()).build()
        val response = HttpClient.newHttpClient().send(request, HttpResponse.BodyHandlers.ofString())
        assertEquals(200, response.statusCode(), response.body())
        return JsonParser.parseString(response.body()).asJsonObject
    }
    @Test fun `saving a new expense does not wait for a stalled server refresh`() {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        val fixture = JsonParser.parseString(File(path!!).readText()).asJsonObject
        val started = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val stall = java.util.concurrent.atomic.AtomicBoolean(false)
        val real = JdkHttpTransport()
        val network = HttpTransport { url, method, headers, body ->
            if (stall.get() && url.endsWith("/info")) { started.countDown(); release.await(10, java.util.concurrent.TimeUnit.SECONDS) }
            real.request(url, method, headers, body)
        }
        val app = ExpenseService(MemoryStore(), JdbcSnapshotDecoder(), network)
        app.login(fixture["serverUrl"].asString, fixture["password"].asString)
        app.chooseBudget(fixture["budgetId"].asString)
        app.chooseAccount(fixture["accountId"].asString)
        val executor = java.util.concurrent.Executors.newFixedThreadPool(2)
        try {
            stall.set(true)
            executor.submit { app.refresh() }
            assertTrue(started.await(5, java.util.concurrent.TimeUnit.SECONDS))
            val save = executor.submit<String> { app.saveExpense(ExpenseDraft("3.21")) }
            assertTrue(save.get(2, java.util.concurrent.TimeUnit.SECONDS).isNotBlank())
        } finally { release.countDown(); executor.shutdown(); executor.awaitTermination(15, java.util.concurrent.TimeUnit.SECONDS) }
    }
}

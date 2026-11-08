package org.actualexpenses.core.protocol

import com.google.gson.JsonParser
import com.google.gson.JsonObject
import com.google.gson.JsonPrimitive
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*
import java.io.File
import java.util.UUID
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse.BodyHandlers

/** Uses the disposable upstream server created by tools/compatibility, never a user's budget. */
class ActualClientIntegrationTest {
    @Test fun `authenticate discover and bootstrap an Actual budget`() {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(!path.isNullOrBlank(), "Run the disposable Actual compatibility fixture first")
        val fixture = JsonParser.parseString(File(path!!).readText()).asJsonObject
        val client = ActualClient(fixture["serverUrl"].asString)
        assertEquals("26.10.0", client.serverVersion())
        client.requireSupportedServer()
        assertTrue(client.login(fixture["password"].asString).token.isNotBlank())
        val budget = client.listBudgets().single { it.fileId == fixture["budgetId"].asString }
        val archive = client.downloadBudget(budget)
        assertEquals("SQLite format 3\u0000", archive.sqlite.copyOfRange(0, 16).toString(Charsets.UTF_8))
        assertTrue(archive.metadata.has("id"))
        val snapshot = JdbcSnapshotDecoder().decode(archive)
        assertEquals("Spending", snapshot.rows.getValue("accounts").single { it["id"].asString == fixture["accountId"].asString }["name"].asString)
        client.pull(budget) // Real upstream Protobuf decoding, not a locally encoded oracle.
    }

    @Test fun `Actual independently reads a created edited deleted expense and retries do not duplicate it`() {
        val fixture = fixture()
        val client = ActualClient(fixture["serverUrl"].asString, fixture["token"].asString)
        val budget = client.listBudgets().single { it.fileId == fixture["budgetId"].asString }
        val clock = LogicalClock()
        client.pull(budget).messages.forEach { clock.observe(it.timestamp) }
        val id = UUID.randomUUID().toString()
        fun field(name: String, value: Any) = ProtocolChange(clock.next(), "transactions", id, name,
            if (value is Number) JsonPrimitive(value) else JsonPrimitive(value.toString()))
        val changes = listOf(
            field("acct", fixture["accountId"].asString), field("amount", -12345), field("date", 20260902),
            field("description", fixture["payeeId"].asString), field("category", fixture["categoryId"].asString),
            field("notes", "Kotlin direct integration"), field("cleared", 1), field("reconciled", 0),
            field("isParent", 0), field("isChild", 0), field("tombstone", 0),
        )
        client.exchange(budget, changes)
        client.exchange(budget, changes)
        var rows = oracle(fixture, "/state")["transactions"].asJsonArray.filter { it.asJsonObject["id"].asString == id }
        assertEquals(1, rows.size, "Retry must keep one transaction")
        var expense = rows.single().asJsonObject
        assertEquals(-12345, expense["amount"].asInt)
        assertEquals("2026-09-02", expense["date"].asString)
        assertEquals(fixture["payeeId"].asString, expense["payee"].asString)
        assertEquals(fixture["categoryId"].asString, expense["category"].asString)
        assertTrue(expense["cleared"].asBoolean)
        assertFalse(expense["reconciled"].asBoolean)

        // Simulate server acceptance followed by a lost HTTP acknowledgement.
        val update = listOf(field("amount", -1001))
        oracle(fixture, "/fault", "POST", """{"mode":"after-accept"}""")
        assertThrows(ActualException::class.java) { client.exchange(budget, update) }
        val acceptedBeforeRetry = oracle(fixture, "/state")
        assertEquals(-1001, acceptedBeforeRetry["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == id }.asJsonObject["amount"].asInt)
        assertTrue(acceptedBeforeRetry["faultEvents"].asJsonArray.any { it.asJsonObject["mode"].asString == "after-accept" })
        client.exchange(budget, update)
        rows = oracle(fixture, "/state")["transactions"].asJsonArray.filter { it.asJsonObject["id"].asString == id }
        assertEquals(1, rows.size)
        expense = rows.single().asJsonObject
        assertEquals(-1001, expense["amount"].asInt)

        // The independent client edits later; delayed older changes must not win by arrival time.
        val delayed = listOf(field("notes", "older queued phone note"), field("amount", -2002))
        oracle(fixture, "/transactions/$id", "PATCH", """{"notes":"newer independent client note"}""")
        client.exchange(budget, delayed)
        expense = oracle(fixture, "/state")["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == id }.asJsonObject
        assertEquals("newer independent client note", expense["notes"].asString)
        assertEquals(-2002, expense["amount"].asInt, "An independent amount change must survive the notes conflict")
        client.pull(budget).messages.forEach { clock.observe(it.timestamp) }
        client.exchange(budget, listOf(field("tombstone", 1)))
        assertFalse(oracle(fixture, "/state")["transactions"].asJsonArray.any { it.asJsonObject["id"].asString == id })
    }

    private fun fixture(): JsonObject {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(!path.isNullOrBlank(), "Run the disposable Actual compatibility fixture first")
        return JsonParser.parseString(File(path!!).readText()).asJsonObject
    }

    private fun oracle(fixture: JsonObject, path: String, method: String = "GET", body: String? = null): JsonObject {
        val request = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + path)).header("Content-Type", "application/json")
            .method(method, body?.let(HttpRequest.BodyPublishers::ofString) ?: HttpRequest.BodyPublishers.noBody()).build()
        val response = HttpClient.newHttpClient().send(request, BodyHandlers.ofString())
        assertEquals(200, response.statusCode(), response.body())
        return JsonParser.parseString(response.body()).asJsonObject
    }
}

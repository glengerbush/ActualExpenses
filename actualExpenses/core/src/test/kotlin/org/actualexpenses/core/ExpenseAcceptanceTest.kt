package org.actualexpenses.core

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import org.actualexpenses.core.protocol.JdbcSnapshotDecoder
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.Test
import java.io.File
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.util.UUID

/** Acceptance at the agreed application → real Actual → independent client seam. */
class ExpenseAcceptanceTest {
    @Test fun `server acceptance with a lost acknowledgement survives restart without creating a duplicate expense`() {
        val fixture = fixture()
        val store = MemoryStore()
        var app = app(fixture, store)
        app.markOffline()
        val id = app.saveExpense(ExpenseDraft("12.34", "2023-04-05", notes = "acceptance acknowledgement loss"))
        try {
            oracle(fixture, "/fault", "POST", """{"mode":"after-accept","onlyWrites":true}""")
            app.refresh()
            assertNotNull(app.state().error)
            assertEquals(1, app.state().pendingCount)
            assertTrue(app.state().expenses.single { it.id == id }.pending)
            val accepted = oracle(fixture, "/state")
            val beforeRetry = accepted["transactions"].asJsonArray.filter { it.asJsonObject["id"].asString == id }
            assertEquals(1, beforeRetry.size, "The server must have accepted this expense before recovery begins")
            assertEquals(-1234, beforeRetry.single().asJsonObject["amount"].asInt)
            assertTrue(accepted["faultEvents"].asJsonArray.any { it.asJsonObject["mode"].asString == "after-accept" })

            app = ExpenseService(store, JdbcSnapshotDecoder())
            assertEquals(1, app.state().pendingCount)
            app.refresh()
            assertNull(app.state().error)
            assertEquals(0, app.state().pendingCount)
            assertFalse(app.state().expenses.single { it.id == id }.pending)
            app.refresh()
            val records = oracle(fixture, "/state")["transactions"].asJsonArray.filter { it.asJsonObject["id"].asString == id }
            assertEquals(1, records.size)
            assertEquals(-1234, records.single().asJsonObject["amount"].asInt)
            assertEquals("2023-04-05", records.single().asJsonObject["date"].asString)
        } finally {
            oracle(fixture, "/transactions/$id", "DELETE")
        }
    }

    @Test fun `cached payee category association survives offline use and a manual override leaves the Actual rule unchanged`() {
        val fixture = fixture()
        val app = app(fixture)
        val payeeId = fixture["payeeId"].asString
        val categoryId = fixture["categoryId"].asString
        val independentBefore = oracle(fixture, "/state")
        val rule = independentBefore["rules"].asJsonArray.map { it.asJsonObject }.single { row ->
            row["conditions"].asJsonArray.any { it.asJsonObject["field"].asString == "payee" && it.asJsonObject["value"].asString == payeeId }
        }
        assertEquals(categoryId, app.state().payeeCategoryAssociations[payeeId])
        app.markOffline()
        assertEquals("Fixture shop", app.state().payees.single { it.id == payeeId }.name)
        assertEquals("Groceries", app.state().categories.single { it.id == categoryId }.name)
        assertEquals(categoryId, app.state().payeeCategoryAssociations[payeeId])
        // The UI offers the cached category, but an explicit blank override remains blank.
        val id = app.saveExpense(ExpenseDraft("2.35", "2026-10-01", "Fixture shop", payeeId, categoryId = null))
        try {
            assertNull(app.state().expenses.single { it.id == id }.categoryId)
            app.refresh()
            assertNull(app.state().error)
            assertEquals(0, app.state().pendingCount)
            val independentAfter = oracle(fixture, "/state")
            val expense = independentAfter["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == id }.asJsonObject
            assertEquals(payeeId, expense["payee"].asString)
            assertTrue(expense["category"].isJsonNull)
            val afterRule = independentAfter["rules"].asJsonArray.single { it.asJsonObject["id"] == rule["id"] }
            assertEquals(rule, afterRule)
            assertEquals(categoryId, app.state().payeeCategoryAssociations[payeeId])
        } finally {
            oracle(fixture, "/transactions/$id", "DELETE")
        }
    }

    @Test fun `offline editing is limited to unsynced offline expenses and an online editor survives connection loss`() {
        val fixture = fixture()
        val store = MemoryStore()
        var app = app(fixture, store)
        val cleanup = mutableListOf<String>()
        try {
            val synced = app.saveExpense(ExpenseDraft("3.00", "2096-01-01"))
            cleanup += synced
            app.refresh()
            assertEquals(0, app.state().pendingCount)
            val onlineEditor = app.beginEdit(synced)
            app.markOffline()
            assertThrows(IllegalArgumentException::class.java) { app.beginEdit(synced) }
            assertFalse(app.state().expenses.single { it.id == synced }.editable)

            // Permission was acquired while online; losing the connection does not discard this edit.
            app.saveExpense(ExpenseDraft("4.50", "2096-01-01", notes = "saved after connection loss"), onlineEditor)
            assertEquals(1, app.state().pendingCount)
            app = ExpenseService(store, JdbcSnapshotDecoder())
            assertTrue(app.state().expenses.single { it.id == synced }.pending)
            app.refresh()
            assertNull(app.state().error)
            val edited = oracle(fixture, "/state")["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == synced }.asJsonObject
            assertEquals(-450, edited["amount"].asInt)
            assertEquals("saved after connection loss", edited["notes"].asString)

            app.markOffline()
            val first = app.saveExpense(ExpenseDraft("1.00", "2001-01-01"))
            val second = app.saveExpense(ExpenseDraft("2.00", "2001-01-02"))
            cleanup += first
            cleanup += second
            assertTrue(app.state().expenses.any { it.id == first && it.pending })
            assertTrue(app.state().expenses.any { it.id == second && it.pending })
            app.saveExpense(ExpenseDraft("1.50", "2001-01-01"), app.beginEdit(first))
            app.deleteExpense(app.beginEdit(second))
            assertEquals(1, app.state().pendingCount)
            assertFalse(app.state().expenses.any { it.id == second })
            app = ExpenseService(store, JdbcSnapshotDecoder())
            assertTrue(app.state().expenses.single { it.id == first }.editable)
            assertEquals(150L, app.state().expenses.single { it.id == first }.amountMinor)
            app.refresh()
            assertNull(app.state().error)
            assertEquals(0, app.state().pendingCount)
            val records = oracle(fixture, "/state")["transactions"].asJsonArray.map { it.asJsonObject }
            assertEquals(-150, records.single { it["id"].asString == first }["amount"].asInt)
            assertFalse(records.any { it["id"].asString == second })
        } finally {
            cleanup.forEach { oracle(fixture, "/transactions/$it", "DELETE") }
        }
    }

    @Test fun `latest ten excludes other transaction types and keeps a newly backdated expense until the next addition`() {
        val fixture = fixture()
        val cleanup = mutableListOf<String>()
        try {
            val ordinary = (1..12).map { day ->
                """{"date":"2098-01-${day.toString().padStart(2, '0')}","amount":-${100 + day},"notes":"acceptance ordinary $day","cleared":true}"""
            }
            val seeds = ordinary + listOf(
                """{"date":"2098-12-31","amount":999,"notes":"acceptance income"}""",
                """{"date":"2098-12-30","amount":-1000,"notes":"acceptance split","subtransactions":[{"amount":-400},{"amount":-600}]}""",
                """{"date":"2098-12-29","amount":-1000,"notes":"acceptance transfer","transfer_id":"${UUID.randomUUID()}"}""",
                """{"date":"2098-12-28","amount":-1000,"notes":"acceptance reconciled","cleared":true}""",
            )
            val ids = oracle(fixture, "/transactions", "POST", """{"transactions":[${seeds.joinToString(",")}]}""")["ids"].asJsonArray.map { it.asString }
            cleanup += ids
            oracle(fixture, "/transactions/${ids.last()}", "PATCH", """{"reconciled":true}""")
            val independent = oracle(fixture, "/state")["transactions"].asJsonArray.map { it.asJsonObject }
            assertEquals(999, independent.single { it["id"].asString == ids[12] }["amount"].asInt)
            assertEquals(2, independent.single { it["id"].asString == ids[13] }["subtransactions"].asJsonArray.size())
            assertTrue(independent.single { it["id"].asString == ids[15] }["reconciled"].asBoolean)

            val app = app(fixture)
            assertEquals(ids.take(12).takeLast(10).reversed(), app.state().expenses.map { it.id })
            val old = app.saveExpense(ExpenseDraft("5.00", "2000-01-01", notes = "backdated retention"))
            cleanup += old
            app.refresh()
            assertNull(app.state().error)
            assertEquals(11, app.state().expenses.size)
            assertEquals(old, app.state().expenses.last().id)
            assertFalse(app.state().expenses.last().pending)
            val edit = app.beginEdit(old)
            app.saveExpense(ExpenseDraft("6.00", "2000-01-01", notes = "backdated edited"), edit)
            app.refresh()
            assertEquals(old, app.state().expenses.last().id, "Edit time must not promote an old transaction")

            val next = app.saveExpense(ExpenseDraft("7.00", "2098-02-01"))
            cleanup += next
            app.refresh()
            assertEquals(10, app.state().expenses.size)
            assertEquals(next, app.state().expenses.first().id)
            assertFalse(app.state().expenses.any { it.id == old })
        } finally {
            cleanup.forEach { oracle(fixture, "/transactions/$it", "DELETE") }
        }
    }

    private class MemoryStore : StateStore {
        private var value: String? = null
        override fun read() = value
        override fun write(value: String) { this.value = value }
    }
    private fun fixture(): JsonObject {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(!path.isNullOrBlank(), "Start the disposable Actual fixture")
        return JsonParser.parseString(File(path!!).readText()).asJsonObject
    }
    private fun app(fixture: JsonObject, store: StateStore = MemoryStore()): ExpenseService =
        ExpenseService(store, JdbcSnapshotDecoder()).also {
            it.login(fixture["serverUrl"].asString, fixture["password"].asString)
            it.chooseBudget(fixture["budgetId"].asString)
            it.chooseAccount(fixture["accountId"].asString)
            assertNull(it.state().error)
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

package org.actualexpenses.core

import com.google.gson.JsonArray
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

/** Regression checks at the agreed app-workflow boundary, verified by the official Actual client. */
class ExpenseDependencyTest {
    @Test fun `deleting an offline expense preserves its new payee used by another expense`() {
        val fixture = fixture()
        val store = ExpenseServiceIntegrationTest.MemoryStore()
        var app = app(fixture, store)
        app.markOffline()
        val name = "Shared offline shop ${UUID.randomUUID()}"
        val first = app.saveExpense(ExpenseDraft("1.00", payee = name))
        val payee = app.state().payees.single { it.name == name }
        val second = app.saveExpense(ExpenseDraft("2.00", payee = name, payeeId = payee.id))
        app.deleteExpense(app.beginEdit(first))
        assertEquals(1, app.state().pendingCount)
        app = ExpenseService(store, JdbcSnapshotDecoder())
        app.refresh()
        assertNull(app.state().error)
        assertEquals(0, app.state().pendingCount)
        val actual = oracle(fixture, "/state")
        assertFalse(actual["transactions"].asJsonArray.any { it.asJsonObject["id"].asString == first })
        val expense = actual["transactions"].asJsonArray.single { it.asJsonObject["id"].asString == second }.asJsonObject
        assertFalse(expense["payee"].isJsonNull, "Deleting the first expense must not delete the payee needed by the second")
        val syncedPayee = actual["payees"].asJsonArray.single { it.asJsonObject["id"].asString == expense["payee"].asString }.asJsonObject
        assertEquals(name, syncedPayee["name"].asString)
        assertEquals(name, app.state().expenses.single { it.id == second }.payee)
    }

    @Test fun `new same-day expenses stay among the latest ten after the retention exception moves on`() {
        val fixture = fixture()
        val date = "2099-01-01"
        val seeds = JsonArray().apply { repeat(10) { index -> add(JsonObject().apply {
            addProperty("date", date); addProperty("amount", -1); addProperty("cleared", true)
            addProperty("notes", "Same-date ordering seed $index")
        }) } }
        val cleanup = oracle(fixture, "/transactions", "POST", JsonObject().apply { add("transactions", seeds) }.toString())["ids"].asJsonArray.map { it.asString }.toMutableList()
        try {
            val app = app(fixture, ExpenseServiceIntegrationTest.MemoryStore())
            val first = app.saveExpense(ExpenseDraft("3.00", date)).also(cleanup::add)
            app.refresh()
            assertNull(app.state().error)
            val second = app.saveExpense(ExpenseDraft("4.00", date)).also(cleanup::add)
            app.refresh()
            assertNull(app.state().error)
            val visible = app.state().expenses
            assertEquals(10, visible.size, "Both newly added same-date expenses belong in the ordinary latest ten")
            assertTrue(visible.any { it.id == first }, "A new same-day expense must remain visible after another expense is added")
            assertTrue(visible.any { it.id == second })
            val actualIds = oracle(fixture, "/state")["transactions"].asJsonArray.map { it.asJsonObject["id"].asString }
            assertTrue(actualIds.indexOf(second) < actualIds.indexOf(first), "Actual should order the newer phone expense first")
        } finally {
            cleanup.forEach { oracle(fixture, "/transactions/$it", "DELETE") }
        }
    }

    private fun fixture(): JsonObject {
        val path = System.getenv("ACTUAL_FIXTURE")
        assumeTrue(path != null, "Start the disposable Actual fixture")
        return JsonParser.parseString(File(path!!).readText()).asJsonObject
    }

    private fun app(fixture: JsonObject, store: StateStore) = ExpenseService(store, JdbcSnapshotDecoder()).apply {
        login(fixture["serverUrl"].asString, fixture["password"].asString)
        chooseBudget(fixture["budgetId"].asString)
        chooseAccount(fixture["accountId"].asString)
    }

    private fun oracle(fixture: JsonObject, path: String, method: String = "GET", body: String? = null): JsonObject {
        val request = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + path)).header("Content-Type", "application/json")
            .method(method, body?.let(HttpRequest.BodyPublishers::ofString) ?: HttpRequest.BodyPublishers.noBody()).build()
        val response = HttpClient.newHttpClient().send(request, HttpResponse.BodyHandlers.ofString())
        assertEquals(200, response.statusCode(), response.body())
        return JsonParser.parseString(response.body()).asJsonObject
    }
}

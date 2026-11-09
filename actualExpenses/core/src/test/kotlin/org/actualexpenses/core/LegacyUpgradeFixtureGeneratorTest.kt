package org.actualexpenses.core

import com.google.gson.*
import java.io.File
import java.net.URI
import java.net.http.*
import java.nio.file.*
import java.util.UUID
import kotlin.test.*
import org.actualexpenses.core.protocol.JdbcSnapshotDecoder
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.Test

/** Opt-in generator: unchanged released ExpenseService against disposable pinned Actual 26.10. */
class LegacyUpgradeFixtureGeneratorTest {
    @Test
    fun generate() {
        val fixturePath = System.getenv("ACTUAL_FIXTURE")
        val outputPath = System.getenv("ACTUAL_UPGRADE_FIXTURE_OUTPUT")
        assumeTrue(
            fixturePath != null && outputPath != null,
            "Explicit disposable oracle fixture required",
        )
        val fixture = JsonParser.parseString(File(fixturePath!!).readText()).asJsonObject
        val output = Path.of(outputPath!!)
        Files.createDirectories(output)
        val http = HttpClient.newHttpClient()
        fun oracle(path: String, body: String? = null, method: String = "POST"): ByteArray {
            val request = HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + path))
            if (body != null)
                request
                    .header("Content-Type", "application/json")
                    .method(method, HttpRequest.BodyPublishers.ofString(body))
            val response = http.send(request.build(), HttpResponse.BodyHandlers.ofByteArray())
            assertEquals(200, response.statusCode(), response.body().toString(Charsets.UTF_8))
            return response.body()
        }
        class Memory : StateStore {
            var raw: String? = null

            override fun read() = raw

            override fun write(value: String) {
                raw = value
            }
        }
        fun old(): Pair<Memory, ExpenseService> {
            val store = Memory()
            val service = ExpenseService(store, JdbcSnapshotDecoder())
            service.login(fixture["serverUrl"].asString, fixture["password"].asString)
            service.chooseBudget(fixture["budgetId"].asString)
            service.chooseAccount(fixture["accountId"].asString)
            return store to service
        }
        fun capture(name: String, store: Memory, service: ExpenseService) {
            assertTrue(service.state().pendingCount > 0)
            val expected = oracle("/state") // independently reads settled upstream financial rows
            Files.writeString(output.resolve("$name.state.json"), store.raw!!)
            Files.write(output.resolve("$name.zip"), oracle("/archive"))
            Files.write(output.resolve("$name.oracle.json"), expected)
        }
        fun lost(service: ExpenseService) {
            oracle("/fault", """{"mode":"after-accept","onlyWrites":true}""")
            service.refresh()
            assertNotNull(service.state().error)
            assertTrue(service.state().pendingCount > 0)
        }
        val id = UUID.randomUUID().toString()
        run {
            val (store, service) = old()
            service.markOffline()
            service.saveExpense(
                ExpenseDraft(
                    "12.34",
                    "2020-01-02",
                    "Upgrade corner shop",
                    notes = "lost create",
                    creationId = id,
                )
            )
            lost(service)
            capture("lost-create", store, service)
        }
        run {
            val (store, service) = old()
            val edit = service.beginEdit(id)
            service.saveExpense(edit.original!!.copy(amount = "13.45", notes = "lost edit"), edit)
            lost(service)
            capture("lost-edit", store, service)
        }
        run {
            val (store, service) = old()
            service.deleteExpense(service.beginEdit(id))
            lost(service)
            capture("lost-delete", store, service)
        }
        run {
            val (store, service) = old()
            val seed = service.state().expenses.first()
            val edit = service.beginEdit(seed.id)
            service.markOffline()
            service.saveExpense(
                edit.original!!.copy(amount = "14.56", notes = "pending edit"),
                edit,
            )
            val added =
                service.saveExpense(
                    ExpenseDraft(
                        "2.34",
                        "2020-01-01",
                        "Pending shop",
                        categoryId = fixture["categoryId"].asString,
                        notes = "pending creation",
                    )
                )
            val correction = service.beginEdit(added)
            service.saveExpense(
                correction.original!!.copy(
                    amount = "3.45",
                    categoryId = null,
                    notes = "corrected offline",
                ),
                correction,
            )
            service.deleteExpense(edit)
            oracle("/transactions/${seed.id}", """{"notes":"newer remote edit"}""", "PATCH")
            capture("pending", store, service)
        }
    }

    @Test
    fun setupOnly() {
        val fixturePath = System.getenv("ACTUAL_FIXTURE")
        val outputPath = System.getenv("ACTUAL_UPGRADE_FIXTURE_OUTPUT")
        assumeTrue(fixturePath != null && outputPath != null, "Explicit disposable oracle required")
        val fixture = JsonParser.parseString(File(fixturePath!!).readText()).asJsonObject
        val store =
            object : StateStore {
                var raw: String? = null

                override fun read() = raw

                override fun write(value: String) {
                    raw = value
                }
            }
        val service = ExpenseService(store, JdbcSnapshotDecoder())
        service.login(fixture["serverUrl"].asString, fixture["password"].asString)
        assertNull(service.state().budgetId)
        assertEquals(0, service.state().pendingCount)
        val output = Path.of(outputPath!!)
        Files.createDirectories(output)
        Files.writeString(output.resolve("setup-only.state.json"), store.raw!!)
    }

    @Test
    fun deliverCapturedPending() {
        val fixturePath = System.getenv("ACTUAL_FIXTURE")
        val outputPath = System.getenv("ACTUAL_UPGRADE_FIXTURE_OUTPUT")
        assumeTrue(fixturePath != null && outputPath != null, "Explicit disposable oracle required")
        val fixture = JsonParser.parseString(File(fixturePath!!).readText()).asJsonObject
        val raw =
            Files.readString(
                Path.of("../../actualBudget/tests/fixtures/android-upgrade/pending.state.json")
            )
        val original = JsonParser.parseString(raw).asJsonObject
        require(
            original["server"] == fixture["serverUrl"] &&
                original.getAsJsonObject("budget")["fileId"] == fixture["budgetId"]
        ) {
            "Only the original disposable fixture can receive these messages"
        }
        val store =
            object : StateStore {
                var state = raw

                override fun read() = state

                override fun write(value: String) {
                    state = value
                }
            }
        val service = ExpenseService(store, JdbcSnapshotDecoder())
        service.refresh()
        assertNull(service.state().error)
        assertEquals(0, service.state().pendingCount)
        val response =
            HttpClient.newHttpClient()
                .send(
                    HttpRequest.newBuilder(URI(fixture["oracleUrl"].asString + "/state"))
                        .GET()
                        .build(),
                    HttpResponse.BodyHandlers.ofString(),
                )
        assertEquals(200, response.statusCode())
        val output = Path.of(outputPath!!)
        Files.createDirectories(output)
        Files.writeString(output.resolve("pending.delivered.oracle.json"), response.body())
    }
}

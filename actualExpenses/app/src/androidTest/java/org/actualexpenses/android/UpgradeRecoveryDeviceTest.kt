package org.actualexpenses.android

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.google.gson.JsonParser
import java.io.File
import java.security.MessageDigest
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Visible recovery with real application storage/HTTP. Only disposable emulator fixtures. */
class UpgradeRecoveryDeviceTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val args
        get() = InstrumentationRegistry.getArguments()

    private fun originalPreserved() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val review = File(context.getExternalFilesDir(null), "review")
        val expected = File(review, "android20-original.sha256").readText()
        val bytes = File(context.noBackupFilesDir, "expenses.state").readBytes()
        val actual =
            MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") {
                "%02x".format(it.toInt() and 255)
            }
        assertEquals(expected, actual)
        val case = File(review, "android20-case.txt").readText()
        val raw =
            InstrumentationRegistry.getInstrumentation()
                .context
                .assets
                .open("$case.state.json")
                .bufferedReader()
                .use { it.readText() }
        val actualRaw = requireNotNull(EncryptedStateStore(context).read())
        val wanted = JsonParser.parseString(raw).asJsonObject
        val actualJson = JsonParser.parseString(actualRaw).asJsonObject
        val changed =
            (wanted.keySet() + actualJson.keySet()).filter { wanted[it] != actualJson[it] }
        assertTrue("Released state values changed at: ${changed.joinToString()}", changed.isEmpty())
        // Exact ciphertext above is the immediate force-stopped upgrade boundary.
        // The released Android codec may reorder SavedState object fields when
        // its Snackbar dismisses a network error; message arrays/IDs/HLCs stay exact.

    }

    private fun screenshot(default: String) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val directory = File(context.getExternalFilesDir(null), "review").apply { mkdirs() }
        val name = args.getString("captureName") ?: default
        require(name.matches(Regex("[a-z0-9-]+")))
        File(directory, "$name.png").outputStream().use {
            compose
                .onRoot()
                .captureToImage()
                .asAndroidBitmap()
                .compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    @Test
    fun captureRecovery() {
        assumeTrue(
            "Explicit disposable captureName required",
            args.getString("captureName") != null,
        )
        compose.waitUntil(15_000) {
            compose.onAllNodesWithText("Recover saved expenses").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Find imported budget").performScrollTo().assertIsDisplayed()
        originalPreserved()
        screenshot("android20-recovery")
    }

    @Test
    fun recoverThroughVisibleControlsAndRetainInterruptedDelivery() {
        val server = args.getString("fixtureServer")
        assumeTrue("Explicit disposable fixtureServer required", server != null)
        require(server!!.startsWith("http://10.0.2.2:") || server.startsWith("http://127.0.0.1:"))
        compose.waitUntil(15_000) {
            compose.onAllNodesWithText("Recover saved expenses").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("actualBudget server address").performTextReplacement(server)
        compose.onNodeWithText("Find imported budget").performScrollTo().performClick()
        val graph = (compose.activity.application as ExpensesApplication).graph
        try {
            compose.waitUntil(30_000) {
                !graph.state.value.legacyRecovery &&
                    graph.state.value.pendingCount > 0 &&
                    graph.state.value.error != null
            }
        } catch (failure: Throwable) {
            println("MIGRATION_GRAPH=" + com.google.gson.Gson().toJson(graph.state.value))
            println(compose.onRoot().printToString())
            screenshot("android20-migration-failure")
            throw failure
        }
        assertEquals(args.getString("fixtureBudget"), graph.state.value.budgetId)
        originalPreserved()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val state =
            JsonParser.parseString(EncryptedStateStore(context, "expenses.api-v1.state").read())
                .asJsonObject
        assertFalse(state["legacyFingerprint"].isJsonNull)
        assertTrue(state.getAsJsonArray("pending").size() > 0)
        screenshot("android20-interrupted")
    }

    @Test
    fun foregroundRecoveryAfterSignedReinstall() {
        assumeTrue(
            "Explicit disposable fixtureBudget required",
            args.getString("fixtureBudget") != null,
        )
        val graph = (compose.activity.application as ExpensesApplication).graph
        compose.waitUntil(45_000) {
            !graph.loading.value &&
                !graph.state.value.legacyRecovery &&
                graph.state.value.pendingCount == 0 &&
                graph.state.value.online
        }
        assertEquals(args.getString("fixtureBudget"), graph.state.value.budgetId)
        originalPreserved()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val state =
            JsonParser.parseString(EncryptedStateStore(context, "expenses.api-v1.state").read())
                .asJsonObject
        assertEquals(0, state.getAsJsonArray("pending").size())
        screenshot("android20-recovered")
    }

    @Test
    fun migrationReplayOverLanAndTailnetAddresses() {
        val routes = args.getString("fixtureRoutes")?.split(',')
        assumeTrue("Explicit disposable LAN/tailnet routes required", routes != null)
        require(routes!!.size == 2 && routes.all { java.net.URI(it).scheme == "http" })
        require(
            java.net.URI(routes[0]).host.startsWith("192.168.") &&
                java.net.URI(routes[1]).host.startsWith("100.")
        )
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val original = requireNotNull(EncryptedStateStore(context).read())
        val snapshots = routes.map { route ->
            val memory =
                object : org.actualexpenses.core.StateStore {
                    var raw: String? = null

                    override fun read() = raw

                    override fun write(value: String) {
                        raw = value
                    }
                }
            val legacy =
                object : org.actualexpenses.core.StateStore {
                    override fun read() = original

                    override fun write(value: String): Unit =
                        error("Route replay must never rewrite original work")
                }
            val client = org.actualexpenses.core.api.ApiExpenseService(memory, legacy)
            client.connect(route)
            assertEquals(args.getString("fixtureBudget"), client.state().budgetId)
            client.refresh()
            assertNull(client.state().error)
            assertEquals(0, client.state().pendingCount)
            assertEquals(route, client.state().server)
            client.state()
        }
        assertEquals(snapshots[0].expenses, snapshots[1].expenses)
        assertEquals(snapshots[0].selectedAccountId, snapshots[1].selectedAccountId)
        assertEquals(snapshots[0].payeeCategoryAssociations, snapshots[1].payeeCategoryAssociations)
        originalPreserved()
        println(
            "LAN_TAILNET_REPLAY_PASS routes=${routes.joinToString()} ids=${snapshots[0].expenses.map {it.id}}"
        )
    }
}

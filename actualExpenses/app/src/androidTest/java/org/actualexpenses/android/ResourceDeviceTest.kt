package org.actualexpenses.android

import android.net.TrafficStats
import android.os.Debug
import android.os.Process
import android.os.SystemClock
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.platform.app.InstrumentationRegistry
import com.google.gson.Gson
import java.io.File
import java.security.MessageDigest
import org.junit.After
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Opt-in emulator observation of the real UI, storage and HTTP adapters. */
class ResourceDeviceTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val graph get() = (context.applicationContext as ExpensesApplication).graph
    private val observations = mutableListOf<Map<String, Any?>>()
    private val baseline get() = InstrumentationRegistry.getArguments().getString("resourceBaseline") == "true"

    @After fun saveObservations() {
        if (observations.isNotEmpty()) {
            File(context.getExternalFilesDir(null), "resource-observations.json")
                .writeText(Gson().toJson(observations))
        }
    }

    private fun counters(): Map<String, Long?> {
        val memory = Debug.MemoryInfo().also(Debug::getMemoryInfo)
        val io = runCatching {
            File("/proc/self/io").readLines().associate {
                val parts = it.split(":", limit = 2)
                parts[0] to parts[1].trim().toLong()
            }
        }.getOrDefault(emptyMap())
        return mapOf(
            "elapsed_ms" to SystemClock.elapsedRealtime(),
            "process_cpu_ms" to Process.getElapsedCpuTime(),
            "pss_kib" to memory.totalPss.toLong(),
            "uid_rx_bytes" to TrafficStats.getUidRxBytes(Process.myUid()).takeIf { it >= 0 },
            "uid_tx_bytes" to TrafficStats.getUidTxBytes(Process.myUid()).takeIf { it >= 0 },
            "storage_read_bytes" to io["read_bytes"],
            "storage_write_bytes" to io["write_bytes"],
            "syscall_write_bytes" to io["wchar"],
        )
    }

    private fun observe(phase: String, sample: Int, action: () -> Unit) {
        val before = counters()
        val started = SystemClock.elapsedRealtime()
        action()
        val actionElapsed = SystemClock.elapsedRealtime() - started
        val after = counters()
        observations += mapOf("phase" to phase, "sample" to sample, "action_elapsed_ms" to actionElapsed, "before" to before, "after" to after)
        if (phase.endsWith("_idle")) {
            listOf("uid_rx_bytes", "uid_tx_bytes").forEach { key ->
                if (before[key] != null) assertEquals("Idle network traffic: $key", before[key], after[key])
            }
        }
    }

    private fun ready() {
        compose.waitUntil(45_000) {
            compose.onAllNodesWithContentDescription("Refresh expenses")
                .filter(isEnabled()).fetchSemanticsNodes().size == 1 && graph.state.value.pendingCount == 0
        }
        compose.waitForIdle()
        assertNull(graph.state.value.error)
        assertTrue(graph.state.value.online)
        assertEquals(0, graph.state.value.pendingCount)
    }

    private fun cacheHash(): String = MessageDigest.getInstance("SHA-256")
        .digest(File(context.noBackupFilesDir, if (baseline) "expenses.state" else "expenses.api-v1.state").readBytes())
        .joinToString("") { "%02x".format(it.toInt() and 255) }

    private fun settleNetwork() {
        fun network() = TrafficStats.getUidRxBytes(Process.myUid()) to TrafficStats.getUidTxBytes(Process.myUid())
        var previous = network()
        var quiet = 0
        repeat(10) {
            Thread.sleep(1_000)
            val current = network()
            quiet = if (current == previous) quiet + 1 else 0
            if (quiet == 2) return
            previous = current
        }
        fail("Network did not settle before idle observation")
    }

    @Test fun refreshExpenseAndIdleCosts() {
        val args = InstrumentationRegistry.getArguments()
        val server = args.getString("resourceServer")
        assumeTrue("Explicit disposable emulator resourceServer required", server != null)
        require(server!!.matches(Regex("http://10\\.0\\.2\\.2:[0-9]+")))
        val budget = requireNotNull(args.getString("resourceBudget"))
        val account = requireNotNull(args.getString("resourceAccount"))
        val samples = (args.getString("resourceSamples") ?: "20").toInt()
        require(samples in 1..20)
        compose.waitUntil(15_000) { compose.onAllNodesWithText(if (baseline) "Connect to Actual" else "Connect to actualBudget").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Server address").performTextInput(server)
        if (baseline) {
            val secret = File(context.filesDir, "resource-baseline-password")
            compose.onNodeWithText("Actual password").performTextInput(secret.readText().trim())
        }
        compose.onNodeWithText("Connect", substring = false).performScrollTo().performClick()
        compose.waitUntil(30_000) { compose.onAllNodesWithText("Choose a budget").fetchSemanticsNodes().isNotEmpty() }
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(budget))
        compose.onNodeWithText(budget).performScrollTo().performClick()
        compose.waitUntil(30_000) { compose.onAllNodesWithText("Choose an account").fetchSemanticsNodes().isNotEmpty() }
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(account))
        compose.onNodeWithText(account).performScrollTo().performClick()
        ready()
        repeat(samples) { index ->
            val hash = cacheHash()
            observe("unchanged_refresh", index) {
                compose.onNodeWithContentDescription("Refresh expenses").performClick()
                ready()
            }
            val refreshed = cacheHash()
            observations[observations.lastIndex] = observations.last() + ("cache_unchanged" to (hash == refreshed))
            if (!baseline) assertEquals("Unchanged refresh rewrote encrypted state", hash, refreshed)
            observe("warm_foreground", index) {
                compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
                compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
                ready()
            }
            val foreground = cacheHash()
            observations[observations.lastIndex] = observations.last() + ("cache_unchanged" to (refreshed == foreground))
            if (!baseline) assertEquals("Unchanged foreground rewrote encrypted state", refreshed, foreground)
        }
        repeat(samples) { index ->
            val payee = "Resource expense $budget $index"
            compose.onNodeWithContentDescription("Add expense").performClick()
            compose.onNode(hasSetTextAction() and hasText("Amount", substring = true)).performTextInput("4.27")
            compose.onNodeWithText("Payee (optional)").performTextInput(payee)
            compose.onNodeWithText("Save expense").performScrollTo()
            observe("expense_acknowledgement", index) {
                compose.onNodeWithText("Save expense").performClick()
                ready()
                compose.waitUntil(45_000) {
                    graph.state.value.pendingCount == 0 && graph.state.value.expenses.any { it.payee == payee && it.amountMinor == 427L }
                }
                // A backdated purchase may be pinned after ten newer expenses.
                if (compose.onAllNodesWithText(payee).fetchSemanticsNodes().isEmpty()) {
                    compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText(payee))
                }
                compose.onNodeWithText(payee).assertExists()
            }
        }
        // Expense acknowledgement can precede the trailing foreground schedule
        // check. Preserve its counters separately; do not call that work idle.
        observe("settlement", 0) { settleNetwork() }
        val hash = cacheHash()
        observe("foreground_idle", 0) { Thread.sleep(35_000) }
        assertEquals("Foreground idle rewrote encrypted state", hash, cacheHash())
        compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
        observe("background_idle", 0) { Thread.sleep(35_000) }
        assertEquals("Background idle rewrote encrypted state", hash, cacheHash())
        compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        ready()
    }
}

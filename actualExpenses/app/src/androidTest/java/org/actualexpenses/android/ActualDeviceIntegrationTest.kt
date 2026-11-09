package org.actualexpenses.android

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Optional real-server acceptance. Only a disposable fixture on the emulator host is accepted. */
class ActualDeviceIntegrationTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun setupAndSaveThroughTheRealAndroidAdapters() {
        val args = InstrumentationRegistry.getArguments()
        val server = args.getString("fixtureServer")
        assumeTrue("Pass disposable fixtureServer to run", server != null)
        require(server!!.startsWith("http://10.0.2.2:")) { "Device test accepts only the disposable emulator-host server" }
        compose.waitUntil(15_000) { compose.onAllNodesWithText("Connect to actualBudget").fetchSemanticsNodes().isNotEmpty() }
        screenshot("01-setup")
        compose.onNodeWithText("Server address").performTextInput(server)
        compose.onNodeWithText("Connect", substring = false).performScrollTo().performClick()
        compose.waitUntil(30_000) { compose.onAllNodesWithText("Native device fixture").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Native device fixture").performScrollTo().performClick()
        compose.waitUntil(30_000) { compose.onAllNodesWithText("Spending").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Spending").performClick()
        compose.waitUntil(15_000) { compose.onAllNodesWithContentDescription("Add expense").fetchSemanticsNodes().isNotEmpty() }
        screenshot("02-register")
        val purchase="Device acceptance ${System.currentTimeMillis()}"
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNode(hasSetTextAction() and hasText("Amount", substring = true)).performTextInput("4.27")
        compose.onNodeWithText("Payee (optional)").performTextInput(purchase)
        compose.onNodeWithText("Save expense").performScrollTo()
        screenshot("03-editor")
        compose.onNodeWithText("Save expense").performClick()
        val graph = (compose.activity.application as ExpensesApplication).graph
        compose.waitUntil(45_000) {
            val state = graph.state.value
            state.pendingCount == 0 && state.expenses.any { it.payee == purchase && it.amountMinor == 427L }
        }
        compose.onNodeWithText(purchase).assertExists()
        screenshot("04-saved")
    }

    private fun screenshot(name: String) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val directory = File(context.getExternalFilesDir(null), "review").apply { mkdirs() }
        File(directory, "$name.png").outputStream().use {
            compose.onRoot().captureToImage().asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}

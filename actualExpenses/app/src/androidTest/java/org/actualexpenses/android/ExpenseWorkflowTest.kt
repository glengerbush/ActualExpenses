package org.actualexpenses.android

import android.graphics.Bitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.runtime.mutableStateOf
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.time.LocalDate
import org.actualexpenses.core.EditSession
import org.actualexpenses.core.ExpenseContext
import org.actualexpenses.core.ExpenseDraft

/** Test the public screen, not composition helpers or private state. Fixture data is synthetic. */
class ExpenseWorkflowTest {
    @get:Rule val compose = createComposeRule()
    private val actions = RecordingActions()
    private val account = Choice("spending", "Spending")
    private val state = ExpenseUiState(screen = Screen.EXPENSES, selectedBudget = account, online = true, expenseContext = ExpenseContext("http://fixture", "budget", "spending", "00000000000000000000000000000000", "USD", 2, "comma-dot"))

    @Test fun expenseHistoryUsesBudgetNavigationWithoutAccounts() {
        compose.setContent { ActualExpensesTheme { ExpensesScreen(state, actions) } }
        compose.onNodeWithContentDescription("Choose account").assertDoesNotExist()
        compose.onNodeWithContentDescription("Add expense").assertIsDisplayed()
        val directory = File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null), "review").apply { mkdirs() }
        File(directory, "unified-history.png").outputStream().use { compose.onRoot().captureToImage().asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it) }
        compose.onNodeWithContentDescription("Choose budget").performClick()
        compose.runOnIdle { assertTrue(actions.budgetSelection) }
    }

    @Test fun rejectedSplitEditHasExplicitRecoveryOutsideExpenseList() {
        compose.setContent { ActualExpensesTheme { ExpensesScreen(state.copy(splitRecovery=true,pendingCount=1),actions) } }
        compose.onNodeWithText("This expense became a split or transfer.",substring=true).assertIsDisplayed()
        compose.onNodeWithText("Discard rejected expense edit").assertIsDisplayed()
        val directory=File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null),"review").apply{mkdirs()}
        File(directory,"split-recovery.png").outputStream().use {compose.onRoot().captureToImage().asAndroidBitmap().compress(Bitmap.CompressFormat.PNG,100,it)}
        compose.onNodeWithText("Discard rejected expense edit").performClick()
        compose.runOnIdle {assertTrue(actions.discardedSplit)}
    }

    @Test fun amountOnlyExpenseNeedsNoOptionalFields() {
        compose.setContent { ActualExpensesTheme { ExpensesScreen(state, actions) } }
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNodeWithText("Amount (USD)").performTextInput("12.34")
        compose.onNodeWithText("Save expense").performScrollTo().performClick()
        compose.runOnIdle {
            assertEquals("12.34", actions.saved?.amount)
            assertEquals("", actions.saved?.payee)
            assertNull(actions.saved?.categoryId)
            assertEquals(LocalDate.now(), actions.saved?.date)
        }
    }

    @Test fun zeroAmountDoesNotSaveAndExplainsCorrection() {
        compose.setContent { ActualExpensesTheme { ExpensesScreen(state, actions) } }
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNodeWithText("Amount (USD)").performTextInput("0")
        compose.onNodeWithText("Save expense").performScrollTo().performClick()
        compose.onNodeWithText("Enter a positive expense amount", substring = true).assertExists()
        compose.runOnIdle { assertNull(actions.saved) }
    }

    @Test fun offlineSyncedExpenseCannotOpenButAddingWorks() {
        val row = ExpenseRow("coffee", -325, LocalDate.now(), "Corner Coffee", null, "", "", false, false, false)
        compose.setContent { ActualExpensesTheme { ExpensesScreen(state.copy(online = false, expenses = listOf(row)), actions) } }
        compose.onNodeWithText("Corner Coffee").assertIsNotEnabled()
        compose.runOnIdle { assertNull(actions.editing) }
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNodeWithText("Amount (USD)").assertExists()
    }

    @Test fun payeeAssociationAutofillsAndOverrideRemainsExpenseOnly() {
        val known = state.copy(payees = listOf(Choice("cafe", "Corner Coffee")), categories = listOf(Choice("food", "Food"), Choice("gifts", "Gifts")), associations = mapOf("cafe" to "food"))
        compose.setContent { ActualExpensesTheme { ExpensesScreen(known, actions) } }
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNodeWithText("Amount (USD)").performTextInput("7.50")
        compose.onNodeWithText("Payee (optional)").performTextInput("Corner")
        compose.onNodeWithText("Corner Coffee").performScrollTo().performClick()
        compose.onNodeWithText("Food").performScrollTo().performClick()
        compose.onNodeWithText("Gifts").performClick()
        compose.onNodeWithText("Save expense").performScrollTo().performClick()
        compose.runOnIdle {
            assertEquals("gifts", actions.saved?.categoryId)
            assertEquals("food", known.associations["cafe"])
        }
    }

    @Test fun deletionNeedsConfirmationShowingTheExpense() {
        val row = ExpenseRow("coffee", -325, LocalDate.of(2026, 10, 16), "Corner Coffee", null, "", "", false, false, true)
        compose.setContent { ActualExpensesTheme { ExpensesScreen(state.copy(expenses = listOf(row)), actions) } }
        compose.onNodeWithText("Corner Coffee").performClick()
        compose.onNodeWithContentDescription("Delete expense").performClick()
        compose.onNodeWithText("Delete expense?").assertExists()
        compose.onNode(hasText("Corner Coffee", substring = true) and hasText("3.25", substring = true)).assertExists()
        compose.runOnIdle { assertNull(actions.deleted) }
        compose.onNodeWithText("Delete", substring = false).performClick()
        compose.runOnIdle { assertEquals("coffee", actions.deleted) }
    }

    @Test fun recreatedEditorObservesSaveCompletionWithoutOldCallback() {
        val restoration = StateRestorationTester(compose)
        val live = mutableStateOf(state)
        restoration.setContent { ActualExpensesTheme { ExpensesScreen(live.value, actions) } }
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNodeWithText("Amount (USD)").performTextInput("8.50")
        compose.onNodeWithText("Save expense").performScrollTo().performClick()
        val operation = actions.saved!!.operationId
        compose.runOnIdle { live.value = state.copy(savingOperationId = operation) }
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Saving…").assertIsNotEnabled()
        // The original composition/callback is gone; lifecycle-owned completion must close it.
        compose.runOnIdle { live.value = state.copy(completedOperationId = operation) }
        compose.onNodeWithContentDescription("Add expense").assertExists()
        compose.runOnIdle { assertEquals(1, actions.saveCount) }
    }

    @Test fun serverSelectionNeedsNoCredentials() {
        compose.setContent { ActualExpensesTheme { ExpensesScreen(ExpenseUiState(), actions) } }
        compose.onNodeWithText("Connect to actualBudget").assertExists()
        compose.onNodeWithText("Actual password").assertDoesNotExist()
        compose.onNodeWithText("Server address").performTextInput("http://budget:5006")
        compose.onNodeWithText("Connect",substring=false).performScrollTo().performClick()
        compose.runOnIdle { assertEquals("http://budget:5006",actions.server) }
    }

    @Test fun preservedLegacyWorkShowsRecoveryWithoutSetupOrExpenseActions() {
        compose.setContent { ActualExpensesTheme { ExpensesScreen(ExpenseUiState(screen=Screen.RECOVERY,recoveryMessage="Previous budget and pending changes are preserved. Keep app data intact."),actions) } }
        compose.onNodeWithText("Recover saved expenses").assertExists()
        compose.onNodeWithText("Keep app data intact.",substring=true).assertExists()
        compose.onNodeWithText("Connect",substring=false).assertDoesNotExist()
        compose.onNodeWithContentDescription("Add expense").assertDoesNotExist()
    }

    @Test fun restoredFormKeepsOriginalCurrencyAndBudgetContext() {
        val context=ExpenseContext("http://original","budget","spending","original-epoch","USD",2,"comma-dot")
        val live=mutableStateOf(state.copy(expenseContext=context))
        val restoration=StateRestorationTester(compose)
        restoration.setContent {ActualExpensesTheme {ExpensesScreen(live.value,actions)}}
        compose.onNodeWithContentDescription("Add expense").performClick()
        compose.onNodeWithText("Amount (USD)").performTextInput("12.34")
        compose.runOnIdle {live.value=state.copy(currency="JPY",decimalPlaces=0,expenseContext=context.copy(epoch="restored-epoch",currency="JPY",decimalPlaces=0))}
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Amount (USD)").assertExists()
        compose.onNodeWithText("Save expense").performScrollTo().performClick()
        compose.runOnIdle {assertEquals(context,actions.saved?.context);assertEquals("12.34",actions.saved?.amount)}
    }

    @Test fun restoredEditorKeepsOriginalFieldBaselineAndDeletionContext() {
        val context=ExpenseContext("http://original","budget","spending","original-epoch","USD",2,"comma-dot")
        val original=ExpenseDraft("3.25","2026-10-16","Corner Coffee",notes="Original notes")
        val session=EditSession("coffee",true,original,context)
        actions.session=session
        val row=ExpenseRow("coffee",325,LocalDate.of(2026,10,16),"Corner Coffee",null,"","Original notes",false,false,true)
        val live=mutableStateOf(state.copy(expenses=listOf(row),expenseContext=context))
        val restoration=StateRestorationTester(compose)
        restoration.setContent {ActualExpensesTheme {ExpensesScreen(live.value,actions)}}
        compose.onNodeWithText("Corner Coffee").performClick()
        compose.runOnIdle {live.value=state.copy(currency="JPY",decimalPlaces=0,expenses=listOf(row.copy(amountMinor=999,notes="Remote notes")),expenseContext=context.copy(epoch="restored-epoch",currency="JPY",decimalPlaces=0))}
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithText("Amount (USD)").assertExists()
        compose.onNodeWithText("Original notes").assertExists()
        compose.onNodeWithText("Save expense").performScrollTo().performClick()
        compose.runOnIdle {assertEquals(session,actions.saved?.edit);assertEquals("3.25",actions.saved?.amount)}
        compose.onNodeWithContentDescription("Delete expense").performClick()
        compose.onNode(hasText("Corner Coffee",substring=true) and hasText("3.25",substring=true)).assertExists()
        compose.onNodeWithText("Delete",substring=false).performClick()
        compose.runOnIdle {assertEquals(session,actions.deletedSession)}
    }

    @Test fun scheduleRecoveryKeepsExpenseCaptureAndExplicitRequestControls(){
        compose.setContent{ActualExpensesTheme{ExpensesScreen(state.copy(scheduledWorkPending=true,scheduledWorkError="Review older dates in actualBudget before retrying."),actions)}}
        compose.onNodeWithContentDescription("Add expense").assertIsDisplayed()
        if (InstrumentationRegistry.getArguments().getString("scheduleCapture") == "true") {
            compose.waitForIdle()
            InstrumentationRegistry.getInstrumentation().sendStatus(2, android.os.Bundle().apply {
                putString("stream", "SCHEDULE_CAPTURE_READY\n")
            })
            Thread.sleep(10_000)
        }
        compose.onNodeWithText("Retry scheduled entries").performScrollTo().performClick()
        assertTrue(actions.retriedSchedules)
        compose.onNodeWithText("Cancel saved schedule request").performScrollTo().performClick()
        assertTrue(actions.cancelledSchedules)
    }

    private class RecordingActions : ExpenseUiActions {
        override fun showServer() = Unit
        override fun cancelConnection() = Unit
        var saved: ExpenseForm? = null
        var saveCount = 0
        var editing: String? = null
        var deleted: String? = null
        var session: EditSession? = null
        var deletedSession: EditSession? = null
        override fun beginEdit(id: String, completed: (EditSession) -> Unit) { editing = id; completed(session ?: EditSession(id,true)) }
        var server: String? = null
        override fun connect(server: String) {this.server=server}
        override fun chooseBudget(id: String) = Unit
        var budgetSelection = false
        override fun showBudgets() { budgetSelection = true }
        var retriedSchedules=false
        var cancelledSchedules=false
        override fun retryScheduledWork(){retriedSchedules=true}
        override fun cancelScheduledWork(){cancelledSchedules=true}
        override fun refresh() = Unit
        override fun save(form: ExpenseForm) { saved = form; saveCount++ }
        override fun delete(session: EditSession, operationId: String) { deleted = session.transactionId;deletedSession=session }
        var discardedSplit = false
        override fun discardRejectedSplitEdit() {discardedSplit=true}
        override fun dismissMessage() = Unit
    }
}

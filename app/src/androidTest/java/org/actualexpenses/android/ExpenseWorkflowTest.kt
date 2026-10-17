package org.actualexpenses.android

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.runtime.mutableStateOf
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.time.LocalDate

/** Test the public screen, not composition helpers or private state. Fixture data is synthetic. */
class ExpenseWorkflowTest {
    @get:Rule val compose = createComposeRule()
    private val actions = RecordingActions()
    private val account = Choice("spending", "Spending")
    private val state = ExpenseUiState(screen = Screen.EXPENSES, selectedAccount = account, accounts = listOf(account), online = true)

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
        compose.onNodeWithText("Enter a positive amount with up to 2 decimal places.").assertExists()
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
        actions.completeAutomatically = false
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

    private class RecordingActions : ExpenseUiActions {
        override fun backToLogin() = Unit
        var saved: ExpenseForm? = null
        var completeAutomatically = true
        var saveCount = 0
        var editing: String? = null
        var deleted: String? = null
        override fun beginEdit(id: String, completed: () -> Unit) { editing = id; completed() }
        override fun login(server: String, password: String) = Unit
        override fun chooseBudget(id: String) = Unit
        override fun chooseAccount(id: String) = Unit
        override fun showAccounts() = Unit
        override fun refresh() = Unit
        override fun save(form: ExpenseForm, completed: () -> Unit) { saved = form; saveCount++; if (completeAutomatically) completed() }
        override fun delete(id: String, operationId: String, completed: () -> Unit) { deleted = id; completed() }
        override fun dismissMessage() = Unit
    }
}

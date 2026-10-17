package org.actualexpenses.android

import android.app.DatePickerDialog
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.DateRange
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.ArrowDropDown
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.math.BigDecimal
import java.text.NumberFormat
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Currency
import java.util.UUID

/*
 THESIS: Record a purchase, then return to a compact expense register. No dashboard.
 OWN-WORLD: Native Material 3, quiet surfaces, system type, restrained green action.
 STORY: Account context, delivery state, recent purchases, one Add expense action.
 FIRST VIEWPORT: Top account switch, compact sync state, full-width register, bottom FAB.
 FORM: Brief-pinned minimal native Android Operate; seed 5ae1515c is subordinate to it.
 FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, and DESIGN.md
*/
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ExpensesScreen(state: ExpenseUiState, actions: ExpenseUiActions) {
    var editorId by rememberSaveable { mutableStateOf<String?>(null) }
    val snackbar = remember { SnackbarHostState() }
    val message = state.error ?: state.notice
    LaunchedEffect(message) {
        if (message != null) {
            snackbar.showSnackbar(message, withDismissAction = true, duration = SnackbarDuration.Long)
            actions.dismissMessage()
        }
    }
    BackHandler(editorId != null) { editorId = null }
    if (editorId != null) {
        val original = remember(editorId) { state.expenses.find { it.id == editorId } }
        ExpenseEditor(state, editorId?.takeUnless { it == "new" }, original, actions, snackbar) { editorId = null }
        return
    }
    BackHandler(state.screen == Screen.ACCOUNTS && state.selectedAccount != null) {
        state.selectedAccount?.let { actions.chooseAccount(it.id) }
    }
    BackHandler(state.screen == Screen.BUDGETS) { actions.backToLogin() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    if (state.screen == Screen.EXPENSES) {
                        TextButton(onClick = actions::showAccounts) {
                            Text(state.selectedAccount?.name ?: "Account", style = MaterialTheme.typography.titleLarge,
                                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                            Icon(Icons.Outlined.ArrowDropDown, contentDescription = "Choose account")
                        }
                    } else Text("ActualExpenses")
                },
                navigationIcon = {
                    if (state.screen == Screen.BUDGETS) IconButton(onClick = actions::backToLogin) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, "Back to sign in") }
                    else if (state.screen == Screen.ACCOUNTS && state.selectedAccount != null) IconButton(onClick = { actions.chooseAccount(state.selectedAccount.id) }) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, "Back to expenses")
                    }
                },
                actions = {
                    if (state.screen != Screen.LOGIN) {
                        IconButton(onClick = actions::refresh, enabled = !state.busy) {
                            Icon(Icons.Outlined.Refresh, if (state.screen == Screen.EXPENSES) "Refresh expenses" else "Refresh list")
                        }
                    }
                },
            )
        },
        floatingActionButton = {
            if (state.screen == Screen.EXPENSES) ExtendedFloatingActionButton(
                modifier = Modifier.semantics { contentDescription = "Add expense" },
                onClick = { editorId = "new" },
                icon = { Icon(Icons.Outlined.Add, contentDescription = null) },
                text = { Text("Add expense") },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth()) {
                if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                if (state.requiresSignIn && state.screen != Screen.LOGIN) TextButton(onClick = actions::backToLogin,
                    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp)) {
                    Text("Sign in to sync · changes saved on this phone")
                }
                when (state.screen) {
                    Screen.LOGIN -> Login(state, actions)
                    Screen.BUDGETS -> Choices("Choose a budget", "Use one budget on this phone.", "No budgets yet. Sync a budget in Actual, then refresh this list.", state.budgets, state.busy, actions::chooseBudget)
                    Screen.ACCOUNTS -> Choices("Choose an account", "Where should expenses go?", "No open accounts. Add or reopen an account in Actual, then refresh this list.", state.accounts, state.busy, actions::chooseAccount)
                    Screen.EXPENSES -> ExpenseList(state) { row -> actions.beginEdit(row.id) { editorId = row.id } }
                }
            }
        }
    }
}

@Composable
private fun Login(state: ExpenseUiState, actions: ExpenseUiActions) {
    var server by rememberSaveable(state.server) { mutableStateOf(state.server) }
    // Password is never persisted in saved-instance state or app storage.
    var password by remember { mutableStateOf("") }
    Column(Modifier.verticalScroll(rememberScrollState()).imePadding().padding(24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Text(if (state.server.isEmpty()) "Connect to Actual" else "Sign in again", style = MaterialTheme.typography.headlineMedium)
        Text("Use your Actual server address and password.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        OutlinedTextField(server, { server = it }, label = { Text("Server address") },
            placeholder = { Text("http://server:5006") }, singleLine = true, enabled = !state.busy,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next), modifier = Modifier.fillMaxWidth())
        OutlinedTextField(password, { password = it }, label = { Text("Actual password") }, singleLine = true,
            visualTransformation = PasswordVisualTransformation(), enabled = !state.busy,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done), modifier = Modifier.fillMaxWidth())
        Button(onClick = { actions.login(server.trim(), password) }, enabled = server.isNotBlank() && password.isNotEmpty() && !state.busy,
            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(if (state.busy) "Connecting…" else "Connect") }
        if (state.pendingCount > 0) Text("${state.pendingCount} pending changes are saved on this phone.", style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun Choices(title: String, description: String, emptyMessage: String, choices: List<Choice>, busy: Boolean, onChoose: (String) -> Unit) {
    LazyColumn(contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item {
            Text(title, style = MaterialTheme.typography.headlineMedium, modifier = Modifier.semantics { heading() })
            Text(description, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 8.dp, bottom = 16.dp))
        }
        if (choices.isEmpty() && !busy) item { Text(emptyMessage) }
        items(choices, key = { it.id }) { item ->
            OutlinedButton(onClick = { onChoose(item.id) }, enabled = !busy, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                Text(item.name, modifier = Modifier.fillMaxWidth(), style = MaterialTheme.typography.titleMedium)
            }
        }
    }
}

@Composable
private fun ExpenseList(state: ExpenseUiState, onEdit: (ExpenseRow) -> Unit) {
    LazyColumn(contentPadding = PaddingValues(bottom = 104.dp)) {
        item {
            Column(Modifier.padding(horizontal = 24.dp, vertical = 12.dp)) {
                Text(if (state.busy) "Syncing…" else if (state.pendingCount > 0) "${state.pendingCount} pending · ${if (state.online) "Connected" else "Offline"}"
                    else if (state.online) "Up to date" else "Offline · expenses saved on this phone",
                    style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("Recent expenses", style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 24.dp).semantics { heading() })
                if (!state.online) Text("You can add expenses. Synced expenses can be edited when connected.", style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 8.dp))
            }
        }
        if (state.expenses.isEmpty()) item {
            Column(Modifier.padding(horizontal = 24.dp, vertical = 40.dp)) {
                Text("Ready for your next purchase", style = MaterialTheme.typography.titleLarge)
                Text("Tap Add expense to record it. Only ordinary, unreconciled expenses appear here.",
                    modifier = Modifier.padding(top = 12.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        items(state.expenses, key = { it.id }) { expense ->
            ListItem(
                modifier = Modifier.clickable(enabled = expense.editable && !expense.pendingDeletion) { onEdit(expense) },
                headlineContent = { Text(expense.payee.ifBlank { "Expense" }, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                supportingContent = {
                    Column {
                        Text(listOf(dateLabel(expense.date), expense.category).filter { it.isNotBlank() }.joinToString(" · "), maxLines = 2, overflow = TextOverflow.Ellipsis)
                        if (expense.pending) Text(if (expense.pendingDeletion) "Deletion pending" else "Pending sync", color = MaterialTheme.colorScheme.primary,
                            style = MaterialTheme.typography.labelMedium)
                    }
                },
                trailingContent = { Text(money(expense.amountMinor, state.currency, state.decimalPlaces), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Medium) },
            )
            HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.outlineVariant)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ExpenseEditor(state: ExpenseUiState, transactionId: String?, existing: ExpenseRow?, actions: ExpenseUiActions, snackbar: SnackbarHostState, close: () -> Unit) {
    val key = transactionId ?: "new"
    val operationId = rememberSaveable(key) { UUID.randomUUID().toString() }
    var amount by rememberSaveable(key) { mutableStateOf(existing?.let { BigDecimal.valueOf(it.amountMinor).abs().movePointLeft(state.decimalPlaces).toPlainString() } ?: "") }
    var payee by rememberSaveable(key) { mutableStateOf(existing?.payee ?: "") }
    var categoryId by rememberSaveable(key) { mutableStateOf(existing?.categoryId) }
    var date by rememberSaveable(key) { mutableStateOf((existing?.date ?: LocalDate.now()).toString()) }
    var notes by rememberSaveable(key) { mutableStateOf(existing?.notes ?: "") }
    var showNotes by rememberSaveable(key) { mutableStateOf(!existing?.notes.isNullOrBlank()) }
    var categoryPicker by rememberSaveable { mutableStateOf(false) }
    var deleteConfirmation by rememberSaveable { mutableStateOf(false) }
    var payeeSuggestions by remember { mutableStateOf(false) }
    var submitted by remember { mutableStateOf(false) }
    val saving = state.savingOperationId == operationId
    LaunchedEffect(state.completedOperationId, state.expenses, operationId) {
        if (state.completedOperationId == operationId ||
            (transactionId == null && state.expenses.any { it.id == operationId }) ||
            (transactionId != null && state.expenses.any { it.id == transactionId && it.pendingDeletion })) close()
    }
    BackHandler(saving) { /* Finish the local atomic save before leaving this form. */ }
    val number = amount.replace(',', '.').toBigDecimalOrNull()
    val valid = number != null && number > BigDecimal.ZERO && number.scale() <= state.decimalPlaces && number.movePointRight(state.decimalPlaces) <= BigDecimal.valueOf(Long.MAX_VALUE)
    val context = LocalContext.current
    val save: () -> Unit = {
        submitted = true
        if (valid && !saving) {
            actions.save(ExpenseForm(transactionId, amount, LocalDate.parse(date), payee.trim(), categoryId, notes, operationId), close)
        }
    }
    Scaffold(
        topBar = { TopAppBar(title = { Text(if (transactionId == null) "Add expense" else "Edit expense") },
            navigationIcon = { IconButton(onClick = close, enabled = !saving) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, "Back") } },
            actions = { if (existing != null) IconButton(onClick = { deleteConfirmation = true }, enabled = !saving) { Icon(Icons.Outlined.Delete, "Delete expense") } }) },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(state.selectedAccount?.name.orEmpty(), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                OutlinedTextField(amount, { amount = it }, label = { Text(if (state.currency.isBlank()) "Amount" else "Amount (${state.currency})") },
                    textStyle = MaterialTheme.typography.headlineMedium, singleLine = true, enabled = !saving, isError = submitted && !valid,
                    supportingText = { if (submitted && !valid) Text(if (state.decimalPlaces == 0) "Enter a positive whole amount." else "Enter a positive amount with up to ${state.decimalPlaces} decimal places.") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Next), modifier = Modifier.fillMaxWidth())
                OutlinedTextField(payee, { payee = it; payeeSuggestions = true }, label = { Text("Payee (optional)") }, singleLine = true, enabled = !saving,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next), modifier = Modifier.fillMaxWidth())
                if (payeeSuggestions && payee.isNotBlank()) {
                    val matches = state.payees.filter { it.name.contains(payee, ignoreCase = true) }.take(5)
                    if (matches.isNotEmpty()) Column {
                        matches.forEach { choice ->
                            TextButton(onClick = { payee = choice.name; payeeSuggestions = false; categoryId = state.associations[choice.id] }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                                Text(choice.name, modifier = Modifier.fillMaxWidth())
                            }
                        }
                    }
                }
                OutlinedButton(onClick = { categoryPicker = true }, enabled = !saving, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                    Text(state.categories.find { it.id == categoryId }?.name ?: "Category (optional)", modifier = Modifier.weight(1f))
                    Icon(Icons.Outlined.ArrowDropDown, contentDescription = null)
                }
                OutlinedButton(onClick = {
                    val selected = LocalDate.parse(date)
                    DatePickerDialog(context, { _, y, m, d -> date = LocalDate.of(y, m + 1, d).toString() }, selected.year, selected.monthValue - 1, selected.dayOfMonth).show()
                }, enabled = !saving, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                    Text(dateLabel(LocalDate.parse(date)), Modifier.weight(1f))
                    Icon(Icons.Outlined.DateRange, contentDescription = "Change date")
                }
                if (showNotes) OutlinedTextField(notes, { notes = it }, label = { Text("Notes (optional)") }, enabled = !saving, minLines = 2, maxLines = 5, modifier = Modifier.fillMaxWidth())
                else TextButton(onClick = { showNotes = true }, enabled = !saving, modifier = Modifier.heightIn(min = 48.dp)) { Text("Add notes") }
                Button(onClick = save, enabled = !saving, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) { Text(if (saving) "Saving…" else "Save expense") }
                if (!state.online) Text("Saved on this phone; syncs when connected.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    if (categoryPicker) CategoryPicker(state.categories, categoryId, { categoryId = it; categoryPicker = false }, { categoryPicker = false })
    if (deleteConfirmation && existing != null) AlertDialog(
        onDismissRequest = { deleteConfirmation = false }, title = { Text("Delete expense?") },
        text = { Text("${money(existing.amountMinor, state.currency, state.decimalPlaces)} · ${existing.payee.ifBlank { "No payee" }}\n${dateLabel(existing.date)}") },
        confirmButton = { TextButton(onClick = { deleteConfirmation = false; actions.delete(existing.id, operationId, close) }) { Text("Delete", color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = { deleteConfirmation = false }) { Text("Keep expense") } },
    )
}

@Composable
private fun CategoryPicker(categories: List<Choice>, selected: String?, choose: (String?) -> Unit, dismiss: () -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    AlertDialog(onDismissRequest = dismiss, title = { Text("Choose category") },
        text = {
            Column {
                OutlinedTextField(query, { query = it }, label = { Text("Search categories") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                LazyColumn(Modifier.heightIn(max = 320.dp).padding(top = 8.dp)) {
                    item { TextButton(onClick = { choose(null) }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { this.selected = selected == null }) { Text("Uncategorized", Modifier.fillMaxWidth()) } }
                    items(categories.filter { it.name.contains(query, ignoreCase = true) }, key = { it.id }) { category ->
                        TextButton(onClick = { choose(category.id) }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { this.selected = category.id == selected }) {
                            Text(category.name, modifier = Modifier.fillMaxWidth(), fontWeight = if (category.id == selected) FontWeight.Bold else FontWeight.Normal)
                        }
                    }
                }
            }
        }, confirmButton = { TextButton(onClick = dismiss) { Text("Cancel") } })
}

private fun dateLabel(date: LocalDate) = date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
private fun money(minor: Long, currency: String, decimalPlaces: Int): String = (if (currency.isBlank()) NumberFormat.getNumberInstance() else NumberFormat.getCurrencyInstance()).apply {
    if (currency.isNotBlank()) runCatching { Currency.getInstance(currency) }.getOrNull()?.let { this.currency = it }
    minimumFractionDigits = decimalPlaces
    maximumFractionDigits = decimalPlaces
}.format(BigDecimal.valueOf(minor).abs().movePointLeft(decimalPlaces))

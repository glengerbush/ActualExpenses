package org.actualexpenses.android

import android.app.Application
import android.content.Context
import androidx.work.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.actualexpenses.core.AppState
import org.actualexpenses.core.api.ApiExpenseService
import java.util.concurrent.TimeUnit

class ExpensesApplication : Application() {
    val graph by lazy { AppGraph(this) }
}

class AppGraph(private val context: Context) {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    val state = MutableStateFlow(AppState())
    val loading = MutableStateFlow(true)
    val fatalError = MutableStateFlow<String?>(null)
    private val mutex = Mutex()
    private val service = scope.async {
        try {
            ApiExpenseService(EncryptedStateStore(context, "expenses.api-v1.state"), EncryptedStateStore(context)).also { state.value = it.state() }
        } catch (error: Exception) {
            fatalError.value = "Saved data could not be opened. Keep app data intact and seek support. ${error.message.orEmpty()}"
            throw error
        } finally { loading.value = false }
    }

    suspend fun <T> access(block: (ApiExpenseService) -> T): T = mutex.withLock {
        val current = service.await()
        try { block(current) } finally { state.value = current.state() }
    }

    suspend fun refresh(foreground:Boolean=true) {
        val current = service.await()
        try {
            if (current.state().serverSelected) current.refresh(foreground=foreground)
        } finally { mutex.withLock { state.value = current.state() } }
    }

    fun schedulePending() {
        if (state.value.pendingCount == 0 || state.value.syncBlocked || !state.value.serverSelected) return
        // A scheduler restriction must never invalidate an expense already saved locally.
        runCatching {
            WorkManager.getInstance(context).enqueueUniqueWork(
                "deliver-pending-expenses", ExistingWorkPolicy.APPEND_OR_REPLACE,
                OneTimeWorkRequestBuilder<PendingSyncWorker>()
                    .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                    .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS).build(),
            )
        }
    }
}

class PendingSyncWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val graph = (applicationContext as ExpensesApplication).graph
        return try {
            if (graph.access { it.state().pendingCount } > 0) graph.refresh(foreground=false)
            val state = graph.access { it.state() }
            when {
                state.pendingCount == 0 -> Result.success()
                !state.serverSelected || state.syncBlocked -> Result.failure()
                else -> Result.retry()
            }
        } catch (_: Exception) { Result.retry() }
    }
}

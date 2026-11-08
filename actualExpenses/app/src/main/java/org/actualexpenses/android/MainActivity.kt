package org.actualexpenses.android

import android.net.ConnectivityManager
import android.net.Network
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Color
import androidx.lifecycle.compose.collectAsStateWithLifecycle

class MainActivity : ComponentActivity() {
    private val model: ExpenseViewModel by viewModels()
    private val connectivity by lazy { getSystemService(ConnectivityManager::class.java) }
    private var registered = false
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) { runOnUiThread { model.refresh() } }
        override fun onLost(network: Network) {
            runOnUiThread { if (connectivity.activeNetwork == null) model.markOffline() else model.refresh() }
        }
    }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            val state by model.state.collectAsStateWithLifecycle()
            ActualExpensesTheme { ExpensesScreen(state, model) }
        }
    }
    override fun onStart() {
        super.onStart()
        if (!registered) runCatching { connectivity.registerDefaultNetworkCallback(callback); registered = true }
        model.refresh()
    }
    override fun onStop() {
        if (registered) { connectivity.unregisterNetworkCallback(callback); registered = false }
        super.onStop()
    }
}

@androidx.compose.runtime.Composable
fun ActualExpensesTheme(content: @androidx.compose.runtime.Composable () -> Unit) {
    val colors = if (isSystemInDarkTheme()) darkColorScheme(
        primary = Color(0xFFA2D4BC), onPrimary = Color(0xFF073828),
        primaryContainer = Color(0xFF245747), onPrimaryContainer = Color(0xFFBEEFD7),
    ) else lightColorScheme(
        primary = Color(0xFF245747), onPrimary = Color.White,
        primaryContainer = Color(0xFFBEEFD7), onPrimaryContainer = Color(0xFF073828),
    )
    MaterialTheme(colorScheme = colors, content = content)
}

package dev.aperte.todav

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.aperte.todav.ui.TodavTheme
import dev.aperte.todav.ui.TodavUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val model: Main by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // The activity has the app's own language when one is set in the system settings.
        I18n.load(this)
        enableEdgeToEdge()
        setContent { TodavTheme { TodavUi(model) } }

        // In front: sync now, then every minute (the service's safety net for lost pushes).
        val core = (application as TodavApp).core
        lifecycleScope.launch(Dispatchers.IO) {
            repeatOnLifecycle(Lifecycle.State.RESUMED) {
                core.syncNow()
                while (true) {
                    delay(60_000)
                    core.tick(true)
                }
            }
        }
    }
}

package dev.aperte.todav

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.aperte.todav.ui.TodavTheme
import dev.aperte.todav.ui.TodavUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val model: Main by viewModels()
    private val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // The activity has the app's own language when one is set in the system settings.
        I18n.load(this)
        hours24 = android.text.format.DateFormat.is24HourFormat(this)
        enableEdgeToEdge()
        val app = application as TodavApp
        setContent {
            val changes by app.changes.collectAsStateWithLifecycle()
            // Light, dark or the system's, as the device-only `color_scheme` says.
            val scheme = androidx.compose.runtime.remember(changes) { app.core.setting("color_scheme") }
            TodavTheme(scheme) { TodavUi(model) }
        }
        if (savedInstanceState == null) handle(intent)
        // The launcher icon's "New Task", in the app's language (a resource label could not use gettext).
        androidx.core.content.pm.ShortcutManagerCompat.setDynamicShortcuts(
            this,
            listOf(
                androidx.core.content.pm.ShortcutInfoCompat.Builder(this, "new_task")
                    .setShortLabel(tr("New Task"))
                    .setIcon(androidx.core.graphics.drawable.IconCompat.createWithResource(this, R.drawable.ic_shortcut_add))
                    .setIntent(Intent(this, MainActivity::class.java).setAction(NEW_TASK))
                    .build(),
            ),
        )

        // In front: sync now, then every minute (the service's safety net for lost pushes).
        lifecycleScope.launch(Dispatchers.IO) {
            repeatOnLifecycle(Lifecycle.State.RESUMED) {
                app.awaitReady()
                app.core.syncNow()
                while (true) {
                    delay(60_000)
                    app.core.tick(true)
                }
            }
        }
        // Reminders need notifications: asked once there is an account (Android 13 and later).
        lifecycleScope.launch {
            app.awaitReady()
            model.screen.collect { s ->
                if (s != null && s.accounts.isNotEmpty()) askNotifications()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    /** A notification's task or day, a shared text, the "New task" shortcut. */
    private fun handle(intent: Intent?) {
        intent ?: return
        intent.getStringExtra("list")?.let {
            model.select(it)
            model.show(View.TASKS)
        }
        intent.getStringExtra("task")?.let { model.open(Dest.Task(it)) }
        if (intent.hasExtra("day")) model.showDay(intent.getLongExtra("day", 0))
        when (intent.action) {
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.let { model.sharedText.value = it }
            NEW_TASK -> {
                model.show(View.TASKS)
                model.sharedText.value = ""
            }
        }
    }

    private var asked = false

    private fun askNotifications() {
        if (asked || Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        asked = true
        val prefs = getSharedPreferences("ui", MODE_PRIVATE)
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED ||
            prefs.getBoolean("asked_notifications", false)
        ) {
            return
        }
        prefs.edit().putBoolean("asked_notifications", true).apply()
        notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    companion object {
        const val NEW_TASK = "dev.aperte.todav.NEW_TASK"
    }
}

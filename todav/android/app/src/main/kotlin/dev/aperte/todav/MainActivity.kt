package dev.aperte.todav

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.KeyboardShortcutGroup
import android.view.KeyboardShortcutInfo
import android.view.Menu
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
import dev.aperte.todav.ui.newEvent
import dev.aperte.todav.ui.newTask
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
        // A widget's month: shown in the month view, for now (the view picked stays saved).
        if (intent.getStringExtra("view") == "month") model.calMode.value = CalMode.MONTH
        // A widget's event: this occurrence in its editor.
        intent.getStringExtra("event")?.let { uid ->
            model.open(
                Dest.EventEdit(
                    intent.getStringExtra("event_list"), uid,
                    intent.getLongExtra("event_rid", 0).takeIf { intent.hasExtra("event_rid") },
                    intent.getLongExtra("event_start", 0), intent.getBooleanExtra("all_day", false),
                ),
            )
        }
        when (intent.action) {
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.let { model.sharedText.value = it }
            NEW_TASK -> {
                model.show(View.TASKS)
                model.sharedText.value = ""
            }
            NEW_EVENT -> newEvent(
                model,
                intent.getLongExtra("day", 0).takeIf { intent.hasExtra("day") },
                intent.getStringExtra("calendar"),
                intent.getLongExtra("account", -1).takeIf { it >= 0 },
            )
        }
    }

    /**
     * The Linux app's keyboard shortcuts, for a hardware keyboard (tablets, foldables, ChromeOS):
     * search, undo, sync, new task or event, the views; Ctrl+, settings.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN && (event.isCtrlPressed || event.keyCode == KeyEvent.KEYCODE_F5)) {
            val calendar = model.view.value == View.CALENDAR
            val handled = when (event.keyCode) {
                KeyEvent.KEYCODE_F -> model.open(Dest.Search).let { true }
                KeyEvent.KEYCODE_Z -> model.undo(null).let { true }
                KeyEvent.KEYCODE_R, KeyEvent.KEYCODE_F5 -> model.sync().let { true }
                KeyEvent.KEYCODE_COMMA -> model.open(Dest.Settings).let { true }
                KeyEvent.KEYCODE_N -> {
                    if (calendar) newEvent(model) else newTask(model)
                    true
                }
                KeyEvent.KEYCODE_T -> {
                    model.pick(dev.aperte.todav.core.midnight(model.core.localNow()))
                    true
                }
                in KeyEvent.KEYCODE_1..KeyEvent.KEYCODE_6 -> {
                    model.setMode(CalMode.entries[event.keyCode - KeyEvent.KEYCODE_1])
                    model.show(View.CALENDAR)
                    true
                }
                else -> false
            }
            if (handled) return true
        }
        return super.dispatchKeyEvent(event)
    }

    override fun onProvideKeyboardShortcuts(data: MutableList<KeyboardShortcutGroup>, menu: Menu?, deviceId: Int) {
        val ctrl = KeyEvent.META_CTRL_ON
        val info = { label: String, key: Int -> KeyboardShortcutInfo(label, key, ctrl) }
        data += KeyboardShortcutGroup(
            "Todav",
            listOf(
                info(tr("Search"), KeyEvent.KEYCODE_F),
                info(tr("Undo"), KeyEvent.KEYCODE_Z),
                info(tr("Sync"), KeyEvent.KEYCODE_R),
                info(tr("New Task"), KeyEvent.KEYCODE_N),
                info(tr("Today"), KeyEvent.KEYCODE_T),
                info(tr("Month"), KeyEvent.KEYCODE_1),
                info(tr("Week"), KeyEvent.KEYCODE_2),
                info(tr("Agenda"), KeyEvent.KEYCODE_3),
                info(tr("Work Week"), KeyEvent.KEYCODE_4),
                info(tr("3 Days"), KeyEvent.KEYCODE_5),
                info(tr("Day"), KeyEvent.KEYCODE_6),
                info(tr("Settings"), KeyEvent.KEYCODE_COMMA),
            ),
        )
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
        const val NEW_EVENT = "dev.aperte.todav.NEW_EVENT"
    }
}

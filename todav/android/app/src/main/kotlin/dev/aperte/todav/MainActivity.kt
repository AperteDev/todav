package dev.aperte.todav

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.CalendarContract
import dev.aperte.todav.core.LinkTarget
import dev.aperte.todav.core.midnight
import dev.aperte.todav.core.parseLink
import java.time.Instant
import java.time.ZoneId
import android.view.KeyEvent
import android.view.KeyboardShortcutGroup
import android.view.KeyboardShortcutInfo
import android.view.Menu
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
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
        app.failed?.let { e ->
            setContent { TodavTheme(null) { OpenFailed(e) } }
            return
        }
        setContent {
            val changes by app.changes.collectAsStateWithLifecycle()
            // Light, dark or the system's, as the device-only `color_scheme` says.
            val scheme = androidx.compose.runtime.remember(changes) { app.core.setting("color_scheme") }
            TodavTheme(scheme) { TodavUi(model) }
        }
        if (savedInstanceState == null) handle(intent)
        // The launcher icon's "New Task" and "New Event", in the app's language (a resource label
        // could not use gettext).
        val shortcut = { id: String, label: String, icon: Int, action: String ->
            androidx.core.content.pm.ShortcutInfoCompat.Builder(this, id)
                .setShortLabel(label)
                .setIcon(androidx.core.graphics.drawable.IconCompat.createWithResource(this, icon))
                .setIntent(Intent(this, MainActivity::class.java).setAction(action))
                .build()
        }
        androidx.core.content.pm.ShortcutManagerCompat.setDynamicShortcuts(
            this,
            listOf(
                shortcut("new_task", tr("New Task"), R.drawable.ic_shortcut_add, NEW_TASK),
                shortcut("new_event", tr("New Event"), R.drawable.ic_shortcut_event, NEW_EVENT),
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
        if ((application as TodavApp).failed == null) handle(intent)
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
            Intent.ACTION_SEND -> if (intent.type == "text/calendar") {
                androidx.core.content.IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, android.net.Uri::class.java)
                    ?.let(::import)
            } else {
                intent.getStringExtra(Intent.EXTRA_TEXT)?.let { model.sharedText.value = it }
            }
            Intent.ACTION_VIEW -> view(intent)
            Intent.ACTION_INSERT -> insertEvent(intent)
            NEW_TASK -> {
                model.show(View.TASKS)
                model.sharedText.value = ""
            }
            NEW_EVENT -> newEvent(
                model.also { it.show(View.CALENDAR) },
                intent.getLongExtra("day", 0).takeIf { intent.hasExtra("day") },
                intent.getStringExtra("calendar"),
                intent.getLongExtra("account", -1).takeIf { it >= 0 },
            )
        }
    }

    /**
     * Something to show, as the system's calendar app: a `todav://` link (its task, or its
     * event's day), a time or an event of the system's calendar store (their day: clocks, At a
     * Glance), an iCalendar file (to import).
     */
    private fun view(intent: Intent) {
        val uri = intent.data ?: return
        val calendar = uri.authority == CalendarContract.AUTHORITY
        val core = (application as TodavApp).core
        val notFound = { model.messages.tryEmit(tr("Not found: it may have been deleted") to null) }
        when {
            uri.scheme == "todav" -> when (val link = parseLink(uri.toString())) {
                is LinkTarget.Task -> if (core.task(link.uid) != null) model.open(Dest.Task(link.uid)) else notFound()
                is LinkTarget.Event -> core.event(link.uid)?.let { model.showDay(it.start) } ?: notFound()
                null -> {}
            }
            calendar && uri.pathSegments.firstOrNull() == "time" ->
                uri.lastPathSegment?.toLongOrNull()?.let { model.showDay(wall(it)) }
            calendar && uri.pathSegments.firstOrNull() == "events" -> {
                // A system calendar's event: its day (Todav shows the calendars turned on).
                val allDay = intent.getBooleanExtra(CalendarContract.EXTRA_EVENT_ALL_DAY, false)
                val begin = intent.getLongExtra(CalendarContract.EXTRA_EVENT_BEGIN_TIME, -1).takeIf { it >= 0 }
                    ?: SystemCalendars.start(this, uri)
                model.showDay(begin?.let { wall(it, allDay) } ?: core.localNow())
            }
            uri.scheme == "content" || uri.scheme == "file" -> import(uri)
        }
    }

    /** Another app's "Add to calendar": a new event with what it gave (title, time, place, notes). */
    private fun insertEvent(intent: Intent) {
        val allDay = intent.getBooleanExtra(CalendarContract.EXTRA_EVENT_ALL_DAY, false)
        val time = { key: String -> intent.getLongExtra(key, -1).takeIf { it >= 0 }?.let { wall(it, allDay) } }
        val begin = time(CalendarContract.EXTRA_EVENT_BEGIN_TIME)
        model.showDay(begin ?: (application as TodavApp).core.localNow())
        newEvent(model, begin?.let(::midnight))
        val new = model.newEvent.value ?: return
        model.newEvent.value = new.copy(
            start = begin?.takeIf { allDay || it != midnight(it) } ?: new.start,
            allDay = allDay,
            end = time(CalendarContract.EXTRA_EVENT_END_TIME),
            title = intent.getStringExtra(CalendarContract.Events.TITLE).orEmpty(),
            location = intent.getStringExtra(CalendarContract.Events.EVENT_LOCATION).orEmpty(),
            notes = intent.getStringExtra(CalendarContract.Events.DESCRIPTION).orEmpty(),
        )
    }

    /** Read iCalendar file `uri`, then ask where its events and tasks go (`ImportDialog`). */
    private fun import(uri: android.net.Uri) {
        lifecycleScope.launch(Dispatchers.IO) {
            try {
                val name = contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)
                    ?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
                    ?: uri.lastPathSegment.orEmpty()
                val text = contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString() } ?: return@launch
                model.importing.value = name to text
            } catch (e: Exception) {
                model.messages.tryEmit(
                    tr("Cannot open {name}: {error}").replace("{name}", uri.lastPathSegment.orEmpty())
                        .replace("{error}", e.message.orEmpty()) to null,
                )
            }
        }
    }

    /**
     * The Linux app's keyboard shortcuts, for a hardware keyboard (tablets, foldables, ChromeOS):
     * search, undo, sync, new task or event, the views; Ctrl+, settings.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if ((application as TodavApp).failed == null && event.action == KeyEvent.ACTION_DOWN &&
            (event.isCtrlPressed || event.keyCode == KeyEvent.KEYCODE_F5)
        ) {
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

/**
 * Android's time `ms` (UTC milliseconds) as the core's: local wall-clock seconds. An all-day
 * time is its day's midnight UTC, which is that day already.
 */
private fun wall(ms: Long, allDay: Boolean = false): Long {
    val s = Math.floorDiv(ms, 1000L)
    return if (allDay) s else s + ZoneId.systemDefault().rules.getOffset(Instant.ofEpochMilli(ms)).totalSeconds
}

/** The data could not be opened (left by a newer Todav, say): only why. */
@Composable
private fun OpenFailed(e: Throwable) {
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.fillMaxSize().safeDrawingPadding().padding(32.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(tr("Todav Cannot Open Its Data"), style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
            Text(reason(e), color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
        }
    }
}

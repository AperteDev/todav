package dev.aperte.todav.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import androidx.compose.material.icons.outlined.Check
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import dev.aperte.todav.SystemCalendars
import android.net.Uri
import android.os.Build
import android.provider.Settings as SystemSettings
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.add
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.Login
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.material.icons.outlined.AccountCircle
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Badge
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.FavoriteBorder
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.LocalCafe
import androidx.compose.material.icons.outlined.VolunteerActivism
import androidx.compose.material.icons.outlined.Widgets
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.PhoneAndroid
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeFlexibleTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Dest
import dev.aperte.todav.Main
import dev.aperte.todav.SignIn
import dev.aperte.todav.WidgetKind
import dev.aperte.todav.WidgetsSettings
import dev.aperte.todav.accountName
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.dayLabel
import dev.aperte.todav.hours24
import dev.aperte.todav.ptr
import dev.aperte.todav.reason
import dev.aperte.todav.reminderPresets
import dev.aperte.todav.time
import dev.aperte.todav.tr
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The settings' pages, as the Linux app's Preferences (the extension's become the widget's). */
enum class Page { GENERAL, ACCOUNTS, TASKS, CALENDAR, WIDGETS, ABOUT }

/** The minutes a duration setting may take (`todav_app::prefs::DURATIONS`). */
private val DURATIONS = listOf(15L, 30, 45, 60, 90, 120, 180)

private fun durationNames() = listOf(
    tr("15 minutes"), tr("30 minutes"), tr("45 minutes"), tr("1 hour"), tr("1.5 hours"), tr("2 hours"), tr("3 hours"),
)

fun sortNames() = listOf(
    tr("Manual"), tr("Newest first"), tr("Oldest first"), tr("Due date, soonest first"),
    tr("Due date, latest first"), tr("Priority, highest first"),
)

/** The keys of `sortNames`, as `todav_app::prefs::SORTS`. */
val SORTS = listOf("manual", "newest", "oldest", "due_soonest", "due_latest", "priority")

@Composable
private fun Frame(model: Main, title: String, content: @Composable () -> Unit) = SettingsFrame(title, model::back, content)

/** A settings page: its title in a large bar with a back button, its sections under it, centred on wide screens. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsFrame(title: String, onBack: () -> Unit, content: @Composable () -> Unit) {
    val bar = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
    BoxWithConstraints {
        // The bar's back button and title over the content, centred on wide screens; its colour
        // still across the whole width.
        val side = ((maxWidth - SETTINGS_WIDTH) / 2).coerceAtLeast(0.dp)
        Scaffold(
            modifier = Modifier.nestedScroll(bar.nestedScrollConnection),
            topBar = {
                LargeFlexibleTopAppBar(
                    title = { Text(title) },
                    navigationIcon = {
                        Tip(tr("Back")) {
                            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, tr("Back")) }
                        }
                    },
                    windowInsets = TopAppBarDefaults.windowInsets.add(WindowInsets(left = side, right = side)),
                    scrollBehavior = bar,
                )
            },
        ) { padding ->
            Column(
                Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Column(Modifier.widthIn(max = SETTINGS_WIDTH).fillMaxWidth().padding(bottom = 24.dp)) { content() }
            }
        }
    }
}

/** The widest the settings' rows get, centred. */
private val SETTINGS_WIDTH = 720.dp

/** The settings' first page: its pages, as Android's own settings list them. */
@Composable
fun SettingsPage(model: Main) {
    Frame(model, tr("Settings")) {
        // Widgets only while some are placed.
        val context = LocalContext.current
        val widgets = remember { WidgetKind.placed(context).isNotEmpty() }
        val pages = listOfNotNull(
            Triple(Page.GENERAL, tr("General"), Icons.Outlined.Palette),
            Triple(Page.ACCOUNTS, tr("Accounts"), Icons.Outlined.AccountCircle),
            Triple(Page.TASKS, tr("Tasks"), Icons.Outlined.TaskAlt),
            Triple(Page.CALENDAR, tr("Calendar"), Icons.Outlined.CalendarMonth),
            Triple(Page.WIDGETS, tr("Widgets"), Icons.Outlined.Widgets).takeIf { widgets },
            Triple(Page.ABOUT, tr("About Todav"), Icons.Outlined.Info),
        )
        Section(null) {
            pages.forEach { (page, title, icon) ->
                Item(
                    headlineContent = { Text(title) },
                    leadingContent = { Icon(icon, null) },
                    modifier = Modifier.clickable { model.open(Dest.SettingsPage(page)) },
                )
            }
        }
    }
}

@Composable
fun SettingsSubpage(model: Main, page: Page) {
    when (page) {
        Page.GENERAL -> Frame(model, tr("General")) { General(model) }
        Page.ACCOUNTS -> Frame(model, tr("Accounts")) { Accounts(model) }
        Page.TASKS -> Frame(model, tr("Tasks")) { TaskSettings(model) }
        Page.CALENDAR -> Frame(model, tr("Calendar")) { CalendarSettings(model) }
        Page.WIDGETS -> Frame(model, tr("Widgets")) { WidgetsSettings(model) }
        Page.ABOUT -> Frame(model, tr("About Todav")) { About(model) }
    }
}

// --- rows ---------------------------------------------------------------------------------------

/** A setting kept in the core (synced unless device-only), read again on every change. */
@Composable
fun setting(model: Main, key: String): String? {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    return load(changes, key, initial = model.core.setting(key)) { model.core.setting(key) }
}

/** A choice among `names` (stored as `values`, the first the default) in a dialog of radio buttons. */
@Composable
private fun ChoiceRow(
    model: Main,
    title: String,
    key: String,
    values: List<String>,
    names: List<String>,
    subtitle: String? = null,
    enabled: Boolean = true,
    shown: Int? = null,
    onPick: ((Int) -> Unit)? = null,
) {
    val value = setting(model, key)
    val current = shown ?: values.indexOf(value).coerceAtLeast(0)
    PickRow(title, names, current, subtitle, enabled) { i -> if (onPick != null) onPick(i) else model.setSetting(key, values[i]) }
}

/** A row naming the one of `names` picked (`current`), which a dialog of radio buttons changes. */
@Composable
fun PickRow(
    title: String,
    names: List<String>,
    current: Int,
    subtitle: String? = null,
    enabled: Boolean = true,
    onPick: (Int) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Item(
        headlineContent = { Text(title) },
        supportingContent = {
            Column {
                Text(names.getOrElse(current) { "" })
                subtitle?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            }
        },
        modifier = Modifier.clickable(enabled = enabled) { open = true },
    )
    if (open) {
        Choose(title, names, current, onDismiss = { open = false }) { i ->
            open = false
            onPick(i)
        }
    }
}

@Composable
fun Choose(title: String, names: List<String>, current: Int, onDismiss: () -> Unit, onPick: (Int) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                names.forEachIndexed { i, n ->
                    Row(
                        Modifier.fillMaxWidth().selectable(i == current, role = Role.RadioButton) { onPick(i) }
                            .padding(vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(i == current, null)
                        Text(n, Modifier.padding(start = 16.dp))
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

/** An on/off setting, on unless it says "false" (or off unless "true", with `default` false). */
@Composable
private fun SwitchRow(model: Main, title: String, key: String, subtitle: String? = null, default: Boolean = true) {
    val value = setting(model, key)
    val on = if (default) value != "false" else value == "true"
    Item(
        headlineContent = { Text(title) },
        supportingContent = subtitle?.let { { Text(it) } },
        trailingContent = { Switch(on, { model.setSetting(key, it.toString()) }) },
        modifier = Modifier.clickable { model.setSetting(key, (!on).toString()) },
    )
}

@Composable
private fun DurationRow(model: Main, title: String, subtitle: String, key: String) {
    val v = setting(model, key)?.toLongOrNull() ?: 60
    ChoiceRow(
        model, title, key, DURATIONS.map { "$it" }, durationNames(), subtitle,
        shown = DURATIONS.indexOf(v).takeIf { it >= 0 } ?: 3,
    )
}

// --- pages ------------------------------------------------------------------------------------

@Composable
private fun General(model: Main) {
    val context = LocalContext.current
    Section(tr("Appearance")) {
        ChoiceRow(
            model, tr("Style"), "color_scheme", listOf("", "light", "dark"),
            listOf(ptr("style", "System"), ptr("style", "Light"), ptr("style", "Dark")),
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            Item(
                headlineContent = { Text(tr("Language")) },
                supportingContent = { Text(tr("Todav’s language in the system settings")) },
                modifier = Modifier.clickable {
                    context.startActivity(
                        Intent(SystemSettings.ACTION_APP_LOCALE_SETTINGS, Uri.fromParts("package", context.packageName, null)),
                    )
                },
            )
        }
    }
    Section(tr("Startup")) {
        ChoiceRow(
            model, tr("Open at start"), "start_view", listOf("last", "tasks", "calendar"),
            listOf(tr("Last used"), tr("Tasks"), tr("Calendar")),
        )
    }
    Section(tr("Default App")) { DefaultCalendarRow(model) }
    Section(tr("Notifications")) {
        Item(
            headlineContent = { Text(tr("Notifications")) },
            supportingContent = { Text(tr("Reminder notifications, in the system settings")) },
            leadingContent = { Icon(Icons.Outlined.Notifications, null) },
            modifier = Modifier.clickable {
                context.startActivity(
                    Intent(SystemSettings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(SystemSettings.EXTRA_APP_PACKAGE, context.packageName),
                )
            },
        )
    }
}

/** What launchers and clocks open as the calendar app. */
private val calendarApp = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_APP_CALENDAR)

/** The default calendar app, or null when the system asks each time. */
private fun defaultCalendar(context: Context): ActivityInfo? {
    val pm = context.packageManager
    val chosen = pm.resolveActivity(calendarApp, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo ?: return null
    // Without a default, the system's chooser answers, which is none of the calendar apps.
    return chosen.takeIf { c -> pm.queryIntentActivities(calendarApp, 0).any { it.activityInfo.packageName == c.packageName } }
}

/**
 * Default App → Calendar: the app named, a check when it is Todav, else a button doing what
 * Android lets an app do (no app may make itself the default): with none yet, the system asks
 * which (Todav, then Always); with another app's, that app's Open by default settings, to clear
 * its defaults, and back from there, the system asks.
 */
@Composable
private fun DefaultCalendarRow(model: Main) {
    val context = LocalContext.current
    var current by remember { mutableStateOf(defaultCalendar(context)) }
    // Sent to clear another app's defaults.
    var clearing by remember { mutableStateOf(false) }
    // Read again on coming back from the system's question or settings.
    LifecycleResumeEffect(Unit) {
        current = defaultCalendar(context)
        if (clearing && current == null) context.startActivity(calendarApp)
        clearing = false
        onPauseOrDispose {}
    }
    val ours = current?.packageName == context.packageName
    Item(
        headlineContent = { Text(tr("Calendar")) },
        supportingContent = { Text(current?.loadLabel(context.packageManager)?.toString() ?: tr("None")) },
        trailingContent = {
            if (ours) {
                Icon(Icons.Outlined.Check, null)
            } else {
                TextButton(onClick = {
                    val other = current
                    if (other == null) {
                        context.startActivity(calendarApp)
                    } else {
                        val page = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                            SystemSettings.ACTION_APP_OPEN_BY_DEFAULT_SETTINGS
                        } else {
                            SystemSettings.ACTION_APPLICATION_DETAILS_SETTINGS
                        }
                        model.messages.tryEmit(tr("Clear the default preferences there, then come back") to null)
                        clearing = true
                        context.startActivity(Intent(page, Uri.fromParts("package", other.packageName, null)))
                    }
                }) { Text(tr("Make Default")) }
            }
        },
    )
}

@Composable
private fun Accounts(model: Main) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val accounts = load(changes, initial = model.core.accounts()) { model.core.accounts() }
    // The order everything lists accounts in: dragged by the handles, as in the Linux app.
    var order by remember(accounts) { mutableStateOf(accounts) }
    // The handles' gestures outlive recompositions: they read the order through this.
    val current by androidx.compose.runtime.rememberUpdatedState(order)
    var held by remember { mutableStateOf<Long?>(null) }
    var shift by remember { mutableStateOf(0f) }
    Section(tr("Accounts")) {
        // Keyed by account, so a row keeps its handle's gesture while it moves.
        order.forEach { a -> androidx.compose.runtime.key(a.id) {
            Item(
                headlineContent = { Text(accountName(a)) },
                supportingContent = { Text(accountKind(model, a)) },
                leadingContent = {
                    Icon(
                        when (a.kind) {
                            AccountKind.LOCAL -> Icons.Outlined.PhoneAndroid
                            AccountKind.SYSTEM -> Icons.Outlined.CalendarMonth
                            else -> Icons.Outlined.AccountCircle
                        },
                        null,
                    )
                },
                trailingContent = if (order.size > 1) {
                    {
                        Icon(
                            Icons.Default.DragHandle,
                            tr("Drag to Move"),
                            Modifier.pointerInput(a.id) {
                                detectVerticalDragGestures(
                                    onDragStart = {
                                        held = a.id
                                        shift = 0f
                                    },
                                    onVerticalDrag = { change, dy ->
                                        change.consume()
                                        shift += dy
                                        // Past half a row (with the gap below it): one place up or down.
                                        val row = 74.dp.toPx()
                                        val i = current.indexOfFirst { it.id == a.id }
                                        val to = (i + Math.round(shift / row)).coerceIn(0, current.lastIndex)
                                        if (to != i) {
                                            order = current.toMutableList().apply { add(to, removeAt(i)) }
                                            shift -= (to - i) * row
                                        }
                                    },
                                    onDragEnd = {
                                        held = null
                                        val ids = current.map { it.id }
                                        model.act {
                                            model.core.reorderAccounts(ids)
                                            model.todav.changed()
                                        }
                                    },
                                    onDragCancel = {
                                        held = null
                                        order = accounts
                                    },
                                )
                            },
                        )
                    }
                } else {
                    null
                },
                containerColor = if (held == a.id) {
                    MaterialTheme.colorScheme.surfaceContainerHighest
                } else {
                    MaterialTheme.colorScheme.surfaceContainer
                },
                modifier = Modifier.clickable { model.open(Dest.Account(a.id)) },
            )
        } }
        Item(
            headlineContent = { Text(tr("Add Account")) },
            leadingContent = { Icon(Icons.Outlined.Add, null) },
            modifier = Modifier.clickable { model.open(Dest.AddAccount) },
        )
    }
    val servers = accounts.filter { it.kind == AccountKind.NEXTCLOUD }
    if (servers.isNotEmpty()) {
        Section(tr("Sync")) {
            val current = model.core.settingsAccount()
            ChoiceRow(
                model, tr("Sync settings with"), "settings_account",
                listOf("") + servers.map { "${it.id}" },
                listOf(ptr("settings sync", "Off")) + servers.map { accountName(it) },
                subtitle = tr("Accounts and servers always stay on this device"),
                shown = servers.indexOfFirst { it.id == current } + 1,
            ) { i -> model.act { model.core.setSettingsAccount(servers.getOrNull(i - 1)?.id) } }
        }
        Section(tr("Push")) { PushRow(model) }
    }
}

/** How an account connects, under its name. */
fun accountKind(model: Main, a: dev.aperte.todav.core.Account): String = when {
    !a.enabled -> ptr("account", "Off")
    a.kind == AccountKind.LOCAL -> tr("Lists and calendars that stay on this device")
    a.kind == AccountKind.SYSTEM -> tr("Calendars of the accounts on this device")
    !model.core.connected(a.id) -> tr("Not connected")
    else -> tr("Nextcloud server")
}

/** The UnifiedPush distributor pushes come through, or how to get one. */
@Composable
private fun PushRow(model: Main) {
    val context = LocalContext.current
    val distributor = remember { model.todav.push.distributor() }
    Item(
        headlineContent = { Text(tr("Push")) },
        supportingContent = {
            Text(
                distributor?.let { tr("Through {app}: changes made elsewhere show up within seconds").replace("{app}", it) }
                    ?: tr("Install a UnifiedPush distributor such as ntfy to see changes made elsewhere within seconds; until then they show up at the next check"),
            )
        },
        modifier = Modifier.clickable(enabled = distributor == null) {
            context.startActivity(Intent(Intent.ACTION_VIEW, "https://unifiedpush.org/users/distributors/".toUri()))
        },
    )
}

@Composable
private fun TaskSettings(model: Main) {
    Section(tr("Tasks")) {
        OrderRows(model, "task", tr("Sort at start"), tr("Add new tasks"))
        ChoiceRow(
            model, tr("Completed tasks"), "done_tasks", listOf("group", "in_place", "last"),
            listOf(tr("In a separate group"), tr("Keep in place"), tr("Move to the end")),
        )
        DurationRow(model, tr("Default duration"), tr("When a task gets a time, or is planned on the calendar"), "task_duration")
    }
    Section(tr("Stars and Pins")) {
        SwitchRow(model, tr("Stars"), "show_stars", tr("A star with each task’s priority, and the Important page"))
        SwitchRow(model, tr("Pins"), "show_pins", tr("Pin tasks to the top of their list"))
    }
    Section(tr("Swipe Actions")) {
        // As `todav_app::prefs::SWIPES`.
        val values = listOf("off", "complete", "star", "pin", "delete")
        val names = listOf(ptr("swipe", "Off"), tr("Complete"), tr("Star"), tr("Pin"), tr("Delete"))
        ChoiceRow(model, tr("Swipe right"), "swipe_right", values, names)
        ChoiceRow(model, tr("Swipe left"), "swipe_left", values, names)
    }
    Section(tr("Subtasks")) {
        OrderRows(model, "sub", tr("Sort"), tr("Add new subtasks"))
        ChoiceRow(
            model, tr("Completed subtasks"), "done_subtasks", listOf("show", "hide", "last"),
            listOf(tr("Show"), tr("Hide"), tr("Move to the end")),
        )
        ChoiceRow(
            model, tr("Completing a task with open subtasks"), "complete_subtasks", listOf("complete", "keep", "ask"),
            listOf(tr("Completes them too"), tr("Leaves them open"), tr("Asks each time")),
        )
    }
    Section(tr("Reminders")) { ReminderRows(model) }
    Section(tr("Categories")) {
        ChoiceRow(
            model, tr("Show categories"), "category_view", listOf("group", "tags", "none"),
            listOf(tr("Group by category"), tr("As tags"), tr("Hide")),
        )
    }
}

/**
 * A "Sort" row and its "Add new …" row: sorting by creation date sets (and locks) where new
 * items go; manual and due date sorting let the user pick. Tasks pick their sort at start
 * (`start_task_sort`), the last used first, which the task page's sort menu sets; picking one
 * sorts by it now too.
 */
@Composable
private fun OrderRows(model: Main, which: String, sortTitle: String, newTitle: String) {
    val sort = setting(model, "${which}_sort")
    val index = SORTS.indexOf(sort).coerceAtLeast(0)
    if (which == "task") {
        ChoiceRow(model, sortTitle, "start_task_sort", listOf("last") + SORTS, listOf(tr("Last used")) + sortNames()) { i ->
            model.setSetting("start_task_sort", (listOf("last") + SORTS)[i])
            if (i > 0) model.setSetting("task_sort", SORTS[i - 1])
        }
    } else {
        ChoiceRow(model, sortTitle, "${which}_sort", SORTS, sortNames())
    }
    val manual = index != 1 && index != 2
    val start = when (index) {
        1 -> true
        2 -> false
        else -> setting(model, "${which}_new") == "start"
    }
    ChoiceRow(
        model, newTitle, "${which}_new", listOf("start", "end"), listOf(tr("At the start"), tr("At the end")),
        subtitle = if (manual) null else tr("Set by the sort order"),
        enabled = manual,
        shown = if (start) 0 else 1,
    )
}

/** The default for new reminders and when reminders of all-day tasks go off. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReminderRows(model: Main) {
    val core = model.core
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val allDayTime = load(changes, initial = core.allDayTime()) { core.allDayTime() }
    val presets = reminderPresets(core.reminderPresets(false), false, false, allDayTime)
    val current = load(changes, initial = core.defaultReminder(false)) { core.defaultReminder(false) }
    ChoiceRow(
        model, tr("New reminders"), "reminder_default", presets.map { "${it.second}" }, presets.map { it.first },
        subtitle = tr("For tasks due at a time"),
        shown = presets.indexOfFirst { it.second == current }.coerceAtLeast(0),
    )
    var pick by remember { mutableStateOf(false) }
    Item(
        headlineContent = { Text(tr("Time for all-day tasks")) },
        supportingContent = { Text(tr("When reminders of tasks due on a date go off")) },
        trailingContent = { Text(time(allDayTime)) },
        modifier = Modifier.clickable { pick = true },
    )
    if (pick) {
        TimeDialog(tr("Time for all-day tasks"), allDayTime, onDismiss = { pick = false }) { s ->
            model.setSetting("reminder_time", "${s / 60}")
        }
    }
}

@Composable
private fun CalendarSettings(model: Main) {
    Section(tr("Views")) {
        ChoiceRow(
            model, tr("View at start"), "start_calendar_view",
            listOf("last", "month", "week", "work_week", "three_days", "day", "agenda"),
            listOf(tr("Last used"), tr("Month"), tr("Week"), tr("Work Week"), tr("3 Days"), tr("Day"), tr("Agenda")),
        )
        val days by model.workDays.collectAsStateWithLifecycle()
        var pick by remember { mutableStateOf(false) }
        Item(
            headlineContent = { Text(tr("Work week days")) },
            supportingContent = { Text(workDaysText(days)) },
            modifier = Modifier.clickable { pick = true },
        )
        if (pick) WorkDaysDialog(model) { pick = false }
    }
    Section(tr("Events")) {
        DurationRow(model, tr("Default duration"), tr("When an event is added at a time, or an all-day one gets a time"), "event_duration")
    }
}

@Composable
private fun About(model: Main) {
    val context = LocalContext.current
    val version = remember { context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "" }
    val open = { url: String -> context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
    Section(null) {
        Item(headlineContent = { Text("Todav") }, supportingContent = { Text(tr("Version {version}").replace("{version}", version)) })
        Item(
            headlineContent = { Text(tr("Source Code")) },
            supportingContent = { Text("github.com/AperteDev/todav") },
            modifier = Modifier.clickable { open("https://github.com/AperteDev/todav") },
        )
        Item(
            headlineContent = { Text(tr("Report an Issue")) },
            modifier = Modifier.clickable { open("https://github.com/AperteDev/todav/issues") },
        )
        Item(
            headlineContent = { Text(tr("License")) },
            supportingContent = { Text("GNU General Public License 3.0 or later") },
            modifier = Modifier.clickable { open("https://www.gnu.org/licenses/gpl-3.0.html") },
        )
    }
    // As the Linux app's About → Support (`DONATIONS` in its window.rs).
    Section(tr("Support")) {
        listOf(
            Triple(tr("Support on Ko-fi"), "ko-fi.com/N5E229GJPZ", Icons.Outlined.LocalCafe),
            Triple(tr("Sponsor on GitHub"), "github.com/sponsors/SelfRef", Icons.Outlined.FavoriteBorder),
            Triple(tr("Donate on Liberapay"), "liberapay.com/SelfRef/donate", Icons.Outlined.VolunteerActivism),
        ).forEach { (title, url, icon) ->
            Item(
                headlineContent = { Text(title) },
                supportingContent = { Text(url) },
                leadingContent = { Icon(icon, null) },
                modifier = Modifier.clickable { open("https://$url") },
            )
        }
    }
    Text(
        tr("Todav is developed in my free time. If it keeps your days in order, you can support its development."),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(start = 32.dp, end = 32.dp, top = 12.dp),
    )
}

// --- accounts --------------------------------------------------------------------------------

/** Adding an account from Settings: the welcome page's choice. */
@Composable
fun AddAccountPage(model: Main) {
    Frame(model, tr("Add Account")) {
        Column(Modifier.padding(16.dp), verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(16.dp)) {
            AccountChoice(model, welcome = false)
        }
    }
}

/**
 * An account's page: on or off, its name, the connection (signing in again), the server's status
 * and Remove, as the Linux app's.
 */
@Composable
fun AccountPage(model: Main, id: Long) {
    val core = model.core
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val account = load(changes, id, initial = core.account(id)) { core.account(id) }
    if (account == null) {
        LaunchedEffect(Unit) { model.back() }
        return
    }
    var name by rememberSaveable { mutableStateOf(account.name) }
    var removing by remember { mutableStateOf<String?>(null) }
    var confirm by remember { mutableStateOf(false) }
    var signingIn by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    Frame(model, accountName(account)) {
        Section(null) {
            Item(
                headlineContent = { Text(tr("Use This Account")) },
                supportingContent = { Text(tr("Off hides its lists and calendars and stops syncing them; nothing is deleted")) },
                trailingContent = {
                    Switch(account.enabled, { on ->
                        model.act {
                            core.setAccountEnabled(id, on)
                            model.todav.changed()
                            core.syncNow()
                        }
                    })
                },
            )
            FieldItem(
                name,
                { name = it },
                tr("Name"),
                icon = Icons.Outlined.Badge,
                placeholder = accountName(account.copy(name = "")),
                singleLine = true,
            )
        }
        LaunchedEffect(name) {
            if (name != account.name) {
                kotlinx.coroutines.delay(600)
                model.act { core.renameAccount(id, name.trim()) }
            }
        }
        if (account.kind == AccountKind.NEXTCLOUD) {
            val connected = core.connected(id)
            Section(tr("Connection")) {
                Item(
                    headlineContent = {
                        Text(if (connected) tr("Signed in as {name}").replace("{name}", account.user) else tr("Not connected"))
                    },
                    supportingContent = { Text(account.url) },
                )
                if (!signingIn) {
                    Item(
                        headlineContent = { Text(tr("Sign In Again")) },
                        leadingContent = { Icon(Icons.AutoMirrored.Outlined.Login, null) },
                        modifier = Modifier.clickable { signingIn = true },
                    )
                }
            }
            if (signingIn) {
                var server by rememberSaveable { mutableStateOf(account.url) }
                Column(Modifier.padding(16.dp), verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(12.dp)) {
                    SignInForm(model, server, { server = it }, account.user) { url, user, password ->
                        if (user != account.user) {
                            model.signIn.value = SignIn.Failed(
                                tr("Signed in as {name}: add that as another account").replace("{name}", user),
                            )
                        } else {
                            try {
                                core.reconnect(model.todav.passwords, id, url, password)
                                model.signIn.value = SignIn.Idle
                                scope.launch { signingIn = false }
                                core.syncNow()
                                model.todav.changed()
                            } catch (e: Exception) {
                                model.signIn.value = SignIn.Failed(reason(e))
                            }
                        }
                    }
                }
            }
            if (connected) ServerStatus(model, id)
        }
        if (account.kind == AccountKind.SYSTEM) SystemCalendarRows(model)
        Section(null) {
            Item(
                headlineContent = { Text(tr("Remove Account"), color = MaterialTheme.colorScheme.error) },
                leadingContent = { Icon(Icons.Outlined.Delete, null, tint = MaterialTheme.colorScheme.error) },
                modifier = Modifier.clickable { confirm = true },
            )
        }
    }
    val title = tr("Remove {name}?").replace("{name}", accountName(account))
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(title) },
            text = {
                Text(
                    when (account.kind) {
                        AccountKind.LOCAL ->
                            tr("Its lists and calendars, with everything in them, are deleted from this device. This cannot be undone.")
                        AccountKind.SYSTEM ->
                            tr("Its calendars leave Todav; they stay on this device, in the accounts they come from.")
                        else ->
                            tr("Its lists and calendars leave this device; they stay on the server. Switch it off instead to only hide them.")
                    },
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirm = false
                    scope.launch {
                        val error = remove(model, id, false)
                        if (error == null) model.back() else removing = error
                    }
                }) { Text(tr("Remove"), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text(tr("Cancel")) } },
        )
    }
    removing?.let { error ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text(tr("Remove Without Uploading?")) },
            text = { Text(tr("{error}. Changes made here that the server does not have yet will be lost.").replace("{error}", error)) },
            confirmButton = {
                TextButton(onClick = {
                    removing = null
                    scope.launch {
                        remove(model, id, true)
                        model.back()
                    }
                }) { Text(tr("Remove Anyway"), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(tr("Cancel")) } },
        )
    }
}

/** Remove account `id` (`force`: without uploading its changes); the reason it could not, or null. */
private suspend fun remove(model: Main, id: Long, force: Boolean): String? = withContext(Dispatchers.IO) {
    try {
        // Added again, it starts with none turned on.
        if (model.core.account(id)?.kind == AccountKind.SYSTEM) model.core.setSetting("system_calendars", "")
        model.core.removeAccount(model.todav.passwords, id, force)
        model.todav.changed()
        null
    } catch (e: Exception) {
        reason(e)
    }
}

/** The system's calendars by account, each turned on (copied into Todav) or off; none at first. */
@Composable
private fun SystemCalendarRows(model: Main) {
    val context = LocalContext.current
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    var allowed by remember { mutableStateOf(SystemCalendars.allowed(context)) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        allowed = granted
        if (granted) {
            model.todav.changed()
            model.todav.systemCalendarsChanged()
        }
    }
    if (!allowed) {
        Section(tr("Calendars")) {
            Item(
                headlineContent = { Text(tr("Allow Access to Calendars")) },
                supportingContent = { Text(tr("To show the calendars of the accounts on this device")) },
                modifier = Modifier.clickable { ask.launch(Manifest.permission.READ_CALENDAR) },
            )
        }
        return
    }
    val calendars = load(changes, initial = null) { SystemCalendars.all(context) } ?: return
    val on = load(changes, initial = SystemCalendars.enabled(model.core)) { SystemCalendars.enabled(model.core) }
    val inTodav = load(changes, initial = emptyMap()) { SystemCalendars.inTodav(context, model.core) }
    if (calendars.isEmpty()) {
        Section(tr("Calendars")) {
            Item(
                headlineContent = { Text(tr("No calendars on this device")) },
                supportingContent = { Text(tr("Accounts added in the system’s settings, such as Google, bring theirs")) },
            )
        }
    }
    calendars.groupBy { it.account }.forEach { (account, cals) ->
        Section(account.ifEmpty { tr("Calendars") }) {
            cals.forEach { c ->
                Item(
                    headlineContent = { Text(c.name) },
                    // Turned on, its events would show twice.
                    supportingContent = inTodav[c.id]?.let { name ->
                        @Composable {
                            Text(
                                tr("Already in Todav as “{name}”").replace("{name}", name),
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    },
                    leadingContent = { Box(Modifier.size(14.dp).background(Color(c.color or 0xFF000000.toInt()), CircleShape)) },
                    trailingContent = {
                        Switch(c.id in on, { v ->
                            model.act {
                                SystemCalendars.setEnabled(model.core, c.id, v)
                                model.todav.changed()
                                model.todav.systemCalendarsChanged()
                            }
                        })
                    },
                )
            }
        }
    }
}

/** The server as `status.php` describes it, the last sync and push, as the Linux app's. */
@Composable
private fun ServerStatus(model: Main, id: Long) {
    val core = model.core
    val sync by model.syncState.collectAsStateWithLifecycle()
    val server = load(id, initial = tr("Checking…")) {
        try {
            val info = core.serverInfo(id)
            val name = "${info.product} ${info.version}"
            if (info.maintenance) tr("{server}, in maintenance mode").replace("{server}", name) else name
        } catch (e: Exception) {
            tr("Cannot reach the server: {error}").replace("{error}", reason(e))
        }
    }
    val now = core.localNow()
    val status = when {
        sync == SyncState.Syncing -> tr("Syncing…")
        else -> core.syncError(id)?.let { tr("Failed: {error}").replace("{error}", it) }
            ?: core.lastSync(id)?.let { t ->
                val local = t + now - System.currentTimeMillis() / 1000
                tr("Last synced: {time}").replace("{time}", dayLabel(local, false, now))
            }
            ?: tr("Not synced yet")
    }
    val push = remember(sync) { core.pushStatus(id) }
    val distributor = remember { model.todav.push.distributor() }
    val text = when {
        push.calendars == 0u -> tr("Checked at the next sync")
        push.supported == 0u -> tr("Not available on this server, so changes made elsewhere show up at the next check. Install the DAV Push app on Nextcloud to see them within seconds")
        distributor == null -> tr("Available on this server. Install a UnifiedPush distributor such as ntfy to see changes made elsewhere within seconds")
        push.registered == 0u -> tr("Registering…")
        else -> tr("On: changes made elsewhere show up within seconds")
    }
    Section(tr("Server Status")) {
        Item(headlineContent = { Text(tr("Server")) }, supportingContent = { Text(server) })
        Item(headlineContent = { Text(tr("Sync")) }, supportingContent = { Text(status) })
        Item(headlineContent = { Text(tr("DAV Push")) }, supportingContent = { Text(text) })
    }
}

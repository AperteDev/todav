package dev.aperte.todav.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings as SystemSettings
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
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
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.PhoneAndroid
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberTimePickerState
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Dest
import dev.aperte.todav.Main
import dev.aperte.todav.SignIn
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
enum class Page { GENERAL, ACCOUNTS, TASKS, CALENDAR, ABOUT }

/** The minutes a duration setting may take (`todav_app::prefs::DURATIONS`). */
private val DURATIONS = listOf(15L, 30, 45, 60, 90, 120, 180)

private fun durationNames() = listOf(
    tr("15 minutes"), tr("30 minutes"), tr("45 minutes"), tr("1 hour"), tr("1.5 hours"), tr("2 hours"), tr("3 hours"),
)

private fun sortNames() = listOf(
    tr("Manual"), tr("Newest first"), tr("Oldest first"), tr("Due date, soonest first"),
    tr("Due date, latest first"), tr("Priority, highest first"),
)

/** The keys of `sortNames`, as `todav_app::prefs::SORTS`. */
private val SORTS = listOf("manual", "newest", "oldest", "due_soonest", "due_latest", "priority")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Frame(model: Main, title: String, content: @Composable () -> Unit) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    IconButton(onClick = model::back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, tr("Back")) }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(Modifier.widthIn(max = 720.dp).fillMaxWidth().padding(bottom = 24.dp)) { content() }
        }
    }
}

/** The settings' first page: its pages, as Android's own settings list them. */
@Composable
fun SettingsPage(model: Main) {
    Frame(model, tr("Settings")) {
        val pages = listOf(
            Triple(Page.GENERAL, tr("General"), Icons.Outlined.Palette),
            Triple(Page.ACCOUNTS, tr("Accounts"), Icons.Outlined.AccountCircle),
            Triple(Page.TASKS, tr("Tasks"), Icons.Outlined.TaskAlt),
            Triple(Page.CALENDAR, tr("Calendar"), Icons.Outlined.CalendarMonth),
            Triple(Page.ABOUT, tr("About Todav"), Icons.Outlined.Info),
        )
        pages.forEach { (page, title, icon) ->
            ListItem(
                headlineContent = { Text(title) },
                leadingContent = { Icon(icon, null) },
                modifier = Modifier.clickable { model.open(Dest.SettingsPage(page)) },
            )
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
        Page.ABOUT -> Frame(model, tr("About Todav")) { About(model) }
    }
}

// --- rows ---------------------------------------------------------------------------------------

@Composable
private fun Group(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 16.dp, top = 24.dp, bottom = 8.dp),
    )
}

/** A setting kept in the core (synced unless device-only), read again on every change. */
@Composable
private fun setting(model: Main, key: String): String? {
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
    var open by remember { mutableStateOf(false) }
    ListItem(
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
            if (onPick != null) onPick(i) else model.act { model.core.setSetting(key, values[i]) }
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
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = subtitle?.let { { Text(it) } },
        trailingContent = { Switch(on, { model.act { model.core.setSetting(key, it.toString()) } }) },
        modifier = Modifier.clickable { model.act { model.core.setSetting(key, (!on).toString()) } },
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
    Group(tr("Appearance"))
    ChoiceRow(
        model, tr("Style"), "color_scheme", listOf("", "light", "dark"),
        listOf(ptr("style", "System"), ptr("style", "Light"), ptr("style", "Dark")),
    )
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        ListItem(
            headlineContent = { Text(tr("Language")) },
            supportingContent = { Text(tr("Todav’s language in the system settings")) },
            modifier = Modifier.clickable {
                context.startActivity(
                    Intent(SystemSettings.ACTION_APP_LOCALE_SETTINGS, Uri.fromParts("package", context.packageName, null)),
                )
            },
        )
    }
    Group(tr("Startup"))
    ChoiceRow(
        model, tr("Open at start"), "start_view", listOf("last", "tasks", "calendar"),
        listOf(tr("Last used"), tr("Tasks"), tr("Calendar")),
    )
    Group(tr("Notifications"))
    ListItem(
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

@Composable
private fun Accounts(model: Main) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val accounts = load(changes, initial = model.core.accounts()) { model.core.accounts() }
    Group(tr("Accounts"))
    // The order everything lists accounts in: dragged by the handles, as in the Linux app.
    var order by remember(accounts) { mutableStateOf(accounts) }
    // The handles' gestures outlive recompositions: they read the order through this.
    val current by androidx.compose.runtime.rememberUpdatedState(order)
    var held by remember { mutableStateOf<Long?>(null) }
    var shift by remember { mutableStateOf(0f) }
    // Keyed by account, so a row keeps its handle's gesture while it moves.
    order.forEach { a -> androidx.compose.runtime.key(a.id) {
        ListItem(
            headlineContent = { Text(accountName(a)) },
            supportingContent = { Text(accountKind(model, a)) },
            leadingContent = {
                Icon(if (a.kind == AccountKind.LOCAL) Icons.Outlined.PhoneAndroid else Icons.Outlined.AccountCircle, null)
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
                                    // Past half a row: one place up or down.
                                    val row = 72.dp.toPx()
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
            tonalElevation = if (held == a.id) 4.dp else 0.dp,
            modifier = Modifier.clickable { model.open(Dest.Account(a.id)) },
        )
    } }
    ListItem(
        headlineContent = { Text(tr("Add Account")) },
        leadingContent = { Icon(Icons.Outlined.Add, null) },
        modifier = Modifier.clickable { model.open(Dest.AddAccount) },
    )
    val servers = accounts.filter { it.kind == AccountKind.NEXTCLOUD }
    if (servers.isNotEmpty()) {
        Group(tr("Sync"))
        val current = model.core.settingsAccount()
        ChoiceRow(
            model, tr("Sync settings with"), "settings_account",
            listOf("") + servers.map { "${it.id}" },
            listOf(ptr("settings sync", "Off")) + servers.map { accountName(it) },
            subtitle = tr("Accounts and servers always stay on this device"),
            shown = servers.indexOfFirst { it.id == current } + 1,
        ) { i -> model.act { model.core.setSettingsAccount(servers.getOrNull(i - 1)?.id) } }
        Group(tr("Push"))
        PushRow(model)
    }
}

/** How an account connects, under its name. */
fun accountKind(model: Main, a: dev.aperte.todav.core.Account): String = when {
    !a.enabled -> ptr("account", "Off")
    a.kind == AccountKind.LOCAL -> tr("Lists and calendars that stay on this device")
    !model.core.connected(a.id) -> tr("Not connected")
    else -> tr("Nextcloud server")
}

/** The UnifiedPush distributor pushes come through, or how to get one. */
@Composable
private fun PushRow(model: Main) {
    val context = LocalContext.current
    val distributor = remember { model.todav.push.distributor() }
    ListItem(
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
    Group(tr("Tasks"))
    OrderRows(model, "task", tr("Sort"), tr("Add new tasks"))
    ChoiceRow(
        model, tr("Completed tasks"), "done_tasks", listOf("group", "in_place", "last"),
        listOf(tr("In a separate group"), tr("Keep in place"), tr("Move to the end")),
    )
    DurationRow(model, tr("Default duration"), tr("When a task gets a time, or is planned on the calendar"), "task_duration")
    Group(tr("Stars and Pins"))
    SwitchRow(model, tr("Stars"), "show_stars", tr("A star with each task’s priority, and the Important page"))
    SwitchRow(model, tr("Pins"), "show_pins", tr("Pin tasks to the top of their list"))
    Group(tr("Subtasks"))
    OrderRows(model, "sub", tr("Sort"), tr("Add new subtasks"))
    ChoiceRow(
        model, tr("Completed subtasks"), "done_subtasks", listOf("show", "hide", "last"),
        listOf(tr("Show"), tr("Hide"), tr("Move to the end")),
    )
    ChoiceRow(
        model, tr("Completing a task with open subtasks"), "complete_subtasks", listOf("complete", "keep", "ask"),
        listOf(tr("Completes them too"), tr("Leaves them open"), tr("Asks each time")),
    )
    Group(tr("Reminders"))
    ReminderRows(model)
    Group(tr("Categories"))
    ChoiceRow(
        model, tr("Show categories"), "category_view", listOf("group", "tags", "none"),
        listOf(tr("Group by category"), tr("As tags"), tr("Hide")),
    )
}

/**
 * A "Sort" row and its "Add new …" row: sorting by creation date sets (and locks) where new
 * items go; manual and due date sorting let the user pick.
 */
@Composable
private fun OrderRows(model: Main, which: String, sortTitle: String, newTitle: String) {
    val sort = setting(model, "${which}_sort")
    val index = SORTS.indexOf(sort).coerceAtLeast(0)
    ChoiceRow(model, sortTitle, "${which}_sort", SORTS, sortNames())
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
    ListItem(
        headlineContent = { Text(tr("Time for all-day tasks")) },
        supportingContent = { Text(tr("When reminders of tasks due on a date go off")) },
        trailingContent = { Text(time(allDayTime)) },
        modifier = Modifier.clickable { pick = true },
    )
    if (pick) {
        val state = rememberTimePickerState((allDayTime / 3600).toInt(), (allDayTime / 60 % 60).toInt(), hours24)
        AlertDialog(
            onDismissRequest = { pick = false },
            confirmButton = {
                TextButton(onClick = {
                    pick = false
                    model.act { core.setSetting("reminder_time", "${state.hour * 60 + state.minute}") }
                }) { Text(tr("OK")) }
            },
            dismissButton = { TextButton(onClick = { pick = false }) { Text(tr("Cancel")) } },
            text = { TimePicker(state) },
        )
    }
}

@Composable
private fun CalendarSettings(model: Main) {
    Group(tr("Views"))
    ChoiceRow(
        model, tr("View at start"), "start_calendar_view", listOf("last", "month", "week", "agenda"),
        listOf(tr("Last used"), tr("Month"), tr("Week"), tr("Agenda")),
    )
    Group(tr("Events"))
    DurationRow(model, tr("Default duration"), tr("When an event is added at a time, or an all-day one gets a time"), "event_duration")
}

@Composable
private fun About(model: Main) {
    val context = LocalContext.current
    val version = remember { context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "" }
    val open = { url: String -> context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
    ListItem(headlineContent = { Text("Todav") }, supportingContent = { Text(tr("Version {version}").replace("{version}", version)) })
    ListItem(
        headlineContent = { Text(tr("Source Code")) },
        supportingContent = { Text("github.com/AperteDev/todav") },
        modifier = Modifier.clickable { open("https://github.com/AperteDev/todav") },
    )
    ListItem(
        headlineContent = { Text(tr("Report an Issue")) },
        modifier = Modifier.clickable { open("https://github.com/AperteDev/todav/issues") },
    )
    ListItem(
        headlineContent = { Text(tr("License")) },
        supportingContent = { Text("GNU General Public License 3.0 or later") },
        modifier = Modifier.clickable { open("https://www.gnu.org/licenses/gpl-3.0.html") },
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
        ListItem(
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
        OutlinedTextField(
            name,
            { name = it },
            label = { Text(tr("Name")) },
            placeholder = { Text(accountName(account.copy(name = ""))) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        )
        LaunchedEffect(name) {
            if (name != account.name) {
                kotlinx.coroutines.delay(600)
                model.act { core.renameAccount(id, name.trim()) }
            }
        }
        if (account.kind == AccountKind.NEXTCLOUD) {
            Group(tr("Connection"))
            val connected = core.connected(id)
            ListItem(
                headlineContent = {
                    Text(if (connected) tr("Signed in as {name}").replace("{name}", account.user) else tr("Not connected"))
                },
                supportingContent = { Text(account.url) },
            )
            if (!signingIn) {
                ListItem(
                    headlineContent = { Text(tr("Sign In Again")) },
                    leadingContent = { Icon(Icons.AutoMirrored.Outlined.Login, null) },
                    modifier = Modifier.clickable { signingIn = true },
                )
            } else {
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
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        ListItem(
            headlineContent = { Text(tr("Remove Account"), color = MaterialTheme.colorScheme.error) },
            leadingContent = { Icon(Icons.Outlined.Delete, null, tint = MaterialTheme.colorScheme.error) },
            modifier = Modifier.clickable { confirm = true },
        )
    }
    val title = tr("Remove {name}?").replace("{name}", accountName(account))
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(title) },
            text = {
                Text(
                    if (account.kind == AccountKind.LOCAL) {
                        tr("Its lists and calendars, with everything in them, are deleted from this device. This cannot be undone.")
                    } else {
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
        model.core.removeAccount(model.todav.passwords, id, force)
        model.todav.changed()
        null
    } catch (e: Exception) {
        reason(e)
    }
}

/** The server as `status.php` describes it, the last sync and push, as the Linux app's. */
@Composable
private fun ServerStatus(model: Main, id: Long) {
    val core = model.core
    val sync by model.syncState.collectAsStateWithLifecycle()
    Group(tr("Server Status"))
    val server = load(id, initial = tr("Checking…")) {
        try {
            val info = core.serverInfo(id)
            val name = "${info.product} ${info.version}"
            if (info.maintenance) tr("{server}, in maintenance mode").replace("{server}", name) else name
        } catch (e: Exception) {
            tr("Cannot reach the server: {error}").replace("{error}", reason(e))
        }
    }
    ListItem(headlineContent = { Text(tr("Server")) }, supportingContent = { Text(server) })
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
    ListItem(headlineContent = { Text(tr("Sync")) }, supportingContent = { Text(status) })
    val push = remember(sync) { core.pushStatus(id) }
    val distributor = remember { model.todav.push.distributor() }
    val text = when {
        push.calendars == 0u -> tr("Checked at the next sync")
        push.supported == 0u -> tr("Not available on this server, so changes made elsewhere show up at the next check. Install the DAV Push app on Nextcloud to see them within seconds")
        distributor == null -> tr("Available on this server. Install a UnifiedPush distributor such as ntfy to see changes made elsewhere within seconds")
        push.registered == 0u -> tr("Registering…")
        else -> tr("On: changes made elsewhere show up within seconds")
    }
    ListItem(headlineContent = { Text(tr("DAV Push")) }, supportingContent = { Text(text) })
}

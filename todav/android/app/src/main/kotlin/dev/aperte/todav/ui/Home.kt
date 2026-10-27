package dev.aperte.todav.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.TaskAlt
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.StarOutline
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DrawerState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Dest
import dev.aperte.todav.IMPORTANT
import dev.aperte.todav.Main
import dev.aperte.todav.Place
import dev.aperte.todav.Screen
import dev.aperte.todav.Ticking
import dev.aperte.todav.UPCOMING
import dev.aperte.todav.View
import dev.aperte.todav.accountName
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.ntr
import dev.aperte.todav.tr
import kotlinx.coroutines.launch

/** How wide the window is, by Material's window size classes. */
enum class Width { COMPACT, MEDIUM, EXPANDED }

/** The drawer, for the panes' menu buttons; null where there is none to open. */
val LocalDrawer = compositionLocalOf<DrawerState?> { null }

/** The snackbar of the home screen. */
val LocalSnackbar = compositionLocalOf { SnackbarHostState() }

/**
 * Tasks and Calendar: switched by a navigation bar on phones and a rail on medium widths, side
 * by side on expanded ones (unfolded foldables, tablets). The drawer holds the task lists and
 * the calendars to show.
 */
@Composable
fun Home(model: Main) {
    val screen = model.screen.collectAsStateWithLifecycle().value ?: return
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    val view by model.view.collectAsStateWithLifecycle()
    val ticking by model.ticking.collectAsStateWithLifecycle()
    ticking?.let { SubtasksDialog(it, model::answer) }
    Messages(model, screen, snackbar)

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = when {
            maxWidth < 600.dp -> Width.COMPACT
            maxWidth < 840.dp -> Width.MEDIUM
            else -> Width.EXPANDED
        }
        val both = width == Width.EXPANDED
        ModalNavigationDrawer(
            drawerState = drawer,
            gesturesEnabled = drawer.isOpen,
            drawerContent = {
                ModalDrawerSheet {
                    Drawer(model, screen, if (both) null else view) {
                        scope.launch { drawer.close() }
                    }
                }
            },
        ) {
            Scaffold(
                snackbarHost = { SnackbarHost(snackbar) },
                bottomBar = { if (width == Width.COMPACT) Switcher(view, model::show) },
                contentWindowInsets = WindowInsets(0),
            ) { padding ->
                androidx.compose.runtime.CompositionLocalProvider(
                    LocalDrawer provides drawer,
                    LocalSnackbar provides snackbar,
                ) {
                    Row(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
                        if (width == Width.MEDIUM) Rail(view, model::show)
                        when {
                            both -> {
                                Box(Modifier.weight(1f).fillMaxHeight()) { TasksPane(model, screen) }
                                VerticalDivider()
                                Box(Modifier.weight(1f).fillMaxHeight()) { CalendarPane(model) }
                            }
                            view == View.TASKS -> TasksPane(model, screen)
                            else -> CalendarPane(model)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Switcher(view: View, onShow: (View) -> Unit) {
    NavigationBar {
        NavigationBarItem(
            selected = view == View.TASKS,
            onClick = { onShow(View.TASKS) },
            icon = { Icon(if (view == View.TASKS) Icons.Filled.TaskAlt else Icons.Outlined.TaskAlt, null) },
            label = { Text(tr("Tasks")) },
        )
        NavigationBarItem(
            selected = view == View.CALENDAR,
            onClick = { onShow(View.CALENDAR) },
            icon = { Icon(if (view == View.CALENDAR) Icons.Filled.CalendarMonth else Icons.Outlined.CalendarMonth, null) },
            label = { Text(tr("Calendar")) },
        )
    }
}

@Composable
private fun Rail(view: View, onShow: (View) -> Unit) {
    NavigationRail {
        NavigationRailItem(
            selected = view == View.TASKS,
            onClick = { onShow(View.TASKS) },
            icon = { Icon(if (view == View.TASKS) Icons.Filled.TaskAlt else Icons.Outlined.TaskAlt, null) },
            label = { Text(tr("Tasks")) },
        )
        NavigationRailItem(
            selected = view == View.CALENDAR,
            onClick = { onShow(View.CALENDAR) },
            icon = { Icon(if (view == View.CALENDAR) Icons.Filled.CalendarMonth else Icons.Outlined.CalendarMonth, null) },
            label = { Text(tr("Calendar")) },
        )
    }
}

/** Snackbars: messages with Undo, uploads the server refused, failed syncs. */
@Composable
private fun Messages(model: Main, screen: Screen, snackbar: SnackbarHostState) {
    LaunchedEffect(Unit) {
        model.messages.collect { (text, change) ->
            val action = if (change != null) tr("Undo") else null
            if (snackbar.showSnackbar(text, action) == SnackbarResult.ActionPerformed) model.undo(change)
        }
    }
    LaunchedEffect(Unit) {
        model.refused.collect { items ->
            for (r in items) {
                val text = tr("“{title}” was not saved on the server: {reason}")
                    .replace("{title}", r.summary).replace("{reason}", r.error)
                snackbar.showSnackbar(text)
            }
        }
    }
    val sync by model.syncState.collectAsStateWithLifecycle()
    val failed = sync as? SyncState.Failed
    LaunchedEffect(failed) {
        if (failed == null) return@LaunchedEffect
        val names = screen.accounts.associate { it.id to accountName(it) }
        val error = failed.failures.joinToString("; ") { f ->
            f.account?.let { names[it] }?.let { "$it: ${f.error}" } ?: f.error
        }
        snackbar.showSnackbar(tr("Sync failed: {error}").replace("{error}", error))
    }
}

/** Whether completing or reopening a task takes its subtasks along (`onAnswer(null)`: cancelled). */
@Composable
private fun SubtasksDialog(t: Ticking, onAnswer: (Boolean?) -> Unit) {
    val n = t.subtasks.toLong()
    val body = if (t.done) {
        ntr("“{title}” has {n} open subtask.", "“{title}” has {n} open subtasks.", n)
    } else {
        ntr("“{title}” has {n} completed subtask.", "“{title}” has {n} completed subtasks.", n)
    }
    AlertDialog(
        onDismissRequest = { onAnswer(null) },
        title = { Text(if (t.done) tr("Complete Subtasks Too?") else tr("Reopen Subtasks Too?")) },
        text = { Text(body.replace("{title}", t.task.summary).replace("{n}", "$n")) },
        confirmButton = {
            TextButton(onClick = { onAnswer(true) }) {
                Text(if (t.done) tr("Complete All") else tr("Reopen All"))
            }
        },
        dismissButton = {
            Row {
                TextButton(onClick = { onAnswer(null) }) { Text(tr("Cancel")) }
                TextButton(onClick = { onAnswer(false) }) { Text(tr("Only This Task")) }
            }
        },
    )
}

/**
 * The task lists (with Upcoming and Important) and, with the calendar shown, the calendars to
 * show or hide; `view` null: both are shown. New list and Settings at the end.
 */
@Composable
private fun Drawer(model: Main, screen: Screen, view: View?, close: () -> Unit) {
    var newList by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<String?>(null) }
    val calendars by model.calendarLists.collectAsStateWithLifecycle()
    LazyColumn(contentPadding = PaddingValues(vertical = 12.dp)) {
        item {
            Text(
                "Todav",
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(horizontal = 28.dp, vertical = 16.dp),
            )
        }
        if (view != View.CALENDAR) {
            items(screen.places, key = { "t:" + it.key }) { place ->
                place.heading?.let { DrawerHeading(it) }
                NavigationDrawerItem(
                    label = { Text(place.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    icon = { PlaceIcon(place) },
                    badge = { if (place.count > 0) Text("${place.count}") },
                    selected = place.key == screen.current && view != null,
                    onClick = {
                        model.select(place.key)
                        model.show(View.TASKS)
                        close()
                    },
                    modifier = Modifier.padding(horizontal = 12.dp).longPress(place.key != UPCOMING && place.key != IMPORTANT) {
                        editing = place.key
                    },
                )
            }
        }
        if (view != View.TASKS) {
            item(key = "calendars") {
                if (view == null) HorizontalDivider(Modifier.padding(vertical = 8.dp, horizontal = 28.dp))
            }
            items(calendars, key = { "c:" + it.href }) { c ->
                c.heading?.let { DrawerHeading(it) }
                NavigationDrawerItem(
                    label = { Text(c.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    icon = {
                        Checkbox(
                            checked = c.shown,
                            onCheckedChange = { model.setShown(c.href, it) },
                            colors = CheckboxDefaults.colors(checkedColor = parseColor(c.color)),
                            modifier = Modifier.size(24.dp),
                        )
                    },
                    selected = false,
                    onClick = { model.setShown(c.href, !c.shown) },
                    modifier = Modifier.padding(horizontal = 12.dp).longPress(true) { editing = c.href },
                )
            }
        }
        item(key = "end") {
            HorizontalDivider(Modifier.padding(vertical = 8.dp, horizontal = 28.dp))
            NavigationDrawerItem(
                label = { Text(tr("New List…")) },
                icon = { Icon(Icons.Outlined.Add, null) },
                selected = false,
                onClick = { newList = true },
                modifier = Modifier.padding(horizontal = 12.dp),
            )
            NavigationDrawerItem(
                label = { Text(tr("Settings")) },
                icon = { Icon(Icons.Outlined.Settings, null) },
                selected = false,
                onClick = {
                    close()
                    model.open(Dest.Settings)
                },
                modifier = Modifier.padding(horizontal = 12.dp),
            )
        }
    }
    if (newList) ListDialog(model, null, view == View.CALENDAR) { newList = false }
    editing?.let { href -> ListDialog(model, href, false) { editing = null } }
}

@Composable
private fun DrawerHeading(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(start = 28.dp, top = 16.dp, bottom = 8.dp),
    )
}

@Composable
private fun PlaceIcon(place: Place) {
    when (place.key) {
        UPCOMING -> Icon(Icons.Outlined.CalendarMonth, null)
        IMPORTANT -> Icon(Icons.Outlined.StarOutline, null)
        else -> Box(Modifier.size(12.dp).background(parseColor(place.color), CircleShape))
    }
}

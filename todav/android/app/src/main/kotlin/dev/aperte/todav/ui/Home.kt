package dev.aperte.todav.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.TaskAlt
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.StarOutline
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconToggleButton
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
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

/** The snackbar of the home screen. */
val LocalSnackbar = compositionLocalOf { SnackbarHostState() }

/**
 * Tasks or Calendar, switched by a navigation bar on phones and a rail from 600 dp, Material's
 * medium width (unfolded foldables, some under 700 dp, tablets, phones on their side).
 * Their menu button shows the sidebar (the task lists or the calendars to show): a page over the
 * whole width on phones, a pane beside the view, shown at first, on wider screens.
 */
@Composable
fun Home(model: Main) {
    val screen = model.screen.collectAsStateWithLifecycle().value ?: return
    val snackbar = remember { SnackbarHostState() }
    val view by model.view.collectAsStateWithLifecycle()
    val ticking by model.ticking.collectAsStateWithLifecycle()
    ticking?.let { SubtasksDialog(it, model::answer) }
    Messages(model, screen, snackbar)
    // Each its own: turning a phone doesn't cover its view with a page left open in the pane.
    var page by rememberSaveable { mutableStateOf(false) }
    var pane by rememberSaveable { mutableStateOf(true) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= 600.dp
        val room = maxWidth.value
        val sidebar = if (wide) pane else page
        val menu = { if (wide) pane = !pane else page = !page }
        val show = { v: View ->
            page = false
            model.show(v)
        }
        // The add button, for what is shown: a task or an event.
        val add = { if (view == View.TASKS) newTask(model) else newEvent(model) }
        BackHandler(enabled = !wide && page) { page = false }
        Scaffold(
            snackbarHost = { SnackbarHost(snackbar) },
            bottomBar = { if (!wide) Switcher(view, page, menu, show, add) },
            floatingActionButton = {
                if (wide) Box(Modifier.navigationBarsPadding()) { AddButton(addLabel(view), raised = true, onClick = add) }
            },
            contentWindowInsets = WindowInsets(0),
        ) { padding ->
            androidx.compose.runtime.CompositionLocalProvider(LocalSnackbar provides snackbar) {
                Row(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
                    if (wide) {
                        Rail(view, pane, menu, show)
                        AnimatedVisibility(
                            pane,
                            enter = expandHorizontally(expandFrom = Alignment.Start),
                            exit = shrinkHorizontally(shrinkTowards = Alignment.Start),
                        ) {
                            // Dragged by its edge, from 200 dp up to what leaves the view 320 dp.
                            val most = (room - 80 - 320).coerceIn(200f, 400f)
                            val saved by model.sidebarWidth.collectAsStateWithLifecycle()
                            var width by remember(saved) { mutableFloatStateOf(saved) }
                            Row(Modifier.fillMaxHeight()) {
                                Sidebar(model, screen, view, Modifier.width(width.coerceIn(200f, most).dp)) {}
                                Edge(
                                    onDrag = { width = (width.coerceIn(200f, most) + it).coerceIn(200f, most) },
                                    onDone = { model.setSidebarWidth(width.coerceIn(200f, most)) },
                                )
                            }
                        }
                    }
                    Box(Modifier.weight(1f).fillMaxHeight()) {
                        Panes(model, screen, view, Modifier.fillMaxSize())
                        if (!wide) {
                            androidx.compose.animation.AnimatedVisibility(
                                sidebar,
                                enter = slideInHorizontally { -it },
                                exit = slideOutHorizontally { -it },
                            ) {
                                Sidebar(model, screen, view, Modifier.fillMaxSize()) { page = false }
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * Tasks and Calendar as two pages, both kept composed so either shows at once; the buttons slide
 * between them (not swipes: rows and months take those).
 */
@Composable
private fun Panes(model: Main, screen: Screen, view: View, modifier: Modifier) {
    val pager = rememberPagerState(initialPage = view.ordinal) { 2 }
    LaunchedEffect(view) { if (pager.currentPage != view.ordinal) pager.animateScrollToPage(view.ordinal) }
    HorizontalPager(pager, modifier, userScrollEnabled = false, beyondViewportPageCount = 1) { page ->
        if (page == View.TASKS.ordinal) TasksPane(model, screen) else CalendarPane(model)
    }
}

private fun addLabel(view: View) = if (view == View.TASKS) tr("Add Task") else tr("New Event")

/** What the menu button shows: the task lists or the calendars. */
/** The menu button's name, the same with either view: task lists and calendars are both lists. */
private fun menuLabel() = tr("Lists")

/**
 * The sidebar's menu button (selected while its page is shown), Tasks and Calendar, and the add
 * button at the end (as a bottom app bar has it), to take less room.
 */
@Composable
private fun Switcher(view: View, page: Boolean, onMenu: () -> Unit, onShow: (View) -> Unit, onAdd: () -> Unit) {
    NavigationBar {
        NavigationBarItem(
            selected = page,
            onClick = onMenu,
            icon = { Icon(Icons.Default.Menu, null) },
            label = { Text(menuLabel()) },
        )
        NavigationBarItem(
            selected = !page && view == View.TASKS,
            onClick = { onShow(View.TASKS) },
            icon = { Icon(if (view == View.TASKS) Icons.Filled.TaskAlt else Icons.Outlined.TaskAlt, null) },
            label = { Text(tr("Tasks")) },
        )
        NavigationBarItem(
            selected = !page && view == View.CALENDAR,
            onClick = { onShow(View.CALENDAR) },
            icon = { Icon(if (view == View.CALENDAR) Icons.Filled.CalendarMonth else Icons.Outlined.CalendarMonth, null) },
            label = { Text(tr("Calendar")) },
        )
        Box(Modifier.padding(end = 12.dp)) { AddButton(addLabel(view), raised = false, onClick = onAdd) }
    }
}

/** The sidebar's menu button, then Tasks and Calendar; the add button floats in the corner. */
@Composable
private fun Rail(view: View, pane: Boolean, onMenu: () -> Unit, onShow: (View) -> Unit) {
    NavigationRail(
        header = {
            Tip(menuLabel()) {
                IconToggleButton(checked = pane, onCheckedChange = { onMenu() }) { Icon(Icons.Default.Menu, menuLabel()) }
            }
        },
    ) {
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
 * The sidebar pane's edge, in its colour, with the line beside the view: a handle in its middle
 * drags it wider or narrower (`onDrag` by dp), `onDone` when let go.
 */
@Composable
private fun Edge(onDrag: (Float) -> Unit, onDone: () -> Unit) {
    val density = LocalDensity.current
    Row(Modifier.fillMaxHeight().background(MaterialTheme.colorScheme.surfaceContainerLow)) {
        Box(
            Modifier.width(12.dp).fillMaxHeight().draggable(
                rememberDraggableState { px -> onDrag(with(density) { px.toDp().value }) },
                Orientation.Horizontal,
                onDragStopped = { onDone() },
            ),
            Alignment.Center,
        ) {
            Box(Modifier.size(4.dp, 32.dp).background(MaterialTheme.colorScheme.outline.copy(alpha = 0.5f), CircleShape))
        }
        VerticalDivider()
    }
}

/**
 * The sidebar: the task lists (with Upcoming and Important) or, with the calendar shown, the
 * calendars to show or hide; New list and Settings at the end. `close` hides its page on phones,
 * after picking a list.
 */
@Composable
private fun Sidebar(model: Main, screen: Screen, view: View, modifier: Modifier, close: () -> Unit) {
    var newList by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<String?>(null) }
    val calendars by model.calendarLists.collectAsStateWithLifecycle()
    LazyColumn(
        modifier.background(MaterialTheme.colorScheme.surfaceContainerLow)
            .windowInsetsPadding(WindowInsets.systemBars.only(WindowInsetsSides.Vertical)),
        contentPadding = PaddingValues(vertical = 12.dp),
    ) {
        item {
            Text(
                "Todav",
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(horizontal = 28.dp, vertical = 16.dp),
            )
        }
        if (view == View.TASKS) {
            items(screen.places, key = { "t:" + it.key }) { place ->
                place.heading?.let { DrawerHeading(it) }
                NavigationDrawerItem(
                    label = { Text(place.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    icon = { PlaceIcon(place) },
                    badge = { if (place.count > 0) Text("${place.count}") },
                    selected = place.key == screen.current,
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
        if (view == View.CALENDAR) {
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
                label = { Text(if (view == View.CALENDAR) tr("New Calendar…") else tr("New List…")) },
                icon = { Icon(Icons.Outlined.Add, null) },
                selected = false,
                onClick = { newList = true },
                modifier = Modifier.padding(horizontal = 12.dp),
            )
            NavigationDrawerItem(
                label = { Text(tr("Settings")) },
                icon = { Icon(Icons.Outlined.Settings, null) },
                selected = false,
                onClick = { model.open(Dest.Settings) },
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

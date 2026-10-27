package dev.aperte.todav.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.PushPin
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.CalendarToday
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material.icons.outlined.FilterList
import androidx.compose.material.icons.outlined.Label
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.rememberSwipeToDismissBoxState
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Dest
import dev.aperte.todav.IMPORTANT
import dev.aperte.todav.Main
import dev.aperte.todav.Row as TaskRow
import dev.aperte.todav.Screen
import dev.aperte.todav.UPCOMING
import dev.aperte.todav.core.Due
import dev.aperte.todav.core.DueValue
import dev.aperte.todav.core.PriorityValue
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.TaskPatch
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.overdue
import dev.aperte.todav.date
import dev.aperte.todav.dueLabel
import dev.aperte.todav.ntr
import dev.aperte.todav.ptr
import dev.aperte.todav.tr
import kotlinx.coroutines.launch
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.zIndex
import kotlinx.coroutines.delay

/** The task list shown (a list, Upcoming or Important), with its top bar and the add button. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TasksPane(model: Main, screen: Screen) {
    val drawer = LocalDrawer.current
    val scope = rememberCoroutineScope()
    val sync by model.syncState.collectAsStateWithLifecycle()
    var adding by rememberSaveable { mutableStateOf(false) }
    // The subtask being added to (its parent), from a row's menu.
    var addingTo by remember { mutableStateOf<TodavTask?>(null) }
    var pulling by remember { mutableStateOf(false) }
    LaunchedEffect(sync) { if (sync != SyncState.Syncing) pulling = false }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(screen.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            ntr("{n} open", "{n} open", screen.open.toLong()).replace("{n}", "${screen.open}"),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = { scope.launch { drawer?.open() } }) {
                        Icon(Icons.Default.Menu, tr("Task lists"))
                    }
                },
                actions = {
                    IconButton(onClick = { model.open(Dest.Search) }) { Icon(Icons.Default.Search, tr("Search")) }
                    screen.list?.takeIf { screen.categories.isNotEmpty() }?.let { list ->
                        CategoryFilter(screen) { model.filterBy(list.href, it) }
                    }
                    if (sync == SyncState.Syncing && !pulling) {
                        CircularProgressIndicator(Modifier.padding(12.dp).size(24.dp), strokeWidth = 2.dp)
                    } else if (model.core.canSync()) {
                        IconButton(onClick = model::sync) { Icon(Icons.Default.Refresh, tr("Sync")) }
                    }
                },
            )
        },
        floatingActionButton = {
            if (screen.list != null) {
                FloatingActionButton(onClick = { adding = true }) { Icon(Icons.Default.Add, tr("Add Task")) }
            }
        },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = pulling && sync == SyncState.Syncing,
            onRefresh = {
                pulling = true
                model.sync()
            },
            modifier = Modifier.fillMaxSize(),
        ) {
            TaskList(model, screen, padding) { addingTo = it }
        }
    }
    val list = screen.list
    if (adding && list != null) {
        AddSheet(model, list.href, null, screen.filter) { adding = false }
    }
    // Shared text, or the "New task" shortcut: into the list shown, else the first one.
    val shared by model.sharedText.collectAsStateWithLifecycle()
    shared?.let { text ->
        val target = list?.href ?: screen.places.firstOrNull { it.key != UPCOMING && it.key != IMPORTANT }?.key
        if (target == null) {
            model.sharedText.value = null
        } else {
            AddSheet(model, target, null, null, initial = text) { model.sharedText.value = null }
        }
    }
    addingTo?.let { parent -> AddSheet(model, parent.listHref, parent, parent.category) { addingTo = null } }
}

/** The categories of the list shown, to show one only. */
@Composable
private fun CategoryFilter(screen: Screen, onPick: (String?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) {
            Icon(
                Icons.Outlined.FilterList,
                tr("Categories"),
                tint = if (screen.filter != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(
                text = { Text(tr("All Categories")) },
                leadingIcon = { if (screen.filter == null) Icon(Icons.Default.Check, null) },
                onClick = {
                    open = false
                    onPick(null)
                },
            )
            screen.categories.forEach { c ->
                DropdownMenuItem(
                    text = { Text(c) },
                    leadingIcon = { if (screen.filter == c) Icon(Icons.Default.Check, null) },
                    onClick = {
                        open = false
                        onPick(c)
                    },
                )
            }
        }
    }
}

/** A drag in progress: the row, its key, and the finger in the list's item coordinates. */
private class Drag(val row: TaskRow, val key: String, val grab: Float, y: Float) {
    var y by mutableFloatStateOf(y)
    var moved by mutableStateOf(false)
}

/** Where a drag would drop: above, into or below a row (`mode` -1, 0, 1), or on a group's heading. */
private data class Target(val key: String, val row: TaskRow?, val group: dev.aperte.todav.Group?, val mode: Int)

@Composable
private fun TaskList(model: Main, screen: Screen, padding: PaddingValues, onAddSubtask: (TodavTask) -> Unit) {
    var showCompleted by rememberSaveable { mutableStateOf(false) }
    if (screen.groups.isEmpty() && screen.completed.isEmpty()) {
        Empty(screen, padding)
        return
    }
    val state = rememberLazyListState()
    val haptics = LocalHapticFeedback.current
    var drag by remember { mutableStateOf<Drag?>(null) }
    var menuFor by remember { mutableStateOf<String?>(null) }
    // What each key is, for finding the row or heading under the finger.
    val rows = remember(screen) {
        buildMap {
            for (g in screen.groups) {
                g.title?.let { put("group:$it", g) }
                g.rows.forEach { put("${g.title}:${it.task.uid}", it) }
            }
            screen.completed.forEach { put("done:${it.task.uid}", it) }
        }
    }
    val core = model.core
    fun targetAt(d: Drag): Target? {
        val item = state.layoutInfo.visibleItemsInfo.firstOrNull { d.y >= it.offset && d.y < it.offset + it.size } ?: return null
        val key = item.key as? String ?: return null
        if (key == d.key) return null
        return when (val what = rows[key]) {
            is dev.aperte.todav.Group -> if (what.drop) Target(key, null, what, 0) else null
            is TaskRow -> {
                val ts = what.slot ?: return null
                val ds = d.row.slot ?: return null
                val at = (d.y - item.offset) / item.size
                val mode = if (at < 0.3f) -1 else if (at > 0.7f) 1 else 0
                val ok = if (mode == 0) {
                    core.canAdopt(d.row.task, what.task)
                } else {
                    core.canDrop(d.row.task, ds, what.task, ts) &&
                        !dev.aperte.todav.core.staysPut(d.row.task, ds, what.task, ts, mode < 0)
                }
                if (ok) Target(key, what, null, mode) else null
            }
            else -> null
        }
    }
    val target = drag?.let { d -> if (d.moved) targetAt(d) else null }
    // Near the top or the bottom, the list scrolls under a held row.
    LaunchedEffect(drag) {
        while (true) {
            val d = drag ?: break
            val info = state.layoutInfo
            val edge = 120f
            val step = when {
                d.y < info.viewportStartOffset + padding.calculateTopPadding().value * 2 + edge -> -24f
                d.y > info.viewportEndOffset - info.afterContentPadding - edge -> 24f
                else -> 0f
            }
            if (step != 0f && d.moved) d.y += state.scrollBy(step)
            delay(16)
        }
    }
    LazyColumn(
        state = state,
        // While a row is held, its moves are the drag's, not a scroll's.
        userScrollEnabled = drag == null,
        contentPadding = PaddingValues(top = padding.calculateTopPadding(), bottom = 88.dp),
        modifier = Modifier.fillMaxSize().pointerInput(screen) {
            detectDragGesturesAfterLongPress(
                onDragStart = { at ->
                    val y = at.y + state.layoutInfo.viewportStartOffset
                    val item = state.layoutInfo.visibleItemsInfo.firstOrNull { y >= it.offset && y < it.offset + it.size }
                    val row = item?.let { rows[it.key] as? TaskRow }
                    if (item != null && row != null) {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        drag = Drag(row, item.key as String, y - item.offset, y)
                    }
                },
                onDrag = { change, amount ->
                    val d = drag ?: return@detectDragGesturesAfterLongPress
                    change.consume()
                    d.y += amount.y
                    if (!d.moved && kotlin.math.abs(d.y - (d.grab + (state.layoutInfo.visibleItemsInfo.firstOrNull { it.key == d.key }?.offset ?: 0))) > viewConfiguration.touchSlop) {
                        d.moved = d.row.slot != null
                    }
                },
                onDragEnd = {
                    val d = drag ?: return@detectDragGesturesAfterLongPress
                    drag = null
                    if (!d.moved) {
                        menuFor = d.key
                        return@detectDragGesturesAfterLongPress
                    }
                    val t = targetAt(d) ?: return@detectDragGesturesAfterLongPress
                    val ds = d.row.slot ?: return@detectDragGesturesAfterLongPress
                    model.together(tr("“{title}” moved").replace("{title}", d.row.task.summary)) {
                        when {
                            t.group != null -> core.dropOnCategory(d.row.task, ds, t.group.category)
                            t.mode == 0 -> core.adopt(d.row.task, t.row!!.task)
                            else -> core.dropTask(d.row.task, ds, t.row!!.task, t.row.slot!!, t.mode < 0)
                        }
                    }
                },
                onDragCancel = { drag = null },
            )
        },
    ) {
        for (group in screen.groups) {
            group.title?.let { title ->
                item(key = "group:$title", contentType = "heading") {
                    val on = target?.key == "group:$title"
                    Box(Modifier.background(if (on) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent)) { Heading(title) }
                }
            }
            items(group.rows, key = { "${group.title}:${it.task.uid}" }, contentType = { "task" }) { row ->
                val key = "${group.title}:${row.task.uid}"
                TaskItem(model, screen, row, key, drag, target, state, menuFor == key, { menuFor = null }, onAddSubtask)
            }
        }
        if (screen.completed.isNotEmpty()) {
            item(key = "completed") {
                Row(
                    Modifier.fillMaxWidth().clickable { showCompleted = !showCompleted }
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        tr("Completed ({n})").replace("{n}", "${screen.completed.size}"),
                        style = MaterialTheme.typography.titleSmall,
                        modifier = Modifier.weight(1f),
                    )
                    Icon(if (showCompleted) Icons.Default.ExpandLess else Icons.Default.ExpandMore, null)
                }
            }
            if (showCompleted) {
                items(screen.completed, key = { "done:${it.task.uid}" }, contentType = { "task" }) { row ->
                    val key = "done:${row.task.uid}"
                    TaskItem(model, screen, row, key, drag, target, state, menuFor == key, { menuFor = null }, onAddSubtask)
                }
            }
        }
    }
}

@Composable
private fun Empty(screen: Screen, padding: PaddingValues) {
    val (title, description) = when (screen.current) {
        UPCOMING -> tr("Nothing Scheduled") to tr("Tasks with a due date show up here")
        IMPORTANT -> tr("No Important Tasks") to tr("Tasks of high priority show up here")
        else -> tr("All Done") to null
    }
    Column(
        Modifier.fillMaxSize().padding(padding).padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        description?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable
fun Heading(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface)
            .padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp),
    )
}

/**
 * A task: its check, title, note or list, due date and star. Tapped it opens; held, its menu (the
 * list moves it when dragged on); swiped right it is completed (or reopened), left deleted (with
 * Undo).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TaskItem(
    model: Main,
    screen: Screen,
    row: TaskRow,
    key: String,
    drag: Drag?,
    target: Target?,
    state: LazyListState,
    menu: Boolean,
    onMenuClosed: () -> Unit,
    onAddSubtask: (TodavTask) -> Unit,
) {
    val task = row.task
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    var moving by remember { mutableStateOf(false) }
    val dragged = drag?.key == key && drag.moved
    val line = MaterialTheme.colorScheme.primary
    val mode = target?.takeIf { it.key == key }?.mode
    val dismiss = rememberSwipeToDismissBoxState()
    SwipeToDismissBox(
        state = dismiss,
        gesturesEnabled = drag == null,
        modifier = Modifier
            .zIndex(if (dragged) 1f else 0f)
            .graphicsLayer {
                if (dragged && drag != null) {
                    val now = state.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.offset ?: 0
                    translationY = drag.y - drag.grab - now
                    shadowElevation = 8.dp.toPx()
                }
            }
            .drawWithContent {
                drawContent()
                val w = 3.dp.toPx()
                when (mode) {
                    -1 -> drawRect(line, size = Size(size.width, w))
                    1 -> drawRect(line, Offset(0f, size.height - w), Size(size.width, w))
                }
            },
        onDismiss = {
            when (it) {
                SwipeToDismissBoxValue.EndToStart -> model.delete(task)
                SwipeToDismissBoxValue.StartToEnd -> {
                    haptics.performHapticFeedback(HapticFeedbackType.ToggleOn)
                    model.setDone(task, !task.done)
                    scope.launch { dismiss.reset() }
                }
                SwipeToDismissBoxValue.Settled -> {}
            }
        },
        backgroundContent = {
            val start = dismiss.dismissDirection == SwipeToDismissBoxValue.StartToEnd
            Box(
                Modifier.fillMaxSize()
                    .background(if (start) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.errorContainer)
                    .padding(horizontal = 24.dp),
                if (start) Alignment.CenterStart else Alignment.CenterEnd,
            ) {
                if (start) {
                    Icon(Icons.Default.Check, if (task.done) tr("Reopen") else tr("Complete"), tint = MaterialTheme.colorScheme.onPrimaryContainer)
                } else {
                    Icon(Icons.Default.Delete, tr("Delete"), tint = MaterialTheme.colorScheme.onErrorContainer)
                }
            }
        },
    ) {
        Row(
            Modifier.fillMaxWidth()
                .background(if (mode == 0) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
                .clickable { model.open(Dest.Task(task.uid)) }
                .padding(start = (8 + 24 * row.depth).dp, end = 4.dp, top = 2.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(checked = task.done, onCheckedChange = {
                haptics.performHapticFeedback(if (it) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
                model.setDone(task, it)
            })
            Column(Modifier.weight(1f).padding(vertical = 8.dp)) {
                Text(
                    task.summary,
                    textDecoration = if (task.done) TextDecoration.LineThrough else null,
                    color = if (task.done) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                )
                row.subtitle?.takeIf { it.isNotBlank() }?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Meta(model, task, row, screen.now)
            }
            row.counts?.let { c ->
                TextButton(onClick = { model.toggle(task.uid) }) {
                    Text("${c.done}/${c.open + c.done}", style = MaterialTheme.typography.labelMedium)
                    Icon(if (row.expanded) Icons.Default.ExpandLess else Icons.Default.ExpandMore, tr("Subtasks"))
                }
            }
            if (screen.pins && task.pinned && row.depth == 0) {
                Icon(Icons.Default.PushPin, tr("Pinned"), Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
            }
            if (screen.stars) Star(model, task)
        }
        RowMenu(model, screen, task, row.depth, menu, onMenuClosed, { onAddSubtask(task) }) { moving = true }
    }
    HorizontalDivider(Modifier.padding(start = (56 + 24 * row.depth).dp))
    if (moving) MoveDialog(model, task) { moving = false }
}

/** Due date, reminder bell, its event and (as tags) its category, under the title. */
@Composable
private fun Meta(model: Main, task: TodavTask, row: TaskRow, now: Long) {
    val due = dueLabel(task, now)
    val event = task.eventUid?.let { uid -> remember(uid) { runCatching { model.core.event(uid) }.getOrNull() } }
    if (due == null && row.tag == null && event == null) return
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (due != null) {
            val color = if (overdue(task, now) && !task.done) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant
            Icon(Icons.Outlined.CalendarToday, null, Modifier.size(14.dp), tint = color)
            Text(
                due,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Medium,
                color = color,
                modifier = Modifier.widthIn(max = 160.dp),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (task.reminders.isNotEmpty()) {
                Icon(Icons.Outlined.Notifications, tr("Reminder"), Modifier.size(14.dp), tint = color)
            }
        }
        event?.let {
            Icon(Icons.Outlined.Event, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.tertiary)
            Text(
                it.summary,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.tertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = 140.dp).clickable { model.showDay(it.start) },
            )
        }
        row.tag?.let {
            Icon(Icons.Outlined.Label, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.secondary)
            Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.secondary, maxLines = 1)
        }
    }
}

/** The star in a task's priority colour; its menu sets the level, as the edit page does. */
@Composable
private fun Star(model: Main, task: TodavTask) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) { StarIcon(task.priority) }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            PRIORITIES.reversed().forEach { p ->
                DropdownMenuItem(
                    text = { Text(priorityName(p)) },
                    leadingIcon = { StarIcon(p) },
                    trailingIcon = { if (priorityIndex(p) == priorityIndex(task.priority)) Icon(Icons.Default.Check, null) },
                    onClick = {
                        open = false
                        model.update(task.uid, TaskPatch(priority = PriorityValue(p)))
                    },
                )
            }
        }
    }
}

@Composable
private fun RowMenu(
    model: Main,
    screen: Screen,
    task: TodavTask,
    depth: Int,
    open: Boolean,
    onClose: () -> Unit,
    onAddSubtask: () -> Unit,
    onMove: () -> Unit,
) {
    DropdownMenu(expanded = open, onDismissRequest = onClose) {
        DropdownMenuItem(text = { Text(tr("Edit…")) }, onClick = {
            onClose()
            model.open(Dest.Task(task.uid))
        })
        if (!task.done) {
            DropdownMenuItem(text = { Text(tr("Add Subtask")) }, onClick = {
                onClose()
                onAddSubtask()
            })
        }
        if (screen.pins && depth == 0 && task.parentUid == null) {
            DropdownMenuItem(text = { Text(if (task.pinned) tr("Unpin") else tr("Pin")) }, onClick = {
                onClose()
                model.update(task.uid, TaskPatch(pinned = !task.pinned))
            })
        }
        if (task.parentUid != null) {
            DropdownMenuItem(text = { Text(tr("Make Top-Level Task")) }, onClick = {
                onClose()
                model.act {
                    model.core.promote(task.uid)
                    model.synced()
                }
            })
        }
        DropdownMenuItem(text = { Text(tr("Move to List…")) }, onClick = {
            onClose()
            onMove()
        })
        DropdownMenuItem(
            text = { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) },
            onClick = {
                onClose()
                model.delete(task)
            },
        )
    }
}

/** The task lists a task can move to (with its subtasks). */
@Composable
fun MoveDialog(model: Main, task: TodavTask, onDismiss: () -> Unit) {
    val lists = remember { model.core.byPlace(model.core.lists()).filter { it.writable && it.href != task.listHref } }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Move to List")) },
        text = {
            LazyColumn {
                items(lists, key = { it.href }) { l ->
                    Row(
                        Modifier.fillMaxWidth().clickable {
                            onDismiss()
                            model.move(task, l.href)
                        }.padding(vertical = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        Box(Modifier.size(12.dp).background(parseColor(dev.aperte.todav.core.listColor(l)), androidx.compose.foundation.shape.CircleShape))
                        Text(l.displayName)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

/**
 * Adding a task (or a subtask of `parent`): its title, and chips for a due date, a category
 * and a priority, as Google Tasks' sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddSheet(model: Main, list: String, parent: TodavTask?, category: String?, initial: String = "", onDismiss: () -> Unit) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var text by rememberSaveable { mutableStateOf(initial) }
    var due by rememberSaveable { mutableStateOf<Long?>(null) }
    var cat by rememberSaveable { mutableStateOf(category) }
    var priority by rememberSaveable { mutableStateOf<Long?>(null) }
    var pickDate by remember { mutableStateOf(false) }
    var pickCategory by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val categories = remember(list) { model.core.categories(list).map { it.name } }
    val add = {
        if (text.isNotBlank()) {
            val patch = TaskPatch(
                due = due?.let { DueValue(Due(it, true)) },
                priority = priority?.let { PriorityValue(it) },
                reminders = due?.let { listOf(model.core.defaultReminder(true)) },
            )
            model.add(list, text, parent?.uid, cat, patch.takeIf { due != null || priority != null })
            text = ""
            onDismiss()
        }
    }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet) {
        Column(Modifier.padding(horizontal = 16.dp).padding(bottom = 16.dp).imePadding()) {
            parent?.let {
                Text(
                    tr("Subtask of “{title}”").replace("{title}", it.summary),
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    placeholder = { Text(if (parent == null) tr("New task") else tr("New subtask")) },
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { add() }),
                    singleLine = true,
                    modifier = Modifier.weight(1f).focusRequester(focus),
                )
                IconButton(onClick = add, enabled = text.isNotBlank()) {
                    Icon(Icons.AutoMirrored.Filled.Send, tr("Add Task"))
                }
            }
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = due != null,
                    onClick = { if (due == null) pickDate = true else due = null },
                    label = { Text(due?.let { date(it) } ?: tr("Due Date")) },
                    leadingIcon = { Icon(Icons.Outlined.CalendarToday, null, Modifier.size(18.dp)) },
                )
                if (categories.isNotEmpty() || cat != null) {
                    Box {
                        FilterChip(
                            selected = cat != null,
                            onClick = { pickCategory = true },
                            label = { Text(cat ?: tr("Category")) },
                            leadingIcon = { Icon(Icons.Outlined.Label, null, Modifier.size(18.dp)) },
                        )
                        DropdownMenu(expanded = pickCategory, onDismissRequest = { pickCategory = false }) {
                            DropdownMenuItem(text = { Text(ptr("category", "None")) }, onClick = {
                                pickCategory = false
                                cat = null
                            })
                            categories.forEach { c ->
                                DropdownMenuItem(text = { Text(c) }, onClick = {
                                    pickCategory = false
                                    cat = c
                                })
                            }
                        }
                    }
                }
                FilterChip(
                    selected = priority != null,
                    onClick = { priority = if (priority == null) 1 else null },
                    label = { Text(tr("Important")) },
                    leadingIcon = { StarIcon(priority) },
                )
            }
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    if (pickDate) {
        val state = rememberDatePickerState(initialSelectedDateMillis = System.currentTimeMillis())
        DatePickerDialog(
            onDismissRequest = { pickDate = false },
            confirmButton = {
                TextButton(onClick = {
                    pickDate = false
                    due = state.selectedDateMillis?.let { it / 1000 - Math.floorMod(it / 1000, DAY) }
                }) { Text(tr("OK")) }
            },
            dismissButton = { TextButton(onClick = { pickDate = false }) { Text(tr("Cancel")) } },
        ) { DatePicker(state) }
    }
}

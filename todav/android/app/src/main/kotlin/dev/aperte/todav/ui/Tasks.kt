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
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.IconToggleButton
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.CalendarToday
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material.icons.outlined.FilterList
import androidx.compose.material.icons.automirrored.outlined.Label
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
import androidx.compose.material.icons.automirrored.outlined.FormatIndentDecrease
import androidx.compose.material.icons.filled.PushPin
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.PushPin
import androidx.compose.material.icons.outlined.SubdirectoryArrowRight
import androidx.compose.ui.draw.clip
import androidx.compose.material3.DropdownMenuGroup
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.DropdownMenuPopup
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.material3.rememberDatePickerState
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
import dev.aperte.todav.core.SwipeAction
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.StarOutline
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.TaskPatch
import dev.aperte.todav.core.Item
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
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.zIndex
import kotlinx.coroutines.delay

/** A new task: the add sheet, into the list shown (else the first one). */
fun newTask(model: Main) {
    model.sharedText.value = ""
}

/** The task list shown (a list, Upcoming or Important), with its top bar. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun TasksPane(model: Main, screen: Screen) {
    val sync by model.syncState.collectAsStateWithLifecycle()
    // The subtask being added to (its parent), from a row's menu.
    var addingTo by remember { mutableStateOf<TodavTask?>(null) }
    // Pulled to refresh: shown until the sync it asked for has run, once, without dropping in
    // between (the sync starts a moment after the pull ends).
    var pulling by remember { mutableStateOf(false) }
    var started by remember { mutableStateOf(false) }
    LaunchedEffect(sync, pulling) {
        when {
            !pulling -> started = false
            sync == SyncState.Syncing -> started = true
            started -> pulling = false
            else -> {
                // No sync came of it (nothing to sync with): not held up for good.
                delay(3000)
                pulling = false
            }
        }
    }
    // One row, the title and its count beside the buttons: tinted once the list scrolls under it.
    val bar = TopAppBarDefaults.pinnedScrollBehavior()
    val pull = rememberPullToRefreshState()
    val list = rememberLazyListState()
    // Pull to refresh only from a gesture that starts with the list at its top and pulls straight
    // down: one that scrolled the list on its way (down, then back up to the top) stops there.
    var fromTop by remember { mutableStateOf(true) }

    Scaffold(
        modifier = Modifier.nestedScroll(bar.nestedScrollConnection),
        topBar = {
            TopAppBar(
                title = { Text(screen.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                subtitle = { Text(ntr("{n} open", "{n} open", screen.open.toLong()).replace("{n}", "${screen.open}")) },
                scrollBehavior = bar,
                actions = {
                    Tip(tr("Search")) {
                        IconButton(onClick = { model.open(Dest.Search) }) { Icon(Icons.Default.Search, tr("Search")) }
                    }
                    SortMenu(model)
                    screen.list?.takeIf { screen.categories.isNotEmpty() }?.let { list ->
                        CategoryFilter(screen) { model.filterBy(list.href, it) }
                    }
                    if (model.core.canSync()) SyncButton(sync == SyncState.Syncing && !pulling, model::sync)
                },
            )
        },
    ) { padding ->
        val refreshing = pulling
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = {
                pulling = true
                model.sync()
            },
            modifier = Modifier.fillMaxSize().pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    fromTop = !list.canScrollBackward
                    do {
                        val e = awaitPointerEvent(PointerEventPass.Initial)
                        if (list.canScrollBackward) fromTop = false
                    } while (e.changes.any { it.pressed })
                }
            },
            state = pull,
            enabled = fromTop || refreshing,
            indicator = {
                PullToRefreshDefaults.LoadingIndicator(
                    pull, refreshing, Modifier.align(Alignment.TopCenter).padding(top = padding.calculateTopPadding()),
                )
            },
        ) {
            TaskList(model, screen, list, padding) { addingTo = it }
        }
    }
    val shown = screen.list
    // Shared text, or a new task (the add button, the shortcut): into the list shown, in the
    // category it is filtered to, else the first list.
    val shared by model.sharedText.collectAsStateWithLifecycle()
    shared?.let { text ->
        val target = shown?.href ?: screen.places.firstOrNull { it.key != UPCOMING && it.key != IMPORTANT }?.key
        if (target == null) {
            model.sharedText.value = null
        } else {
            AddSheet(model, target, null, screen.filter.takeIf { shown != null }, initial = text) { model.sharedText.value = null }
        }
    }
    addingTo?.let { parent -> AddSheet(model, parent.listHref, parent, parent.category) { addingTo = null } }
}

/** The sync button: its icon turns while `syncing`, finishing its turn when that ends. */
@Composable
private fun SyncButton(syncing: Boolean, onClick: () -> Unit) {
    val turn = remember { Animatable(0f) }
    LaunchedEffect(syncing) {
        if (syncing) {
            while (true) turn.animateTo(turn.value + 360f, tween(900, easing = LinearEasing))
        } else if (turn.value % 360f != 0f) {
            turn.animateTo(kotlin.math.ceil(turn.value / 360f) * 360f, tween(400, easing = LinearOutSlowInEasing))
            turn.snapTo(0f)
        }
    }
    Tip(tr("Sync")) {
        IconButton(onClick = onClick) {
            Icon(Icons.Default.Refresh, tr("Sync"), Modifier.graphicsLayer { rotationZ = turn.value })
        }
    }
}

/** The task sort of Settings → Tasks, at hand. */
@Composable
private fun SortMenu(model: Main) {
    var open by remember { mutableStateOf(false) }
    val sort = setting(model, "task_sort")
    Box {
        Tip(tr("Sort")) {
            IconButton(onClick = { open = true }) { Icon(Icons.AutoMirrored.Filled.Sort, tr("Sort")) }
        }
        PickMenu(open, { open = false }, sortNames(), SORTS.indexOf(sort).coerceAtLeast(0)) {
            model.setSetting("task_sort", SORTS[it])
        }
    }
}

/** The categories of the list shown, to show one only. */
@Composable
private fun CategoryFilter(screen: Screen, onPick: (String?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        Tip(tr("Categories")) {
            IconButton(onClick = { open = true }) {
                Icon(
                    Icons.Outlined.FilterList,
                    tr("Categories"),
                    tint = if (screen.filter != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        PickMenu(
            open,
            { open = false },
            listOf(tr("All Categories")) + screen.categories,
            screen.categories.indexOf(screen.filter) + 1,
        ) { onPick(screen.categories.getOrNull(it - 1)) }
    }
}

/** A drag in progress: the row, its key and height, and the finger in the list's item coordinates. */
private class Drag(val row: TaskRow, val key: String, val size: Int, val grab: Float, y: Float) {
    var y by mutableFloatStateOf(y)
    var moved by mutableStateOf(false)

    /** Where it would land (null: nowhere it may go), worked out as the finger moves. */
    var target by mutableStateOf<Target?>(null)

    /** The gap it would land in, as the index in the list it goes before (-1: none, as for a heading). */
    var gap by mutableIntStateOf(-1)

    /** Over a place beside a row that only manual order would keep: said why, as nothing opens there. */
    var sorted by mutableStateOf(false)

    /** Dropped: the row settles into its gap while the move is saved. */
    var landing by mutableStateOf(false)
}

/** `target` eased while `live` (a drag, or its drop settling); at once otherwise, as when the list takes its new order. */
@Composable
private fun eased(target: Float, live: Boolean): Float {
    val value by animateFloatAsState(target, label = "gap")
    return if (live) value else target
}

/** Where a drag would drop: above, into or below a row (`mode` -1, 0, 1), or on a group's heading. */
private data class Target(val key: String, val row: TaskRow?, val group: dev.aperte.todav.Group?, val mode: Int)

@Composable
private fun TaskList(model: Main, screen: Screen, state: LazyListState, padding: PaddingValues, onAddSubtask: (TodavTask) -> Unit) {
    var showCompleted by rememberSaveable { mutableStateOf(false) }
    // The task tapped: its details sheet, as the calendar's (pulled up, its page).
    var details by remember { mutableStateOf<TodavTask?>(null) }
    details?.let { ItemSheet(model, Item.Task(it)) { details = null } }
    if (screen.groups.isEmpty() && screen.completed.isEmpty()) {
        Empty(screen, padding)
        return
    }
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    // Gone with the list it was on: a drop ends when the list comes back in its new order, all
    // rows in their places at once (no gap closing over them).
    var drag by remember(screen) { mutableStateOf<Drag?>(null) }
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
    // The held row's landing place is a gap between the list's items: the items between its own
    // place and the gap move aside to open it. Beside a row it lands beside it; into a row (as
    // a subtask) right under it, one level deeper; onto a heading it takes that category, which
    // only highlights the heading.
    val order = remember(screen, showCompleted) {
        buildList {
            for (g in screen.groups) {
                g.title?.let { add("group:$it") }
                g.rows.forEach { add("${g.title}:${it.task.uid}") }
            }
            if (screen.completed.isNotEmpty()) add("completed")
            if (showCompleted) screen.completed.forEach { add("done:${it.task.uid}") }
        }.withIndex().associate { (i, k) -> k to i }
    }
    // Not in manual order: a drag reorders nothing, so no row moves aside for it.
    val sorted = remember(screen) { core.order().tasks != dev.aperte.todav.core.Sort.MANUAL }
    /** How far the item at `key` moves aside for the gap of `d`. */
    fun shift(d: Drag?, key: String): Float {
        val gap = d?.gap?.takeIf { it >= 0 } ?: return 0f
        val (i, from) = (order[key] ?: return 0f) to (order[d.key] ?: return 0f)
        return when {
            gap > from && i > from && i < gap -> -d.size.toFloat()
            gap <= from && i >= gap && i < from -> d.size.toFloat()
            else -> 0f
        }
    }

    /**
     * Off every row (the gap, its own place, past the end): not into the row it left, which
     * would hold on as the finger moved down off it, but below it, in the same gap; or nowhere.
     */
    fun leftRows(d: Drag) {
        val t = d.target?.takeIf { it.mode == 0 } ?: return
        val row = t.row ?: return
        val (ts, ds) = (row.slot ?: return) to (d.row.slot ?: return)
        val ok = core.canDrop(d.row.task, ds, row.task, ts) &&
            !dev.aperte.todav.core.staysPut(d.row.task, ds, row.task, ts, false)
        d.target = if (ok) t.copy(mode = 1) else null
        if (!ok) d.sorted = core.needsManual(d.row.task, ds, row.task, ts)
    }

    /**
     * Where `d` would land with the finger where it is: the item under it as drawn (moved aside
     * or not), by thirds: the top one above it, the bottom one below it, the middle one into it.
     * Over the gap itself nothing changes; an item moving aside moves out from under the finger,
     * so the place holds still. Called as the finger moves (the gesture outlives compositions).
     */
    fun aim(d: Drag) {
        val item = state.layoutInfo.visibleItemsInfo.firstOrNull {
            val top = it.offset + shift(d, it.key as? String ?: "")
            it.key != d.key && d.y >= top && d.y < top + it.size
        } ?: return leftRows(d)
        val key = item.key as? String ?: return
        d.sorted = false
        val (t, g) = when (val what = rows[key]) {
            is dev.aperte.todav.Group -> (if (what.drop) Target(key, null, what, 0) else null) to -1
            is TaskRow -> {
                val ts = what.slot
                val ds = d.row.slot
                val i = order[key]
                if (ts == null || ds == null || i == null) {
                    null to -1
                } else {
                    val at = (d.y - item.offset - shift(d, key)) / item.size
                    val mode = if (at < 0.3f) -1 else if (at > 0.7f) 1 else 0
                    val ok = if (mode == 0) {
                        core.canAdopt(d.row.task, what.task)
                    } else {
                        core.canDrop(d.row.task, ds, what.task, ts) &&
                            !dev.aperte.todav.core.staysPut(d.row.task, ds, what.task, ts, mode < 0)
                    }
                    if (!ok && mode != 0) d.sorted = core.needsManual(d.row.task, ds, what.task, ts)
                    // Into a row of a sorted list: highlighted only, as no row may move there.
                    val gap = when {
                        mode < 0 -> i
                        mode == 0 && sorted -> -1
                        else -> i + 1
                    }
                    if (ok) Target(key, what, null, mode) to gap else null to -1
                }
            }
            else -> null to -1
        }
        d.target = t
        d.gap = g
    }
    val target = drag?.target
    // A tick each time the landing place changes, as the list makes room.
    LaunchedEffect(drag?.gap, target?.key, target?.mode) {
        if (target != null && drag?.landing == false) haptics.performHapticFeedback(HapticFeedbackType.SegmentFrequentTick)
    }
    /** The top of the gap of `d`, in the list's item coordinates. */
    fun gapTop(d: Drag): Float? {
        val keys = order.keys.toList()
        val f = order[d.key] ?: return null
        val g = d.gap.takeIf { it >= 0 } ?: return null
        val visible = state.layoutInfo.visibleItemsInfo
        return if (g > f) {
            visible.firstOrNull { it.key == keys.getOrNull(g - 1) }?.let { (it.offset + it.size - d.size).toFloat() }
        } else {
            visible.firstOrNull { it.key == keys.getOrNull(g) }?.offset?.toFloat()
        }
    }
    // A drop that changes nothing leaves the list as it was: the gap closes after a moment.
    LaunchedEffect(drag?.landing) {
        if (drag?.landing == true) {
            delay(1500)
            drag = null
        }
    }
    // Near the top or the bottom, the list scrolls under a held row.
    LaunchedEffect(drag) {
        while (true) {
            val d = drag ?: break
            if (d.landing) break
            val info = state.layoutInfo
            val edge = 120f
            val step = when {
                d.y < info.viewportStartOffset + padding.calculateTopPadding().value * 2 + edge -> -24f
                d.y > info.viewportEndOffset - info.afterContentPadding - edge -> 24f
                else -> 0f
            }
            if (step != 0f && d.moved) {
                d.y += state.scrollBy(step)
                aim(d)
            }
            delay(16)
        }
    }
    LazyColumn(
        state = state,
        // While a row is held, its moves are the drag's, not a scroll's.
        userScrollEnabled = drag == null,
        contentPadding = PaddingValues(top = padding.calculateTopPadding(), bottom = 88.dp),
        modifier = Modifier.fillMaxSize().pointerInput(screen, showCompleted) {
            detectDragGesturesAfterLongPress(
                onDragStart = { at ->
                    val y = at.y + state.layoutInfo.viewportStartOffset
                    val item = state.layoutInfo.visibleItemsInfo.firstOrNull { y >= it.offset && y < it.offset + it.size }
                    val row = item?.let { rows[it.key] as? TaskRow }
                    if (item != null && row != null) {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        drag = Drag(row, item.key as String, item.size, y - item.offset, y)
                    }
                },
                onDrag = { change, amount ->
                    val d = drag?.takeIf { !it.landing } ?: return@detectDragGesturesAfterLongPress
                    change.consume()
                    d.y += amount.y
                    if (!d.moved && kotlin.math.abs(d.y - (d.grab + (state.layoutInfo.visibleItemsInfo.firstOrNull { it.key == d.key }?.offset ?: 0))) > viewConfiguration.touchSlop) {
                        d.moved = d.row.slot != null
                    }
                    if (d.moved) aim(d)
                },
                onDragEnd = {
                    val d = drag?.takeIf { !it.landing } ?: return@detectDragGesturesAfterLongPress
                    if (!d.moved) {
                        drag = null
                        menuFor = d.key
                        return@detectDragGesturesAfterLongPress
                    }
                    val t = d.target
                    val ds = d.row.slot
                    if (t == null || ds == null) {
                        drag = null
                        return@detectDragGesturesAfterLongPress
                    }
                    // Kept until the list comes back moved: the row glides into its gap meanwhile.
                    d.landing = true
                    gapTop(d)?.let { top ->
                        scope.launch { androidx.compose.animation.core.animate(d.y, top + d.grab) { v, _ -> d.y = v } }
                    }
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
                    val dy = eased(shift(drag, "group:$title"), drag != null)
                    Box(
                        Modifier.graphicsLayer { translationY = dy }
                            .background(if (on) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent),
                    ) { Heading(title) }
                }
            }
            items(group.rows, key = { "${group.title}:${it.task.uid}" }, contentType = { "task" }) { row ->
                val key = "${group.title}:${row.task.uid}"
                TaskItem(model, screen, row, key, drag, target, shift(drag, key), state, menuFor == key, { menuFor = null }, onAddSubtask) { details = it }
            }
        }
        if (screen.completed.isNotEmpty()) {
            item(key = "completed") {
                val dy = eased(shift(drag, "completed"), drag != null)
                Row(
                    Modifier.graphicsLayer { translationY = dy }.fillMaxWidth().clickable { showCompleted = !showCompleted }
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
                    TaskItem(model, screen, row, key, drag, target, shift(drag, key), state, menuFor == key, { menuFor = null }, onAddSubtask) { details = it }
                }
            }
        }
    }
    // Why nothing opens beside a row while the list is sorted (laid over the list: its parent is a Box).
    androidx.compose.animation.AnimatedVisibility(
        visible = drag?.sorted == true,
        modifier = Modifier.fillMaxSize().padding(16.dp).wrapContentHeight(Alignment.Bottom),
        enter = fadeIn(),
        exit = fadeOut(),
    ) {
        Surface(
            shape = RoundedCornerShape(12.dp),
            color = MaterialTheme.colorScheme.inverseSurface,
            contentColor = MaterialTheme.colorScheme.inverseOnSurface,
            shadowElevation = 4.dp,
        ) {
            Text(tr("To reorder, set Sort to Manual"), Modifier.padding(horizontal = 16.dp, vertical = 12.dp))
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
 * list moves it when dragged on); swiped right or left, what Settings say for that way (completed
 * or reopened, starred, pinned, deleted with Undo), or not swiped at all (the default).
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
    /** How far it moves aside for the held row's gap. */
    shift: Float,
    state: LazyListState,
    menu: Boolean,
    onMenuClosed: () -> Unit,
    onAddSubtask: (TodavTask) -> Unit,
    onOpen: (TodavTask) -> Unit,
) {
    val task = row.task
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    var moving by remember { mutableStateOf(false) }
    // Lifted once held (a long press alone does what a tap does), if it can move at all.
    val dragged = drag?.key == key && drag.row.slot != null
    val mode = target?.takeIf { it.key == key }?.mode
    val dy = eased(shift, drag != null)
    // Held: indented as deep as it would land (beside a row: that row's depth; into it: one deeper).
    val depth = target?.takeIf { dragged }?.let { t -> t.row?.let { it.depth + (if (t.mode == 0) 1 else 0) - row.depth } } ?: 0
    val dx by animateFloatAsState(depth * 24f, label = "depth")
    val dismiss = rememberSwipeToDismissBoxState()
    // Each way it is swiped as Settings say, if this task can take that: pinning only top-level ones.
    val usable = { a: SwipeAction -> a != SwipeAction.OFF && (a != SwipeAction.PIN || pinnable(screen, task, row.depth)) }
    SwipeToDismissBox(
        state = dismiss,
        enableDismissFromStartToEnd = usable(screen.swipeRight),
        enableDismissFromEndToStart = usable(screen.swipeLeft),
        gesturesEnabled = drag == null,
        modifier = Modifier
            .zIndex(if (dragged) 1f else 0f)
            .graphicsLayer {
                if (dragged && drag != null) {
                    val now = state.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.offset ?: 0
                    translationY = drag.y - drag.grab - now
                    translationX = dx.dp.toPx()
                    shadowElevation = 8.dp.toPx()
                } else {
                    translationY = dy
                }
            },
        onDismiss = {
            val action = when (it) {
                SwipeToDismissBoxValue.StartToEnd -> screen.swipeRight
                SwipeToDismissBoxValue.EndToStart -> screen.swipeLeft
                SwipeToDismissBoxValue.Settled -> SwipeAction.OFF
            }
            when (action) {
                SwipeAction.COMPLETE -> {
                    haptics.performHapticFeedback(HapticFeedbackType.ToggleOn)
                    model.setDone(task, !task.done)
                }
                SwipeAction.STAR -> model.update(task.uid, TaskPatch(priority = PriorityValue(if (priorityColor(task.priority) == null) 1 else null)))
                SwipeAction.PIN -> model.update(task.uid, TaskPatch(pinned = !task.pinned))
                SwipeAction.DELETE -> model.delete(task)
                SwipeAction.OFF -> {}
            }
            // Deleted, it goes (Undo brings it back); else it slides back into place.
            if (action != SwipeAction.DELETE) scope.launch { dismiss.reset() }
        },
        backgroundContent = {
            val start = dismiss.dismissDirection == SwipeToDismissBoxValue.StartToEnd
            SwipeBackground(if (start) screen.swipeRight else screen.swipeLeft, task, start)
        },
    ) {
        Row(
            Modifier.fillMaxWidth()
                .background(if (mode == 0) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
                .clickable { onOpen(task) }
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
            // Its priority shows; it is set in the options.
            if (screen.stars && priorityColor(task.priority) != null) {
                Box(Modifier.size(18.dp)) { StarIcon(task.priority) }
            }
            // The options, also opened by a long press on the row.
            var options by remember { mutableStateOf(false) }
            Box {
                Tip(tr("More Options")) {
                    IconButton(onClick = { options = true }) { Icon(Icons.Default.MoreVert, tr("More Options")) }
                }
                RowMenu(model, screen, task, row.depth, menu || options, {
                    options = false
                    onMenuClosed()
                }, { onAddSubtask(task) }) { moving = true }
            }
        }
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
            Icon(Icons.AutoMirrored.Outlined.Label, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.secondary)
            Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.secondary, maxLines = 1)
        }
    }
}

/** Under a task swiped (to the right: `start`), what letting go does: its colour and icon. */
@Composable
private fun SwipeBackground(action: SwipeAction, task: TodavTask, start: Boolean) {
    val colors = MaterialTheme.colorScheme
    val (container, content) = when (action) {
        SwipeAction.COMPLETE -> colors.primaryContainer to colors.onPrimaryContainer
        SwipeAction.STAR -> colors.tertiaryContainer to colors.onTertiaryContainer
        SwipeAction.PIN -> colors.secondaryContainer to colors.onSecondaryContainer
        SwipeAction.DELETE -> colors.errorContainer to colors.onErrorContainer
        SwipeAction.OFF -> colors.surface to colors.onSurface
    }
    Box(
        Modifier.fillMaxSize().background(container).padding(horizontal = 24.dp),
        if (start) Alignment.CenterStart else Alignment.CenterEnd,
    ) {
        val icon = when (action) {
            SwipeAction.COMPLETE -> Icons.Default.Check
            SwipeAction.STAR -> if (priorityColor(task.priority) == null) Icons.Filled.Star else Icons.Outlined.StarOutline
            SwipeAction.PIN -> if (task.pinned) Icons.Outlined.PushPin else Icons.Filled.PushPin
            SwipeAction.DELETE -> Icons.Default.Delete
            SwipeAction.OFF -> null
        }
        icon?.let { Icon(it, null, tint = content) }
    }
}

/** Top-level rows of a list can be pinned, as on Linux (with pins shown). */
private fun pinnable(screen: Screen, task: TodavTask, depth: Int) = screen.pins && depth == 0 && task.parentUid == null

/**
 * A row's options: its priority as a row of stars (the current one marked), then Edit, Add
 * Subtask, Pin, Make Top-Level Task, Move to List and Delete.
 */
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
    val actions = listOfNotNull(
        Triple(tr("Edit…"), Icons.Outlined.Edit) { model.open(Dest.Task(task.uid)) },
        Triple(tr("Add Subtask"), Icons.Outlined.SubdirectoryArrowRight, onAddSubtask).takeIf { !task.done },
        Triple(if (task.pinned) tr("Unpin") else tr("Pin"), if (task.pinned) Icons.Filled.PushPin else Icons.Outlined.PushPin) {
            model.update(task.uid, TaskPatch(pinned = !task.pinned))
        }.takeIf { pinnable(screen, task, depth) },
        Triple(tr("Make Top-Level Task"), Icons.AutoMirrored.Outlined.FormatIndentDecrease) {
            model.act {
                model.core.promote(task.uid)
                model.synced()
            }
        }.takeIf { task.parentUid != null },
        Triple(tr("Move to List…"), Icons.AutoMirrored.Outlined.DriveFileMove, onMove),
    )
    val groups = if (screen.stars) 3 else 2
    DropdownMenuPopup(expanded = open, onDismissRequest = onClose) {
        if (screen.stars) {
            DropdownMenuGroup(MenuDefaults.groupShape(0, groups)) {
                MenuDefaults.DropdownMenuGroupLabel { Text(tr("Priority")) }
                Row(Modifier.padding(horizontal = 8.dp)) {
                    PRIORITIES.forEach { p ->
                        Tip(priorityName(p)) {
                            IconToggleButton(
                                priorityIndex(p) == priorityIndex(task.priority),
                                {
                                    onClose()
                                    model.update(task.uid, TaskPatch(priority = PriorityValue(p)))
                                },
                                shapes = IconButtonDefaults.toggleableShapes(),
                                colors = IconButtonDefaults.iconToggleButtonColors(checkedContainerColor = MaterialTheme.colorScheme.secondaryContainer),
                            ) { StarIcon(p) }
                        }
                    }
                }
            }
            Spacer(Modifier.height(MenuDefaults.GroupSpacing))
        }
        DropdownMenuGroup(MenuDefaults.groupShape(groups - 2, groups)) {
            actions.forEachIndexed { i, (label, icon, act) ->
                DropdownMenuItem(
                    onClick = {
                        onClose()
                        act()
                    },
                    text = { Text(label) },
                    shape = MenuDefaults.itemShape(i, actions.size).shape,
                    leadingIcon = { Icon(icon, null) },
                )
            }
        }
        Spacer(Modifier.height(MenuDefaults.GroupSpacing))
        DropdownMenuGroup(MenuDefaults.groupShape(groups - 1, groups)) {
            DropdownMenuItem(
                onClick = {
                    onClose()
                    model.delete(task)
                },
                text = { Text(tr("Delete")) },
                shape = MenuDefaults.itemShape(0, 1).shape,
                leadingIcon = { Icon(Icons.Outlined.Delete, null) },
                colors = MenuDefaults.itemColors(
                    textColor = MaterialTheme.colorScheme.error,
                    leadingIconColor = MaterialTheme.colorScheme.error,
                ),
            )
        }
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
            LazyColumn(Modifier.clip(RoundedCornerShape(24.dp)), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                items(lists, key = { it.href }) { l ->
                    // This package's row, not the core's Item imported here.
                    dev.aperte.todav.ui.Item(
                        headlineContent = { Text(l.displayName) },
                        leadingContent = { ColorMark(dev.aperte.todav.core.listColor(l)) },
                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                        modifier = Modifier.clickable {
                            onDismiss()
                            model.move(task, l.href)
                        },
                    )
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

/**
 * Adding a task (or a subtask of `parent`): its title, and chips for a due date, a category
 * and a priority, as Google Tasks' sheet. Pulled up, it becomes the task page with what was
 * entered, for everything else.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddSheet(model: Main, list: String, parent: TodavTask?, category: String?, initial: String = "", onDismiss: () -> Unit) {
    var text by rememberSaveable { mutableStateOf(initial) }
    var due by rememberSaveable { mutableStateOf<Long?>(null) }
    var cat by rememberSaveable { mutableStateOf(category) }
    var priority by rememberSaveable { mutableStateOf<Long?>(null) }
    var pickDate by remember { mutableStateOf(false) }
    var pickCategory by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val categories = remember(list) { model.core.categories(list).map { it.name } }
    DetailsSheet(
        onDismiss,
        full = { sheet ->
            // What the sheet has as it is pulled up.
            val draft = remember {
                TodavTask(
                    uid = "", listHref = list, summary = text.trim(), description = null, status = "NEEDS-ACTION",
                    done = false, completedAt = null, category = cat, parentUid = parent?.uid, priority = priority,
                    due = due, dueAllDay = true, reminders = due?.let { listOf(it + model.core.defaultReminder(true)) } ?: emptyList(),
                    sortOrder = null, created = null, lastModified = null, start = null, startAllDay = false,
                    eventUid = null, syncError = null, pinned = false,
                )
            }
            TaskPage(model, "", sheet, draft)
        },
    ) { sheet ->
        val add = {
            if (text.isNotBlank()) {
                val patch = TaskPatch(
                    due = due?.let { DueValue(Due(it, true)) },
                    priority = priority?.let { PriorityValue(it) },
                    reminders = due?.let { listOf(model.core.defaultReminder(true)) },
                )
                model.add(list, text, parent?.uid, cat, patch.takeIf { due != null || priority != null })
                text = ""
                sheet.close()
            }
        }
        LaunchedEffect(Unit) { focus.requestFocus() }
        Column(Modifier.padding(horizontal = 16.dp).padding(bottom = 16.dp)) {
            parent?.let {
                Text(
                    tr("Subtask of “{title}”").replace("{title}", it.summary),
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 16.dp, bottom = 8.dp),
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Group(Modifier.weight(1f)) {
                    FieldItem(
                        text,
                        { text = it },
                        null,
                        Modifier.focusRequester(focus),
                        placeholder = if (parent == null) tr("New task") else tr("New subtask"),
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { add() }),
                        singleLine = true,
                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                }
                Tip(tr("Add Task")) {
                    FilledIconButton(onClick = add, enabled = text.isNotBlank(), modifier = Modifier.size(56.dp)) {
                        Icon(Icons.AutoMirrored.Filled.Send, tr("Add Task"))
                    }
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
                            leadingIcon = { Icon(Icons.AutoMirrored.Outlined.Label, null, Modifier.size(18.dp)) },
                        )
                        PickMenu(
                            pickCategory,
                            { pickCategory = false },
                            listOf(ptr("category", "None")) + categories,
                            categories.indexOf(cat) + 1,
                        ) { cat = categories.getOrNull(it - 1) }
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

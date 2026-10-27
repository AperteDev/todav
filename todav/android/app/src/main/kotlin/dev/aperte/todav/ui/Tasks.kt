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
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.IconToggleButton
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.CalendarToday
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material.icons.outlined.FilterList
import androidx.compose.material.icons.outlined.Label
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
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
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MediumFlexibleTopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.SheetValue
import androidx.compose.material3.rememberBottomSheetState
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

/**
 * The task list shown (a list, Upcoming or Important), with its top bar, and the add button
 * when `addButton` (no navigation bar or rail holds it: both panes are shown).
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun TasksPane(model: Main, screen: Screen, addButton: Boolean) {
    val drawer = LocalDrawer.current
    val scope = rememberCoroutineScope()
    val sync by model.syncState.collectAsStateWithLifecycle()
    // The subtask being added to (its parent), from a row's menu.
    var addingTo by remember { mutableStateOf<TodavTask?>(null) }
    var pulling by remember { mutableStateOf(false) }
    LaunchedEffect(sync) { if (sync != SyncState.Syncing) pulling = false }
    val bar = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
    val pull = rememberPullToRefreshState()
    val list = rememberLazyListState()
    // Pull to refresh only from a gesture that starts with the list at its top and the bar
    // expanded: one scrolling up from further down stops there, its rest opening the bar.
    var fromTop by remember { mutableStateOf(true) }

    Scaffold(
        modifier = Modifier.nestedScroll(bar.nestedScrollConnection),
        topBar = {
            MediumFlexibleTopAppBar(
                title = { Text(screen.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                subtitle = { Text(ntr("{n} open", "{n} open", screen.open.toLong()).replace("{n}", "${screen.open}")) },
                scrollBehavior = bar,
                navigationIcon = {
                    Tip(tr("Task lists")) {
                        IconButton(onClick = { scope.launch { drawer?.open() } }) {
                            Icon(Icons.Default.Menu, tr("Task lists"))
                        }
                    }
                },
                actions = {
                    Tip(tr("Search")) {
                        IconButton(onClick = { model.open(Dest.Search) }) { Icon(Icons.Default.Search, tr("Search")) }
                    }
                    screen.list?.takeIf { screen.categories.isNotEmpty() }?.let { list ->
                        CategoryFilter(screen) { model.filterBy(list.href, it) }
                    }
                    if (model.core.canSync()) SyncButton(sync == SyncState.Syncing && !pulling, model::sync)
                },
            )
        },
        floatingActionButton = { if (addButton) AddButton(tr("Add Task"), inBar = false) { newTask(model) } },
    ) { padding ->
        val refreshing = pulling && sync == SyncState.Syncing
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = {
                pulling = true
                model.sync()
            },
            modifier = Modifier.fillMaxSize().pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    fromTop = !list.canScrollBackward && bar.state.collapsedFraction == 0f
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
     * Where `d` would land with the finger where it is: the item under it as drawn (moved aside
     * or not), by thirds: the top one above it, the bottom one below it, the middle one into it.
     * Over the gap itself nothing changes; an item moving aside moves out from under the finger,
     * so the place holds still. Called as the finger moves (the gesture outlives compositions).
     */
    fun aim(d: Drag) {
        val item = state.layoutInfo.visibleItemsInfo.firstOrNull {
            val top = it.offset + shift(d, it.key as? String ?: "")
            it.key != d.key && d.y >= top && d.y < top + it.size
        } ?: return
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
                    if (ok) Target(key, what, null, mode) to (if (mode < 0) i else i + 1) else null to -1
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
            Text(tr("To reorder, set Sort to Manual in Settings"), Modifier.padding(horizontal = 16.dp, vertical = 12.dp))
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
    val dragged = drag?.key == key && drag.moved
    val mode = target?.takeIf { it.key == key }?.mode
    val dy = eased(shift, drag != null)
    // Held: indented as deep as it would land (beside a row: that row's depth; into it: one deeper).
    val depth = target?.takeIf { dragged }?.let { t -> t.row?.let { it.depth + (if (t.mode == 0) 1 else 0) - row.depth } } ?: 0
    val dx by animateFloatAsState(depth * 24f, label = "depth")
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
                    translationX = dx.dp.toPx()
                    shadowElevation = 8.dp.toPx()
                } else {
                    translationY = dy
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
            Icon(Icons.Outlined.Label, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.secondary)
            Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.secondary, maxLines = 1)
        }
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
    DropdownMenu(expanded = open, onDismissRequest = onClose) {
        if (screen.stars) {
            Text(
                tr("Priority"),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 16.dp, top = 4.dp),
            )
            Row(Modifier.padding(horizontal = 8.dp)) {
                PRIORITIES.forEach { p ->
                    Tip(priorityName(p)) {
                        IconToggleButton(
                            priorityIndex(p) == priorityIndex(task.priority),
                            {
                                onClose()
                                model.update(task.uid, TaskPatch(priority = PriorityValue(p)))
                            },
                            colors = IconButtonDefaults.iconToggleButtonColors(checkedContainerColor = MaterialTheme.colorScheme.surfaceContainerHighest),
                        ) { StarIcon(p) }
                    }
                }
            }
            HorizontalDivider(Modifier.padding(vertical = 4.dp))
        }
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
        if (pinnable(screen, task, depth)) {
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
    // Open from the start rather than sliding up: it rises with the keyboard, in one movement
    // instead of the keyboard and then the sheet.
    val sheet = rememberBottomSheetState(initialValue = SheetValue.Expanded, enabledValues = setOf(SheetValue.Hidden, SheetValue.Expanded))
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

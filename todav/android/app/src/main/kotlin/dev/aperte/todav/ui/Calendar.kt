package dev.aperte.todav.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PageSize
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.CalendarViewDay
import androidx.compose.material.icons.outlined.CalendarViewMonth
import androidx.compose.material.icons.outlined.Inbox
import androidx.compose.material.icons.outlined.CalendarViewWeek
import androidx.compose.material.icons.outlined.Today
import androidx.compose.material.icons.outlined.ViewAgenda
import androidx.compose.material.icons.outlined.ViewColumn
import androidx.compose.material.icons.outlined.ViewWeek
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Surface
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import dev.aperte.todav.core.TodavList
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.scrollable
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Density
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionOnScreen
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.CalMode
import dev.aperte.todav.Dest
import dev.aperte.todav.Main
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.zIndex
import dev.aperte.todav.core.Change
import dev.aperte.todav.core.EventPatch
import dev.aperte.todav.core.AgendaEntry
import dev.aperte.todav.core.BlockColumn
import dev.aperte.todav.core.DayBlock
import dev.aperte.todav.core.Due
import dev.aperte.todav.core.DueValue
import dev.aperte.todav.core.Event
import dev.aperte.todav.core.Item
import dev.aperte.todav.core.Scope
import dev.aperte.todav.core.TaskPatch
import dev.aperte.todav.core.TextValue
import dev.aperte.todav.core.Time
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.columns
import dev.aperte.todav.core.daysOfWeek
import dev.aperte.todav.core.midnight
import dev.aperte.todav.core.monthFirst
import dev.aperte.todav.core.onDay
import dev.aperte.todav.core.taskWhen
import dev.aperte.todav.core.weekStart
import dev.aperte.todav.date
import dev.aperte.todav.dayLabel
import dev.aperte.todav.fields
import dev.aperte.todav.format
import dev.aperte.todav.hourLabel
import dev.aperte.todav.ntr
import dev.aperte.todav.time
import dev.aperte.todav.ptr
import dev.aperte.todav.tr
import java.time.LocalDate
import java.time.temporal.WeekFields
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Read from the core off the main thread, again whenever one of `keys` changes. */
@Composable
fun <T> load(vararg keys: Any?, initial: T, f: () -> T): T =
    produceState(initial, *keys) { value = withContext(Dispatchers.IO) { f() } }.value

/** The locale's first weekday, as the core counts them (0 Sunday). */
fun firstWeekday(): Long = (WeekFields.of(Locale.getDefault()).firstDayOfWeek.value % 7).toLong()

private const val PAGES = 2400
private const val MIDDLE = PAGES / 2

fun Item.title(): String = when (this) {
    is Item.Event -> event.summary
    is Item.Task -> task.summary
}

fun Item.list(): String = when (this) {
    is Item.Event -> event.listHref
    is Item.Task -> task.listHref
}

fun Item.done(): Boolean = this is Item.Task && task.done

/** When an event happens, in words: its days, or its day and times. */
fun eventWhen(e: Event, now: Long): String {
    if (e.allDay) {
        val last = e.end - DAY
        return if (last <= e.start) dayLabel(e.start, true, now) else "${date(e.start)} – ${date(last)}"
    }
    return if (midnight(e.start) == midnight(e.end - 1)) {
        "${dayLabel(e.start, true, now)}, ${time(e.start)} – ${time(e.end)}"
    } else {
        "${dayLabel(e.start, false, now)} – ${dayLabel(e.end, false, now)}"
    }
}

/**
 * The calendar: Month, Week or Agenda, as the Linux app's, with Today and the view menu, and the
 * button adding an event when `addButton` (both panes shown). Months and weeks change with a swipe.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CalendarPane(model: Main, addButton: Boolean) {
    val drawer = LocalDrawer.current
    val scope = rememberCoroutineScope()
    val mode by model.calMode.collectAsStateWithLifecycle()
    val picked by model.day.collectAsStateWithLifecycle()
    val now = remember { model.core.localNow() }
    val day = picked ?: midnight(now)
    var details by remember { mutableStateOf<Item?>(null) }
    var viewMenu by remember { mutableStateOf(false) }
    var toPlan by remember { mutableStateOf(false) }
    val planning by model.planning.collectAsStateWithLifecycle()
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    // Changed on another device and synced.
    LaunchedEffect(changes) { model.workDays.value = withContext(Dispatchers.IO) { model.core.workDays() } }
    // Each list's colour, once for all pages: a page coming into view has them at once.
    val colors = load(changes, initial = emptyMap<String, String>()) { model.core.colors() }
    val title = when (mode) {
        CalMode.AGENDA -> tr("Agenda")
        else -> format(day, "LLLL yyyy").replaceFirstChar { it.titlecase() }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    Tip(tr("Calendars")) {
                        IconButton(onClick = { scope.launch { drawer?.open() } }) { Icon(Icons.Default.Menu, tr("Calendars")) }
                    }
                },
                actions = {
                    Tip(tr("Today")) {
                        IconButton(onClick = { model.pick(midnight(model.core.localNow())) }) {
                            Icon(Icons.Outlined.Today, tr("Today"))
                        }
                    }
                    if (mode.hours()) {
                        Tip(tr("To Plan")) {
                            IconButton(onClick = { toPlan = true }) { Icon(Icons.Outlined.Inbox, tr("To Plan")) }
                        }
                    }
                    Tip(tr("Search")) {
                        IconButton(onClick = { model.open(Dest.Search) }) { Icon(Icons.Default.Search, tr("Search")) }
                    }
                    Box {
                        Tip(tr("View")) {
                            IconButton(onClick = { viewMenu = true }) { Icon(modeIcon(mode), tr("View")) }
                        }
                        DropdownMenu(expanded = viewMenu, onDismissRequest = { viewMenu = false }) {
                            for (m in CalMode.MENU) {
                                DropdownMenuItem(
                                    text = { Text(modeName(m)) },
                                    leadingIcon = { Icon(modeIcon(m), null) },
                                    trailingIcon = { if (m == mode) Icon(Icons.Default.Check, null) },
                                    onClick = {
                                        viewMenu = false
                                        model.setMode(m)
                                    },
                                )
                            }
                        }
                    }
                },
            )
        },
        floatingActionButton = { if (addButton) AddButton(tr("New Event"), inBar = false) { newEvent(model) } },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            planning?.takeIf { mode.hours() }?.let { t ->
                Row(
                    Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.secondaryContainer).padding(start = 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        tr("Tap an hour to plan “{title}”").replace("{title}", t.summary),
                        color = MaterialTheme.colorScheme.onSecondaryContainer,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = { model.planning.value = null }) { Text(tr("Cancel")) }
                }
            }
            Box(Modifier.fillMaxSize()) {
                when (mode) {
                    CalMode.MONTH -> MonthView(model, day, colors) { details = it }
                    CalMode.WEEK -> WeekView(model, day, colors, work = false) { details = it }
                    CalMode.WORK_WEEK -> WeekView(model, day, colors, work = true) { details = it }
                    CalMode.THREE_DAYS -> DaysView(model, day, colors, 3) { details = it }
                    CalMode.DAY -> DaysView(model, day, colors, 1) { details = it }
                    CalMode.AGENDA -> AgendaView(model, day) { details = it }
                }
            }
        }
    }
    details?.let { ItemSheet(model, it) { details = null } }
    if (toPlan) {
        // Due by the end of what is shown.
        val until = when (mode) {
            CalMode.THREE_DAYS -> day + 3 * DAY
            CalMode.DAY -> day + DAY
            else -> weekStart(day, firstWeekday()) + 7 * DAY
        }
        ToPlanSheet(model, until, colors) { toPlan = false }
    }
}

/** Whether the view shows hours (a week, some days or one). */
private fun CalMode.hours() = this != CalMode.MONTH && this != CalMode.AGENDA

/** A weekday's name (0 Sunday), `pattern` "EEEE" whole, "EEE" short. */
private fun weekdayName(weekday: Long, pattern: String) =
    // 1970-01-04 was a Sunday.
    format((3 + weekday) * DAY, pattern).replaceFirstChar { it.titlecase() }

/** The Work Week view's days, shortly: "Mon, Tue, Wed, Thu, Fri". */
fun workDaysText(days: List<Long>): String {
    val first = firstWeekday()
    return (0 until 7L).map { (first + it) % 7 }.filter { it in days }.joinToString(", ") { weekdayName(it, "EEE") }
}

/** Pick the Work Week view's days, one to six. */
@Composable
fun WorkDaysDialog(model: Main, onDismiss: () -> Unit) {
    val days by model.workDays.collectAsStateWithLifecycle()
    val first = firstWeekday()
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Work Week Days")) },
        text = {
            Column {
                Text(tr("The days the Work Week view shows: one to six."), Modifier.padding(bottom = 8.dp))
                for (i in 0 until 7L) {
                    val d = (first + i) % 7
                    val on = d in days
                    // The last day left can't be turned off, nor a seventh on.
                    val enabled = if (on) days.size > 1 else days.size < 6
                    Row(
                        Modifier.fillMaxWidth().toggleable(on, enabled, Role.Checkbox) {
                            model.setWorkDays(if (on) days - d else (days + d).sorted())
                        },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(on, null, enabled = enabled)
                        Text(weekdayName(d, "EEEE"), Modifier.padding(start = 8.dp))
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Close")) } },
    )
}

/** A new event on the day the calendar shows (the add button, the shortcut). */
fun newEvent(model: Main) {
    val day = model.day.value ?: midnight(model.core.localNow())
    model.open(Dest.EventEdit(null, null, null, newStart(model, day), false))
}

/** A new event's start: the next full hour on `day` when it is today, else 9:00 there. */
private fun newStart(model: Main, day: Long): Long {
    val now = model.core.localNow()
    return if (midnight(now) == day) now - Math.floorMod(now, 3600L) + 3600 else day + 9 * 3600
}

private fun modeName(m: CalMode) = when (m) {
    CalMode.MONTH -> tr("Month")
    CalMode.WEEK -> tr("Week")
    CalMode.WORK_WEEK -> tr("Work Week")
    CalMode.THREE_DAYS -> tr("3 Days")
    CalMode.DAY -> tr("Day")
    CalMode.AGENDA -> tr("Agenda")
}

private fun modeIcon(m: CalMode) = when (m) {
    CalMode.MONTH -> Icons.Outlined.CalendarViewMonth
    CalMode.WEEK -> Icons.Outlined.CalendarViewWeek
    CalMode.WORK_WEEK -> Icons.Outlined.ViewWeek
    CalMode.THREE_DAYS -> Icons.Outlined.ViewColumn
    CalMode.DAY -> Icons.Outlined.CalendarViewDay
    CalMode.AGENDA -> Icons.Outlined.ViewAgenda
}

/** The month of page `page` around the month of `day`. */
private fun monthOf(day: Long, page: Int): Long {
    val d = LocalDate.ofEpochDay(day / DAY).withDayOfMonth(1).plusMonths((page - MIDDLE).toLong())
    return d.toEpochDay() * DAY
}

// --- month ----------------------------------------------------------------------------------

/**
 * Six weeks around the month, each day with what is on it (three at most, then "+n"); the day
 * picked lists its events and tasks underneath. Swiped, the next or previous month.
 */
@Composable
private fun MonthView(model: Main, day: Long, colors: Map<String, String>, onItem: (Item) -> Unit) {
    val browsed by model.browsed.collectAsStateWithLifecycle()
    // Pages count from the month shown when the view opened; picking a day elsewhere moves there.
    val base = remember { monthFirst(day) }
    val pager = rememberPagerState(initialPage = MIDDLE + monthsBetween(base, day)) { PAGES }
    LaunchedEffect(day) {
        val target = MIDDLE + monthsBetween(base, day)
        if (pager.currentPage != target) pager.scrollToPage(target)
    }
    LaunchedEffect(pager) {
        snapshotFlow { pager.settledPage }.collect { page ->
            val month = monthOf(base, page)
            if (monthFirst(model.day.value ?: day) != month) {
                // Today when it is in that month, else its first day.
                val today = midnight(model.core.localNow())
                val to = if (monthFirst(today) == month) today else month
                model.browsed.value = to
                model.day.value = to
            }
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth > maxHeight
        val grid: @Composable (Modifier) -> Unit = { m ->
            // The months either side are made ready beforehand, so a swipe finds them filled in.
            HorizontalPager(pager, m, beyondViewportPageCount = 1, key = { it }) { page ->
                MonthGrid(model, monthOf(base, page), day.takeIf { it != browsed }, colors, onItem)
            }
        }
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                grid(Modifier.weight(1.4f).fillMaxHeight())
                DayList(model, day, onItem, Modifier.weight(1f))
            }
        } else {
            Column(Modifier.fillMaxSize()) {
                grid(Modifier.fillMaxWidth().weight(1.5f))
                HorizontalDivider()
                DayList(model, day, onItem, Modifier.weight(1f))
            }
        }
    }
}

private fun monthsBetween(a: Long, b: Long): Int {
    val x = LocalDate.ofEpochDay(a / DAY)
    val y = LocalDate.ofEpochDay(b / DAY)
    return (y.year - x.year) * 12 + (y.monthValue - x.monthValue)
}

@Composable
private fun MonthGrid(model: Main, month: Long, picked: Long?, colors: Map<String, String>, onItem: (Item) -> Unit) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val start = weekStart(month, firstWeekday())
    // What is on each of the 42 days, sorted out off the main thread: `onDay` is a call into the core.
    val days = load(changes, start, initial = List(42) { emptyList<Item>() }) {
        val items = model.core.items(start, start + 42 * DAY)
        List(42) { n -> items.filter { onDay(it, start + n * DAY) } }
    }
    val today = midnight(model.core.localNow())
    val thisMonth = fields(month).monthValue
    // A chip held and moved: the day under it, and the drop to apply.
    var target by remember { mutableStateOf<Long?>(null) }
    var moved by remember { mutableStateOf<Pair<Item, Change>?>(null) }
    moved?.let { (item, change) -> Reschedule(model, item, change) { moved = null } }
    Column(Modifier.fillMaxSize().padding(horizontal = 4.dp)) {
        WeekdayNames(start)
        for (w in 0 until 6) {
            Row(Modifier.fillMaxWidth().weight(1f)) {
                for (d in 0 until 7) {
                    val at = start + (w * 7 + d) * DAY
                    val here = days[w * 7 + d]
                    DayCell(
                        at, here, colors, at == today, at == picked || at == target, fields(at).monthValue == thisMonth,
                        Modifier.weight(1f).fillMaxHeight().pointerInput(at, here) {
                            // Held on a chip and moved: to the day it is let go over.
                            var held: Item? = null
                            var total = Offset.Zero
                            detectDragGesturesAfterLongPress(
                                onDragStart = { p ->
                                    held = here.getOrNull(((p.y - 26.dp.toPx()) / 15.dp.toPx()).toInt())
                                    total = Offset.Zero
                                    target = held?.let { at }
                                },
                                onDrag = { change, amount ->
                                    if (held != null) {
                                        change.consume()
                                        total += amount
                                        val rows = Math.round(total.y / size.height)
                                        val cols = Math.round(total.x / size.width)
                                        target = at + (rows * 7L + cols) * DAY
                                    }
                                },
                                onDragEnd = {
                                    val (item, day) = held to target
                                    target = null
                                    if (item != null && day != null && day != at) moved = item to dev.aperte.todav.core.dayDrop(item, day, false)
                                },
                                onDragCancel = { target = null },
                            )
                        },
                    ) { model.pick(at) }
                }
            }
        }
    }
}

/** The first letters of the weekdays of the week from `start`, over the month grid's columns. */
@Composable
private fun WeekdayNames(start: Long) {
    Row(Modifier.fillMaxWidth()) {
        for (d in 0 until 7) {
            Text(
                format(start + d * DAY, "EEEEE"),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.weight(1f).padding(vertical = 4.dp),
            )
        }
    }
}

/** The Day view's week, a row of the month grid: the day shown picked, a tap shows another. */
@Composable
private fun WeekStrip(model: Main, day: Long, colors: Map<String, String>) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val start = weekStart(day, firstWeekday())
    val days = load(changes, start, initial = List(7) { emptyList<Item>() }) {
        val items = model.core.items(start, start + 7 * DAY)
        List(7) { n -> items.filter { onDay(it, start + n * DAY) } }
    }
    val today = midnight(model.core.localNow())
    Column(Modifier.fillMaxWidth().padding(horizontal = 4.dp).padding(bottom = 4.dp)) {
        WeekdayNames(start)
        Row(Modifier.fillMaxWidth().height(76.dp)) {
            for (d in 0 until 7) {
                val at = start + d * DAY
                DayCell(at, days[d], colors, at == today, at == day, true, Modifier.weight(1f).fillMaxHeight()) { model.pick(at) }
            }
        }
    }
    HorizontalDivider()
}

@Composable
private fun DayCell(
    day: Long,
    items: List<Item>,
    colors: Map<String, String>,
    today: Boolean,
    picked: Boolean,
    inMonth: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val outline = MaterialTheme.colorScheme.outlineVariant
    Column(
        modifier.clip(RoundedCornerShape(8.dp))
            .background(if (picked) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent)
            .border(0.5.dp, outline.copy(alpha = 0.4f), RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(2.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier.size(22.dp).background(if (today) MaterialTheme.colorScheme.primary else Color.Transparent, CircleShape),
            Alignment.Center,
        ) {
            Text(
                "${fields(day).dayOfMonth}",
                style = MaterialTheme.typography.labelMedium,
                color = when {
                    today -> MaterialTheme.colorScheme.onPrimary
                    inMonth -> MaterialTheme.colorScheme.onSurface
                    else -> MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)
                },
            )
        }
        BoxWithConstraints(Modifier.fillMaxWidth().weight(1f)) {
            // As many chips as fit; the rest as "+n".
            val fit = ((maxHeight.value - 2) / 15).toInt().coerceAtLeast(0)
            val shown = if (items.size <= fit) items else items.take((fit - 1).coerceAtLeast(0))
            Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
                shown.forEach { i -> Chip(i, colors) }
                if (shown.size < items.size) {
                    Text(
                        "+${items.size - shown.size}",
                        fontSize = 10.sp,
                        lineHeight = 12.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

/** An item in its list's colour: a small one-line chip, or `large`, a row to tap. */
@Composable
private fun Chip(item: Item, colors: Map<String, String>, large: Boolean = false, onClick: (() -> Unit)? = null) {
    val color = parseColor(colors[item.list()])
    Text(
        item.title(),
        fontSize = if (large) 14.sp else 10.sp,
        lineHeight = if (large) 20.sp else 13.sp,
        maxLines = if (large) 2 else 1,
        overflow = if (large) TextOverflow.Ellipsis else TextOverflow.Clip,
        color = if (item is Item.Event) Color.White else MaterialTheme.colorScheme.onSurface,
        textDecoration = if (item.done()) TextDecoration.LineThrough else null,
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(if (large) 12.dp else 3.dp))
            .background(if (item is Item.Event) color else color.copy(alpha = 0.25f))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = if (large) 12.dp else 2.dp, vertical = if (large) 14.dp else 0.dp),
    )
}

/**
 * A day's items above the hours, as many as fit (then "+n"). Chips this small are hard to hit, so
 * the whole day is one target: a tap opens its only item, or shows them all larger to pick from.
 */
@Composable
private fun TopCell(day: Long, now: Long, items: List<Item>, colors: Map<String, String>, onItem: (Item) -> Unit, modifier: Modifier) {
    var open by remember { mutableStateOf(false) }
    var at by remember { mutableStateOf(Offset.Zero) }
    val chip = with(LocalDensity.current) { 13.sp.toDp() } + 1.dp
    BoxWithConstraints(
        modifier.onGloballyPositioned { at = it.positionOnScreen() }
            .clickable(enabled = items.isNotEmpty()) { if (items.size == 1) onItem(items[0]) else open = true }.padding(1.dp),
    ) {
        val fit = ((maxHeight + 1.dp) / chip).toInt().coerceAtLeast(1)
        val shown = if (items.size <= fit) items else items.take(fit - 1)
        Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
            shown.forEach { Chip(it, colors) }
            if (shown.size < items.size) {
                Text("+${items.size - shown.size}", fontSize = 10.sp, lineHeight = 13.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    if (open) DayItems(day, now, at, items, colors, onDismiss = { open = false }, onPick = onItem)
}

/** A popup over the whole window (its content places itself). */
private object Whole : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize) =
        IntOffset.Zero
}

/**
 * All of `day`'s `items` as large rows over the dimmed screen, growing out of the day above the
 * hours (`at`, on the screen); a tap picks one, a tap outside closes it.
 */
@Composable
private fun DayItems(day: Long, now: Long, at: Offset, items: List<Item>, colors: Map<String, String>, onDismiss: () -> Unit, onPick: (Item) -> Unit) {
    val shown = remember { MutableTransitionState(false).apply { targetState = true } }
    // Gone once it has shrunk away.
    if (!shown.targetState && shown.isIdle) LaunchedEffect(Unit) { onDismiss() }
    // Where the popup is on the screen: it may start below the status bar.
    var origin by remember { mutableStateOf(Offset.Zero) }
    Popup(Whole, onDismissRequest = { shown.targetState = false }, properties = PopupProperties(focusable = true)) {
        BoxWithConstraints(Modifier.fillMaxSize().onGloballyPositioned { origin = it.positionOnScreen() }) {
            AnimatedVisibility(shown, enter = fadeIn(), exit = fadeOut()) {
                Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.32f)).pointerInput(Unit) { detectTapGestures { shown.targetState = false } })
            }
            val width = 280.dp
            val room = with(LocalDensity.current) { (constraints.maxWidth - width.roundToPx()).coerceAtLeast(0) }
            AnimatedVisibility(
                shown,
                Modifier.offset { IntOffset((at.x - origin.x).toInt().coerceIn(0, room), (at.y - origin.y).toInt().coerceAtLeast(0)) },
                enter = fadeIn() + expandVertically(expandFrom = Alignment.Top),
                exit = fadeOut() + shrinkVertically(shrinkTowards = Alignment.Top),
            ) {
                Surface(
                    Modifier.width(width).heightIn(max = 440.dp),
                    shape = RoundedCornerShape(24.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerHigh,
                    shadowElevation = 6.dp,
                ) {
                    Column(Modifier.verticalScroll(rememberScrollState()).padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(
                            dayLabel(day, true, now).replaceFirstChar { it.titlecase() },
                            style = MaterialTheme.typography.titleSmall,
                            modifier = Modifier.padding(start = 4.dp, bottom = 4.dp),
                        )
                        items.forEach { item ->
                            Chip(item, colors, large = true) {
                                shown.targetState = false
                                onPick(item)
                            }
                        }
                    }
                }
            }
        }
    }
}

/** What is on `day`, as agenda rows, with a heading. */
@Composable
private fun DayList(model: Main, day: Long, onItem: (Item) -> Unit, modifier: Modifier) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val entries = load(changes, day, initial = emptyList<AgendaEntry>()) { model.core.agenda(day, 1) }
    val now = model.core.localNow()
    LazyColumn(modifier.fillMaxWidth(), contentPadding = PaddingValues(bottom = 88.dp)) {
        item { Heading(dayLabel(day, true, now).replaceFirstChar { it.titlecase() }) }
        if (entries.isEmpty()) {
            item {
                Text(
                    tr("Nothing on this day"),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp),
                )
            }
        }
        items(entries, key = { "${it.item.list()}:${it.item.title()}:${(it.item as? Item.Event)?.event?.start ?: 0}:${it.time}" }) {
            AgendaRow(model, it, onItem)
        }
    }
}

// --- agenda ---------------------------------------------------------------------------------

/** The coming 60 days from `day`, day by day, as the Linux app's agenda. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun AgendaView(model: Main, day: Long, onItem: (Item) -> Unit) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val entries = load(changes, day, initial = emptyList<AgendaEntry>()) { model.core.agenda(day, 60) }
    val now = model.core.localNow()
    val state = rememberLazyListState()
    LaunchedEffect(day) { state.scrollToItem(0) }
    if (entries.isEmpty()) {
        Box(Modifier.fillMaxSize(), Alignment.Center) {
            Text(tr("Nothing scheduled"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }
    LazyColumn(Modifier.fillMaxSize(), state, contentPadding = PaddingValues(bottom = 88.dp)) {
        entries.groupBy { it.day }.forEach { (d, es) ->
            stickyHeader(key = "d$d") { Heading(dayLabel(d, true, now).replaceFirstChar { it.titlecase() }) }
            items(es, key = { "$d:${it.item.list()}:${it.item.title()}:${it.time}:${(it.item as? Item.Event)?.event?.start}" }) {
                AgendaRow(model, it, onItem)
            }
        }
    }
}

@Composable
private fun AgendaRow(model: Main, e: AgendaEntry, onItem: (Item) -> Unit) {
    val item = e.item
    Row(
        Modifier.fillMaxWidth().clickable { onItem(item) }.padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            when (val t = e.time) {
                Time.AllDay -> tr("All day")
                Time.Date -> ""
                is Time.At -> time(t.time)
                is Time.Until -> tr("Until {time}").replace("{time}", time(t.end))
            },
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.width(64.dp),
        )
        if (item is Item.Task) {
            Checkbox(
                checked = item.task.done,
                onCheckedChange = { model.setDone(item.task, it) },
                modifier = Modifier.size(24.dp),
            )
        } else {
            Box(Modifier.size(12.dp).background(parseColor(e.color), CircleShape))
        }
        Column(Modifier.weight(1f)) {
            Text(
                item.title(),
                textDecoration = if (item.done()) TextDecoration.LineThrough else null,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            (item as? Item.Event)?.event?.location?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
            }
        }
    }
}

// --- week -----------------------------------------------------------------------------------

private val HOUR: Dp = 48.dp

/** Width of the hour labels; height of the day names over the hours. */
private val GUTTER: Dp = 40.dp
private val HEAD: Dp = 52.dp

/**
 * The week holding `day`, or its work days (`work`): an hour grid with the timed events and
 * planned tasks (overlapping ones side by side), what takes no hours in a row above it, and a
 * line at the current time. Tap an empty hour to add an event there; swipe for other weeks.
 */
@Composable
private fun WeekView(model: Main, day: Long, colors: Map<String, String>, work: Boolean, onItem: (Item) -> Unit) {
    val browsed by model.browsed.collectAsStateWithLifecycle()
    val workDays by model.workDays.collectAsStateWithLifecycle()
    val first = firstWeekday()
    val base = remember { weekStart(day, first) }
    // The columns, days from the week's start: the same in every week.
    val cols = remember(work, workDays, base) {
        if (work) daysOfWeek(base, first, workDays).map { ((it - base) / DAY).toInt() } else (0 until 7).toList()
    }
    val pager = rememberPagerState(initialPage = MIDDLE + ((weekStart(day, first) - base) / (7 * DAY)).toInt()) { PAGES }
    LaunchedEffect(day) {
        val target = MIDDLE + ((weekStart(day, first) - base) / (7 * DAY)).toInt()
        if (pager.currentPage != target) pager.scrollToPage(target)
    }
    LaunchedEffect(pager) {
        snapshotFlow { pager.settledPage }.collect { page ->
            val start = base + (page - MIDDLE) * 7 * DAY
            val now = model.day.value ?: day
            if (weekStart(now, first) != start) {
                val today = midnight(model.core.localNow())
                val to = if (weekStart(today, first) == start) today else start
                model.browsed.value = to
                model.day.value = to
            }
        }
    }
    val scroll = rememberScrollState()
    LaunchedEffect(Unit) { scroll.scrollTo((HOUR.value * 7.5f * 2.6f).toInt()) }
    // The weeks either side are made ready beforehand, so a swipe finds them filled in.
    HorizontalPager(pager, Modifier.fillMaxSize(), beyondViewportPageCount = 1, key = { it }) { page ->
        Week(model, base + (page - MIDDLE) * 7 * DAY, cols, day.takeIf { it != browsed }, colors, scroll, onItem)
    }
}

/**
 * `count` days from `day` (three, or one under its week from the month grid), their hours as the
 * week's. Swiped, they move a day at a time, any days next to each other; the first shown is the
 * day picked.
 */
@Composable
private fun DaysView(model: Main, day: Long, colors: Map<String, String>, count: Int, onItem: (Item) -> Unit) {
    // A page a day, `count` of them on the screen.
    val base = remember { day }
    val pager = rememberPagerState(initialPage = MIDDLE + ((day - base) / DAY).toInt()) { PAGES }
    LaunchedEffect(day) {
        val target = MIDDLE + ((day - base) / DAY).toInt()
        if (pager.currentPage != target) pager.scrollToPage(target)
    }
    LaunchedEffect(pager) {
        snapshotFlow { pager.settledPage }.collect { page ->
            val first = base + (page - MIDDLE) * DAY
            if (first != (model.day.value ?: day)) model.pick(first)
        }
    }
    val scroll = rememberScrollState()
    LaunchedEffect(Unit) { scroll.scrollTo((HOUR.value * 7.5f * 2.6f).toInt()) }
    // The row above the hours is as tall on every day shown, so their hours line up: as the
    // fullest of them needs.
    val tops = remember { mutableStateMapOf<Int, Int>() }
    val shown by remember { derivedStateOf { pager.layoutInfo.visiblePagesInfo.map { it.index } } }
    val top = topHeight(shown.maxOfOrNull { tops[it] ?: 0 } ?: 0)
    Column(Modifier.fillMaxSize()) {
        if (count == 1) WeekStrip(model, day, colors)
        Row(Modifier.fillMaxSize()) {
            // The hour labels stay put while the days move.
            Column(Modifier.width(GUTTER).fillMaxHeight()) {
                Spacer(Modifier.height((if (count > 1) HEAD else 0.dp) + top + 1.dp))
                Column(Modifier.fillMaxSize().verticalScroll(scroll)) {
                    for (h in 0 until 24) Box(Modifier.height(HOUR)) { HourLabel(h) }
                }
            }
            BoxWithConstraints(Modifier.weight(1f).fillMaxHeight()) {
                HorizontalPager(
                    pager,
                    Modifier.fillMaxSize(),
                    pageSize = PageSize.Fixed(maxWidth / count),
                    beyondViewportPageCount = 1,
                    key = { it },
                ) { page ->
                    Week(
                        model, base + (page - MIDDLE) * DAY, listOf(0), null, colors, scroll, onItem,
                        header = count > 1, top = top, onTop = { tops[page] = it },
                    )
                }
            }
        }
    }
}

/**
 * A day page's hours scrolled with `state` as `verticalScroll` would, but clipped only above and
 * below, so a held block can be drawn over the next days' pages. How far it goes is set by the
 * hour labels' `verticalScroll` on the same state.
 */
private fun Modifier.hoursScroll(state: androidx.compose.foundation.ScrollState) = this
    // A clipping layer, which taps respect too: the hours scrolled up stay out of the row above.
    .graphicsLayer {
        clip = true
        shape = TopAndBottom
    }
    .scrollable(state, Orientation.Vertical, reverseDirection = true)
    .layout { measurable, constraints ->
        val p = measurable.measure(constraints.copy(maxHeight = Constraints.Infinity))
        layout(p.width, constraints.maxHeight) { p.place(0, -state.value) }
    }

/** Its bounds above and below; far past them at the sides. */
private object TopAndBottom : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density) =
        Outline.Rectangle(Rect(-size.width * 8, 0f, size.width * 9, size.height))
}

/** Height of the row above the hours holding `n` chips a day. */
private fun topHeight(n: Int): Dp = if (n == 0) 0.dp else (n * 14 + 2).dp.coerceAtMost(96.dp)

/** Hour `h`'s label, at the line starting it (none at midnight). */
@Composable
private fun HourLabel(h: Int) {
    Text(
        if (h == 0) "" else hourLabel(h),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.width(GUTTER).offset(y = (-7).dp).padding(end = 4.dp),
        textAlign = TextAlign.End,
    )
}

/** A block in the week's hours: the item's index, its piece on that day, and its column there. */
private class Block(val item: Int, val piece: dev.aperte.todav.core.Segment, val column: BlockColumn)

/** A week's items, each day's blocks in the hours, and what is on each day without hours. */
private class WeekItems(val items: List<Item>, val blocks: List<List<Block>>, val top: List<List<Item>>)

/**
 * The hours of the days `cols` (days from `start`, at most a week on), a column each. With
 * `top`, the row above the hours is that tall, and `onTop` learns how many chips a day it holds
 * at most; without the gutter, the day names are `HEAD` tall: the caller lines the hour labels
 * up with that.
 */
@Composable
private fun Week(
    model: Main,
    start: Long,
    cols: List<Int>,
    picked: Long?,
    colors: Map<String, String>,
    scroll: androidx.compose.foundation.ScrollState,
    onItem: (Item) -> Unit,
    header: Boolean = true,
    top: Dp? = null,
    onTop: ((Int) -> Unit)? = null,
) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    // The hour labels: here, or beside all the days (`top` given).
    val gutter = top == null
    val inset = if (gutter) GUTTER else 0.dp
    // All read and laid out at once off the main thread: `hours`, `segments`, `columns` and `onDay`
    // call into the core, too slow for every frame of a drag.
    val none = WeekItems(emptyList(), cols.map { emptyList() }, cols.map { emptyList() })
    val week = load(changes, start, cols, initial = none) {
        val items = model.core.items(start, start + (cols.last() + 1) * DAY)
        // Each item's pieces in the hours (None: in the row above them).
        val timed = items.mapIndexedNotNull { i, it -> model.core.hours(it)?.let { _ -> i to model.core.segments(it, start) } }
        val inHours = timed.map { it.first }.toSet()
        val untimed = items.filterIndexed { i, _ -> i !in inHours }
        val blocks = cols.map { d ->
            val pieces = timed.flatMap { (i, segs) -> segs.filter { it.day == d.toLong() }.map { i to it } }
            val laid = columns(pieces.mapIndexed { n, (_, s) -> DayBlock(n.toUInt(), s.from, s.to) }).associateBy { it.index.toInt() }
            pieces.mapIndexedNotNull { n, (i, s) -> laid[n]?.let { Block(i, s, it) } }
        }
        WeekItems(items, blocks, cols.map { d -> untimed.filter { onDay(it, start + d * DAY) } })
    }
    val items = week.items
    val fullest = week.top.maxOfOrNull { it.size } ?: 0
    LaunchedEffect(fullest) { onTop?.invoke(fullest) }
    var drag by remember { mutableStateOf<Held?>(null) }
    // A drop to apply: the item and where its start goes.
    var moved by remember { mutableStateOf<Pair<Item, Long>?>(null) }
    moved?.let { (item, at) ->
        val change = remember(item, at) { model.core.slotDrop(item, at) }
        Reschedule(model, item, change) { moved = null }
    }
    val now = model.core.localNow()
    val today = midnight(now)
    // Held, a block may be moved over the next days' pages: drawn over them.
    Column(Modifier.fillMaxSize().zIndex(if (drag != null) 1f else 0f)) {
        // Day names and dates; in a week, a tap picks the day.
        if (header) Row(Modifier.fillMaxWidth().then(if (gutter) Modifier else Modifier.height(HEAD)).padding(start = inset)) {
            for (d in cols) {
                val at = start + d * DAY
                Column(
                    Modifier.weight(1f).then(if (gutter) Modifier.clickable { model.pick(at) } else Modifier).padding(vertical = 4.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(format(at, "EEE"), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Box(
                        Modifier.size(28.dp).background(
                            when {
                                at == today -> MaterialTheme.colorScheme.primary
                                at == picked -> MaterialTheme.colorScheme.secondaryContainer
                                else -> Color.Transparent
                            },
                            CircleShape,
                        ),
                        Alignment.Center,
                    ) {
                        Text(
                            "${fields(at).dayOfMonth}",
                            style = MaterialTheme.typography.titleSmall,
                            color = if (at == today) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }
        }
        // What takes no hours: all-day events and tasks due on a date.
        val topH = top ?: topHeight(fullest)
        if (topH > 0.dp) {
            Row(Modifier.fillMaxWidth().padding(start = inset).height(topH)) {
                cols.forEachIndexed { n, d ->
                    TopCell(start + d * DAY, now, week.top.getOrElse(n) { emptyList() }, colors, onItem, Modifier.weight(1f).fillMaxHeight())
                }
            }
        }
        HorizontalDivider()
        Box(Modifier.fillMaxSize().then(if (gutter) Modifier.verticalScroll(scroll) else Modifier.hoursScroll(scroll))) {
            val line = MaterialTheme.colorScheme.outlineVariant
            val surface = MaterialTheme.colorScheme.surface
            Column {
                for (h in 0 until 24) {
                    Row(Modifier.height(HOUR)) {
                        if (gutter) HourLabel(h)
                        for (d in cols) {
                            val at = start + d * DAY + h * 3600
                            Box(
                                Modifier.weight(1f).fillMaxHeight().border(0.25.dp, line)
                                    .clickable {
                                        val t = model.planning.value
                                        if (t == null) {
                                            model.open(Dest.EventEdit(null, null, null, at, false))
                                        } else {
                                            model.planning.value = null
                                            model.update(t.uid, model.core.planAt(t, at))
                                        }
                                    },
                            )
                        }
                    }
                }
            }
            // The blocks, day by day.
            Row(Modifier.padding(start = inset).height(HOUR * 24)) {
                cols.forEachIndexed { n, d ->
                    BoxWithConstraints(Modifier.weight(1f).fillMaxHeight()) {
                        val w = maxWidth
                        week.blocks.getOrElse(n) { emptyList() }.forEach { b ->
                            val (i, s, c) = Triple(b.item, b.piece, b.column)
                            val item = items[i]
                            val color = parseColor(colors[item.list()])
                            val height = (HOUR * ((s.to - s.from).coerceAtLeast(20) / 60f)) - 1.dp
                            val held = drag?.takeIf { it.item == i && it.day == d }
                            Box(
                                Modifier
                                    .offset(x = w * c.column.toInt() / c.columns.toInt(), y = HOUR * (s.from / 60f))
                                    .zIndex(if (held != null) 1f else 0f)
                                    .graphicsLayer {
                                        held?.let {
                                            translationX = it.offset.x
                                            translationY = it.offset.y
                                            shadowElevation = 8.dp.toPx()
                                        }
                                    }
                                    .width(w / c.columns.toInt() - 1.dp)
                                    .height(height)
                                    .clip(RoundedCornerShape(4.dp))
                                    // A task's light shade made solid, so the hour lines don't show through.
                                    .background(if (item is Item.Event) color else color.copy(alpha = 0.3f).compositeOver(surface))
                                    .clickable { onItem(item) }
                                    // Held and moved: to another hour (15-minute steps) or day.
                                    .pointerInput(item, start, cols, n) {
                                        detectDragGesturesAfterLongPress(
                                            onDragStart = { drag = Held(i, d, Offset.Zero) },
                                            onDrag = { change, amount ->
                                                change.consume()
                                                drag = drag?.let { it.copy(offset = it.offset + amount) }
                                            },
                                            onDragEnd = {
                                                val h = drag ?: return@detectDragGesturesAfterLongPress
                                                drag = null
                                                val minutes = s.from + (h.offset.y / HOUR.toPx() * 60).toLong()
                                                val stepped = (Math.round(minutes / 15.0) * 15).coerceIn(0, 24 * 60 - 15)
                                                // Columns over: a shown day, or past the last, the
                                                // days next to it.
                                                val over = Math.round(h.offset.x / w.toPx())
                                                val to = cols.getOrNull(n + over) ?: (d + over)
                                                if (stepped != s.from || to != d) {
                                                    moved = Pair(item, start + to * DAY + stepped * 60)
                                                }
                                            },
                                            onDragCancel = { drag = null },
                                        )
                                    }
                                    .padding(2.dp),
                            ) {
                                Text(
                                    item.title(),
                                    fontSize = 10.sp,
                                    lineHeight = 12.sp,
                                    color = if (item is Item.Event) Color.White else MaterialTheme.colorScheme.onSurface,
                                    textDecoration = if (item.done()) TextDecoration.LineThrough else null,
                                    overflow = TextOverflow.Clip,
                                )
                            }
                        }
                        // The current time.
                        if (start + d * DAY == today) {
                            Box(
                                Modifier.offset(y = HOUR * ((now - today) / 3600f)).fillMaxWidth().height(2.dp)
                                    .background(MaterialTheme.colorScheme.error),
                            )
                        }
                    }
                }
            }
        }
    }
}

/** A block held in the week view: which item, on which day, and how far it was moved. */
private data class Held(val item: Int, val day: Int, val offset: Offset)

/** `item` moved as `change` says (the core's slot or day drop); a repeating event asks which occurrences. */
@Composable
private fun Reschedule(model: Main, item: Item, change: Change, onDone: () -> Unit) {
    val core = model.core
    when (change) {
        is Change.Task -> LaunchedEffect(Unit) {
            val t = (item as Item.Task).task
            model.update(t.uid, change.patch)
            onDone()
        }
        is Change.Event -> {
            val e = (item as Item.Event).event
            ScopeDialog(tr("Change Repeating Event?"), e.recurring, delete = false, onDismiss = onDone) { scope ->
                onDone()
                model.act {
                    core.updateEvent(e.listHref, e.uid, e.recurrenceId, scope, EventPatch(time = change.time))
                    model.synced()
                }
            }
        }
    }
}

/**
 * "To Plan": open tasks without a time slot, undated or due by `until` (the end of the week
 * shown), as the Linux calendar's sidebar list. One picked waits for an hour to be tapped.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ToPlanSheet(model: Main, until: Long, colors: Map<String, String>, onDismiss: () -> Unit) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    // Shown once read: a sheet whose list fills in after it opens jumps from full height to half.
    // Hidden lists' tasks are left out, as on the calendar.
    val tasks = load(changes, until, initial = null as List<TodavTask>?) {
        val hidden = model.core.hidden()
        model.core.unscheduled(until).filter { t -> hidden.none { dev.aperte.todav.core.sameList(it, t.listHref) } }
    } ?: return
    val now = model.core.localNow()
    // As the Linux sidebar: due by a date first, then without one.
    val groups = tasks.partition { it.due != null }.toList()
        .zip(listOf(ptr("to plan", "Due"), tr("No date")))
        .filter { it.first.isNotEmpty() }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Text(tr("To Plan"), style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(horizontal = 24.dp))
        if (tasks.isEmpty()) {
            Text(tr("Nothing to plan"), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(24.dp))
        }
        LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
            groups.forEach { (group, title) ->
                item(key = title) {
                    Text(
                        title,
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 24.dp, top = 16.dp, bottom = 4.dp),
                    )
                }
                items(group, key = { it.uid }) { t -> PlanRow(t, colors, now) { model.planning.value = t; onDismiss() } }
            }
        }
    }
}

/** A task in "To Plan": its list's colour, its title and the day it is due (red when overdue). */
@Composable
private fun PlanRow(t: TodavTask, colors: Map<String, String>, now: Long, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 24.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(10.dp).background(parseColor(colors[t.listHref]), CircleShape))
        Text(t.summary, Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        t.due?.let {
            Text(
                format(it, "d MMM"),
                style = MaterialTheme.typography.bodyMedium,
                color = if (dev.aperte.todav.core.overdue(t, now)) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

// --- details ---------------------------------------------------------------------------------

/**
 * An event or a task, as the Linux app's details popover: its title (edited in place), when,
 * where, its calendar or list, notes, Edit and Delete, then its tasks (an event's) or subtasks (a
 * task's) with a field adding one. Opened half way; pulled all the way up, it opens the edit page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ItemSheet(model: Main, item: Item, onDismiss: () -> Unit) {
    val core = model.core
    val now = core.localNow()
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    var askDelete by remember { mutableStateOf(false) }
    // A repeating event's new title, waiting for which occurrences it is for.
    var renamed by remember { mutableStateOf<String?>(null) }
    val lists = load(initial = emptyMap<String, TodavList>()) { (core.calendars() + core.lists()).associateBy { it.href } }
    val colors = load(changes, initial = emptyMap<String, String>()) { core.colors() }
    val edit = {
        onDismiss()
        when (item) {
            is Item.Event -> item.event.let { e -> model.open(Dest.EventEdit(e.listHref, e.uid, e.recurrenceId, e.start, e.allDay)) }
            is Item.Task -> model.open(Dest.Task(item.task.uid))
        }
    }
    val sheet = rememberModalBottomSheetState()
    // While a field is typed in, the sheet goes up to show it over the keyboard, not to the edit page.
    var typing by remember { mutableStateOf(false) }
    LaunchedEffect(typing) { if (typing) sheet.expand() }
    LaunchedEffect(sheet) {
        snapshotFlow { sheet.currentValue }.collect { if (it == SheetValue.Expanded && !typing) edit() }
    }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet) {
        Column(
            // As tall as the screen, so it opens half way and can be pulled up.
            Modifier.fillMaxSize().padding(horizontal = 24.dp).padding(bottom = 24.dp).imePadding()
                .onFocusChanged { typing = it.hasFocus }
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            when (item) {
                is Item.Event -> {
                    val e = item.event
                    TitleField(e.summary, editable = lists[e.listHref]?.writable == true) { renamed = it }
                    Text(eventWhen(e, now), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    e.location?.takeIf { it.isNotBlank() }?.let { Text(it) }
                    ListLine(colors[e.listHref], lists[e.listHref]?.displayName ?: "")
                    if (e.reminders.isNotEmpty()) {
                        Text(
                            ntr("{n} reminder", "{n} reminders", e.reminders.size.toLong()).replace("{n}", "${e.reminders.size}"),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    e.description?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = edit) { Text(tr("Edit…")) }
                        TextButton(onClick = { askDelete = true }) { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) }
                    }
                    val tasks = remember(changes, e.uid) { core.eventTasks(e.uid) }
                    SectionTitle(tr("Tasks"))
                    tasks.forEach { t -> CheckRow(model, t) }
                    AddLine(tr("Add a task for this event")) { text ->
                        val list = core.setting("last_list")?.takeIf { l -> core.lists().any { it.href == l && it.writable } }
                            ?: core.byPlace(core.lists()).firstOrNull { it.writable }?.href
                        if (list != null) {
                            model.add(list, text, patch = TaskPatch(due = DueValue(Due(e.start, e.allDay)), event = TextValue(e.uid)))
                        }
                    }
                }
                is Item.Task -> {
                    val t = remember(changes) { core.task(item.task.uid) } ?: item.task
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(t.done, { model.setDone(t, it) })
                        TitleField(t.summary, done = t.done) { model.update(t.uid, TaskPatch(summary = it)) }
                    }
                    // Its slot or due date, if any (from the task list it may have none).
                    if (t.start != null || t.due != null) {
                        val w = taskWhen(t)
                        Text(dayLabel(w.at, w.allDay, now), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    ListLine(colors[t.listHref], listOfNotNull(lists[t.listHref]?.displayName, t.category).joinToString(" · "))
                    if (priorityColor(t.priority) != null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            StarIcon(t.priority)
                            Spacer(Modifier.width(8.dp))
                            Text(priorityName(t.priority))
                        }
                    }
                    t.description?.takeIf { it.isNotBlank() }?.let { Text(it) }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = edit) { Text(tr("Edit…")) }
                        dev.aperte.todav.core.unplan(t)?.let { patch ->
                            TextButton(onClick = {
                                onDismiss()
                                model.update(t.uid, patch)
                            }) { Text(tr("Unplan")) }
                        }
                        TextButton(onClick = {
                            onDismiss()
                            model.delete(t)
                        }) { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) }
                    }
                    val subs = remember(changes, t.uid) { core.subtasks(t.uid, false) + core.subtasks(t.uid, true) }
                    SectionTitle(tr("Subtasks"))
                    subs.forEach { s -> CheckRow(model, s) }
                    if (!t.done) AddLine(tr("Add Subtask")) { text -> model.add(t.listHref, text, t.uid, t.category) }
                }
            }
        }
    }
    if (renamed != null && item is Item.Event) {
        val e = item.event
        ScopeDialog(tr("Change Repeating Event?"), e.recurring, delete = false, onDismiss = { renamed = null }) { scope ->
            val name = renamed ?: return@ScopeDialog
            renamed = null
            model.act {
                core.updateEvent(e.listHref, e.uid, e.recurrenceId, scope, EventPatch(summary = name))
                model.synced()
            }
        }
    }
    if (askDelete && item is Item.Event) {
        val e = item.event
        ScopeDialog(tr("Delete Repeating Event?"), e.recurring, delete = true, onDismiss = { askDelete = false }) { scope ->
            askDelete = false
            onDismiss()
            model.act {
                core.deleteEvent(e.listHref, e.uid, e.recurrenceId, scope)
                model.synced()
                model.messages.tryEmit(tr("“{title}” deleted").replace("{title}", e.summary) to core.lastChange())
            }
        }
    }
}

/**
 * A details sheet's title, `editable` in place: a tap puts the cursor in it, Done or leaving it
 * gives `onRename` the new title (not an empty or unchanged one).
 */
@Composable
private fun TitleField(title: String, editable: Boolean = true, done: Boolean = false, onRename: (String) -> Unit) {
    var text by remember(title) { mutableStateOf(title) }
    var focused by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val save = {
        val t = text.trim()
        if (t.isNotEmpty() && t != title) onRename(t) else text = title
    }
    val style = MaterialTheme.typography.headlineSmall.copy(
        color = MaterialTheme.colorScheme.onSurface,
        textDecoration = if (done) TextDecoration.LineThrough else null,
    )
    BasicTextField(
        text,
        { v ->
            // One line of text, however long: Enter ends the edit.
            text = v.replace("\n", "")
            if ('\n' in v) focus.clearFocus()
        },
        Modifier.fillMaxWidth().onFocusChanged {
            if (focused && !it.isFocused) save()
            focused = it.isFocused
        },
        enabled = editable,
        textStyle = style,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
        cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
    )
}

/** Which calendar or list an item is in: its colour and name, as the Linux popover shows it. */
@Composable
private fun ListLine(color: String?, name: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(12.dp).background(parseColor(color), CircleShape))
        Text(name, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun CheckRow(model: Main, t: TodavTask) {
    Row(
        Modifier.fillMaxWidth().clickable { model.open(Dest.Task(t.uid)) },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(t.done, { model.setDone(t, it) })
        Text(t.summary, textDecoration = if (t.done) TextDecoration.LineThrough else null, modifier = Modifier.weight(1f))
    }
}

@Composable
private fun AddLine(placeholder: String, onAdd: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    OutlinedTextField(
        text,
        { text = it },
        placeholder = { Text(placeholder) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = {
            if (text.isNotBlank()) onAdd(text.trim())
            text = ""
        }),
        modifier = Modifier.fillMaxWidth(),
    )
}

/**
 * Which occurrences of a repeating event a change or delete is for; a single event goes
 * straight on (`then(Scope.ALL)`), unless it is a delete, which asks once.
 */
@Composable
fun ScopeDialog(title: String, recurring: Boolean, delete: Boolean, onDismiss: () -> Unit, then: (Scope) -> Unit) {
    if (!recurring && !delete) {
        LaunchedEffect(Unit) { then(Scope.ALL) }
        return
    }
    if (!recurring) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(tr("Delete Event?")) },
            confirmButton = { TextButton(onClick = { then(Scope.ALL) }) { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) } },
            dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
        )
        return
    }
    val color = if (delete) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                Text(tr("The event repeats."))
                TextButton(onClick = { then(Scope.THIS) }) { Text(tr("Only This Event"), color = color) }
                TextButton(onClick = { then(Scope.FOLLOWING) }) { Text(tr("This and Following"), color = color) }
                TextButton(onClick = { then(Scope.ALL) }) { Text(tr("All Events"), color = color) }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

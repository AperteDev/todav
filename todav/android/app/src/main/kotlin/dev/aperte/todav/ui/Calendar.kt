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
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.CalendarViewMonth
import androidx.compose.material.icons.outlined.Inbox
import androidx.compose.material.icons.outlined.CalendarViewWeek
import androidx.compose.material.icons.outlined.Today
import androidx.compose.material.icons.outlined.ViewAgenda
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
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
import androidx.compose.runtime.getValue
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
 * The calendar: Month, Week or Agenda, as the Linux app's, with Today, the view menu and a
 * button adding an event. Months and weeks change with a swipe.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CalendarPane(model: Main) {
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
    val title = when (mode) {
        CalMode.AGENDA -> tr("Agenda")
        else -> format(day, "LLLL yyyy").replaceFirstChar { it.titlecase() }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = { scope.launch { drawer?.open() } }) { Icon(Icons.Default.Menu, tr("Calendars")) }
                },
                actions = {
                    IconButton(onClick = { model.day.value = midnight(model.core.localNow()) }) {
                        Icon(Icons.Outlined.Today, tr("Today"))
                    }
                    if (mode == CalMode.WEEK) {
                        IconButton(onClick = { toPlan = true }) { Icon(Icons.Outlined.Inbox, tr("To Plan")) }
                    }
                    IconButton(onClick = { model.open(Dest.Search) }) { Icon(Icons.Default.Search, tr("Search")) }
                    Box {
                        IconButton(onClick = { viewMenu = true }) { Icon(modeIcon(mode), tr("View")) }
                        DropdownMenu(expanded = viewMenu, onDismissRequest = { viewMenu = false }) {
                            for (m in CalMode.entries) {
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
        floatingActionButton = {
            FloatingActionButton(onClick = { model.open(Dest.EventEdit(null, null, null, newStart(model, day), false)) }) {
                Icon(Icons.Default.Add, tr("New Event"))
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            planning?.takeIf { mode == CalMode.WEEK }?.let { t ->
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
                    CalMode.MONTH -> MonthView(model, day) { details = it }
                    CalMode.WEEK -> WeekView(model, day) { details = it }
                    CalMode.AGENDA -> AgendaView(model, day) { details = it }
                }
            }
        }
    }
    details?.let { ItemSheet(model, it) { details = null } }
    if (toPlan) ToPlanSheet(model, weekStart(day, firstWeekday()) + 7 * DAY) { toPlan = false }
}

/** A new event's start: the next full hour on `day` when it is today, else 9:00 there. */
private fun newStart(model: Main, day: Long): Long {
    val now = model.core.localNow()
    return if (midnight(now) == day) now - Math.floorMod(now, 3600L) + 3600 else day + 9 * 3600
}

private fun modeName(m: CalMode) = when (m) {
    CalMode.MONTH -> tr("Month")
    CalMode.WEEK -> tr("Week")
    CalMode.AGENDA -> tr("Agenda")
}

private fun modeIcon(m: CalMode) = when (m) {
    CalMode.MONTH -> Icons.Outlined.CalendarViewMonth
    CalMode.WEEK -> Icons.Outlined.CalendarViewWeek
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
private fun MonthView(model: Main, day: Long, onItem: (Item) -> Unit) {
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
                model.day.value = if (monthFirst(today) == month) today else month
            }
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth > maxHeight
        val grid: @Composable (Modifier) -> Unit = { m ->
            HorizontalPager(pager, m, beyondViewportPageCount = 0, key = { it }) { page ->
                MonthGrid(model, monthOf(base, page), day, onItem)
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
private fun MonthGrid(model: Main, month: Long, picked: Long, onItem: (Item) -> Unit) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val start = weekStart(month, firstWeekday())
    val items = load(changes, start, initial = emptyList<Item>()) { model.core.items(start, start + 42 * DAY) }
    val colors = load(changes, initial = emptyMap<String, String>()) { model.core.colors() }
    val today = midnight(model.core.localNow())
    val thisMonth = fields(month).monthValue
    Column(Modifier.fillMaxSize().padding(horizontal = 4.dp)) {
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
        for (w in 0 until 6) {
            Row(Modifier.fillMaxWidth().weight(1f)) {
                for (d in 0 until 7) {
                    val at = start + (w * 7 + d) * DAY
                    val here = items.filter { onDay(it, at) }
                    DayCell(
                        at, here, colors, at == today, at == picked, fields(at).monthValue == thisMonth,
                        Modifier.weight(1f).fillMaxHeight(),
                    ) { model.day.value = at }
                }
            }
        }
    }
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

@Composable
private fun Chip(item: Item, colors: Map<String, String>) {
    val color = parseColor(colors[item.list()])
    Text(
        item.title(),
        fontSize = 10.sp,
        lineHeight = 13.sp,
        maxLines = 1,
        overflow = TextOverflow.Clip,
        color = if (item is Item.Event) Color.White else MaterialTheme.colorScheme.onSurface,
        textDecoration = if (item.done()) TextDecoration.LineThrough else null,
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(3.dp))
            .background(if (item is Item.Event) color else color.copy(alpha = 0.25f))
            .padding(horizontal = 2.dp),
    )
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

/**
 * The week holding `day`: an hour grid with the timed events and planned tasks (overlapping
 * ones side by side), what takes no hours in a row above it, and a line at the current time.
 * Tap an empty hour to add an event there; swipe for other weeks.
 */
@Composable
private fun WeekView(model: Main, day: Long, onItem: (Item) -> Unit) {
    val first = firstWeekday()
    val base = remember { weekStart(day, first) }
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
                model.day.value = if (weekStart(today, first) == start) today else start
            }
        }
    }
    val scroll = rememberScrollState()
    LaunchedEffect(Unit) { scroll.scrollTo((HOUR.value * 7.5f * 2.6f).toInt()) }
    HorizontalPager(pager, Modifier.fillMaxSize(), key = { it }) { page ->
        Week(model, base + (page - MIDDLE) * 7 * DAY, day, scroll, onItem)
    }
}

@Composable
private fun Week(model: Main, start: Long, picked: Long, scroll: androidx.compose.foundation.ScrollState, onItem: (Item) -> Unit) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val items = load(changes, start, initial = emptyList<Item>()) { model.core.items(start, start + 7 * DAY) }
    val colors = load(changes, initial = emptyMap<String, String>()) { model.core.colors() }
    // Each item's pieces in the hours (None: in the row above them).
    val timed = load(items, start, initial = emptyList<Pair<Int, List<dev.aperte.todav.core.Segment>>>()) {
        items.mapIndexedNotNull { i, it -> model.core.hours(it)?.let { _ -> i to model.core.segments(it, start) } }
    }
    val inHours = timed.map { it.first }.toSet()
    var drag by remember { mutableStateOf<Held?>(null) }
    // A drop to apply: the item and where its start goes.
    var moved by remember { mutableStateOf<Pair<Item, Long>?>(null) }
    moved?.let { (item, at) -> Reschedule(model, item, at) { moved = null } }
    val now = model.core.localNow()
    val today = midnight(now)
    Column(Modifier.fillMaxSize()) {
        // Day names and dates.
        Row(Modifier.fillMaxWidth().padding(start = 40.dp)) {
            for (d in 0 until 7) {
                val at = start + d * DAY
                Column(
                    Modifier.weight(1f).clickable { model.day.value = at }.padding(vertical = 4.dp),
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
        val top = items.withIndex().filter { it.index !in inHours }
        if (top.isNotEmpty()) {
            Row(Modifier.fillMaxWidth().padding(start = 40.dp).heightIn(max = 96.dp).verticalScroll(rememberScrollState())) {
                for (d in 0 until 7) {
                    val at = start + d * DAY
                    Column(Modifier.weight(1f).padding(1.dp), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                        top.filter { onDay(it.value, at) }.forEach { (_, item) ->
                            Box(Modifier.clickable { onItem(item) }) { Chip(item, colors) }
                        }
                    }
                }
            }
        }
        HorizontalDivider()
        Box(Modifier.fillMaxSize().verticalScroll(scroll)) {
            val line = MaterialTheme.colorScheme.outlineVariant
            Column {
                for (h in 0 until 24) {
                    Row(Modifier.height(HOUR)) {
                        Text(
                            if (h == 0) "" else hourLabel(h),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.width(40.dp).offset(y = (-7).dp).padding(end = 4.dp),
                            textAlign = TextAlign.End,
                        )
                        for (d in 0 until 7) {
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
            Row(Modifier.padding(start = 40.dp).height(HOUR * 24)) {
                for (d in 0 until 7) {
                    val pieces = timed.flatMap { (i, segs) -> segs.filter { it.day == d.toLong() }.map { i to it } }
                    val cols = columns(pieces.mapIndexed { n, (_, s) -> DayBlock(n.toUInt(), s.from, s.to) })
                        .associateBy { it.index.toInt() }
                    BoxWithConstraints(Modifier.weight(1f).fillMaxHeight()) {
                        val w = maxWidth
                        pieces.forEachIndexed { n, (i, s) ->
                            val c = cols[n] ?: return@forEachIndexed
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
                                    .background(if (item is Item.Event) color else color.copy(alpha = 0.3f))
                                    .clickable { onItem(item) }
                                    // Held and moved: to another hour (15-minute steps) or day.
                                    .pointerInput(item, d) {
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
                                                val days = Math.round(h.offset.x / w.toPx()).toLong()
                                                if (stepped != s.from || days != 0L) {
                                                    moved = Pair(item, start + (d + days) * DAY + stepped * 60)
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

/** `item` moved to start at `at`: as the core's slot drop says; a repeating event asks which occurrences. */
@Composable
private fun Reschedule(model: Main, item: Item, at: Long, onDone: () -> Unit) {
    val core = model.core
    val change = remember(item, at) { core.slotDrop(item, at) }
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
private fun ToPlanSheet(model: Main, until: Long, onDismiss: () -> Unit) {
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val tasks = load(changes, until, initial = emptyList<TodavTask>()) { model.core.unscheduled(until) }
    val now = model.core.localNow()
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Text(tr("To Plan"), style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(horizontal = 24.dp))
        if (tasks.isEmpty()) {
            Text(tr("Nothing to plan"), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(24.dp))
        }
        LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
            items(tasks, key = { it.uid }) { t ->
                androidx.compose.material3.ListItem(
                    headlineContent = { Text(t.summary) },
                    supportingContent = dev.aperte.todav.dueLabel(t, now)?.let { { Text(it) } },
                    modifier = Modifier.clickable {
                        model.planning.value = t
                        onDismiss()
                    },
                )
            }
        }
    }
}

// --- details ---------------------------------------------------------------------------------

/**
 * An event or a task on the calendar, as the Linux app's details popover: when, where, the
 * calendar, notes, its tasks (an event's) or subtasks (a task's) with a field adding one; Edit and
 * Delete.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ItemSheet(model: Main, item: Item, onDismiss: () -> Unit) {
    val core = model.core
    val now = core.localNow()
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    var askDelete by remember { mutableStateOf(false) }
    val lists = load(initial = emptyMap<String, String>()) { (core.calendars() + core.lists()).associate { it.href to it.displayName } }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(bottom = 24.dp).imePadding()
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            when (item) {
                is Item.Event -> {
                    val e = item.event
                    Text(e.summary, style = MaterialTheme.typography.headlineSmall)
                    Text(eventWhen(e, now), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    e.location?.takeIf { it.isNotBlank() }?.let { Text(it) }
                    Text(lists[e.listHref] ?: "", style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                    if (e.reminders.isNotEmpty()) {
                        Text(
                            ntr("{n} reminder", "{n} reminders", e.reminders.size.toLong()).replace("{n}", "${e.reminders.size}"),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    e.description?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
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
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = {
                            onDismiss()
                            model.open(Dest.EventEdit(e.listHref, e.uid, e.recurrenceId, e.start, e.allDay))
                        }) { Text(tr("Edit…")) }
                        TextButton(onClick = { askDelete = true }) { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) }
                    }
                }
                is Item.Task -> {
                    val t = remember(changes) { core.task(item.task.uid) } ?: item.task
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(t.done, { model.setDone(t, it) })
                        Text(
                            t.summary,
                            style = MaterialTheme.typography.headlineSmall,
                            textDecoration = if (t.done) TextDecoration.LineThrough else null,
                        )
                    }
                    val w = taskWhen(t)
                    Text(dayLabel(w.at, w.allDay, now), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(
                        listOfNotNull(lists[t.listHref], t.category).joinToString(" · "),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.primary,
                    )
                    if (priorityColor(t.priority) != null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            StarIcon(t.priority)
                            Spacer(Modifier.width(8.dp))
                            Text(priorityName(t.priority))
                        }
                    }
                    t.description?.takeIf { it.isNotBlank() }?.let { Text(it) }
                    val subs = remember(changes, t.uid) { core.subtasks(t.uid, false) + core.subtasks(t.uid, true) }
                    SectionTitle(tr("Subtasks"))
                    subs.forEach { s -> CheckRow(model, s) }
                    if (!t.done) AddLine(tr("Add Subtask")) { text -> model.add(t.listHref, text, t.uid, t.category) }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = {
                            onDismiss()
                            model.open(Dest.Task(t.uid))
                        }) { Text(tr("Edit…")) }
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
                }
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

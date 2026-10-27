package dev.aperte.todav

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalSize
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.glance.background
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxHeight
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextAlign
import androidx.glance.text.TextDecoration
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import dev.aperte.todav.core.Item
import dev.aperte.todav.core.Time
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.dateOf
import dev.aperte.todav.core.midnight
import dev.aperte.todav.core.midnightOn
import dev.aperte.todav.core.monthFirst
import dev.aperte.todav.core.sameList
import dev.aperte.todav.core.weekStart
import dev.aperte.todav.ui.DAY
import dev.aperte.todav.ui.done
import dev.aperte.todav.ui.firstWeekday
import dev.aperte.todav.ui.list
import dev.aperte.todav.ui.parseColor
import dev.aperte.todav.ui.title
import kotlin.math.ceil
import kotlin.math.floor

/** How far a widget is turned from today: months, or days for the day widget. */
private val OFFSET = intPreferencesKey("offset")
private val BY = ActionParameters.Key<Int>("by")

/** An arrow of a calendar widget: turned on or back `BY`. */
class Turn : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val by = parameters[BY] ?: 0
        updateAppWidgetState(context, glanceId) { it[OFFSET] = (it[OFFSET] ?: 0) + by }
        val id = GlanceAppWidgetManager(context).getAppWidgetId(glanceId)
        WidgetKind.of(context, id)?.widget()?.update(context, glanceId)
    }
}

/**
 * A calendar widget's account and its calendars and task lists shown: those shown in the app, or
 * with its own picked, those of its account picked (none: all).
 */
fun widgetCalendars(context: Context, app: TodavApp, id: Int): Pair<Long?, List<TodavList>> {
    val all = app.core.byPlace((app.core.calendars() + app.core.lists()).distinctBy { it.href })
    // Unless the widget picks its own: those shown in the app, of every account.
    if (!WidgetPrefs.own(context, id)) {
        val hidden = app.core.hidden()
        return null to all.filter { l -> hidden.none { sameList(it, l.href) } }
    }
    val account = widgetAccount(context, app, id, all)
    val mine = all.filter { it.account == account }
    val picked = WidgetPrefs.calendars(context, id)
    return account to mine.filter { it.href in picked }.ifEmpty { mine }
}

/**
 * A calendar widget's add button: a new event on `day`, in the only calendar it shows that takes
 * events, else with its account picked and the calendar left to choose.
 */
private fun newEvent(context: Context, id: Int, account: Long?, shown: List<TodavList>, day: Long): Intent {
    val calendars = shown.filter { it.holdsEvents && it.writable }
    return Intent(context, MainActivity::class.java).setAction(MainActivity.NEW_EVENT).putExtra("day", day)
        .setData(Uri.parse("todav-add://$id/$day"))
        .apply {
            if (calendars.size == 1) putExtra("calendar", calendars[0].href) else account?.let { putExtra("account", it) }
        }
}

/** A calendar widget's header: arrows around `title` (tapped: `open`, the month or day in the app), then its buttons. */
@Composable
private fun TurnHeader(context: Context, id: Int, title: String, open: Intent, add: Intent) {
    Row(GlanceModifier.fillMaxWidth().padding(bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        WidgetButton(R.drawable.ic_widget_prev, tr("Previous"), actionRunCallback<Turn>(actionParametersOf(BY to -1)))
        Text(
            title,
            maxLines = 1,
            style = TextStyle(fontWeight = FontWeight.Bold, fontSize = 16.sp, textAlign = TextAlign.Center, color = GlanceTheme.colors.onSurface),
            modifier = GlanceModifier.defaultWeight().clickable(actionStartActivity(open)),
        )
        WidgetButton(R.drawable.ic_widget_next, tr("Next"), actionRunCallback<Turn>(actionParametersOf(BY to 1)))
        Spacer(GlanceModifier.width(4.dp))
        WidgetButtons(context, id, add)
    }
}

/** The first of the month `offset` months from the one holding `day`. */
private fun monthAt(day: Long, offset: Int): Long {
    val d = dateOf(monthFirst(day))
    return midnightOn(d.year, d.month + offset, 1)
}

/** A month's name and year, as a title ("Październik 2026"). */
private fun monthTitle(first: Long) = format(first, "LLLL yyyy").replaceFirstChar { it.titlecase() }

/** The weekdays' short names over a month from `start`. */
@Composable
private fun WeekdayNames(start: Long) {
    Row(GlanceModifier.fillMaxWidth()) {
        for (i in 0 until 7) {
            Text(
                format(start + i * DAY, "EEE"),
                maxLines = 1,
                style = TextStyle(fontSize = 12.sp, textAlign = TextAlign.Center, color = GlanceTheme.colors.onSurfaceVariant),
                modifier = GlanceModifier.defaultWeight(),
            )
        }
    }
}

/**
 * A day's number, `size` high: on today's accent when it is today, dim outside the month shown.
 * One view, a box only for today: a widget has 500 at most.
 */
@Composable
private fun DayNumber(day: Long, today: Long, inMonth: Boolean, size: Dp, modifier: GlanceModifier = GlanceModifier) {
    val now = day == today
    val text = @Composable { m: GlanceModifier ->
        Text(
            "${dateOf(day).day}",
            maxLines = 1,
            style = TextStyle(
                fontSize = 13.sp,
                textAlign = TextAlign.Center,
                fontWeight = if (now) FontWeight.Bold else FontWeight.Normal,
                color = when {
                    now -> GlanceTheme.colors.onPrimary
                    inMonth -> GlanceTheme.colors.onSurface
                    else -> GlanceTheme.colors.outline
                },
            ),
            modifier = m,
        )
    }
    if (now) {
        Box(modifier.height(size), contentAlignment = Alignment.Center) {
            Box(GlanceModifier.size(size).cornerRadius(size / 2).background(GlanceTheme.colors.primary), contentAlignment = Alignment.Center) {
                text(GlanceModifier)
            }
        }
    } else {
        text(modifier.height(size).padding(top = (size - 17.dp) / 2))
    }
}

// --- month --------------------------------------------------------------------------------------

/**
 * The month widget, as the GNOME extension's month: its days with a dot for each calendar with
 * something on it (three at most), arrows turning the month; a day opens the app there.
 */
class MonthWidget : TodavWidget() {
    @Composable
    override fun Content(context: Context, app: TodavApp, widget: Int, changes: Long) {
        val core = app.core
        val offset = currentState(OFFSET) ?: 0
        val today = midnight(core.localNow())
        val first = monthAt(today, offset)
        val next = monthAt(today, offset + 1)
        val start = weekStart(first, firstWeekday())
        val weeks = ceil(((next - start) / DAY) / 7.0).toInt()
        val (account, shown) = remember(changes) { widgetCalendars(context, app, widget) }
        val dots = remember(changes, offset, shown) { core.dayColors(start, weeks * 7L, shown.map { it.href }) }
        val density = context.resources.displayMetrics.density
        val images = remember { HashMap<List<String>, android.graphics.Bitmap>() }
        WidgetFrame(context) {
            TurnHeader(context, widget, monthTitle(first), openDay(context, first, month = true), newEvent(context, widget, account, shown, today))
            WeekdayNames(start)
            for (w in 0 until weeks) {
                Row(GlanceModifier.fillMaxWidth().defaultWeight()) {
                    for (d in 0 until 7) {
                        val day = start + (w * 7 + d) * DAY
                        Column(
                            GlanceModifier.defaultWeight().fillMaxHeight().clickable(actionStartActivity(openDay(context, day))),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            DayNumber(day, today, day in first until next, 28.dp, GlanceModifier.fillMaxWidth())
                            val colors = dots.getOrNull(w * 7 + d).orEmpty()
                            if (colors.isEmpty()) {
                                Spacer(GlanceModifier.height(6.dp))
                            } else {
                                Image(
                                    ImageProvider(dotImage(colors, density, images)), null,
                                    GlanceModifier.height(6.dp).width((colors.size * 8 - 2).dp),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * A day's dots, one per colour (6 dp, 2 dp apart), as one image: an image a dot made the month too
 * big to send to the launcher. The same colours share an image (`cache`), sent once.
 */
private fun dotImage(colors: List<String>, density: Float, cache: MutableMap<List<String>, android.graphics.Bitmap>) =
    cache.getOrPut(colors) {
        val d = 6 * density
        val gap = 2 * density
        val bitmap = android.graphics.Bitmap.createBitmap(
            Math.round(colors.size * (d + gap) - gap), Math.round(d), android.graphics.Bitmap.Config.ARGB_8888,
        )
        val canvas = android.graphics.Canvas(bitmap)
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG)
        colors.forEachIndexed { i, c ->
            paint.color = parseColor(c).toArgb()
            canvas.drawCircle(i * (d + gap) + d / 2, d / 2, d / 2, paint)
        }
        bitmap
    }

class MonthWidgetReceiver : TodavWidgetReceiver() {
    override val glanceAppWidget = MonthWidget()
}

// --- agenda -------------------------------------------------------------------------------------

/** Days the agenda widget looks ahead, as the extension's. */
private const val AGENDA_DAYS = 7L

/** A day's heading in the agenda: "Today · wtorek 27 października", else the date. */
private fun agendaHeading(day: Long, today: Long): String {
    val date = format(day, "EEEE d MMMM")
    return when ((day - today) / DAY) {
        0L -> "${tr("Today")} · $date"
        1L -> "${tr("Tomorrow")} · $date"
        else -> date.replaceFirstChar { it.titlecase() }
    }
}

/**
 * The agenda widget, as the GNOME extension's agenda: the coming week day by day, an event's dot
 * or a task's circle (checked when done) in its calendar's colour, its time and title; a row
 * opens the app on its day, or on the task.
 */
class AgendaWidget : TodavWidget() {
    @Composable
    override fun Content(context: Context, app: TodavApp, widget: Int, changes: Long) {
        val core = app.core
        val today = midnight(core.localNow())
        val (account, shown) = remember(changes) { widgetCalendars(context, app, widget) }
        val entries = remember(changes, shown) { core.agenda(today, AGENDA_DAYS, shown.map { it.href }) }
        WidgetFrame(context) {
            Row(GlanceModifier.fillMaxWidth().padding(start = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    tr("Agenda"),
                    maxLines = 1,
                    style = TextStyle(fontWeight = FontWeight.Bold, fontSize = 16.sp, color = GlanceTheme.colors.onSurface),
                    modifier = GlanceModifier.defaultWeight(),
                )
                WidgetButtons(context, widget, newEvent(context, widget, account, shown, today))
            }
            if (entries.isEmpty()) WidgetEmpty(tr("Nothing planned this week"))
            val rows: List<Any> = entries.groupBy { it.day }.flatMap { (day, es) -> listOf(agendaHeading(day, today)) + es }
            LazyColumn {
                items(rows) { r ->
                    if (r is String) WidgetHeading(r) else AgendaRow(context, r as dev.aperte.todav.core.AgendaEntry)
                }
            }
        }
    }

    @Composable
    private fun AgendaRow(context: Context, e: dev.aperte.todav.core.AgendaEntry) {
        val item = e.item
        val open = openItem(context, item)
        Row(
            GlanceModifier.fillMaxWidth().clickable(actionStartActivity(open)).padding(horizontal = 4.dp, vertical = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            WidgetMark(item is Item.Task, item.done(), widgetColor(e.color), 18.dp)
            Spacer(GlanceModifier.width(8.dp))
            Text(
                when (val t = e.time) {
                    Time.AllDay -> tr("All day")
                    Time.Date -> ""
                    is Time.At -> time(t.time)
                    is Time.Until -> tr("Until {time}").replace("{time}", time(t.end))
                },
                maxLines = 1,
                style = TextStyle(fontSize = 12.sp, color = GlanceTheme.colors.onSurfaceVariant),
                modifier = GlanceModifier.width(if (hours24) 48.dp else 64.dp),
            )
            Text(
                item.title(),
                maxLines = 1,
                style = TextStyle(
                    color = if (item.done()) GlanceTheme.colors.onSurfaceVariant else GlanceTheme.colors.onSurface,
                    textDecoration = if (item.done()) TextDecoration.LineThrough else null,
                ),
            )
        }
    }
}

class AgendaWidgetReceiver : TodavWidgetReceiver() {
    override val glanceAppWidget = AgendaWidget()
}

// --- day ----------------------------------------------------------------------------------------

/** An hour's height on the day widget's timeline. */
private val HOUR = 48.dp

/** A block of the day's timeline: from and to (local times), in a lane of its own among those it overlaps. */
private class Block(val item: Item, val from: Long, val to: Long, val color: String?)

/** Blocks in lanes: each in the first whose last one ends before it starts (as shown, `min` long at least). */
private fun lanes(blocks: List<Block>, min: Long): List<List<Block>> {
    val lanes = mutableListOf<MutableList<Block>>()
    for (b in blocks.sortedBy { it.from }) {
        val lane = lanes.firstOrNull { l -> l.last().let { maxOf(it.to, it.from + min) } <= b.from }
        if (lane != null) lane += b else lanes += mutableListOf(b)
    }
    return lanes
}

/**
 * The day widget, as the app's Day view: what takes all day on top, then the hours from about
 * now (from the first event on other days) with the events and planned tasks in them, a line at
 * the time; arrows turn the day.
 */
class DayWidget : TodavWidget() {
    @Composable
    override fun Content(context: Context, app: TodavApp, widget: Int, changes: Long) {
        val core = app.core
        val offset = currentState(OFFSET) ?: 0
        val now = core.localNow()
        val today = midnight(now)
        val day = today + offset * DAY
        val (account, shown) = remember(changes) { widgetCalendars(context, app, widget) }
        val (allDay, timed) = remember(changes, day, shown) {
            val colors = core.colors()
            val items = core.items(day, day + DAY, shown.map { it.href })
            val blocks = items.mapNotNull { i ->
                core.hours(i)?.let { h -> Block(i, maxOf(h.from, day), minOf(h.to, day + DAY), colors[i.list()]) }
            }
            items.filter { core.hours(it) == null }.map { it to colors[it.list()] } to blocks
        }
        val title = when (offset) {
            0 -> tr("Today")
            1 -> tr("Tomorrow")
            -1 -> tr("Yesterday")
            else -> format(day, "EEE d MMM").replaceFirstChar { it.titlecase() }
        }
        WidgetFrame(context) {
            TurnHeader(context, widget, title, openDay(context, day), newEvent(context, widget, account, shown, day))
            // From an hour before now today, else from the first thing (8:00 at the latest).
            val from = if (offset == 0) {
                (((now - day) / 3600).toInt() - 1).coerceIn(0, 23)
            } else {
                minOf(timed.minOfOrNull { ((it.from - day) / 3600).toInt() } ?: 8, 8)
            }
            // What takes all day on top, scrolling with the hours.
            LazyColumn {
                items(allDay) { (item, color) -> AllDay(context, item, color) }
                item { Timeline(context, day, from, timed, if (offset == 0) now else null) }
            }
        }
    }

    @Composable
    private fun AllDay(context: Context, item: Item, color: String?) {
        Box(GlanceModifier.fillMaxWidth().padding(bottom = 2.dp)) {
            // A task at a time but of no length goes here too, after its time.
            val at = (item as? Item.Task)?.task?.let { t ->
                if (t.start != null) t.start.takeUnless { t.startAllDay } else t.due.takeUnless { t.dueAllDay }
            }
            Text(
                listOfNotNull(at?.let(::time), item.title()).joinToString("  "),
                maxLines = 1,
                style = TextStyle(
                    fontSize = 12.sp,
                    color = ColorProvider(androidx.compose.ui.graphics.Color.White),
                    textDecoration = if (item.done()) TextDecoration.LineThrough else null,
                ),
                modifier = GlanceModifier.fillMaxWidth().cornerRadius(6.dp).background(widgetColor(color))
                    .clickable(actionStartActivity(openItem(context, item))).padding(horizontal = 8.dp, vertical = 3.dp),
            )
        }
    }

    @Composable
    private fun Timeline(context: Context, day: Long, fromHour: Int, blocks: List<Block>, now: Long?) {
        val start = day + fromHour * 3600L
        fun dp(seconds: Long): Dp = HOUR * (seconds / 3600f)
        Box(GlanceModifier.fillMaxWidth()) {
            Chunked(
                true, GlanceModifier.fillMaxWidth(),
                (fromHour until 24).map { h ->
                    @Composable {
                        Row(GlanceModifier.fillMaxWidth().height(HOUR)) {
                            Text(hourLabel(h), style = TextStyle(fontSize = 11.sp, color = GlanceTheme.colors.onSurfaceVariant), modifier = GlanceModifier.width(44.dp))
                            Box(GlanceModifier.defaultWeight().height(1.dp).background(GlanceTheme.colors.outline)) {}
                        }
                    }
                },
            )
            // Four lanes at most side by side.
            Row(GlanceModifier.fillMaxWidth().padding(start = 46.dp)) {
                for (lane in lanes(blocks.filter { it.to > start }, 1800).take(4)) {
                    var at = start
                    val parts = mutableListOf<@Composable () -> Unit>()
                    for (b in lane) {
                        val from = maxOf(b.from, start)
                        val gap = from - at
                        if (gap > 0) parts += @Composable { Spacer(GlanceModifier.height(dp(gap))) }
                        val shown = maxOf(b.to, from + 1800)
                        parts += @Composable {
                            Box(GlanceModifier.fillMaxWidth().height(dp(shown - from)).padding(end = 2.dp, bottom = 1.dp)) {
                                Text(
                                    b.item.title(),
                                    maxLines = maxOf(1, ((shown - from) / 1800).toInt()),
                                    style = TextStyle(
                                        fontSize = 12.sp,
                                        color = ColorProvider(androidx.compose.ui.graphics.Color.White),
                                        textDecoration = if (b.item.done()) TextDecoration.LineThrough else null,
                                    ),
                                    modifier = GlanceModifier.fillMaxSize().cornerRadius(6.dp).background(widgetColor(b.color))
                                        .clickable(actionStartActivity(openItem(context, b.item))).padding(horizontal = 6.dp, vertical = 2.dp),
                                )
                            }
                        }
                        at = shown
                    }
                    Chunked(true, GlanceModifier.defaultWeight(), parts)
                }
            }
            if (now != null && now >= start) {
                Column(GlanceModifier.fillMaxWidth().padding(start = 44.dp)) {
                    Spacer(GlanceModifier.height(dp(now - start)))
                    Box(GlanceModifier.fillMaxWidth().height(2.dp).background(GlanceTheme.colors.error)) {}
                }
            }
        }
    }

}

class DayWidgetReceiver : TodavWidgetReceiver() {
    override val glanceAppWidget = DayWidget()
}

// --- month planner ------------------------------------------------------------------------------

/** A bar of the month planner: an item over the days `first` to `last` of its week (0 to 6). */
private class Bar(val item: Item, val first: Int, val last: Int, val color: String?)

/**
 * The month planner, as Fossify Calendar's month widget: six weeks, each day's number with what is
 * on it as bars across the days it takes, in its calendar's colour, as many as fit; a day opens
 * the app there.
 */
class PlannerWidget : TodavWidget() {
    @Composable
    override fun Content(context: Context, app: TodavApp, widget: Int, changes: Long) {
        val core = app.core
        val offset = currentState(OFFSET) ?: 0
        val today = midnight(core.localNow())
        val first = monthAt(today, offset)
        val next = monthAt(today, offset + 1)
        val start = weekStart(first, firstWeekday())
        val (account, shown) = remember(changes) { widgetCalendars(context, app, widget) }
        val weeks = remember(changes, start, shown) {
            val colors = core.colors()
            val items = core.items(start, start + 42 * DAY, shown.map { it.href })
            (0 until 6).map { w -> bars(items, start + w * 7 * DAY, colors) }
        }
        // The frame's padding, the header and the weekday names off the size; six weeks in the rest.
        val size = LocalSize.current
        val cell = (size.width - 20.dp) / 7
        val week = (size.height - 20.dp - 44.dp - 18.dp) / 6
        val number = 20.dp
        val bar = 15.dp
        val fit = floor(((week - number - 1.dp) / bar)).toInt().coerceIn(0, 8)
        WidgetFrame(context) {
            TurnHeader(context, widget, monthTitle(first), openDay(context, first, month = true), newEvent(context, widget, account, shown, today))
            WeekdayNames(start)
            // Flat, a view for each number and bar: a widget has 500 at most.
            for (w in 0 until 6) {
                val monday = start + w * 7 * DAY
                Column(GlanceModifier.fillMaxWidth().height(week)) {
                    Box(GlanceModifier.fillMaxWidth().height(1.dp).background(GlanceTheme.colors.outline)) {}
                    Row(GlanceModifier.fillMaxWidth()) {
                        for (d in 0 until 7) {
                            val day = monday + d * DAY
                            DayNumber(
                                day, today, day in first until next, number,
                                GlanceModifier.width(cell).clickable(actionStartActivity(openDay(context, day))),
                            )
                        }
                    }
                    for (lane in lanes(weeks[w]).take(fit)) {
                        var at = 0
                        val parts = mutableListOf<@Composable () -> Unit>()
                        for (b in lane) {
                            if (b.first > at) {
                                val gap = cell * (b.first - at)
                                parts += @Composable { Spacer(GlanceModifier.width(gap)) }
                            }
                            val color = parseColor(b.color).let { if (b.item.done()) it.copy(alpha = 0.5f) else it }
                            parts += @Composable {
                                Text(
                                    b.item.title(),
                                    maxLines = 1,
                                    style = TextStyle(fontSize = 10.sp, color = ColorProvider(androidx.compose.ui.graphics.Color.White)),
                                    modifier = GlanceModifier.width(cell * (b.last - b.first + 1) - 2.dp).height(bar - 1.dp)
                                        .cornerRadius(3.dp).background(ColorProvider(color)).padding(horizontal = 3.dp)
                                        .clickable(actionStartActivity(openItem(context, b.item))),
                                )
                            }
                            parts += @Composable { Spacer(GlanceModifier.width(2.dp)) }
                            at = b.last + 1
                        }
                        Chunked(false, GlanceModifier.fillMaxWidth().height(bar), parts)
                    }
                }
            }
        }
    }

    /** What is on the week from `monday` as bars, longest first at each start, all-day ones before timed. */
    private fun bars(items: List<Item>, monday: Long, colors: Map<String, String>): List<Bar> = items.mapNotNull { i ->
        val (from, to) = when (i) {
            is Item.Event -> i.event.start to maxOf(i.event.start, i.event.end - 1)
            is Item.Task -> (i.task.start ?: i.task.due ?: return@mapNotNull null).let { it to it }
        }
        val first = ((midnight(from) - monday) / DAY).toInt()
        val last = ((midnight(to) - monday) / DAY).toInt()
        if (last < 0 || first > 6) null else Bar(i, first.coerceAtLeast(0), last.coerceAtMost(6), colors[i.list()])
    }.sortedWith(compareBy<Bar>({ it.first }, { -(it.last - it.first) }))

    /** Bars in lanes: each in the first where it fits after the last one. */
    private fun lanes(bars: List<Bar>): List<List<Bar>> {
        val lanes = mutableListOf<MutableList<Bar>>()
        for (b in bars) {
            val lane = lanes.firstOrNull { it.last().last < b.first }
            if (lane != null) lane += b else lanes += mutableListOf(b)
        }
        return lanes
    }
}

class PlannerWidgetReceiver : TodavWidgetReceiver() {
    override val glanceAppWidget = PlannerWidget()
}

package dev.aperte.todav

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.action.Action
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ColumnScope
import androidx.glance.layout.Row
import androidx.glance.layout.RowScope
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextDecoration
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider as FixedColor
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.listColor
import dev.aperte.todav.core.section
import dev.aperte.todav.ui.parseColor

/** The home-screen widgets, each with its receiver (named in the manifest, so never renamed). */
enum class WidgetKind(val receiver: Class<out GlanceAppWidgetReceiver>) {
    TASKS(TasksWidgetReceiver::class.java),
    MONTH(MonthWidgetReceiver::class.java),
    AGENDA(AgendaWidgetReceiver::class.java),
    DAY(DayWidgetReceiver::class.java),
    PLANNER(PlannerWidgetReceiver::class.java);

    fun title(): String = when (this) {
        TASKS -> tr("Tasks")
        MONTH -> tr("Month")
        AGENDA -> tr("Agenda")
        DAY -> tr("Day")
        PLANNER -> tr("Month Planner")
    }

    fun widget(): GlanceAppWidget = when (this) {
        TASKS -> TasksWidget()
        MONTH -> MonthWidget()
        AGENDA -> AgendaWidget()
        DAY -> DayWidget()
        PLANNER -> PlannerWidget()
    }

    companion object {
        /** What widget `id` is, if it is one of ours still placed. */
        fun of(context: Context, id: Int): WidgetKind? {
            val provider = AppWidgetManager.getInstance(context).getAppWidgetInfo(id)?.provider ?: return null
            return entries.firstOrNull { it.receiver.name == provider.className }
        }

        /** The widgets placed now, each kind in turn. */
        fun placed(context: Context): List<Pair<WidgetKind, Int>> {
            val m = AppWidgetManager.getInstance(context)
            return entries.flatMap { k -> m.getAppWidgetIds(ComponentName(context, k.receiver)).map { k to it } }
        }

        /** Draw every widget again. */
        suspend fun refreshAll(context: Context) = entries.forEach { runCatching { it.widget().updateAll(context) } }
    }
}

/**
 * The widgets' settings: those of all (background opacity, corner radius) and each widget's,
 * by its id. Device-local by nature, so not in the synced settings.
 */
object WidgetPrefs {
    private fun prefs(c: Context) = c.getSharedPreferences("widgets", Context.MODE_PRIVATE)

    /** The background's opacity, in percent. */
    fun opacity(c: Context) = prefs(c).getInt("opacity", 100)
    fun setOpacity(c: Context, v: Int) = prefs(c).edit().putInt("opacity", v).apply()

    /** The corners' radius, in dp. */
    fun radius(c: Context) = prefs(c).getInt("radius", 24)
    fun setRadius(c: Context, v: Int) = prefs(c).edit().putInt("radius", v).apply()

    /** A task widget's list (an href, `UPCOMING` or `IMPORTANT`); under the bare id, as before. */
    fun list(c: Context, id: Int) = prefs(c).getString("$id", null)
    fun setList(c: Context, id: Int, v: String) = prefs(c).edit().putString("$id", v).apply()

    /** The account a widget shows. */
    fun account(c: Context, id: Int) = prefs(c).getLong("$id.account", -1).takeIf { it >= 0 }
    fun setAccount(c: Context, id: Int, v: Long) = prefs(c).edit().putLong("$id.account", v).remove("$id").remove("$id.calendars").apply()

    /** A calendar widget's calendars and task lists (none: all of its account's). */
    fun calendars(c: Context, id: Int): Set<String> = prefs(c).getStringSet("$id.calendars", null) ?: emptySet()
    fun setCalendars(c: Context, id: Int, v: Set<String>) = prefs(c).edit().putStringSet("$id.calendars", v).apply()

    /** Whether a widget shows its add button. */
    fun add(c: Context, id: Int) = prefs(c).getBoolean("$id.add", true)
    fun setAdd(c: Context, id: Int, v: Boolean) = prefs(c).edit().putBoolean("$id.add", v).apply()

    /** A removed widget's settings go. */
    fun forget(c: Context, id: Int) = prefs(c).edit().apply {
        for (k in listOf("$id", "$id.account", "$id.calendars", "$id.add")) remove(k)
    }.apply()
}

/** Removing a widget removes its settings. */
abstract class TodavWidgetReceiver : GlanceAppWidgetReceiver() {
    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        super.onDeleted(context, appWidgetIds)
        appWidgetIds.forEach { WidgetPrefs.forget(context, it) }
    }
}

/**
 * A widget's account: the one picked, else the first with something it can show (`fits`); and
 * its lists among `all`.
 */
fun widgetAccount(context: Context, app: TodavApp, id: Int, all: List<TodavList>): Long? {
    val accounts = app.core.accounts().filter { it.enabled }.map { it.id }
    return WidgetPrefs.account(context, id)?.takeIf { it in accounts }
        ?: accounts.firstOrNull { a -> all.any { it.account == a } }
}

/**
 * A task widget's list among `mine` (its account's): the one picked (or Upcoming, Important),
 * else the one last opened, else the first.
 */
fun widgetList(context: Context, app: TodavApp, id: Int, mine: List<TodavList>): String? =
    WidgetPrefs.list(context, id)?.takeIf { h -> h == UPCOMING || h == IMPORTANT || mine.any { it.href == h } }
        ?: app.core.setting("last_list")?.takeIf { h -> mine.any { it.href == h } }
        ?: mine.firstOrNull()?.href

/** A Glance widget of Todav's: read again on every change (and its settings') while it shows. */
abstract class TodavWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as TodavApp
        app.awaitReady()
        val widget = GlanceAppWidgetManager(context).getAppWidgetId(id)
        // The app may not have opened in this process: times as the system shows them.
        hours24 = android.text.format.DateFormat.is24HourFormat(context)
        provideContent {
            val changes by app.changes.collectAsState()
            GlanceTheme { Content(context, app, widget, changes) }
        }
    }

    @Composable
    abstract fun Content(context: Context, app: TodavApp, widget: Int, changes: Long)
}

/** A widget's background in the system's colours, with the opacity and corners set for all. */
@Composable
fun WidgetFrame(context: Context, content: @Composable ColumnScope.() -> Unit) {
    val alpha = WidgetPrefs.opacity(context) / 100f
    val (day, night) = if (Build.VERSION.SDK_INT >= 31) {
        dynamicLightColorScheme(context).surface to dynamicDarkColorScheme(context).surface
    } else {
        Color(0xFFF8F9FF) to Color(0xFF111318)
    }
    Column(
        GlanceModifier.fillMaxSize().background(ColorProvider(day.copy(alpha = alpha), night.copy(alpha = alpha)))
            .cornerRadius(WidgetPrefs.radius(context).dp).padding(10.dp),
        content = content,
    )
}

/** A round button of a widget's header showing `icon`. */
@Composable
fun WidgetButton(icon: Int, label: String, action: Action, filled: Boolean = false) {
    val (bg, fg) = if (filled) {
        GlanceTheme.colors.primaryContainer to GlanceTheme.colors.onPrimaryContainer
    } else {
        null to GlanceTheme.colors.onSurfaceVariant
    }
    Box(
        GlanceModifier.size(36.dp).cornerRadius(18.dp).let { m -> bg?.let { m.background(it) } ?: m }.clickable(action),
        contentAlignment = Alignment.Center,
    ) {
        Image(ImageProvider(icon), label, GlanceModifier.size(20.dp), colorFilter = ColorFilter.tint(fg))
    }
}

/** The settings button of widget `id`, and its add button opening `add` when it shows one. */
@Composable
fun RowScope.WidgetButtons(context: Context, id: Int, add: Intent?) {
    // The data keeps each widget's intent apart: intents differing only in extras are one.
    val settings = Intent(context, WidgetConfigure::class.java)
        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id)
        .setData(Uri.parse("todav-widget://$id"))
    WidgetButton(R.drawable.ic_widget_settings, tr("Settings"), actionStartActivity(settings))
    if (add != null && WidgetPrefs.add(context, id)) {
        Spacer(GlanceModifier.width(4.dp))
        WidgetButton(R.drawable.ic_widget_add, tr("Add"), actionStartActivity(add), filled = true)
    }
}

/** A small dim heading of a widget's list, as the extension's categories and days. */
@Composable
fun WidgetHeading(text: String) = Text(
    text,
    maxLines = 1,
    style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Bold, color = GlanceTheme.colors.onSurfaceVariant),
    modifier = GlanceModifier.fillMaxWidth().padding(start = 4.dp, top = 8.dp, bottom = 2.dp),
)

/** A dim line saying there is nothing to show. */
@Composable
fun WidgetEmpty(text: String) = Text(
    text,
    style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant),
    modifier = GlanceModifier.padding(4.dp),
)

/** A task's circle (checked when done), or an event's dot, in `color`. */
@Composable
fun WidgetMark(task: Boolean, done: Boolean, color: androidx.glance.unit.ColorProvider, size: Dp = 20.dp) = Image(
    ImageProvider(if (!task) R.drawable.ic_widget_dot else if (done) R.drawable.ic_widget_ring_done else R.drawable.ic_widget_ring),
    null,
    GlanceModifier.size(size),
    colorFilter = ColorFilter.tint(color),
)

/** A list's colour for a widget. */
fun widgetColor(hex: String?) = FixedColor(parseColor(hex))

/** Opening the app on a task, a list or a day. */
fun openTask(context: Context, uid: String) =
    Intent(context, MainActivity::class.java).putExtra("task", uid).setData(Uri.parse("todav-task://$uid"))

fun openDay(context: Context, day: Long) =
    Intent(context, MainActivity::class.java).putExtra("day", day).setData(Uri.parse("todav-day://$day"))

// --- tasks ------------------------------------------------------------------------------------

/** A row of the task widget: a heading, or a task at its depth. */
private sealed interface TaskLine {
    data class Heading(val text: String) : TaskLine
    data class Task(val task: TodavTask, val depth: Int) : TaskLine
}

/**
 * The task widget, as the GNOME extension's task column: a list (or Upcoming, Important) under
 * its name, by category, subtasks under their tasks, done ones struck through; read-only, a
 * task opens in the app. Its account, list and add button are picked in its settings.
 */
class TasksWidget : TodavWidget() {
    @Composable
    override fun Content(context: Context, app: TodavApp, widget: Int, changes: Long) {
        val (title, color, href, lines) = remember(changes) { content(context, app, widget) }
        val add = Intent(context, MainActivity::class.java).setAction(MainActivity.NEW_TASK)
            .apply { if (href != UPCOMING && href != IMPORTANT) putExtra("list", href) }
            .setData(Uri.parse("todav-add://$widget"))
        WidgetFrame(context) {
            Row(GlanceModifier.fillMaxWidth().padding(start = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Row(
                    GlanceModifier.defaultWeight().clickable(actionStartActivity(openList(context, href))),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (color != null) {
                        WidgetMark(false, false, widgetColor(color), 14.dp)
                        Spacer(GlanceModifier.width(8.dp))
                    }
                    Text(
                        title,
                        maxLines = 1,
                        style = TextStyle(fontWeight = FontWeight.Bold, fontSize = 16.sp, color = GlanceTheme.colors.onSurface),
                    )
                }
                WidgetButtons(context, widget, add.takeIf { href != null })
            }
            if (lines.isEmpty()) WidgetEmpty(tr("All done"))
            LazyColumn {
                items(lines) { line ->
                    when (line) {
                        is TaskLine.Heading -> WidgetHeading(line.text)
                        is TaskLine.Task -> TaskRow(context, line)
                    }
                }
            }
        }
    }

    @Composable
    private fun TaskRow(context: Context, line: TaskLine.Task) {
        val t = line.task
        Row(
            GlanceModifier.fillMaxWidth().clickable(actionStartActivity(openTask(context, t.uid)))
                .padding(start = (4 + line.depth * 20).dp, top = 5.dp, bottom = 5.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            WidgetMark(true, t.done, GlanceTheme.colors.onSurfaceVariant)
            Spacer(GlanceModifier.width(10.dp))
            Text(
                t.summary,
                maxLines = 1,
                style = TextStyle(
                    color = if (t.done) GlanceTheme.colors.onSurfaceVariant else GlanceTheme.colors.onSurface,
                    textDecoration = if (t.done) TextDecoration.LineThrough else null,
                ),
            )
        }
    }

    private data class Shown(val title: String, val color: String?, val href: String?, val lines: List<TaskLine>)

    /** The widget's list in its account (else Upcoming), its colour, and its rows. */
    private fun content(context: Context, app: TodavApp, widget: Int): Shown {
        val core = app.core
        val all = core.byPlace(core.lists())
        val account = widgetAccount(context, app, widget, all)
        val mine = all.filter { it.account == account }
        val href = widgetList(context, app, widget, mine)
        val inAccount = { t: TodavTask -> mine.any { it.href == t.listHref } }
        val now = core.localNow()
        return when (href) {
            null -> Shown("Todav", null, null, emptyList())
            UPCOMING -> Shown(
                tr("Upcoming"), null, href,
                core.upcoming().filter(inAccount).groupBy { sectionTitle(section(it, now)) }
                    .flatMap { (title, ts) -> listOf(TaskLine.Heading(title)) + ts.map { TaskLine.Task(it, 0) } },
            )
            IMPORTANT -> {
                val tasks = core.important().filter(inAccount)
                Shown(
                    tr("Important"), null, href,
                    mine.flatMap { l ->
                        val here = tasks.filter { it.listHref == l.href }
                        if (here.isEmpty()) emptyList() else listOf(TaskLine.Heading(l.displayName)) + here.map { TaskLine.Task(it, 0) }
                    },
                )
            }
            else -> {
                val list = mine.first { it.href == href }
                val order = core.order()
                val groups = core.groupedBy(href, order.tasks, order.subtasks, core.doneView())
                // Category headings only when there are categories, as in the extension.
                val titled = groups.any { it.name != null }
                val lines = groups.flatMap { g ->
                    val depths = HashMap<String, Int>()
                    val rows = g.tasks.map { t ->
                        val depth = t.parentUid?.let { depths[it]?.plus(1) } ?: 0
                        depths[t.uid] = depth
                        TaskLine.Task(t, depth)
                    }
                    if (titled) listOf(TaskLine.Heading(g.name ?: ptr("category", "Other"))) + rows else rows
                }
                Shown(list.displayName, listColor(list), href, lines)
            }
        }
    }

    private fun openList(context: Context, href: String?) =
        Intent(context, MainActivity::class.java).putExtra("list", href).setData(Uri.parse("todav-list://$href"))
}

class TasksWidgetReceiver : TodavWidgetReceiver() {
    override val glanceAppWidget = TasksWidget()
}

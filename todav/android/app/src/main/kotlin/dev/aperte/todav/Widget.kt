package dev.aperte.todav

import android.appwidget.AppWidgetManager
import kotlinx.coroutines.launch
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
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
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

    /** Whether the widgets show a settings button (else: the launcher's own, Settings → Widgets). */
    fun gear(c: Context) = prefs(c).getBoolean("gear", false)
    fun setGear(c: Context, v: Boolean) = prefs(c).edit().putBoolean("gear", v).apply()

    /** Whether the widgets take the system's colours (from the wallpaper), else `color`'s. */
    fun dynamic(c: Context) = prefs(c).getBoolean("dynamic", true)
    fun setDynamic(c: Context, v: Boolean) = prefs(c).edit().putBoolean("dynamic", v).apply()

    /** The widgets' own colours (ARGB), by key: "bg_light", "text_light", "bg_dark", "text_dark". */
    val COLORS = mapOf("bg_light" to 0xFFF8F9FF.toInt(), "text_light" to 0xFF1A1C20.toInt(), "bg_dark" to 0xFF111318.toInt(), "text_dark" to 0xFFE2E2E9.toInt())
    fun color(c: Context, key: String) = prefs(c).getInt(key, COLORS.getValue(key))
    fun setColor(c: Context, key: String, v: Int) = prefs(c).edit().putInt(key, v).apply()

    /** The corners' radius, in dp. */
    fun radius(c: Context) = prefs(c).getInt("radius", 24)
    fun setRadius(c: Context, v: Int) = prefs(c).edit().putInt("radius", v).apply()

    /** A task widget's list (an href, `UPCOMING` or `IMPORTANT`); under the bare id, as before. */
    fun list(c: Context, id: Int) = prefs(c).getString("$id", null)
    fun setList(c: Context, id: Int, v: String) = prefs(c).edit().putString("$id", v).apply()

    /** The account a widget shows. */
    fun account(c: Context, id: Int) = prefs(c).getLong("$id.account", -1).takeIf { it >= 0 }
    fun setAccount(c: Context, id: Int, v: Long) = prefs(c).edit().putLong("$id.account", v).remove("$id").remove("$id.calendars").apply()

    /** Whether a calendar widget picks its own calendars (else: those shown in the app). */
    fun own(c: Context, id: Int) = prefs(c).getBoolean("$id.own", false)
    fun setOwn(c: Context, id: Int, v: Boolean) = prefs(c).edit().putBoolean("$id.own", v).apply()

    /** A calendar widget's own calendars and task lists (none: all of its account's). */
    fun calendars(c: Context, id: Int): Set<String> = prefs(c).getStringSet("$id.calendars", null) ?: emptySet()
    fun setCalendars(c: Context, id: Int, v: Set<String>) = prefs(c).edit().putStringSet("$id.calendars", v).apply()

    /** Whether a widget shows its add button. */
    fun add(c: Context, id: Int) = prefs(c).getBoolean("$id.add", true)
    fun setAdd(c: Context, id: Int, v: Boolean) = prefs(c).edit().putBoolean("$id.add", v).apply()

    /** Whether a task's circle on a widget (the task list, the agenda) completes or reopens it. */
    fun check(c: Context, id: Int) = prefs(c).getBoolean("$id.check", false)
    fun setCheck(c: Context, id: Int, v: Boolean) = prefs(c).edit().putBoolean("$id.check", v).apply()

    /** A removed widget's settings go. */
    fun forget(c: Context, id: Int) = prefs(c).edit().apply {
        for (k in listOf("$id", "$id.account", "$id.calendars", "$id.add", "$id.own", "$id.check")) remove(k)
    }.apply()
}

/** Counts widgets resized, for a running widget to lay itself out again. */
private val resized = kotlinx.coroutines.flow.MutableStateFlow(0)

/** Removing a widget removes its settings; resizing one lays it out again. */
abstract class TodavWidgetReceiver : GlanceAppWidgetReceiver() {
    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        super.onDeleted(context, appWidgetIds)
        appWidgetIds.forEach { WidgetPrefs.forget(context, it) }
    }

    // Glance does nothing on a resize in `SizeMode.Single`.
    override fun onAppWidgetOptionsChanged(context: Context, appWidgetManager: AppWidgetManager, appWidgetId: Int, newOptions: android.os.Bundle) {
        super.onAppWidgetOptionsChanged(context, appWidgetManager, appWidgetId, newOptions)
        resized.value++
        // One no longer running starts again.
        (context.applicationContext as TodavApp).scope.launch {
            runCatching { glanceAppWidget.update(context, GlanceAppWidgetManager(context).getGlanceIdBy(appWidgetId)) }
        }
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
    // One layout for every size the launcher gives (`LocalSize` the upright one): each would be
    // sent whole, and a big update may not arrive.
    override val sizeMode: SizeMode = SizeMode.Single

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as TodavApp
        app.awaitReady()
        val widget = GlanceAppWidgetManager(context).getAppWidgetId(id)
        // The app may not have opened in this process: times as the system shows them.
        hours24 = android.text.format.DateFormat.is24HourFormat(context)
        provideContent {
            // Its size upright (the narrowest and tallest the launcher gives; on its side the
            // bottom may be cut), again when resized.
            // ponytail: one layout for both orientations; a layout each (`SizeMode.Exact`) doubles
            // the update, too big for the launcher with the month planner.
            val resizes by resized.collectAsState()
            val size = androidx.compose.runtime.remember(resizes) {
                val options = AppWidgetManager.getInstance(context).getAppWidgetOptions(widget)
                androidx.compose.ui.unit.DpSize(
                    options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH).dp,
                    options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT).dp,
                ).takeIf { it.width > 0.dp && it.height > 0.dp }
            }
            val changes by app.changes.collectAsState()
            // All of it again: parts reading the widgets' settings would be skipped otherwise.
            androidx.compose.runtime.key(changes) {
                androidx.compose.runtime.CompositionLocalProvider(androidx.glance.LocalSize provides (size ?: androidx.glance.LocalSize.current)) {
                    if (WidgetPrefs.dynamic(context)) {
                        GlanceTheme { Content(context, app, widget, changes) }
                    } else {
                        GlanceTheme(ownColors(context)) { Content(context, app, widget, changes) }
                    }
                }
            }
        }
    }

    @Composable
    abstract fun Content(context: Context, app: TodavApp, widget: Int, changes: Long)
}

/**
 * The widgets' theme from their own colours: text and background for light and dark, with the
 * app's blue (as without the system's colours) for today and the add button.
 */
private fun ownColors(c: Context): androidx.glance.color.ColorProviders {
    fun scheme(dark: Boolean): androidx.compose.material3.ColorScheme {
        val bg = Color(WidgetPrefs.color(c, if (dark) "bg_dark" else "bg_light"))
        val text = Color(WidgetPrefs.color(c, if (dark) "text_dark" else "text_light"))
        val base = if (dark) {
            androidx.compose.material3.darkColorScheme(
                primary = Color(0xFF8EC4EE), onPrimary = Color(0xFF00325A),
                primaryContainer = Color(0xFF00468A), onPrimaryContainer = Color(0xFFD5E3FF),
            )
        } else {
            androidx.compose.material3.lightColorScheme(
                primary = Color(0xFF2F6CB6), onPrimary = Color.White,
                primaryContainer = Color(0xFFD5E3FF), onPrimaryContainer = Color(0xFF001B3C),
            )
        }
        return base.copy(
            surface = bg, background = bg, onSurface = text, onBackground = text,
            onSurfaceVariant = text.copy(alpha = 0.7f), outline = text.copy(alpha = 0.3f),
        )
    }
    return androidx.glance.material3.ColorProviders(light = scheme(false), dark = scheme(true))
}

/**
 * A Saturday's or Sunday's name or number in a widget: its text colour tinted red a little, for
 * light and dark, as the app tints them; `variant`: the secondary text's (the weekdays' names),
 * `dim`: faded (a day of another month).
 */
fun weekendColor(c: Context, variant: Boolean, dim: Boolean = false): androidx.glance.unit.ColorProvider {
    val (light, dark) = when {
        !WidgetPrefs.dynamic(c) -> {
            val a = if (variant) 0.7f else 1f
            Color(WidgetPrefs.color(c, "text_light")).copy(alpha = a) to Color(WidgetPrefs.color(c, "text_dark")).copy(alpha = a)
        }
        Build.VERSION.SDK_INT >= 31 -> {
            val (l, d) = dynamicLightColorScheme(c) to dynamicDarkColorScheme(c)
            if (variant) l.onSurfaceVariant to d.onSurfaceVariant else l.onSurface to d.onSurface
        }
        else -> Color(0xFF1A1C20) to Color(0xFFE2E2E9)
    }
    fun tint(text: Color, red: Color) = androidx.compose.ui.graphics.lerp(text, red, 0.45f).let { if (dim) it.copy(alpha = 0.6f) else it }
    return ColorProvider(tint(light, Color(0xFFBA1A1A)), tint(dark, Color(0xFFFFB4AB)))
}

/**
 * A widget's background in the system's colours or its own, with the opacity and corners set for
 * all; `onClick` for a tap where nothing else in it takes one.
 */
@Composable
fun WidgetFrame(context: Context, onClick: Action? = null, content: @Composable ColumnScope.() -> Unit) {
    val alpha = WidgetPrefs.opacity(context) / 100f
    val (day, night) = when {
        !WidgetPrefs.dynamic(context) -> Color(WidgetPrefs.color(context, "bg_light")) to Color(WidgetPrefs.color(context, "bg_dark"))
        Build.VERSION.SDK_INT >= 31 -> dynamicLightColorScheme(context).surface to dynamicDarkColorScheme(context).surface
        else -> Color(0xFFF8F9FF) to Color(0xFF111318)
    }
    Column(
        GlanceModifier.fillMaxSize().background(ColorProvider(day.copy(alpha = alpha), night.copy(alpha = alpha)))
            .cornerRadius(WidgetPrefs.radius(context).dp).let { m -> onClick?.let { m.clickable(it) } ?: m }.padding(10.dp),
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

/** The settings button of widget `id` when they show, and its add button opening `add` when it shows one. */
@Composable
fun RowScope.WidgetButtons(context: Context, id: Int, add: Intent?) {
    // The data keeps each widget's intent apart: intents differing only in extras are one.
    val settings = Intent(context, WidgetConfigure::class.java)
        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id)
        .setData(Uri.parse("todav-widget://$id"))
    if (WidgetPrefs.gear(context)) WidgetButton(R.drawable.ic_widget_settings, tr("Settings"), actionStartActivity(settings))
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

private val TICK_UID = ActionParameters.Key<String>("uid")
private val TICK_DONE = ActionParameters.Key<Boolean>("done")

/** A task's circle tapped on a widget that may (`WidgetPrefs.check`): completed or reopened. */
class Tick : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val uid = parameters[TICK_UID] ?: return
        val app = context.applicationContext as TodavApp
        app.awaitReady()
        app.core.setDoneUnasked(uid, parameters[TICK_DONE] ?: true)
        app.core.requestSync()
        app.changed()
        val id = GlanceAppWidgetManager(context).getAppWidgetId(glanceId)
        WidgetKind.of(context, id)?.widget()?.update(context, glanceId)
    }
}

/** Task `uid`'s circle; with `check`, tapping it (with room around it for a finger) ticks it. */
@Composable
fun TaskMark(uid: String, done: Boolean, check: Boolean, color: androidx.glance.unit.ColorProvider, size: Dp = 20.dp) {
    if (!check) return WidgetMark(true, done, color, size)
    Box(
        GlanceModifier.cornerRadius(size).clickable(actionRunCallback<Tick>(actionParametersOf(TICK_UID to uid, TICK_DONE to !done)))
            .padding(6.dp),
    ) { WidgetMark(true, done, color, size) }
}

/** A list's colour for a widget. */
fun widgetColor(hex: String?) = FixedColor(parseColor(hex))

/** Opening the app on a task, a list or a day. */
fun openTask(context: Context, uid: String) =
    Intent(context, MainActivity::class.java).putExtra("task", uid).setData(Uri.parse("todav-task://$uid"))

/** Opening the calendar on `day`; with `month`, in the month view. */
fun openDay(context: Context, day: Long, month: Boolean = false) =
    Intent(context, MainActivity::class.java).putExtra("day", day).setData(Uri.parse("todav-day://$day/$month"))
        .apply { if (month) putExtra("view", "month") }

/** Opening an event (this occurrence) in its editor, as a task opens in its. */
fun openEvent(context: Context, e: dev.aperte.todav.core.Event) =
    Intent(context, MainActivity::class.java)
        .putExtra("event", e.uid).putExtra("event_list", e.listHref).putExtra("event_start", e.start).putExtra("all_day", e.allDay)
        .apply { e.recurrenceId?.let { putExtra("event_rid", it) } }
        .setData(Uri.parse("todav-event://${e.listHref.hashCode()}/${e.uid}/${e.start}"))

/** Opening a calendar item: an event or a task, in its editor. */
fun openItem(context: Context, item: dev.aperte.todav.core.Item) = when (item) {
    is dev.aperte.todav.core.Item.Event -> openEvent(context, item.event)
    is dev.aperte.todav.core.Item.Task -> openTask(context, item.task.uid)
}

/** Glance's rows and columns take 10 children at most: `parts` in nested ones of 10 when more. */
@Composable
fun Chunked(vertical: Boolean, modifier: GlanceModifier, parts: List<@Composable () -> Unit>) {
    val group = @Composable { m: GlanceModifier, ps: List<@Composable () -> Unit> ->
        if (vertical) Column(m) { ps.forEach { it() } } else Row(m) { ps.forEach { it() } }
    }
    if (parts.size <= 10) {
        group(modifier, parts)
    } else {
        val inner = if (vertical) GlanceModifier.fillMaxWidth() else GlanceModifier
        group(modifier, parts.chunked(10).map { c -> @Composable { group(inner, c) } })
    }
}

// --- tasks ------------------------------------------------------------------------------------

/** A row of the task widget: a heading, or a task at its depth. */
private sealed interface TaskLine {
    data class Heading(val text: String) : TaskLine
    data class Task(val task: TodavTask, val depth: Int) : TaskLine
}

/**
 * The task widget, as the GNOME extension's task column: a list (or Upcoming, Important) under
 * its name, by category, subtasks under their tasks, done ones struck through; a task opens in
 * the app, and its circle ticks it if the widget's settings say so. Its account, list and add
 * button are picked there too.
 */
class TasksWidget : TodavWidget() {
    @Composable
    override fun Content(context: Context, app: TodavApp, widget: Int, changes: Long) {
        val (title, color, href, lines) = remember(changes) { content(context, app, widget) }
        val check = WidgetPrefs.check(context, widget)
        val add = Intent(context, MainActivity::class.java).setAction(MainActivity.NEW_TASK)
            .apply { if (href != UPCOMING && href != IMPORTANT) putExtra("list", href) }
            .setData(Uri.parse("todav-add://$widget"))
        // Anywhere but a task or a button: the list in the app, as its name.
        val list = actionStartActivity(openList(context, href))
        WidgetFrame(context, list.takeIf { href != null }) {
            Row(GlanceModifier.fillMaxWidth().padding(start = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Row(
                    GlanceModifier.defaultWeight().clickable(list),
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
                        is TaskLine.Task -> TaskRow(context, line, check)
                    }
                }
            }
        }
    }

    @Composable
    private fun TaskRow(context: Context, line: TaskLine.Task, check: Boolean) {
        val t = line.task
        Row(
            GlanceModifier.fillMaxWidth().clickable(actionStartActivity(openTask(context, t.uid)))
                .padding(start = (4 + line.depth * 20).dp, top = if (check) 0.dp else 5.dp, bottom = if (check) 0.dp else 5.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TaskMark(t.uid, t.done, check, GlanceTheme.colors.onSurfaceVariant)
            Spacer(GlanceModifier.width(if (check) 4.dp else 10.dp))
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

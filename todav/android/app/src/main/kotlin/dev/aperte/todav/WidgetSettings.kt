package dev.aperte.todav

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.TextButton
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import dev.aperte.todav.core.colorPresets
import dev.aperte.todav.ui.parseColor
import androidx.compose.foundation.layout.Column
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.CalendarViewDay
import androidx.compose.material.icons.outlined.CalendarViewMonth
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material.icons.outlined.ViewAgenda
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.core.listColor
import dev.aperte.todav.ui.ColorMark
import dev.aperte.todav.ui.Item
import dev.aperte.todav.ui.PickRow
import dev.aperte.todav.ui.Section
import dev.aperte.todav.ui.SettingsFrame
import dev.aperte.todav.ui.TodavTheme
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/** A widget's settings, from its settings button and when it is placed. */
class WidgetConfigure : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val widget = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        if (widget == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()
        // Every setting has a default: leaving at once still places the widget.
        setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widget))
        I18n.load(this)
        hours24 = android.text.format.DateFormat.is24HourFormat(this)
        enableEdgeToEdge()
        val app = application as TodavApp
        setContent {
            val changes by app.changes.collectAsStateWithLifecycle()
            val scheme = remember(changes) { app.core.setting("color_scheme") }
            TodavTheme(scheme) { WidgetPage(app, widget, ::finish) }
        }
    }
}

/** What widget `id` shows, in a line: its list, or its account's calendars picked. */
private fun summary(context: Context, app: TodavApp, kind: WidgetKind, id: Int): String {
    val core = app.core
    return if (kind == WidgetKind.TASKS) {
        val all = core.byPlace(core.lists())
        val account = widgetAccount(context, app, id, all)
        when (val h = widgetList(context, app, id, all.filter { it.account == account })) {
            UPCOMING -> tr("Upcoming")
            IMPORTANT -> tr("Important")
            else -> all.firstOrNull { it.href == h }?.displayName ?: ""
        }
    } else if (!WidgetPrefs.own(context, id)) {
        tr("Calendars shown in the app")
    } else {
        widgetCalendars(context, app, id).second.joinToString(", ") { it.displayName }
    }
}

private fun icon(kind: WidgetKind) = when (kind) {
    WidgetKind.TASKS -> Icons.Outlined.TaskAlt
    WidgetKind.MONTH -> Icons.Outlined.CalendarMonth
    WidgetKind.AGENDA -> Icons.Outlined.ViewAgenda
    WidgetKind.DAY -> Icons.Outlined.CalendarViewDay
    WidgetKind.PLANNER -> Icons.Outlined.CalendarViewMonth
}

/**
 * Drawing the widgets again after a change of their settings: at once, while this shows (the
 * app's own redraw after changes settle may not run before the system freezes it in the
 * background), and the app's views too.
 */
@Composable
private fun redraw(app: TodavApp): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return {
        app.changed()
        scope.launch { WidgetKind.refreshAll(context) }
    }
}

/**
 * Settings → Widgets, while some are placed: how all of them look (the background's opacity, the
 * corners), then each widget, opening its own settings.
 */
@Composable
fun WidgetsSettings(model: Main) {
    val context = LocalContext.current
    val app = model.todav
    val changes by app.changes.collectAsStateWithLifecycle()
    var opacity by remember { mutableFloatStateOf(WidgetPrefs.opacity(context).toFloat()) }
    var radius by remember { mutableFloatStateOf(WidgetPrefs.radius(context).toFloat()) }
    val redraw = redraw(app)
    Section(tr("General")) {
        Item(
            headlineContent = { Text(tr("Background Opacity")) },
            supportingContent = {
                Column {
                    Text("${opacity.roundToInt()} %")
                    Slider(
                        opacity, { opacity = it }, valueRange = 0f..100f, steps = 19,
                        onValueChangeFinished = {
                            WidgetPrefs.setOpacity(context, opacity.roundToInt())
                            redraw()
                        },
                    )
                }
            },
        )
        Item(
            headlineContent = { Text(tr("Corner Radius")) },
            supportingContent = {
                Column {
                    Text("${radius.roundToInt()} dp")
                    Slider(
                        radius, { radius = it }, valueRange = 0f..32f, steps = 7,
                        onValueChangeFinished = {
                            WidgetPrefs.setRadius(context, radius.roundToInt())
                            redraw()
                        },
                    )
                }
            },
        )
        var gear by remember { mutableStateOf(WidgetPrefs.gear(context)) }
        val setGear = { on: Boolean ->
            gear = on
            WidgetPrefs.setGear(context, on)
            redraw()
        }
        Item(
            headlineContent = { Text(tr("Settings Button")) },
            supportingContent = { Text(tr("Without it, a widget's settings open from its menu on the home screen")) },
            trailingContent = { Switch(gear, setGear) },
            modifier = Modifier.clickable { setGear(!gear) },
        )
    }
    // The system's colours, or the widgets' own: background and text for light and dark.
    var dynamic by remember { mutableStateOf(WidgetPrefs.dynamic(context)) }
    val setDynamic = { on: Boolean ->
        dynamic = on
        WidgetPrefs.setDynamic(context, on)
        redraw()
    }
    Section(tr("Colors")) {
        Item(
            headlineContent = { Text(tr("Dynamic Colors")) },
            supportingContent = { Text(tr("The system's colors, from the wallpaper")) },
            trailingContent = { Switch(dynamic, setDynamic) },
            modifier = Modifier.clickable { setDynamic(!dynamic) },
        )
    }
    if (!dynamic) {
        for ((title, mode) in listOf(tr("Light Mode") to "light", tr("Dark Mode") to "dark")) {
            Section(title) {
                ColorRow(tr("Background"), "bg_$mode", redraw)
                ColorRow(tr("Text"), "text_$mode", redraw)
            }
        }
    }
    // A section for each kind placed, a row for each widget of it.
    val placed = remember(changes) { WidgetKind.placed(context) }
    placed.groupBy { it.first }.forEach { (kind, widgets) ->
        Section(kind.title()) {
            widgets.forEach { (_, id) ->
                Item(
                    headlineContent = { Text(remember(changes) { summary(context, app, kind, id) }) },
                    leadingContent = { Icon(icon(kind), null) },
                    trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                    modifier = Modifier.clickable { model.open(Dest.WidgetSettings(id)) },
                )
            }
        }
    }
}

/**
 * Widget `id`'s settings: its account (when there is a choice), its list or calendars, and its
 * add button. Each change shows on the widget at once.
 */
@Composable
fun WidgetPage(app: TodavApp, id: Int, onBack: () -> Unit) {
    val context = LocalContext.current
    val kind = remember { WidgetKind.of(context, id) }
    // Read again after each change made here.
    var version by remember { mutableIntStateOf(0) }
    val redraw = redraw(app)
    val changed = {
        version++
        redraw()
    }
    SettingsFrame(kind?.title() ?: tr("Widgets"), onBack) {
        if (kind == null) return@SettingsFrame
        key(version) {
            val core = app.core
            val tasks = kind == WidgetKind.TASKS
            val all = remember(version) {
                if (tasks) core.byPlace(core.lists()) else core.byPlace((core.calendars() + core.lists()).distinctBy { it.href })
            }
            val accounts = remember(version) { core.accounts().filter { a -> a.enabled && all.any { it.account == a.id } } }
            val account = widgetAccount(context, app, id, all)
            val mine = all.filter { it.account == account }
            Section(null) {
                // A calendar widget's account goes with its own calendars, below.
                if (tasks && accounts.size > 1) {
                    PickRow(tr("Account"), accounts.map { accountName(it) }, accounts.indexOfFirst { it.id == account }) { i ->
                        WidgetPrefs.setAccount(context, id, accounts[i].id)
                        changed()
                    }
                }
                if (tasks) {
                    val keys = listOf(UPCOMING, IMPORTANT) + mine.map { it.href }
                    val names = listOf(tr("Upcoming"), tr("Important")) + mine.map { it.displayName }
                    PickRow(tr("List"), names, keys.indexOf(widgetList(context, app, id, mine))) { i ->
                        WidgetPrefs.setList(context, id, keys[i])
                        changed()
                    }
                }
                Item(
                    headlineContent = { Text(tr("Add Button")) },
                    supportingContent = { Text(if (tasks) tr("Adds a task to the list shown") else tr("Adds an event")) },
                    trailingContent = {
                        Switch(WidgetPrefs.add(context, id), {
                            WidgetPrefs.setAdd(context, id, it)
                            changed()
                        })
                    },
                )
                // Where tasks show with their circles.
                if (tasks || kind == WidgetKind.AGENDA) {
                    Item(
                        headlineContent = { Text(tr("Check Off Tasks")) },
                        supportingContent = { Text(tr("A task’s circle completes or reopens it")) },
                        trailingContent = {
                            Switch(WidgetPrefs.check(context, id), {
                                WidgetPrefs.setCheck(context, id, it)
                                changed()
                            })
                        },
                    )
                }
            }
            if (!tasks) {
                val own = WidgetPrefs.own(context, id)
                // None picked means all; at least one stays picked.
                val picked = WidgetPrefs.calendars(context, id).filter { h -> mine.any { it.href == h } }.toSet()
                    .ifEmpty { mine.map { it.href }.toSet() }
                Section(tr("Calendars")) {
                    val setOwn = { on: Boolean ->
                        WidgetPrefs.setOwn(context, id, on)
                        changed()
                    }
                    Item(
                        headlineContent = { Text(tr("Choose Calendars")) },
                        supportingContent = { Text(tr("Otherwise those shown in the app")) },
                        trailingContent = { Switch(own, setOwn) },
                        modifier = Modifier.clickable { setOwn(!own) },
                    )
                    if (!own) return@Section
                    if (accounts.size > 1) {
                        PickRow(tr("Account"), accounts.map { accountName(it) }, accounts.indexOfFirst { it.id == account }) { i ->
                            WidgetPrefs.setAccount(context, id, accounts[i].id)
                            changed()
                        }
                    }
                    mine.forEach { l ->
                        val on = l.href in picked
                        val toggle = {
                            val next = if (on) picked - l.href else picked + l.href
                            if (next.isNotEmpty()) {
                                WidgetPrefs.setCalendars(context, id, if (next.size == mine.size) emptySet() else next)
                                changed()
                            }
                        }
                        Item(
                            headlineContent = { Text(l.displayName) },
                            leadingContent = { ColorMark(listColor(l)) },
                            trailingContent = { Checkbox(on, { toggle() }) },
                            modifier = Modifier.clickable { toggle() },
                        )
                    }
                }
            }
        }
    }
}

/** A widget colour (`key` of `WidgetPrefs.COLORS`), its swatch opening a dialog to change it. */
@Composable
private fun ColorRow(title: String, key: String, redraw: () -> Unit) {
    val context = LocalContext.current
    var color by remember { mutableIntStateOf(WidgetPrefs.color(context, key)) }
    var open by remember { mutableStateOf(false) }
    Item(
        headlineContent = { Text(title) },
        supportingContent = { Text(hex(color)) },
        trailingContent = { Swatch(color, 32.dp) },
        modifier = Modifier.clickable { open = true },
    )
    if (open) {
        ColorDialog(title, color, { open = false }) {
            open = false
            color = it
            WidgetPrefs.setColor(context, key, it)
            redraw()
        }
    }
}

private fun hex(color: Int) = "#%06X".format(color and 0xFFFFFF)

/** A round swatch of `color`, outlined so a light one shows on a light surface. */
@Composable
private fun Swatch(color: Int, size: androidx.compose.ui.unit.Dp, picked: Boolean = false, onClick: (() -> Unit)? = null) {
    Box(
        Modifier.size(size).clip(CircleShape).background(Color(color))
            .border(if (picked) 3.dp else 1.dp, MaterialTheme.colorScheme.outline, CircleShape)
            .let { m -> onClick?.let { m.clickable(onClick = it) } ?: m },
    )
}

/** Greys from white to black for backgrounds and text, then Nextcloud's colours. */
private val NEUTRALS = listOf(
    0xFFFFFFFF, 0xFFF8F9FF, 0xFFECEEF4, 0xFFD0D3DA, 0xFF9FA3AB, 0xFF6B7078,
    0xFF44474E, 0xFF2B2E35, 0xFF1A1C20, 0xFF111318, 0xFF000000,
).map { it.toInt() }

/** Picking a colour: a swatch, or its hex code. */
@Composable
private fun ColorDialog(title: String, initial: Int, onDismiss: () -> Unit, onPick: (Int) -> Unit) {
    val swatches = remember { NEUTRALS + colorPresets().map { parseColor(it).toArgb() } }
    var picked by remember { mutableIntStateOf(initial) }
    var text by remember { mutableStateOf(hex(initial)) }
    val typed = Regex("#?[0-9A-Fa-f]{6}").matchEntire(text.trim())?.value?.removePrefix("#")?.toInt(16)?.let { it or 0xFF000000.toInt() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    swatches.forEach { c ->
                        Swatch(c, 36.dp, picked = c == picked) {
                            picked = c
                            text = hex(c)
                        }
                    }
                }
                OutlinedTextField(
                    text,
                    {
                        text = it
                        Regex("#?[0-9A-Fa-f]{6}").matchEntire(it.trim())?.let { m ->
                            picked = m.value.removePrefix("#").toInt(16) or 0xFF000000.toInt()
                        }
                    },
                    singleLine = true,
                    leadingIcon = { Swatch(typed ?: picked, 24.dp) },
                    isError = typed == null,
                )
            }
        },
        confirmButton = { TextButton(onClick = { onPick(typed ?: picked) }, enabled = typed != null) { Text(tr("Save")) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

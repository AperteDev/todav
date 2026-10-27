package dev.aperte.todav

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ListItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.ImageProvider
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.action.clickable
import androidx.glance.appwidget.CheckBox
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.lazy.LazyColumn as GlanceList
import androidx.glance.appwidget.lazy.items as glanceItems
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.text.FontWeight
import androidx.glance.text.Text as GlanceText
import androidx.glance.text.TextStyle
import androidx.glance.Image
import androidx.lifecycle.lifecycleScope
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.TodavTask
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import dev.aperte.todav.core.trees
import dev.aperte.todav.ui.TodavTheme
import kotlinx.coroutines.launch

private val UID = ActionParameters.Key<String>("uid")

/** Each widget's list, by widget: device-local by nature, so not in the synced settings. */
private fun lists(context: Context) = context.getSharedPreferences("widgets", Context.MODE_PRIVATE)

/**
 * The home-screen widget: a list's open tasks, ticked off in place, with "+" adding one in the
 * app; the Android counterpart of the GNOME extension. Its list is picked when it is placed
 * (else the one last opened).
 */
class TasksWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val app = context.applicationContext as TodavApp
        app.awaitReady()
        val widget = GlanceAppWidgetManager(context).getAppWidgetId(id)
        provideContent {
            // Read again on every change while the widget's session runs.
            val changes by app.changes.collectAsState()
            val (list, rows, now) = remember(changes) { content(context, app, widget) }
            val href = list?.href
            GlanceTheme {
                Column(
                    GlanceModifier.fillMaxSize().background(GlanceTheme.colors.widgetBackground).cornerRadius(24.dp).padding(12.dp),
                ) {
                    Row(GlanceModifier.fillMaxWidth().padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        GlanceText(
                            list?.displayName ?: "Todav",
                            style = TextStyle(fontWeight = FontWeight.Bold, fontSize = 18.sp, color = GlanceTheme.colors.onSurface),
                            modifier = GlanceModifier.defaultWeight().clickable(actionStartActivity(openList(context, href))),
                        )
                        if (href != null) {
                            Box(
                                GlanceModifier.size(36.dp).cornerRadius(18.dp).background(GlanceTheme.colors.primaryContainer)
                                    .clickable(actionStartActivity(Intent(context, MainActivity::class.java).setAction(MainActivity.NEW_TASK).putExtra("list", href))),
                                contentAlignment = Alignment.Center,
                            ) {
                                Image(ImageProvider(R.drawable.ic_widget_add), tr("Add Task"))
                            }
                        }
                    }
                    if (rows.isEmpty()) {
                        GlanceText(tr("All Done"), style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant))
                    }
                    GlanceList {
                        glanceItems(rows, itemId = { it.first.uid.hashCode().toLong() }) { (t, depth) ->
                            Row(GlanceModifier.fillMaxWidth().padding(start = (depth * 16).dp), verticalAlignment = Alignment.CenterVertically) {
                                CheckBox(
                                    checked = false,
                                    onCheckedChange = actionRunCallback<Tick>(actionParametersOf(UID to t.uid)),
                                )
                                Column(
                                    GlanceModifier.defaultWeight()
                                        .clickable(actionStartActivity(Intent(context, MainActivity::class.java).putExtra("task", t.uid))),
                                ) {
                                    GlanceText(t.summary, maxLines = 2, style = TextStyle(color = GlanceTheme.colors.onSurface))
                                    dueLabel(t, now)?.let {
                                        GlanceText(
                                            it,
                                            style = TextStyle(
                                                fontSize = 12.sp,
                                                color = if (dev.aperte.todav.core.overdue(t, now)) GlanceTheme.colors.error else GlanceTheme.colors.onSurfaceVariant,
                                            ),
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /** The widget's list, its open tasks (with their depth) and the time. */
    private fun content(context: Context, app: TodavApp, widget: Int): Triple<TodavList?, List<Pair<TodavTask, Int>>, Long> {
        val core = app.core
        val all = core.byPlace(core.lists())
        val href = lists(context).getString("$widget", null)?.takeIf { h -> all.any { it.href == h } }
            ?: core.setting("last_list")?.takeIf { h -> all.any { it.href == h } }
            ?: all.firstOrNull()?.href
        val order = core.order()
        // Top-level tasks with their subtasks, open ones only, as the list shows them.
        val rows = href?.let { h ->
            trees(core.ungroupedBy(h, order.tasks, order.subtasks, core.doneView()).filter { !it.done })
                .flatMap { t -> listOf(t.task to 0) + t.subtasks.map { it.task to it.depth.toInt() } }
        } ?: emptyList()
        return Triple(all.firstOrNull { it.href == href }, rows, core.localNow())
    }

    private fun openList(context: Context, href: String?) =
        Intent(context, MainActivity::class.java).putExtra("list", href)

    companion object {
        suspend fun refresh(context: Context) = runCatching { TasksWidget().updateAll(context) }
    }
}

/** A tick in the widget: done (its open subtasks stay, as from a notification), then a sync. */
class Tick : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val uid = parameters[UID] ?: return
        val app = context.applicationContext as TodavApp
        app.awaitReady()
        app.core.setDoneTree(uid, true, false, false)
        app.core.requestSync()
        TasksWidget.refresh(context)
    }
}

class TasksWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget = TasksWidget()
}

/** Picking a new widget's list. */
class WidgetConfigure : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val widget = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        setResult(RESULT_CANCELED)
        if (widget == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()
        I18n.load(this)
        enableEdgeToEdge()
        val app = application as TodavApp
        setContent {
            TodavTheme(null) {
                Pick(app) { href ->
                    lists(this).edit().putString("$widget", href).apply()
                    lifecycleScope.launch {
                        TasksWidget.refresh(this@WidgetConfigure)
                        setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widget))
                        finish()
                    }
                }
            }
        }
    }

    @OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
    @Composable
    private fun Pick(app: TodavApp, onPick: (String) -> Unit) {
        val lists = androidx.compose.runtime.remember { app.core.byPlace(app.core.lists()) }
        Scaffold(topBar = { TopAppBar(title = { Text(tr("Task list")) }) }) { padding ->
            LazyColumn(Modifier.fillMaxSize(), contentPadding = padding) {
                items(lists, key = { it.href }) { l ->
                    ListItem(headlineContent = { Text(l.displayName) }, modifier = Modifier.clickable { onPick(l.href) })
                }
            }
        }
    }
}

package dev.aperte.todav.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Main
import dev.aperte.todav.ntr
import dev.aperte.todav.tr

/**
 * An iCalendar file opened or shared with Todav: where its events and its tasks go (the
 * writable calendars and task lists, by account), then Import. As the Linux app's.
 */
@Composable
fun ImportDialog(model: Main) {
    val file by model.importing.collectAsStateWithLifecycle()
    val (name, ics) = file ?: return
    val core = model.core
    val count = remember(ics) { core.icsCount(ics) }
    val calendars = remember { core.byPlace(core.calendars()).filter { it.writable } }
    val lists = remember { core.byPlace(core.lists()).filter { it.writable } }
    val close = { model.importing.value = null }
    val events = count.events > 0u && calendars.isNotEmpty()
    val tasks = count.tasks > 0u && lists.isNotEmpty()
    if (!events && !tasks) {
        LaunchedEffect(ics) {
            model.messages.tryEmit(
                (
                    if (count.events + count.tasks == 0u) {
                        tr("{name} holds no events or tasks").replace("{name}", name)
                    } else {
                        tr("No calendar or task list to import into")
                    }
                    ) to null,
            )
            close()
        }
        return
    }
    var calendar by remember { mutableIntStateOf(0) }
    var list by remember { mutableIntStateOf(0) }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(tr("Import “{name}”?").replace("{name}", name)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                if (events) {
                    PickRow(
                        tr("Calendar"), calendars.map { it.displayName }, calendar,
                        subtitle = ntr("{n} event", "{n} events", count.events.toLong()).replace("{n}", "${count.events}"),
                    ) { calendar = it }
                }
                if (tasks) {
                    PickRow(
                        tr("Task list"), lists.map { it.displayName }, list,
                        subtitle = ntr("{n} task", "{n} tasks", count.tasks.toLong()).replace("{n}", "${count.tasks}"),
                    ) { list = it }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val eventsTo = calendars.getOrNull(calendar)?.href?.takeIf { events }
                val tasksTo = lists.getOrNull(list)?.href?.takeIf { tasks }
                close()
                model.act {
                    val n = core.importIcs(ics, eventsTo, tasksTo)
                    val total = (n.events + n.tasks).toLong()
                    model.todav.changed()
                    core.requestSync()
                    model.messages.tryEmit(
                        ntr("{n} item imported", "{n} items imported", total).replace("{n}", "$total") to null,
                    )
                }
            }) { Text(tr("Import")) }
        },
        dismissButton = { TextButton(onClick = close) { Text(tr("Cancel")) } },
    )
}

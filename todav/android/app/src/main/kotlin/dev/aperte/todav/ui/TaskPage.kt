package dev.aperte.todav.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.outlined.CalendarToday
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material.icons.outlined.LinkOff
import androidx.compose.material.icons.outlined.PlayArrow
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Main
import dev.aperte.todav.core.Due
import dev.aperte.todav.core.DueValue
import dev.aperte.todav.core.PriorityValue
import dev.aperte.todav.core.ReminderSpan
import dev.aperte.todav.core.TaskPatch
import dev.aperte.todav.core.TextValue
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.reminderOffset
import dev.aperte.todav.core.reminderSpan
import dev.aperte.todav.core.trees
import dev.aperte.todav.dayLabel
import dev.aperte.todav.reminderDirections
import dev.aperte.todav.reminderPresets
import dev.aperte.todav.reminderUnits
import dev.aperte.todav.tr

/**
 * A task's page: what the Linux app's edit dialog has (title, list, category, priority, event,
 * note, start, due date, reminders), its subtasks, and Delete. The fields are saved together
 * with Save (one undo step); subtasks change at once.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskPage(model: Main, uid: String) {
    // Read again on every change, for its subtasks and a sync that changed it.
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val task = remember(changes) { model.core.task(uid) }
    if (task == null) {
        LaunchedEffect(Unit) { model.back() }
        return
    }
    val core = model.core
    val lists = remember { core.byPlace(core.lists()).filter { it.writable || it.href == task.listHref } }
    var summary by rememberSaveable(uid) { mutableStateOf(task.summary) }
    var list by rememberSaveable(uid) { mutableStateOf(task.listHref) }
    var category by rememberSaveable(uid) { mutableStateOf(task.category ?: "") }
    var priority by rememberSaveable(uid) { mutableStateOf(task.priority) }
    var note by rememberSaveable(uid) { mutableStateOf(task.description ?: "") }
    var start by rememberSaveable(uid) { mutableStateOf(task.start) }
    var startAllDay by rememberSaveable(uid) { mutableStateOf(task.startAllDay) }
    var due by rememberSaveable(uid) { mutableStateOf(task.due) }
    var dueAllDay by rememberSaveable(uid) { mutableStateOf(task.dueAllDay) }
    var event by rememberSaveable(uid) { mutableStateOf(task.eventUid) }
    val reminders = remember(uid) { mutableStateListOf<Long>().apply { addAll(task.reminders) } }
    var confirmDelete by remember { mutableStateOf(false) }
    var discard by remember { mutableStateOf(false) }
    val categories = remember(list) { core.categories(list).map { it.name } }

    val changed = summary != task.summary || list != task.listHref || category != (task.category ?: "") ||
        priority != task.priority || note != (task.description ?: "") || start != task.start ||
        (start != null && startAllDay != task.startAllDay) || due != task.due ||
        (due != null && dueAllDay != task.dueAllDay) || event != task.eventUid || reminders.toList() != task.reminders

    fun save() {
        val patch = TaskPatch(
            summary = summary.trim().takeIf { it.isNotEmpty() && it != task.summary },
            description = note.takeIf { it != (task.description ?: "") },
            category = category.trim().takeIf { it != (task.category ?: "") },
            priority = PriorityValue(priority).takeIf { priority != task.priority },
            start = DueValue(start?.let { Due(it, startAllDay) }).takeIf { start != task.start || startAllDay != task.startAllDay },
            due = DueValue(due?.let { Due(it, dueAllDay) }).takeIf { due != task.due || dueAllDay != task.dueAllDay },
            reminders = reminders.toList().takeIf { it != task.reminders },
            event = TextValue(event).takeIf { event != task.eventUid },
        )
        val to = list.takeIf { it != task.listHref }
        model.together(tr("“{title}” saved").replace("{title}", summary.trim())) {
            core.updateTask(uid, patch)
            if (to != null) core.moveTask(uid, to)
        }
        model.back()
    }

    val close = { if (changed) discard = true else model.back() }
    androidx.activity.compose.BackHandler(enabled = changed) { discard = true }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(tr("Edit Task")) },
                navigationIcon = { Tip(tr("Cancel")) { IconButton(onClick = close) { Icon(Icons.Default.Close, tr("Cancel")) } } },
                actions = { TextButton(onClick = ::save, enabled = summary.isNotBlank()) { Text(tr("Save")) } },
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = 640.dp).fillMaxWidth().padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                task.syncError?.let { error ->
                    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
                        Column(Modifier.padding(16.dp)) {
                            Text(tr("Not saved on the server"), style = MaterialTheme.typography.titleSmall)
                            Text(error, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                OutlinedTextField(
                    summary,
                    { summary = it },
                    label = { Text(tr("Title")) },
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    modifier = Modifier.fillMaxWidth(),
                )
                ChoiceField(tr("List"), lists.map { it.displayName }, lists.indexOfFirst { it.href == list }) {
                    list = lists[it].href
                }
                SuggestField(tr("Category"), category, categories) { category = it }
                Text(tr("Priority"), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    PRIORITIES.forEachIndexed { i, p ->
                        SegmentedButton(
                            selected = priorityIndex(priority) == i,
                            onClick = { priority = p },
                            shape = SegmentedButtonDefaults.itemShape(i, PRIORITIES.size),
                            label = { Text(priorityName(p)) },
                        )
                    }
                }
                event?.let { e ->
                    val ev = remember(e) { core.event(e) }
                    ListItem(
                        headlineContent = { Text(ev?.summary ?: tr("Unknown event")) },
                        overlineContent = { Text(tr("For Event")) },
                        supportingContent = { ev?.let { Text(dayLabel(it.start, it.allDay, core.localNow())) } },
                        leadingContent = { Icon(Icons.Outlined.Event, null) },
                        trailingContent = {
                            IconButton(onClick = { event = null }) { Icon(Icons.Outlined.LinkOff, tr("Unlink From Event")) }
                        },
                        modifier = Modifier.clickable(enabled = ev != null) { ev?.let { model.showDay(it.start) } },
                    )
                }
                OutlinedTextField(
                    note,
                    { note = it },
                    label = { Text(tr("Note")) },
                    minLines = 3,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    modifier = Modifier.fillMaxWidth(),
                )
                DateTimeField(tr("Start"), start, startAllDay, Icons.Outlined.PlayArrow, { t, allDay ->
                    start = t
                    startAllDay = allDay
                })
                DateTimeField(tr("Due Date"), due, dueAllDay, Icons.Outlined.CalendarToday, { t, allDay ->
                    // A first due date gets the default reminder; a change between a date and a time
                    // takes the other kind's.
                    if (t != null && (due == null || allDay != dueAllDay)) {
                        reminders.clear()
                        reminders.add(core.defaultReminder(allDay))
                    }
                    if (t == null) reminders.clear()
                    due = t
                    dueAllDay = allDay
                })
                if (due != null) {
                    Reminders(model, reminders, dueAllDay, event = false)
                }
                Subtasks(model, task)
                Button(
                    onClick = { confirmDelete = true },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer,
                        contentColor = MaterialTheme.colorScheme.onErrorContainer,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(tr("Delete Task")) }
            }
        }
    }
    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text(tr("Delete Task?")) },
            text = { Text(tr("“{title}” and its subtasks will be deleted.").replace("{title}", task.summary)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    model.delete(task)
                    model.back()
                }) { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text(tr("Cancel")) } },
        )
    }
    if (discard) {
        AlertDialog(
            onDismissRequest = { discard = false },
            title = { Text(tr("Discard Changes?")) },
            confirmButton = {
                TextButton(onClick = {
                    discard = false
                    model.back()
                }) { Text(tr("Discard")) }
            },
            dismissButton = { TextButton(onClick = { discard = false }) { Text(tr("Keep Editing")) } },
        )
    }
}

/**
 * Reminders as offsets from the due time (or an event's start): each a preset or a custom
 * time span, as the Linux dialogs' rows; "Add Reminder" adds the default one.
 */
@Composable
fun Reminders(model: Main, reminders: MutableList<Long>, allDay: Boolean, event: Boolean) {
    val core = model.core
    val presets = reminderPresets(core.reminderPresets(allDay), allDay, event, core.allDayTime())
    SectionTitle(tr("Reminders"))
    // Rows whose user picked "Custom…" stay custom even when the span matches a preset.
    val custom = remember { mutableStateListOf<Int>() }
    reminders.forEachIndexed { i, offset ->
        val preset = presets.indexOfFirst { it.second == offset }.takeIf { i !in custom }
        Row(verticalAlignment = Alignment.CenterVertically) {
            ChoiceField(
                tr("Reminder"),
                presets.map { it.first } + tr("Custom…"),
                preset ?: presets.size,
                Modifier.weight(1f),
            ) { c ->
                if (c < presets.size) {
                    custom.remove(i)
                    reminders[i] = presets[c].second
                } else {
                    custom.add(i)
                }
            }
            IconButton(onClick = {
                custom.remove(i)
                reminders.removeAt(i)
            }) { Icon(Icons.Default.Close, tr("Remove Reminder")) }
        }
        if (preset == null) {
            val span = reminderSpan(offset)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = "${span.amount}",
                    onValueChange = { v ->
                        v.toLongOrNull()?.takeIf { it in 0..999 }?.let {
                            reminders[i] = reminderOffset(ReminderSpan(it, span.unit, span.before))
                        }
                    },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(),
                    modifier = Modifier.weight(0.7f),
                )
                ChoiceField("", reminderUnits(), span.unit.toInt(), Modifier.weight(1f)) {
                    reminders[i] = reminderOffset(ReminderSpan(span.amount, it.toUInt(), span.before))
                }
                ChoiceField("", reminderDirections(allDay), if (span.before) 0 else 1, Modifier.weight(1.3f)) {
                    reminders[i] = reminderOffset(ReminderSpan(span.amount, span.unit, it == 0))
                }
            }
        }
    }
    OutlinedButton(onClick = { reminders.add(core.defaultReminder(allDay)) }) { Text(tr("Add Reminder")) }
}

/** A task's subtasks, ticked and added at once (each its own undo step). */
@Composable
private fun Subtasks(model: Main, task: TodavTask) {
    val core = model.core
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val subtasks = remember(changes) {
        val all = core.tasks(task.listHref, true)
        trees(all).flatMap { listOf(it.task) + it.subtasks.map { s -> s.task } }
            .let { flat ->
                // Every task under this one, in tree order.
                val under = mutableSetOf(task.uid)
                flat.filter { t -> (t.parentUid in under).also { if (it) under += t.uid } }
            }
    }
    var text by rememberSaveable { mutableStateOf("") }
    SectionTitle(tr("Subtasks"))
    subtasks.forEach { s ->
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().clickable {
            model.open(dev.aperte.todav.Dest.Task(s.uid))
        }) {
            Checkbox(checked = s.done, onCheckedChange = { model.setDone(s, it) })
            Text(
                s.summary,
                textDecoration = if (s.done) TextDecoration.LineThrough else null,
                modifier = Modifier.weight(1f),
            )
        }
    }
    if (!task.done) {
        OutlinedTextField(
            text,
            { text = it },
            placeholder = { Text(tr("Add Subtask")) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = {
                model.add(task.listHref, text, task.uid, task.category)
                text = ""
            }),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

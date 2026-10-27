package dev.aperte.todav.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Notes
import androidx.compose.material.icons.outlined.CalendarToday
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material.icons.outlined.EventRepeat
import androidx.compose.material.icons.outlined.Pin
import androidx.compose.material.icons.outlined.Place
import androidx.compose.material.icons.outlined.Repeat
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.Title
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.material3.ScaffoldDefaults
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
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
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.aperte.todav.Dest
import dev.aperte.todav.Main
import dev.aperte.todav.core.Event
import dev.aperte.todav.core.EventPatch
import dev.aperte.todav.core.EventTime
import dev.aperte.todav.core.Freq
import dev.aperte.todav.core.Repeat
import dev.aperte.todav.core.RepeatValue
import dev.aperte.todav.core.Scope
import dev.aperte.todav.core.listColor
import dev.aperte.todav.core.plainRepeat
import dev.aperte.todav.ntr
import dev.aperte.todav.ptr
import dev.aperte.todav.tr

private val FREQS = listOf(Freq.DAILY, Freq.WEEKLY, Freq.MONTHLY, Freq.YEARLY)

/** The unit of "Every n …" for `freq`, in `n`'s form, as the Linux dialog's. */
private fun unit(freq: Freq, n: Long) = when (freq) {
    Freq.DAILY -> ntr("day", "days", n)
    Freq.WEEKLY -> ntr("week", "weeks", n)
    Freq.MONTHLY -> ntr("month", "months", n)
    else -> ntr("year", "years", n)
}

/**
 * A new event, or an occurrence of one to change: what the Linux event dialog has. Changing or
 * deleting an occurrence of a repeating event asks which occurrences it is for. In a details
 * sheet (`sheet`), Cancel goes back to the details, Save and Delete close it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EventPage(model: Main, dest: Dest.EventEdit, sheet: DetailsSheetState? = null) {
    val cancel = { sheet?.collapse() ?: model.back() }
    val done = { sheet?.close() ?: model.back() }
    val core = model.core
    // The occurrence edited: the one starting at `dest.start` (events() expands the series).
    val ev: Event? = remember(dest) {
        dest.uid?.let { uid ->
            core.events(dest.start - DAY, dest.start + 2 * DAY)
                .firstOrNull { it.uid == uid && it.start == dest.start }
                ?: core.event(uid)
        }
    }
    val calendars = remember {
        core.byPlace(core.calendars()).filter { it.writable || it.href == ev?.listHref }
    }
    val duration = remember { core.eventDuration() }
    var summary by rememberSaveable { mutableStateOf(ev?.summary ?: "") }
    var location by rememberSaveable { mutableStateOf(ev?.location ?: "") }
    var notes by rememberSaveable { mutableStateOf(ev?.description ?: "") }
    var calendar by rememberSaveable {
        mutableStateOf(
            ev?.listHref ?: dest.list
                ?: core.setting("last_calendar")?.takeIf { l -> calendars.any { it.href == l } }
                ?: calendars.firstOrNull()?.href,
        )
    }
    var allDay by rememberSaveable { mutableStateOf(ev?.allDay ?: dest.allDay) }
    var start by rememberSaveable { mutableStateOf(ev?.start ?: dest.start) }
    // An all-day event's end is the midnight after its last day; shown as that last day.
    var end by rememberSaveable {
        mutableStateOf(ev?.end ?: if (dest.allDay) dest.start + DAY else dest.start + duration)
    }
    val rule = ev?.rule
    val plain = remember(rule) { rule?.let { plainRepeat(it) } }
    val custom = rule != null && plain == null
    // 0: none, 1–4: daily … yearly, 5: the kept custom rule.
    var repeat by rememberSaveable {
        mutableStateOf(
            when {
                rule == null -> 0
                plain == null -> 5
                else -> FREQS.indexOf(plain.freq) + 1
            },
        )
    }
    var every by rememberSaveable { mutableStateOf(plain?.interval ?: 1L) }
    var until by rememberSaveable { mutableStateOf(plain?.until) }
    val reminders = remember {
        mutableStateListOf<Long>().apply {
            if (ev != null) addAll(ev.reminders.map { it - ev.start }) else add(core.defaultReminder(allDay))
        }
    }
    var askScope by remember { mutableStateOf<((Scope) -> Unit)?>(null) }
    var askDelete by remember { mutableStateOf(false) }
    var discard by remember { mutableStateOf(false) }

    fun time() = if (allDay) {
        val s = start - Math.floorMod(start, DAY)
        EventTime(s, maxOf(end - Math.floorMod(end, DAY), s + DAY).let { if (Math.floorMod(end, DAY) != 0L && it == s) it + DAY else it }, true)
    } else {
        EventTime(start, maxOf(end, start), false)
    }
    val newRepeat = if (repeat in 1..4) Repeat(FREQS[repeat - 1], every.coerceAtLeast(1), until) else null
    val repeatChanged = when {
        rule != null && repeat == 5 -> false
        rule != null -> plain != newRepeat
        else -> repeat != 0
    }
    val old = ev?.let { EventTime(it.start, it.end, it.allDay) }
    val patch = EventPatch(
        summary = summary.trim().takeIf { it.isNotEmpty() && it != ev?.summary },
        location = location.trim().takeIf { it != (ev?.location ?: "") },
        description = notes.takeIf { it != (ev?.description ?: "") },
        time = time().takeIf { it != old },
        reminders = reminders.toList().takeIf { ev == null || it != ev.reminders.map { r -> r - ev.start } },
        repeat = if (repeatChanged) RepeatValue(newRepeat) else null,
    )
    val moved = ev != null && calendar != ev.listHref
    // Named through `patch`: in a `with(patch)` the page's own `summary`, `location` and
    // `reminders` would be read instead, and an untouched event would count as changed.
    val nothing = patch.summary == null && patch.location == null && patch.description == null &&
        patch.time == null && patch.reminders == null && patch.repeat == null
    val changed = if (ev == null) summary.isNotBlank() else !nothing || moved

    fun save() {
        val list = calendar ?: return
        model.act { core.setSetting("last_calendar", list) }
        if (ev == null) {
            val title = summary.trim().ifEmpty { tr("New Event") }
            model.together {
                val uid = core.addEvent(list, title, time(), reminders.toList())
                val rest = EventPatch(
                    location = location.trim().takeIf { it.isNotEmpty() },
                    description = notes.takeIf { it.isNotBlank() },
                    repeat = newRepeat?.let { RepeatValue(it) },
                )
                if (rest.location != null || rest.description != null || rest.repeat != null) {
                    core.updateEvent(list, uid, null, Scope.ALL, rest)
                }
            }
            done()
            return
        }
        val apply = { scope: Scope ->
            model.together(tr("“{title}” saved").replace("{title}", summary.trim().ifEmpty { ev.summary })) {
                if (!nothing) core.updateEvent(ev.listHref, ev.uid, ev.recurrenceId, scope, patch)
                if (moved) core.moveEvent(ev.listHref, ev.uid, list)
            }
            done()
        }
        // How a series repeats, or which calendar it is in, is about all of it.
        if (ev.recurring && !nothing && !repeatChanged && !moved) askScope = apply else apply(Scope.ALL)
    }

    fun setAllDay(on: Boolean) {
        allDay = on
        if (on) {
            start -= Math.floorMod(start, DAY)
            end = maxOf(end - Math.floorMod(end, DAY) + DAY, start + DAY)
        } else {
            start = start - Math.floorMod(start, DAY) + 9 * 3600
            end = start + duration
        }
        reminders.clear()
        reminders.add(core.defaultReminder(on))
    }

    BackHandler(enabled = changed) { discard = true }
    sheet?.let { LaunchedEffect(changed) { it.locked = changed } }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (ev == null) tr("New Event") else tr("Event")) },
                windowInsets = if (sheet != null) WindowInsets(0) else TopAppBarDefaults.windowInsets,
                colors = if (sheet != null) TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent) else TopAppBarDefaults.topAppBarColors(),
            )
        },
        floatingActionButton = { EditorButtons(calendar != null, { if (changed) discard = true else cancel() }, ::save) },
        containerColor = if (sheet != null) Color.Transparent else MaterialTheme.colorScheme.background,
        contentWindowInsets = if (sheet != null) WindowInsets.navigationBars else ScaffoldDefaults.contentWindowInsets,
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth()) {
                if (calendars.isEmpty()) {
                    Text(
                        tr("No calendar to save to"),
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.padding(horizontal = 32.dp).padding(top = 16.dp),
                    )
                }
                Section(null) {
                    FieldItem(
                        summary,
                        { summary = it },
                        tr("Title"),
                        icon = Icons.Outlined.Title,
                        textStyle = MaterialTheme.typography.titleLarge,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    )
                    FieldItem(location, { location = it }, tr("Location"), icon = Icons.Outlined.Place)
                    ChoiceItem(
                        tr("Calendar"),
                        calendars.map { it.displayName },
                        calendars.indexOfFirst { it.href == calendar },
                        mark = { ColorMark(calendars.getOrNull(it)?.let(::listColor)) },
                    ) { calendar = calendars[it].href }
                }
                Section(null) {
                    Item(
                        headlineContent = { Text(tr("All Day")) },
                        leadingContent = { Icon(Icons.Outlined.Schedule, null) },
                        trailingContent = { Switch(allDay, ::setAllDay) },
                        modifier = Modifier.clickable { setAllDay(!allDay) },
                    )
                    DateTimeField(tr("Starts"), start, allDay, Icons.Outlined.Event, { t, _ ->
                        if (t != null) {
                            // The end moves along, keeping the length.
                            end += t - start
                            start = t
                        }
                    }, clearable = false)
                    DateTimeField(
                        tr("Ends"),
                        if (allDay) end - DAY else end,
                        allDay,
                        Icons.Outlined.CalendarToday,
                        { t, _ -> if (t != null) end = if (allDay) maxOf(t + DAY, start + DAY) else maxOf(t, start) },
                        clearable = false,
                    )
                }
                Section(null) {
                    ChoiceItem(
                        tr("Repeat"),
                        listOf(tr("Does Not Repeat"), tr("Daily"), tr("Weekly"), tr("Monthly"), tr("Yearly")) +
                            if (custom) listOf(ptr("repeat", "Custom")) else emptyList(),
                        repeat,
                        icon = Icons.Outlined.Repeat,
                    ) { repeat = it }
                    if (repeat in 1..4) {
                        FieldItem(
                            "$every",
                            { v -> v.toLongOrNull()?.takeIf { it in 1..999 }?.let { every = it } },
                            tr("Every"),
                            icon = Icons.Outlined.Pin,
                            trailing = { Text(unit(FREQS[repeat - 1], every), Modifier.padding(end = 16.dp)) },
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        )
                        DateTimeField(tr("Until"), until, true, Icons.Outlined.EventRepeat, { t, _ -> until = t?.let { it - Math.floorMod(it, DAY) } }, dateOnly = true)
                    }
                }
                Section(null) {
                    FieldItem(
                        notes,
                        { notes = it },
                        tr("Notes"),
                        icon = Icons.AutoMirrored.Outlined.Notes,
                        minLines = 3,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    )
                }
                Reminders(model, reminders, allDay, event = true)
                if (ev != null) DeleteButton(tr("Delete Event")) { askDelete = true }
                Spacer(Modifier.height(EDITOR_END))
            }
        }
    }
    askScope?.let { apply ->
        ScopeDialog(tr("Change Repeating Event?"), true, delete = false, onDismiss = { askScope = null }) {
            askScope = null
            apply(it)
        }
    }
    if (askDelete && ev != null) {
        ScopeDialog(tr("Delete Repeating Event?"), ev.recurring, delete = true, onDismiss = { askDelete = false }) { scope ->
            askDelete = false
            model.act {
                core.deleteEvent(ev.listHref, ev.uid, ev.recurrenceId, scope)
                model.synced()
                model.messages.tryEmit(tr("“{title}” deleted").replace("{title}", ev.summary) to core.lastChange())
            }
            done()
        }
    }
    if (discard) {
        AlertDialog(
            onDismissRequest = { discard = false },
            title = { Text(tr("Discard Changes?")) },
            confirmButton = {
                TextButton(onClick = {
                    discard = false
                    sheet?.locked = false
                    cancel()
                }) { Text(tr("Discard")) }
            },
            dismissButton = { TextButton(onClick = { discard = false }) { Text(tr("Keep Editing")) } },
        )
    }
}

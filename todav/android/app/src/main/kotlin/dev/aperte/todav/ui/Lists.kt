package dev.aperte.todav.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import dev.aperte.todav.Main
import dev.aperte.todav.accountName
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.colorPresets
import dev.aperte.todav.core.listColor
import dev.aperte.todav.ntr
import dev.aperte.todav.tr

/** Export list `list` to an iCalendar file the user picks (`exportIcs`), with a message after. */
@Composable
private fun ExportButton(model: Main, list: TodavList) {
    val context = LocalContext.current
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/calendar")) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        model.act {
            val ics = model.core.exportIcs(list.href)
            context.contentResolver.openOutputStream(uri, "wt")!!.use { it.write(ics.toByteArray()) }
            model.messages.tryEmit(tr("“{name}” exported").replace("{name}", list.displayName) to null)
        }
    }
    TextButton(onClick = { save.launch("${list.displayName.replace('/', '-')}.ics") }) { Text(tr("Export…")) }
}

/**
 * A new list or calendar (`href` null; `events` picks calendar first), or the one at `href` to
 * rename, recolour or delete: the Linux app's list dialog.
 */
@Composable
fun ListDialog(model: Main, href: String?, events: Boolean, onDismiss: () -> Unit) {
    val core = model.core
    val list: TodavList? = remember(href) { href?.let { h -> (core.lists() + core.calendars()).firstOrNull { it.href == h } } }
    if (href != null && list == null) return onDismiss()
    val presets = remember { colorPresets() }
    val accounts = remember { core.accounts().associateBy { it.id } }
    val places = remember { core.places(events).filter { accounts[it]?.enabled == true } }
    // The system's calendars: changed in Todav only, and back to the system's at will.
    val system = list != null && accounts[list.account]?.kind == dev.aperte.todav.core.AccountKind.SYSTEM
    var name by rememberSaveable { mutableStateOf(list?.displayName ?: "") }
    var color by rememberSaveable { mutableStateOf(list?.let { listColor(it) } ?: presets.first()) }
    var calendar by rememberSaveable { mutableStateOf(list?.holdsEvents ?: events) }
    var place by rememberSaveable { mutableStateOf(places.firstOrNull()) }
    var confirm by remember { mutableStateOf(false) }
    val title = when {
        list != null && list.holdsEvents -> tr("Edit Calendar")
        list != null -> tr("Edit List")
        calendar -> tr("New Calendar")
        else -> tr("New List")
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (list == null) {
                    Connected(listOf(tr("Task List"), tr("Calendar")), if (calendar) 1 else 0, Modifier.fillMaxWidth()) {
                        calendar = it == 1
                    }
                }
                Group {
                    FieldItem(
                        name,
                        { name = it },
                        tr("Name"),
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                    if (list == null && places.size > 1) {
                        ChoiceItem(
                            tr("Save to"),
                            places.map { accounts[it]?.let(::accountName) ?: "" },
                            places.indexOf(place).coerceAtLeast(0),
                            containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                        ) { place = places[it] }
                    }
                }
                if (system) {
                    Text(
                        tr("Only in Todav: the calendar keeps its own name and color elsewhere"),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Text(tr("Color"), style = MaterialTheme.typography.labelLarge)
                ColorSwatches(presets, color, { color = it })
                if (list != null && system) {
                    TextButton(onClick = {
                        onDismiss()
                        model.act { core.resetList(list.href) }
                    }) { Text(tr("Use the System's Name and Color")) }
                }
                if (list != null) ExportButton(model, list)
                // A system calendar is the account's (DAVx⁵, Google): turned off in its page, not deleted.
                if (list != null && list.writable && !system) {
                    TextButton(onClick = { confirm = true }) {
                        Text(
                            if (list.holdsEvents) tr("Delete Calendar") else tr("Delete List"),
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            }
        },
        confirmButton = {
            Button(
                enabled = name.isNotBlank() && (list != null || place != null),
                onClick = {
                    onDismiss()
                    model.act {
                        if (list == null) {
                            val h = core.createList(place!!, name.trim(), color, calendar)
                            if (!calendar) model.select(h)
                        } else {
                            core.editList(list.href, name.trim(), color)
                        }
                        model.synced()
                    }
                },
            ) { Text(if (list == null) tr("Create") else tr("Save")) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
    if (confirm && list != null) {
        val n = remember { core.tasks(list.href, true).size.toLong() }
        val what = when {
            list.holdsEvents && n == 0L -> tr("“{name}” will be deleted with its events.")
            list.holdsEvents -> ntr(
                "“{name}” will be deleted with its events and {n} task.",
                "“{name}” will be deleted with its events and {n} tasks.",
                n,
            )
            n == 0L -> tr("“{name}” will be deleted.")
            else -> ntr("“{name}” will be deleted with its {n} task.", "“{name}” will be deleted with its {n} tasks.", n)
        }.replace("{name}", list.displayName).replace("{n}", "$n")
        // Only Nextcloud keeps what is deleted in a trash bin; other servers delete it for good.
        val nextcloud = core.account(list.account)?.kind == AccountKind.NEXTCLOUD
        val back = if (list.local) {
            tr("It is on this device only and cannot be restored.")
        } else if (nextcloud) {
            tr("It can be restored from the trash bin in Nextcloud Calendar.")
        } else {
            tr("It is deleted on the server too.")
        }
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(if (list.holdsEvents) tr("Delete Calendar?") else tr("Delete List?")) },
            text = { Text("$what $back") },
            confirmButton = {
                TextButton(onClick = {
                    confirm = false
                    onDismiss()
                    model.act {
                        core.deleteList(list.href)
                        model.synced()
                    }
                }) { Text(tr("Delete"), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text(tr("Cancel")) } },
        )
    }
}

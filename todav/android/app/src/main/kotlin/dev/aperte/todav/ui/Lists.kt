package dev.aperte.todav.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import dev.aperte.todav.Main
import dev.aperte.todav.accountName
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.colorPresets
import dev.aperte.todav.core.listColor
import dev.aperte.todav.ntr
import dev.aperte.todav.tr

/**
 * A new list or calendar (`href` null; `events` picks calendar first), or the one at `href` to
 * rename, recolour or delete: the Linux app's list dialog.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ListDialog(model: Main, href: String?, events: Boolean, onDismiss: () -> Unit) {
    val core = model.core
    val list: TodavList? = remember(href) { href?.let { h -> (core.lists() + core.calendars()).firstOrNull { it.href == h } } }
    if (href != null && list == null) return onDismiss()
    val presets = remember { colorPresets() }
    val accounts = remember { core.accounts().associateBy { it.id } }
    val places = remember { core.places().filter { accounts[it]?.enabled == true } }
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
                    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                        listOf(false, true).forEachIndexed { i, ev ->
                            SegmentedButton(
                                selected = calendar == ev,
                                onClick = { calendar = ev },
                                shape = SegmentedButtonDefaults.itemShape(i, 2),
                                label = { Text(if (ev) tr("Calendar") else tr("Task List")) },
                            )
                        }
                    }
                }
                OutlinedTextField(
                    name,
                    { name = it },
                    label = { Text(tr("Name")) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (list == null && places.size > 1) {
                    ChoiceField(
                        tr("Save to"),
                        places.map { accounts[it]?.let(::accountName) ?: "" },
                        places.indexOf(place).coerceAtLeast(0),
                    ) { place = places[it] }
                }
                Text(tr("Color"), style = MaterialTheme.typography.labelLarge)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    presets.forEach { c ->
                        val selected = c.equals(color.take(7), ignoreCase = true)
                        Box(
                            Modifier.size(36.dp).background(parseColor(c), CircleShape)
                                .border(if (selected) 3.dp else 0.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                                .clickable { color = c },
                            Alignment.Center,
                        ) {
                            if (selected) Icon(Icons.Default.Check, null, tint = Color.White)
                        }
                    }
                }
                if (list != null && list.writable) {
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
            TextButton(
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
        val back = if (list.local) {
            tr("It is on this device only and cannot be restored.")
        } else {
            tr("It can be restored from the trash bin in Nextcloud Calendar.")
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

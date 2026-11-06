package dev.aperte.todav.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.outlined.FilterList
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import dev.aperte.todav.Main
import dev.aperte.todav.OTHER
import dev.aperte.todav.Screen
import dev.aperte.todav.ntr
import dev.aperte.todav.ptr
import dev.aperte.todav.tr

/**
 * The categories of the list shown: to show one only (or "Other"), a new one to add, and all of
 * them to put in order, rename or delete.
 */
@Composable
fun CategoryFilter(model: Main, screen: Screen, list: String) {
    var open by remember { mutableStateOf(false) }
    var naming by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf(false) }
    val names = screen.categories
    // All, each category, "Other" (once there are categories), then New Category… and Edit Categories….
    val other = if (names.isEmpty()) emptyList() else listOf(ptr("category", "Other"))
    val actions = listOf(tr("New Category…")) + if (names.isEmpty()) emptyList() else listOf(tr("Edit Categories…"))
    val options = listOf(tr("All Categories")) + names + other + actions
    Box {
        Tip(tr("Categories")) {
            IconButton(onClick = { open = true }) {
                Icon(
                    Icons.Outlined.FilterList,
                    tr("Categories"),
                    tint = if (screen.filter != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        PickMenu(
            open,
            { open = false },
            options,
            when (screen.filter) {
                null -> 0
                OTHER -> names.size + 1
                else -> names.indexOf(screen.filter) + 1
            },
        ) {
            val shown = 1 + names.size + other.size
            when {
                it == 0 -> model.filterBy(list, null)
                it == shown -> naming = true
                it > shown -> editing = true
                it > names.size -> model.filterBy(list, OTHER)
                else -> model.filterBy(list, names[it - 1])
            }
        }
    }
    if (naming) {
        CategoryDialog(null, { naming = false }) {
            naming = false
            model.addCategory(list, it)
        }
    }
    if (editing) CategoriesDialog(model, list, names) { editing = false }
}

/** A category's name: a new one (`current` null) or the one being edited, which `onDelete` deletes. */
@Composable
fun CategoryDialog(current: String?, onDismiss: () -> Unit, onDelete: (() -> Unit)? = null, onDone: (String) -> Unit) {
    var name by rememberSaveable { mutableStateOf(current ?: "") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (current == null) tr("New Category") else tr("Edit Category")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Group {
                    FieldItem(
                        name,
                        { name = it },
                        tr("Name"),
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { if (name.isNotBlank()) onDone(name.trim()) }),
                        containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    )
                }
                if (onDelete != null) {
                    TextButton(onClick = onDelete) { Text(tr("Delete Category"), color = MaterialTheme.colorScheme.error) }
                }
            }
        },
        confirmButton = {
            Button(enabled = name.isNotBlank(), onClick = { onDone(name.trim()) }) {
                Text(if (current == null) tr("Create") else tr("Save"))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

/** Category `name` of `list`: renamed, or deleted once asked again (its tasks stay). */
@Composable
fun CategoryEditor(model: Main, list: String, name: String, onDismiss: () -> Unit) {
    var confirm by remember { mutableStateOf(false) }
    if (confirm) {
        val n = remember { model.core.tasks(list, true).count { it.category == name }.toLong() }
        val body = if (n == 0L) {
            tr("“{name}” will be deleted.")
        } else {
            ntr(
                "“{name}” will be removed from its {n} task, which stays.",
                "“{name}” will be removed from its {n} tasks, which stay.",
                n,
            )
        }.replace("{name}", name).replace("{n}", "$n")
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(tr("Delete Category?")) },
            text = { Text(body) },
            confirmButton = {
                Button(
                    onClick = {
                        onDismiss()
                        model.deleteCategory(list, name)
                    },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                        contentColor = MaterialTheme.colorScheme.onError,
                    ),
                ) { Text(tr("Delete")) }
            },
            dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
        )
    } else {
        CategoryDialog(name, onDismiss, onDelete = { confirm = true }) { to ->
            onDismiss()
            model.renameCategory(list, name, to)
        }
    }
}

/**
 * The categories of `list` in their order: each moves by its handle, and opens to rename or
 * delete when tapped; the order is saved as it is dropped.
 */
@Composable
private fun CategoriesDialog(model: Main, list: String, names: List<String>, onDismiss: () -> Unit) {
    var moving by remember { mutableStateOf<Moving?>(null) }
    // The order it was dropped in, until the categories come back in it.
    var dropped by remember { mutableStateOf<List<String>?>(null) }
    LaunchedEffect(names) { dropped = null }
    var editing by remember { mutableStateOf<String?>(null) }
    val shown = dropped ?: names
    val row = with(LocalDensity.current) { 60.dp.toPx() } // a row and the gap below it
    val haptics = LocalHapticFeedback.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Edit Categories")) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                for (name in shown) {
                    val held = moving?.key == name
                    // The others step aside for the held one, eased; dropped, in the new place at once.
                    val aside by animateFloatAsState(if (held) 0f else moving?.offset(name, row) ?: 0f, label = "aside")
                    Row(
                        Modifier.fillMaxWidth().height(56.dp)
                            .zIndex(if (held) 1f else 0f)
                            .graphicsLayer { translationY = if (held) moving?.shift ?: 0f else if (moving != null) aside else 0f }
                            .background(
                                if (held) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surfaceContainerLowest,
                                RoundedCornerShape(12.dp),
                            )
                            .clickable { editing = name }
                            .padding(start = 16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(name, Modifier.weight(1f), maxLines = 1)
                        if (shown.size > 1) {
                            Icon(
                                Icons.Default.DragHandle,
                                tr("Drag to Move"),
                                Modifier.padding(16.dp).pointerInput(name) {
                                    detectVerticalDragGestures(
                                        onDragStart = {
                                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                            moving = Moving(name, shown)
                                        },
                                        onVerticalDrag = { change, dy ->
                                            change.consume()
                                            moving?.let { it.shift += dy }
                                        },
                                        onDragEnd = {
                                            val m = moving
                                            moving = null
                                            if (m != null && m.target(row) != m.from) {
                                                haptics.performHapticFeedback(HapticFeedbackType.GestureEnd)
                                                dropped = m.order(row)
                                                model.reorderCategories(list, m.order(row))
                                            }
                                        },
                                        onDragCancel = { moving = null },
                                    )
                                },
                            )
                        }
                    }
                    Box(Modifier.height(4.dp))
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Done")) } },
    )
    editing?.let { CategoryEditor(model, list, it) { editing = null } }
}

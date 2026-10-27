package dev.aperte.todav.ui

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.exclude
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.StarOutline
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenu
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.FloatingActionButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.TooltipAnchorPosition
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTooltipState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import dev.aperte.todav.date
import dev.aperte.todav.hours24
import dev.aperte.todav.ptr
import dev.aperte.todav.time
import dev.aperte.todav.tr
import kotlinx.coroutines.withTimeoutOrNull

const val DAY = 86_400L

/**
 * A long press on something that is also clicked (a drawer item): `f` runs and the click that
 * would follow is swallowed.
 */
fun Modifier.longPress(enabled: Boolean, f: () -> Unit): Modifier = if (!enabled) this else pointerInput(f) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        // Held: no lift, no move past the slop (a scroll) before the long-press time.
        val held = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
            while (true) {
                val e = awaitPointerEvent(PointerEventPass.Initial)
                val c = e.changes.firstOrNull { it.id == down.id } ?: break
                if (!c.pressed || (c.position - down.position).getDistance() > viewConfiguration.touchSlop) break
            }
        } == null
        if (held) {
            f()
            // The rest of the gesture is ours: the item underneath sees no click.
            do {
                val e = awaitPointerEvent(PointerEventPass.Initial)
                e.changes.forEach { it.consume() }
            } while (e.changes.any { it.pressed })
        }
    }
}

/** Nextcloud's priorities: 1–4 high (red), 5 medium (yellow), 6–9 low (blue), none. */
fun priorityColor(p: Long?): Color? = when (p) {
    null, 0L -> null
    in 1..4 -> Color(0xFFE01B24)
    5L -> Color(0xFFE5A50A)
    else -> Color(0xFF3584E4)
}

/** The priorities to pick, as the Linux app's: none, low, medium, high. */
val PRIORITIES = listOf<Long?>(null, 9, 5, 1)

fun priorityName(p: Long?): String = when (p) {
    null, 0L -> ptr("priority", "None")
    in 1..4 -> ptr("priority", "High")
    5L -> ptr("priority", "Medium")
    else -> ptr("priority", "Low")
}

/** The priority level's index in [PRIORITIES]. */
fun priorityIndex(p: Long?): Int = when (p) {
    null, 0L -> 0
    in 1..4 -> 3
    5L -> 2
    else -> 1
}

@Composable
fun StarIcon(priority: Long?) {
    val color = priorityColor(priority)
    if (color == null) {
        Icon(Icons.Outlined.StarOutline, tr("Priority"), tint = MaterialTheme.colorScheme.onSurfaceVariant)
    } else {
        Icon(Icons.Filled.Star, tr("Priority"), tint = color)
    }
}

/**
 * A date and maybe a time of day (local wall-clock seconds, as the core's), or none: chips that
 * open Material's date and time pickers, and a button clearing it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DateTimeField(
    label: String,
    at: Long?,
    allDay: Boolean,
    icon: ImageVector,
    onChange: (Long?, Boolean) -> Unit,
    defaultTime: Long = 9 * 3600,
    /** Off: always set (an event's start and end), the time not removable either. */
    clearable: Boolean = true,
    /** A date alone, never a time (an event's last repeat). */
    dateOnly: Boolean = false,
) {
    var pickDate by remember { mutableStateOf(false) }
    var pickTime by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth()) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            AssistChip(
                onClick = { pickDate = true },
                label = { Text(if (at == null) tr("Set Date") else date(at)) },
                leadingIcon = { Icon(icon, null) },
            )
            if (at != null && !dateOnly) {
                AssistChip(
                    onClick = { pickTime = true },
                    label = { Text(if (allDay) tr("Add Time") else time(at)) },
                )
                if (!allDay && clearable) {
                    IconButton(onClick = { onChange(at - Math.floorMod(at, DAY), true) }) {
                        Icon(Icons.Default.Close, tr("Remove Time"))
                    }
                }
            }
            if (at != null && clearable) {
                Row(Modifier.weight(1f)) {}
                TextButton(onClick = { onChange(null, true) }) { Text(tr("Clear")) }
            }
        }
    }
    if (pickDate) {
        val state = rememberDatePickerState(initialSelectedDateMillis = (at ?: (System.currentTimeMillis() / 1000)) * 1000)
        DatePickerDialog(
            onDismissRequest = { pickDate = false },
            confirmButton = {
                TextButton(onClick = {
                    pickDate = false
                    state.selectedDateMillis?.let { ms ->
                        // The picker's UTC midnight is the core's local midnight.
                        val day = ms / 1000 - Math.floorMod(ms / 1000, DAY)
                        val clock = if (at != null && !allDay) Math.floorMod(at, DAY) else 0
                        onChange(day + clock, at == null || allDay)
                    }
                }) { Text(tr("OK")) }
            },
            dismissButton = { TextButton(onClick = { pickDate = false }) { Text(tr("Cancel")) } },
        ) { DatePicker(state) }
    }
    if (pickTime && at != null) {
        val clock = if (allDay) defaultTime else Math.floorMod(at, DAY)
        val state = rememberTimePickerState((clock / 3600).toInt(), (clock / 60 % 60).toInt(), hours24)
        AlertDialog(
            onDismissRequest = { pickTime = false },
            confirmButton = {
                TextButton(onClick = {
                    pickTime = false
                    onChange(at - Math.floorMod(at, DAY) + state.hour * 3600L + state.minute * 60L, false)
                }) { Text(tr("OK")) }
            },
            dismissButton = { TextButton(onClick = { pickTime = false }) { Text(tr("Cancel")) } },
            text = { TimePicker(state) },
        )
    }
}

/** A read-only dropdown field: `options` by label, `selected` the index shown. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChoiceField(
    label: String,
    options: List<String>,
    selected: Int,
    modifier: Modifier = Modifier.fillMaxWidth(),
    enabled: Boolean = true,
    onSelect: (Int) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = open, onExpandedChange = { if (enabled) open = it }, modifier = modifier) {
        OutlinedTextField(
            value = options.getOrElse(selected) { "" },
            onValueChange = {},
            readOnly = true,
            enabled = enabled,
            singleLine = true,
            label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(open) },
            modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable, enabled),
        )
        ExposedDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEachIndexed { i, o ->
                DropdownMenuItem(text = { Text(o) }, onClick = {
                    open = false
                    onSelect(i)
                })
            }
        }
    }
}

/** A text field with suggestions it may also take a new value besides (categories). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SuggestField(label: String, value: String, suggestions: List<String>, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val shown = suggestions.filter { it.contains(value, ignoreCase = true) && it != value }
    ExposedDropdownMenuBox(expanded = open && shown.isNotEmpty(), onExpandedChange = { open = it }) {
        OutlinedTextField(
            value = value,
            onValueChange = {
                onChange(it)
                open = true
            },
            singleLine = true,
            label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(open) },
            modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryEditable),
        )
        ExposedDropdownMenu(expanded = open && shown.isNotEmpty(), onDismissRequest = { open = false }) {
            shown.forEach { s ->
                DropdownMenuItem(text = { Text(s) }, onClick = {
                    open = false
                    onChange(s)
                })
            }
        }
    }
}

/** A section's title on a form page. */
@Composable
fun SectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(top = 8.dp),
    )
}

/** `content` (an icon button) with `label` as a tooltip when it is held or hovered. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Tip(label: String, content: @Composable () -> Unit) = TooltipBox(
    positionProvider = TooltipDefaults.rememberTooltipPositionProvider(TooltipAnchorPosition.Below),
    tooltip = { PlainTooltip { Text(label) } },
    state = rememberTooltipState(),
    content = content,
)

/**
 * The add button (a task or an event, as `label` says): flat at the end of the navigation bar, or
 * `raised` in the bottom end corner where a rail stands in for the bar.
 */
@Composable
fun AddButton(label: String, raised: Boolean, onClick: () -> Unit) = Tip(label) {
    FloatingActionButton(
        onClick = onClick,
        elevation = if (raised) FloatingActionButtonDefaults.elevation() else FloatingActionButtonDefaults.bottomAppBarFabElevation(),
    ) {
        Icon(Icons.Default.Add, label)
    }
}

/**
 * An editor's actions, floating at the bottom end (over the keyboard while it is up): Save, as
 * big as the add button and in the accent colour, dimmed and doing nothing while `canSave` is
 * false; Cancel, small, above it.
 */
@Composable
fun EditorButtons(canSave: Boolean, onCancel: () -> Unit, onSave: () -> Unit) {
    Column(
        Modifier.windowInsetsPadding(WindowInsets.ime.exclude(WindowInsets.navigationBars)),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Tip(tr("Cancel")) {
            SmallFloatingActionButton(
                onClick = onCancel,
                containerColor = MaterialTheme.colorScheme.secondaryContainer,
                contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
            ) { Icon(Icons.Default.Close, tr("Cancel")) }
        }
        Tip(tr("Save")) {
            FloatingActionButton(
                onClick = { if (canSave) onSave() },
                containerColor = if (canSave) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHighest,
                contentColor = if (canSave) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.38f),
                modifier = Modifier.semantics { if (!canSave) disabled() },
            ) { Icon(Icons.Default.Check, tr("Save")) }
        }
    }
}

/** Room at the end of an editor's fields, so `EditorButtons` don't cover the last one. */
val EDITOR_END = 152.dp

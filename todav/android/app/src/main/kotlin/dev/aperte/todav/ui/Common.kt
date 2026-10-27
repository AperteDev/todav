package dev.aperte.todav.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.exclude
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.DropdownMenuGroup
import androidx.compose.material3.DropdownMenuPopup
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.SelectableDropdownMenuItem
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TimePickerDialog
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.ToggleButtonDefaults
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.StarOutline
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
import androidx.compose.runtime.rememberUpdatedState
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
 * A long press on something that is also clicked (a drawer item), and a drag after it: while
 * held, `onMove` gets how far it has been dragged up or down; lifted, `onEnd` whether it was
 * (false: a long press alone). The click that would follow is swallowed. Put it before any
 * modifier that moves what it is on, whose moves would otherwise add to the drag.
 */
@Composable
fun Modifier.holdToMove(enabled: Boolean, onMove: (Float) -> Unit, onEnd: (moved: Boolean) -> Unit): Modifier {
    // The gesture outlives recompositions: it calls the latest of these.
    val move by rememberUpdatedState(onMove)
    val end by rememberUpdatedState(onEnd)
    return if (!enabled) this else pointerInput(Unit) {
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
            if (!held) return@awaitEachGesture
            // The rest of the gesture is ours: the item underneath sees no click.
            var moved = false
            do {
                val e = awaitPointerEvent(PointerEventPass.Initial)
                e.changes.forEach { it.consume() }
                val c = e.changes.firstOrNull { it.id == down.id }
                if (c != null && c.pressed) {
                    val dy = c.position.y - down.position.y
                    if (moved || kotlin.math.abs(dy) > viewConfiguration.touchSlop) {
                        moved = true
                        move(dy)
                    }
                }
            } while (e.changes.any { it.pressed })
            end(moved)
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

/** Rows in one rounded container, under `title` if any: as Android's own settings group them. */
@Composable
fun Section(title: String?, content: @Composable ColumnScope.() -> Unit) {
    if (title != null) {
        Text(
            title,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(start = 32.dp, top = 24.dp, bottom = 8.dp),
        )
    } else {
        Spacer(Modifier.height(16.dp))
    }
    Group(Modifier.padding(horizontal = 16.dp), content)
}

/** Rows as one block: each a box of its own, the block's round corners at its ends. */
@Composable
fun Group(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) = Column(
    modifier.clip(RoundedCornerShape(24.dp)),
    verticalArrangement = Arrangement.spacedBy(2.dp),
    content = content,
)

private val ITEM_SHAPE = RoundedCornerShape(4.dp)

/** A row of a [Group]. */
@Composable
fun Item(
    headlineContent: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    overlineContent: @Composable (() -> Unit)? = null,
    supportingContent: @Composable (() -> Unit)? = null,
    leadingContent: @Composable (() -> Unit)? = null,
    trailingContent: @Composable (() -> Unit)? = null,
    containerColor: Color = MaterialTheme.colorScheme.surfaceContainer,
) {
    ListItem(
        modifier = Modifier.clip(ITEM_SHAPE).then(modifier),
        leadingContent = leadingContent,
        trailingContent = trailingContent,
        overlineContent = overlineContent,
        supportingContent = supportingContent,
        shapes = ListItemDefaults.shapes(shape = ITEM_SHAPE),
        colors = ListItemDefaults.segmentedColors(containerColor = containerColor),
        content = headlineContent,
    )
}

/**
 * A text field as a row of a [Group]: `label` over what is typed (or `placeholder` in it while
 * empty), `icon` before it.
 */
@Composable
fun FieldItem(
    value: String,
    onValueChange: (String) -> Unit,
    label: String?,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    placeholder: String? = null,
    trailing: @Composable (() -> Unit)? = null,
    textStyle: TextStyle = LocalTextStyle.current,
    singleLine: Boolean = false,
    minLines: Int = 1,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    containerColor: Color = MaterialTheme.colorScheme.surfaceContainer,
) = TextField(
    value = value,
    onValueChange = onValueChange,
    modifier = modifier.fillMaxWidth(),
    textStyle = textStyle,
    label = label?.let { { Text(it) } },
    placeholder = placeholder?.let { { Text(it) } },
    // In line with an [Item]'s icon, a little further in.
    leadingIcon = icon?.let { { Icon(it, null, Modifier.offset(x = 4.dp)) } },
    trailingIcon = trailing,
    visualTransformation = visualTransformation,
    keyboardOptions = keyboardOptions,
    keyboardActions = keyboardActions,
    singleLine = singleLine,
    minLines = minLines,
    shape = ITEM_SHAPE,
    colors = TextFieldDefaults.colors(
        focusedContainerColor = containerColor,
        unfocusedContainerColor = containerColor,
        disabledContainerColor = containerColor,
        focusedIndicatorColor = Color.Transparent,
        unfocusedIndicatorColor = Color.Transparent,
        disabledIndicatorColor = Color.Transparent,
    ),
)

/**
 * A choice as a row of a [Group]: `label` over the option picked (`selected` of `options`), a
 * menu of them to pick another. `mark` draws an option's mark (its colour) before it, else `icon`.
 */
@Composable
fun ChoiceItem(
    label: String?,
    options: List<String>,
    selected: Int,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    mark: (@Composable (Int) -> Unit)? = null,
    trailing: @Composable () -> Unit = { Icon(Icons.Default.ArrowDropDown, null) },
    containerColor: Color = MaterialTheme.colorScheme.surfaceContainer,
    onSelect: (Int) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box(modifier) {
        Item(
            headlineContent = { Text(options.getOrElse(selected) { "" }) },
            overlineContent = label?.let { { Text(it) } },
            leadingContent = mark?.let { { it(selected) } } ?: icon?.let { { Icon(it, null) } },
            trailingContent = trailing,
            containerColor = containerColor,
            modifier = Modifier.clickable { open = true },
        )
        PickMenu(open, { open = false }, options, selected, mark, onSelect)
    }
}

/** A list's or calendar's colour as a dot, in an icon's room. */
@Composable
fun ColorMark(color: String?) = Box(Modifier.size(24.dp), Alignment.Center) {
    Box(Modifier.size(12.dp).background(parseColor(color), CircleShape))
}

/**
 * A menu of `options` to pick one, `selected` (if any) marked with a check, or with its `mark`
 * where options have one: picking closes it and gives `onPick` the index.
 */
@Composable
fun PickMenu(
    expanded: Boolean,
    onDismiss: () -> Unit,
    options: List<String>,
    selected: Int,
    mark: (@Composable (Int) -> Unit)? = null,
    onPick: (Int) -> Unit,
) = DropdownMenuPopup(expanded, onDismiss) {
    DropdownMenuGroup(MenuDefaults.groupShapes(), Modifier.verticalScroll(rememberScrollState())) {
        options.forEachIndexed { i, o ->
            SelectableDropdownMenuItem(
                selected = i == selected,
                onClick = {
                    onDismiss()
                    onPick(i)
                },
                text = { Text(o) },
                shapes = MenuDefaults.itemShape(i, options.size),
                leadingIcon = mark?.let { { it(i) } },
                selectedLeadingIcon = mark?.let { { it(i) } } ?: { Icon(Icons.Default.Check, null) },
            )
        }
    }
}

/** One of `options` picked with connected toggle buttons sharing the room. */
@Composable
fun Connected(options: List<String>, selected: Int, modifier: Modifier = Modifier, onSelect: (Int) -> Unit) = Row(
    modifier,
    horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
) {
    options.forEachIndexed { i, o ->
        ToggleButton(
            checked = i == selected,
            onCheckedChange = { onSelect(i) },
            shapes = when (i) {
                0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                options.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
            },
            // Seen on a row's own colour too.
            colors = ToggleButtonDefaults.colors(
                containerColor = MaterialTheme.colorScheme.secondaryContainer,
                contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
            ),
            contentPadding = PaddingValues(horizontal = 8.dp),
            modifier = Modifier.weight(1f).semantics { role = Role.RadioButton },
        ) { Text(o, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    }
}

/** Material's time picker in its dialog at `clock` (seconds into the day); `extra`: buttons before Cancel. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TimeDialog(title: String, clock: Long, onDismiss: () -> Unit, extra: @Composable () -> Unit = {}, onPick: (Long) -> Unit) {
    val state = rememberTimePickerState((clock / 3600).toInt(), (clock / 60 % 60).toInt(), hours24)
    TimePickerDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        confirmButton = {
            TextButton(onClick = {
                onDismiss()
                onPick(state.hour * 3600L + state.minute * 60L)
            }) { Text(tr("OK")) }
        },
        dismissButton = {
            extra()
            TextButton(onClick = onDismiss) { Text(tr("Cancel")) }
        },
    ) { TimePicker(state) }
}

/**
 * A date and maybe a time of day (local wall-clock seconds, as the core's), or none, as a row of
 * a [Group]: tapped, Material's date picker; a button for the time, one clearing it.
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
    Item(
        overlineContent = { Text(label) },
        headlineContent = { Text(if (at == null) tr("Set Date") else date(at)) },
        leadingContent = { Icon(icon, null) },
        trailingContent = if (at == null) null else {
            {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (!dateOnly) {
                        FilledTonalButton(onClick = { pickTime = true }) { Text(if (allDay) tr("Add Time") else time(at)) }
                    }
                    if (clearable) {
                        Tip(tr("Clear")) {
                            IconButton(onClick = { onChange(null, true) }) { Icon(Icons.Default.Close, tr("Clear")) }
                        }
                    }
                }
            }
        },
        modifier = Modifier.clickable { pickDate = true },
    )
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
        val day = at - Math.floorMod(at, DAY)
        TimeDialog(
            label,
            if (allDay) defaultTime else Math.floorMod(at, DAY),
            onDismiss = { pickTime = false },
            extra = {
                if (!allDay && clearable) {
                    TextButton(onClick = {
                        pickTime = false
                        onChange(day, true)
                    }) { Text(tr("Remove Time")) }
                }
            },
        ) { onChange(day + it, false) }
    }
}

/** A text field with suggestions it may also take a new value besides (categories), as a row of a [Group]. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SuggestField(label: String, value: String, suggestions: List<String>, icon: ImageVector, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val shown = suggestions.filter { it.contains(value, ignoreCase = true) && it != value }
    ExposedDropdownMenuBox(expanded = open && shown.isNotEmpty(), onExpandedChange = { open = it }) {
        FieldItem(
            value,
            {
                onChange(it)
                open = true
            },
            label,
            Modifier.menuAnchor(ExposedDropdownMenuAnchorType.PrimaryEditable),
            icon = icon,
            singleLine = true,
            trailing = { ExposedDropdownMenuDefaults.TrailingIcon(open) },
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        )
        ExposedDropdownMenu(expanded = open && shown.isNotEmpty(), onDismissRequest = { open = false }) {
            shown.forEachIndexed { i, s ->
                DropdownMenuItem(
                    onClick = {
                        open = false
                        onChange(s)
                    },
                    text = { Text(s) },
                    shape = MenuDefaults.itemShape(i, shown.size).shape,
                )
            }
        }
    }
}

/**
 * The widest a sheet's or an editor's content gets, centred: details, editors and the add sheet
 * share it. Inside, [Group]s keep 16dp from its edges, and everything else starts 16dp further
 * in, where a row's icon does.
 */
val CONTENT_WIDTH = 640.dp

/** A section's title among [Line]s, in line with their marks. */
@Composable
fun SectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 16.dp, top = 8.dp),
    )
}

/** An editor's title, at the top of its content, in line with its rows' icons. */
@Composable
fun PageTitle(text: String) = Text(
    text,
    style = MaterialTheme.typography.headlineSmall,
    modifier = Modifier.padding(start = 32.dp, end = 32.dp, top = 16.dp),
)

/**
 * A line of a details sheet: `leading` (an icon, a colour, a checkbox) in an icon's room, then
 * `content`, as an [Item]'s are placed, so details and their editor line up.
 */
@Composable
fun Line(leading: @Composable () -> Unit, modifier: Modifier = Modifier, content: @Composable () -> Unit) = Row(
    modifier.fillMaxWidth().padding(horizontal = 16.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    Box(Modifier.size(24.dp), Alignment.Center) { leading() }
    Box(Modifier.weight(1f)) { content() }
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

/** A row of a [Group] adding one more of what is above it. */
@Composable
fun AddItem(label: String, onClick: () -> Unit) = Item(
    headlineContent = { Text(label, color = MaterialTheme.colorScheme.primary) },
    leadingContent = { Icon(Icons.Default.Add, null, tint = MaterialTheme.colorScheme.primary) },
    modifier = Modifier.clickable(onClick = onClick),
)

/** An editor's last button, deleting what it edits. */
@Composable
fun DeleteButton(label: String, onClick: () -> Unit) = FilledTonalButton(
    onClick = onClick,
    colors = ButtonDefaults.filledTonalButtonColors(
        containerColor = MaterialTheme.colorScheme.errorContainer,
        contentColor = MaterialTheme.colorScheme.onErrorContainer,
    ),
    modifier = Modifier.fillMaxWidth().padding(16.dp).padding(top = 8.dp).height(56.dp),
) {
    Icon(Icons.Outlined.Delete, null)
    Spacer(Modifier.size(ButtonDefaults.IconSpacing))
    Text(label)
}

/** Room at the end of an editor's fields, so `EditorButtons` don't cover the last one. */
val EDITOR_END = 152.dp

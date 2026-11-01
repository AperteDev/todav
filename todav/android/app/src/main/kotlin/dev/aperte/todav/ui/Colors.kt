package dev.aperte.todav.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import dev.aperte.todav.core.eventColor
import dev.aperte.todav.core.eventColorPresets
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.aperte.todav.tr

/** The rainbow ring around the custom colour's swatch, as the Linux app's. */
private val RAINBOW = Brush.sweepGradient(
    listOf(Color(0xFFE01B24), Color(0xFFF6D32D), Color(0xFF33D17A), Color(0xFF3584E4), Color(0xFF9141AC), Color(0xFFE01B24)),
)

/** "#RRGGBB" of `color`. */
private fun hex(color: Color) = "#%06X".format(color.toArgb() and 0xFFFFFF)

/**
 * Colour swatches, as the Linux app's: `presets` ("#RRGGBB"), then any colour in a rainbow ring,
 * which shows `chosen` when it is no preset (null: none chosen). `snap` makes a custom colour what
 * it will be saved as.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ColorSwatches(presets: List<String>, chosen: String?, onPick: (String) -> Unit, snap: (String) -> String = { it }) {
    var custom by remember { mutableStateOf(false) }
    val isPreset = chosen == null || presets.any { it.equals(chosen.take(7), ignoreCase = true) }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        presets.forEach { c ->
            val selected = c.equals(chosen?.take(7), ignoreCase = true)
            Box(
                Modifier.size(36.dp).background(parseColor(c), CircleShape)
                    .border(if (selected) 3.dp else 0.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                    .semantics { contentDescription = c; this.selected = selected }
                    .clickable { onPick(c) },
                Alignment.Center,
            ) {
                if (selected) Icon(Icons.Default.Check, null, tint = Color.White)
            }
        }
        val name = tr("Custom Color…")
        Box(
            Modifier.size(36.dp).background(RAINBOW, CircleShape).padding(4.dp)
                .semantics { contentDescription = name; this.selected = !isPreset }
                .clickable { custom = true },
            Alignment.Center,
        ) {
            Box(
                Modifier.size(28.dp).background(MaterialTheme.colorScheme.surfaceContainerHigh, CircleShape).padding(2.dp)
                    .background(if (isPreset) Color.Transparent else parseColor(chosen), CircleShape),
                Alignment.Center,
            ) {
                if (isPreset) {
                    Icon(Icons.Default.MoreHoriz, null, tint = MaterialTheme.colorScheme.onSurface)
                } else {
                    Icon(Icons.Default.Check, null, tint = Color.White)
                }
            }
        }
    }
    if (custom) {
        CustomColorDialog(chosen ?: presets.first(), { custom = false }) {
            custom = false
            onPick(snap(it))
        }
    }
}

/**
 * An event's colour as a box to tap (by its title, in its details and its editor), picking
 * another in [EventColorDialog]: its own `color`, else (null) its calendar's `calendarColor`.
 * Not `enabled` (a calendar it cannot be changed in), only the box.
 */
@Composable
fun EventColorButton(color: String?, calendarColor: String?, enabled: Boolean, onPick: (String?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val name = tr("Color")
    Box(
        Modifier.size(32.dp).clip(CircleShape)
            .then(if (enabled) Modifier.clickable(role = Role.Button) { open = true }.semantics { contentDescription = name } else Modifier),
        Alignment.Center,
    ) {
        Box(Modifier.size(18.dp).background(parseColor(color ?: calendarColor), RoundedCornerShape(4.dp)))
    }
    if (open) {
        EventColorDialog(color, calendarColor, { open = false }) {
            open = false
            onPick(it)
        }
    }
}

/**
 * An event's colour to pick: its calendar's (null), one of the event colours, or any other, which
 * is saved as the closest CSS name (`eventColor`). Picking one closes it.
 */
@Composable
fun EventColorDialog(color: String?, calendarColor: String?, onDismiss: () -> Unit, onPick: (String?) -> Unit) {
    val presets = remember { eventColorPresets() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Color")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Item(
                    headlineContent = { Text(tr("Use the Calendar's Color")) },
                    leadingContent = { ColorMark(calendarColor) },
                    trailingContent = { if (color == null) Icon(Icons.Default.Check, null) },
                    containerColor = MaterialTheme.colorScheme.surfaceContainerHighest,
                    modifier = Modifier.clickable { onPick(null) },
                )
                ColorSwatches(presets, color, { onPick(it) }) { eventColor(it) ?: it }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

/** Any colour, by its hue, saturation and brightness; `initial` ("#rrggbb") to start from. */
@Composable
fun CustomColorDialog(initial: String, onDismiss: () -> Unit, onPick: (String) -> Unit) {
    val hsv = remember(initial) {
        FloatArray(3).also { android.graphics.Color.colorToHSV(parseColor(initial).toArgb(), it) }
    }
    var hue by rememberSaveable { mutableFloatStateOf(hsv[0]) }
    var saturation by rememberSaveable { mutableFloatStateOf(hsv[1]) }
    var brightness by rememberSaveable { mutableFloatStateOf(hsv[2]) }
    fun color(h: Float = hue, s: Float = saturation, v: Float = brightness) =
        Color(android.graphics.Color.HSVToColor(floatArrayOf(h, s, v)))
    val picked = color()
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Custom Color")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(48.dp).background(picked, CircleShape))
                    Text(hex(picked), style = MaterialTheme.typography.titleMedium)
                }
                // Each slider's track shows what moving it does.
                ColorSlider(tr("Hue"), hue / 360f, (0..6).map { color(h = it * 60f, s = 1f, v = 1f) }) { hue = it * 360f }
                ColorSlider(tr("Saturation"), saturation, listOf(color(s = 0f), color(s = 1f))) { saturation = it }
                ColorSlider(tr("Brightness"), brightness, listOf(Color.Black, color(v = 1f))) { brightness = it }
            }
        },
        confirmButton = { TextButton(onClick = { onPick(hex(picked)) }) { Text(tr("Select")) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

/** A slider from 0 to 1 over a track of `colors`, named `label`. */
@Composable
private fun ColorSlider(label: String, value: Float, colors: List<Color>, onChange: (Float) -> Unit) {
    Column {
        Text(label, style = MaterialTheme.typography.labelLarge)
        Box(contentAlignment = Alignment.Center) {
            Box(Modifier.fillMaxWidth().padding(horizontal = 10.dp).height(12.dp).background(Brush.horizontalGradient(colors), CircleShape))
            Slider(
                value,
                onChange,
                Modifier.semantics { contentDescription = label },
                colors = SliderDefaults.colors(activeTrackColor = Color.Transparent, inactiveTrackColor = Color.Transparent),
            )
        }
    }
}

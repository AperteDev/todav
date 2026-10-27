package dev.aperte.todav.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.exponentialDecay
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.AnchoredDraggableState
import androidx.compose.foundation.gestures.DraggableAnchors
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.anchoredDraggable
import androidx.compose.foundation.gestures.animateTo
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import kotlin.math.roundToInt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch

/** Where a details sheet rests. */
enum class Rest { Hidden, Details, Full }

/** What the contents of a `DetailsSheet` can do with it. */
class DetailsSheetState internal constructor(
    internal val drag: AnchoredDraggableState<Rest>,
    private val scope: CoroutineScope,
) {
    /** Unsaved edits: the sheet is not dragged down from under them. */
    var locked by mutableStateOf(false)

    /** Up to the full height (the editor). */
    fun expand() {
        scope.launch { drag.animateTo(Rest.Full) }
    }

    /** Back down to the details. */
    fun collapse() {
        scope.launch { drag.animateTo(Rest.Details) }
    }

    /** Down and gone. */
    fun close() {
        scope.launch { drag.animateTo(Rest.Hidden) }
    }
}

/**
 * A bottom sheet over the whole screen resting as tall as its `details` (raised over the keyboard,
 * scrolling past most of the screen), or, pulled all the way up or by `expand`, at full height
 * with `full` in their place, the one fading into the other as it goes: an item's details growing
 * into its editor. Back or the dimmed screen close it; from full height Back goes to the details.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DetailsSheet(
    onDismiss: () -> Unit,
    full: @Composable (DetailsSheetState) -> Unit,
    details: @Composable (DetailsSheetState) -> Unit,
) {
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val drag = remember {
        @Suppress("DEPRECATION")
        AnchoredDraggableState(
            initialValue = Rest.Hidden,
            positionalThreshold = { it * 0.5f },
            velocityThreshold = { with(density) { 125.dp.toPx() } },
            snapAnimationSpec = spring(),
            decayAnimationSpec = exponentialDecay(),
        )
    }
    val state = remember { DetailsSheetState(drag, scope) }
    // Dragged or sent all the way down: gone.
    LaunchedEffect(drag) { snapshotFlow { drag.settledValue }.drop(1).collect { if (it == Rest.Hidden) onDismiss() } }
    Dialog(
        onDismissRequest = state::close,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false, dismissOnClickOutside = false),
    ) {
        // The dim and the slide are drawn here, not by the window.
        val window = (LocalView.current.parent as? DialogWindowProvider)?.window
        SideEffect {
            window?.setDimAmount(0f)
            window?.setWindowAnimations(0)
        }
        // An editor's own handler, asking about unsaved edits, comes first.
        BackHandler { if (drag.targetValue == Rest.Full) state.collapse() else state.close() }
        val connection = remember(drag) { sheetScroll(drag) { !state.locked } }
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val height = constraints.maxHeight.toFloat()
            val top = WindowInsets.statusBars.getTop(density).toFloat()
            val ime = WindowInsets.ime.getBottom(density).toFloat()
            var handle by remember { mutableFloatStateOf(0f) }
            var body by remember { mutableFloatStateOf(0f) }
            LaunchedEffect(height, top, ime, handle, body) {
                if (body == 0f) return@LaunchedEffect
                // Below a sixth of the screen from the top at most, so there is room to pull it up.
                val rest = (height - handle - body - ime).coerceAtLeast(top + height / 6)
                val first = drag.anchors.size == 0
                drag.updateAnchors(
                    DraggableAnchors {
                        Rest.Hidden at height
                        Rest.Details at rest
                        Rest.Full at top
                    },
                    if (first) Rest.Hidden else drag.targetValue,
                )
                if (first) drag.animateTo(Rest.Details)
            }
            val offset = drag.offset.takeIf { !it.isNaN() } ?: height
            val rest = drag.anchors.positionOf(Rest.Details)
            // 0 at the details or below, 1 at full height.
            val up = if (rest.isNaN()) 0f else ((rest - offset) / (rest - top)).coerceIn(0f, 1f)
            val shown = if (rest.isNaN()) 0f else ((height - offset) / (height - rest)).coerceIn(0f, 1f)
            Box(
                Modifier.fillMaxSize().graphicsLayer { alpha = shown }.background(Color.Black.copy(alpha = 0.32f))
                    .pointerInput(Unit) { detectTapGestures { state.close() } },
            )
            Surface(
                Modifier.offset { IntOffset(0, offset.roundToInt()) }.fillMaxWidth()
                    .height(with(density) { (height - top).toDp() })
                    .nestedScroll(connection)
                    .anchoredDraggable(drag, Orientation.Vertical, enabled = !state.locked),
                shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
                color = MaterialTheme.colorScheme.surfaceContainerLow,
            ) {
                Column {
                    Box(Modifier.fillMaxWidth().onSizeChanged { handle = it.height.toFloat() }, androidx.compose.ui.Alignment.Center) {
                        BottomSheetDefaults.DragHandle()
                    }
                    Box(Modifier.weight(1f)) {
                        if (up < 1f) {
                            Box(
                                Modifier.graphicsLayer { alpha = 1 - up }.verticalScroll(rememberScrollState())
                                    .onSizeChanged { body = it.height.toFloat() }.navigationBarsPadding(),
                            ) { details(state) }
                        }
                        if (up > 0f || drag.targetValue == Rest.Full) {
                            Box(Modifier.fillMaxSize().graphicsLayer { alpha = up }) { full(state) }
                        }
                    }
                }
            }
        }
    }
}

/**
 * Scrolls inside the sheet move the sheet first upward (to full height) and last downward (once
 * the content is at its top), as Material's sheets do; `free` says when it may move.
 */
// ponytail: `settle(velocity)` is deprecated for a fling behaviour; it still does what Material's
// own sheets do here.
@Suppress("DEPRECATION")
private fun sheetScroll(drag: AnchoredDraggableState<Rest>, free: () -> Boolean) = object : NestedScrollConnection {
    override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset =
        if (available.y < 0 && source == NestedScrollSource.UserInput && free()) Offset(0f, drag.dispatchRawDelta(available.y)) else Offset.Zero

    override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset =
        if (source == NestedScrollSource.UserInput && free()) Offset(0f, drag.dispatchRawDelta(available.y)) else Offset.Zero

    override suspend fun onPreFling(available: Velocity): Velocity {
        val moving = available.y < 0 && drag.offset > drag.anchors.minPosition()
        return if (moving && free()) {
            drag.settle(available.y)
            available
        } else {
            Velocity.Zero
        }
    }

    override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
        if (free()) drag.settle(available.y)
        return available
    }
}

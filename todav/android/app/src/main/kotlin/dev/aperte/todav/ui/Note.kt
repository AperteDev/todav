package dev.aperte.todav.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Notes
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.em
import dev.aperte.todav.core.NoteStyle
import dev.aperte.todav.core.noteRead
import dev.aperte.todav.core.noteSpans
import dev.aperte.todav.core.noteToggleTask

/**
 * The note of a task or an event, as light Markdown (`todav_app::note`): read without its marks,
 * links opening and boxes ticking on a tap; a tap elsewhere edits it, its formatting shown and its
 * marks dimmed. An empty note is a field to type in.
 */
@Composable
fun NoteItem(value: String, onValueChange: (String) -> Unit, label: String) {
    var editing by rememberSaveable { mutableStateOf(false) }
    if (editing || value.isBlank()) {
        val focus = remember { FocusRequester() }
        var focused by remember { mutableStateOf(false) }
        val colors = MaterialTheme.colorScheme
        FieldItem(
            value,
            onValueChange,
            label,
            icon = Icons.AutoMirrored.Outlined.Notes,
            minLines = 3,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            visualTransformation = { text ->
                TransformedText(edited(text.text, colors.onSurface.copy(alpha = 0.5f)), OffsetMapping.Identity)
            },
            // Edited while focused, even when it was empty (or it would turn to text once typed in).
            modifier = Modifier.focusRequester(focus).onFocusChanged {
                if (it.isFocused) editing = true else if (focused) editing = false
                focused = it.isFocused
            },
        )
        LaunchedEffect(editing) { if (editing) focus.requestFocus() }
    } else {
        Item(
            overlineContent = { Text(label) },
            headlineContent = { NoteText(value, onValueChange) },
            leadingContent = { Icon(Icons.AutoMirrored.Outlined.Notes, null) },
            modifier = Modifier.clickable { editing = true },
        )
    }
}

/** Note `text` as read: its links open; its boxes tick when `onChange` takes the note ticked. */
@Composable
fun NoteText(text: String, onChange: ((String) -> Unit)?) {
    val link = MaterialTheme.colorScheme.primary
    Text(read(text, link, onChange?.let { change -> { n -> noteToggleTask(text, n.toUInt())?.let(change) } }))
}

/** The look of a range of a note, the same read or edited; null for none. */
private fun look(style: NoteStyle, dim: Color): SpanStyle? = when (style) {
    is NoteStyle.Heading -> SpanStyle(fontWeight = FontWeight.Bold, fontSize = if (style.level <= 2u) 1.25.em else 1.1.em)
    NoteStyle.Strong -> SpanStyle(fontWeight = FontWeight.Bold)
    NoteStyle.Emphasis, NoteStyle.Quote -> SpanStyle(fontStyle = FontStyle.Italic)
    NoteStyle.Strikethrough -> SpanStyle(textDecoration = TextDecoration.LineThrough)
    NoteStyle.Code, NoteStyle.CodeBlock -> SpanStyle(fontFamily = FontFamily.Monospace)
    is NoteStyle.Link -> SpanStyle(textDecoration = TextDecoration.Underline)
    NoteStyle.Mark -> SpanStyle(color = dim)
    is NoteStyle.Task, NoteStyle.ListMark -> null
}

/** Note `text` as edited: formatted, its marks dimmed. */
private fun edited(text: String, dim: Color): AnnotatedString = buildAnnotatedString {
    append(text)
    for (s in noteSpans(text)) {
        look(s.style, dim)?.let { addStyle(it, s.start.toInt(), s.end.toInt()) }
    }
}

/** Note `text` as read (`noteRead`): its links open, its boxes tick (`toggle` with their number). */
private fun read(text: String, link: Color, toggle: ((Int) -> Unit)?): AnnotatedString {
    val note = noteRead(text)
    val linked = TextLinkStyles(SpanStyle(color = link, textDecoration = TextDecoration.Underline))
    return buildAnnotatedString {
        append(note.text)
        var box = 0
        for (s in note.spans.sortedBy { it.start }) {
            val (from, to) = s.start.toInt() to s.end.toInt()
            when (val style = s.style) {
                is NoteStyle.Link -> when (val url = style.url) {
                    null -> look(style, link)?.let { addStyle(it, from, to) }
                    else -> addLink(LinkAnnotation.Url(url, linked), from, to)
                }
                is NoteStyle.Task -> {
                    val n = box++
                    addStyle(SpanStyle(fontSize = 1.3.em, color = link), from, to)
                    if (toggle != null) addLink(LinkAnnotation.Clickable("box$n", TextLinkStyles()) { toggle(n) }, from, to)
                }
                else -> look(style, link)?.let { addStyle(it, from, to) }
            }
        }
    }
}

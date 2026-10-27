package dev.aperte.todav.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.Dest
import dev.aperte.todav.Main
import dev.aperte.todav.core.Event
import dev.aperte.todav.core.EventFilter
import dev.aperte.todav.core.Item
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.dueLabel
import dev.aperte.todav.tr
import java.text.Normalizer

/** Lower case without accents, so "zolw" finds "Żółw", as the Linux app's search. */
fun folded(s: String): String =
    Normalizer.normalize(s.lowercase(), Normalizer.Form.NFD).replace(Regex("\\p{M}+"), "").replace('ł', 'l')

/** Tasks of every list (open ones first) and events whose title, note or place has the words typed. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchPage(model: Main) {
    var query by rememberSaveable { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val changes by model.todav.changes.collectAsStateWithLifecycle()
    val q = folded(query.trim())
    val core = model.core
    val results = load(changes, q, initial = Pair(emptyList<Pair<String, List<TodavTask>>>(), emptyList<Event>())) {
        if (q.isEmpty()) return@load Pair(emptyList(), emptyList())
        val words = q.split(Regex("\\s+"))
        val hit = { text: String -> folded(text).let { t -> words.all { it in t } } }
        val tasks = core.byPlace(core.lists()).mapNotNull { l ->
            core.tasks(l.href, true)
                .filter { hit(it.summary + " " + (it.description ?: "") + " " + (it.category ?: "")) }
                .sortedBy { it.done }
                .takeIf { it.isNotEmpty() }
                ?.let { l.displayName to it }
        }
        val events = core.searchEvents(object : EventFilter {
            override fun hit(event: Event) = hit(event.summary + " " + (event.description ?: "") + " " + (event.location ?: ""))
        })
        tasks to events
    }
    val now = core.localNow()
    val colors = load(changes, initial = emptyMap<String, String>()) { core.colors() }
    var details by remember { mutableStateOf<Item?>(null) }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    TextField(
                        query,
                        { query = it },
                        placeholder = { Text(tr("Search tasks and events")) },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                        colors = TextFieldDefaults.colors(
                            focusedContainerColor = Color.Transparent,
                            unfocusedContainerColor = Color.Transparent,
                            focusedIndicatorColor = Color.Transparent,
                            unfocusedIndicatorColor = Color.Transparent,
                        ),
                        modifier = Modifier.fillMaxWidth().focusRequester(focus),
                    )
                },
                navigationIcon = {
                    Tip(tr("Back")) { IconButton(onClick = model::back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, tr("Back")) } }
                },
                actions = {
                    if (query.isNotEmpty()) {
                        Tip(tr("Clear")) { IconButton(onClick = { query = "" }) { Icon(Icons.Default.Close, tr("Clear")) } }
                    }
                },
            )
        },
    ) { padding ->
        val (tasks, events) = results
        if (q.isNotEmpty() && tasks.isEmpty() && events.isEmpty()) {
            Text(
                tr("Nothing found"),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(padding).padding(24.dp),
            )
            return@Scaffold
        }
        LazyColumn(Modifier.fillMaxSize(), contentPadding = padding) {
            for ((list, ts) in tasks) {
                item(key = "l:$list") { Heading(list) }
                items(ts, key = { "t:" + it.uid }) { t ->
                    ListItem(
                        onClick = { model.open(Dest.Task(t.uid)) },
                        supportingContent = dueLabel(t, now)?.let { { Text(it) } },
                        leadingContent = { Checkbox(t.done, { model.setDone(t, it) }) },
                    ) {
                        Text(
                            t.summary,
                            textDecoration = if (t.done) TextDecoration.LineThrough else null,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            if (events.isNotEmpty()) {
                item(key = "events") { Heading(tr("Events")) }
                items(events, key = { "e:" + it.uid + it.start }) { e ->
                    ListItem(
                        onClick = { details = Item.Event(e) },
                        supportingContent = { Text(eventWhen(e, now)) },
                        leadingContent = { Icon(Icons.Outlined.Event, null, tint = parseColor(colors[e.listHref])) },
                    ) { Text(e.summary, maxLines = 2, overflow = TextOverflow.Ellipsis) }
                }
            }
        }
    }
    details?.let { ItemSheet(model, it) { details = null } }
}

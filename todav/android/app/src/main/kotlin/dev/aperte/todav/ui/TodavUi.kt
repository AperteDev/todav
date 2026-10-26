package dev.aperte.todav.ui

import androidx.activity.compose.BackHandler
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.StarOutline
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDrawerState
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.aperte.todav.IMPORTANT
import dev.aperte.todav.Main
import dev.aperte.todav.Place
import dev.aperte.todav.Row as TaskRow
import dev.aperte.todav.Screen
import dev.aperte.todav.SignIn
import dev.aperte.todav.UPCOMING
import dev.aperte.todav.accountName
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.overdue
import dev.aperte.todav.dueLabel
import dev.aperte.todav.ntr
import dev.aperte.todav.tr
import kotlinx.coroutines.launch

@Composable
fun TodavUi(model: Main) {
    val screen by model.screen.collectAsStateWithLifecycle()
    val current = screen
    when {
        current == null -> Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator() }
        current.accounts.isEmpty() -> Welcome(model)
        else -> Tasks(model, current)
    }
}

// --- welcome ----------------------------------------------------------------------------

/** How to use Todav, then the server to sign in to: as the Linux app's welcome page. */
@Composable
private fun Welcome(model: Main) {
    var server by rememberSaveable { mutableStateOf<String?>(null) }
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.padding(24.dp).navigationBarsPadding().imePadding(),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Spacer(Modifier.height(48.dp))
            Text(tr("Welcome to Todav"), style = MaterialTheme.typography.headlineMedium)
            val entered = server
            if (entered == null) {
                Text(tr("Tasks and calendar synced with your Nextcloud, or kept on this device"))
                Choice(tr("Nextcloud Server"), tr("Sign in to your Nextcloud; Todav gets an app password of its own")) {
                    server = ""
                }
                Choice(tr("On This Device Only"), tr("Lists and calendars that stay on this device")) {
                    model.useLocally()
                }
            } else {
                BackHandler {
                    model.cancelSignIn()
                    server = null
                }
                SignInForm(model, entered) { server = it }
            }
        }
    }
}

@Composable
private fun Choice(title: String, description: String, onClick: () -> Unit) {
    ElevatedCard(onClick = onClick, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(description, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
private fun SignInForm(model: Main, server: String, onServer: (String) -> Unit) {
    val state by model.signIn.collectAsStateWithLifecycle()
    var password by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val s = state
    if (s is SignIn.Browser) {
        LaunchedEffect(s.url) { CustomTabsIntent.Builder().build().launchUrl(context, s.url.toUri()) }
    }
    Text(tr("Sign in with the browser or an app password"))
    OutlinedTextField(
        value = server,
        onValueChange = onServer,
        label = { Text(tr("Nextcloud server")) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
        keyboardActions = KeyboardActions(onGo = { model.signInWithBrowser(server) }),
        modifier = Modifier.fillMaxWidth(),
    )
    when (s) {
        SignIn.Working, is SignIn.Browser -> Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(Modifier.size(24.dp))
            Text(tr("Waiting for Sign-In…"), Modifier.weight(1f))
            TextButton(onClick = model::cancelSignIn) { Text(tr("Cancel")) }
        }
        else -> {
            if (s is SignIn.Failed) Text(s.message, color = MaterialTheme.colorScheme.error)
            Button(onClick = { model.signInWithBrowser(server) }, Modifier.fillMaxWidth()) {
                Text(tr("Sign In with Browser"))
            }
            OutlinedButton(onClick = { password = true }, Modifier.fillMaxWidth()) {
                Text(tr("Sign In with Password…"))
            }
        }
    }
    if (password) PasswordDialog(onDismiss = { password = false }) { login, secret ->
        password = false
        model.signInWithPassword(server, login, secret)
    }
}

@Composable
private fun PasswordDialog(onDismiss: () -> Unit, onConnect: (String, String) -> Unit) {
    var login by rememberSaveable { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Sign In with Password")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(tr("Create an app password in Nextcloud under Settings → Security"))
                OutlinedTextField(login, { login = it }, label = { Text(tr("Login name")) }, singleLine = true)
                OutlinedTextField(
                    secret,
                    { secret = it },
                    label = { Text(tr("App password")) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onConnect(login, secret) }, enabled = login.isNotBlank() && secret.isNotEmpty()) {
                Text(tr("Connect"))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

// --- tasks --------------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Tasks(model: Main, screen: Screen) {
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    val sync by model.syncState.collectAsStateWithLifecycle()
    val completing by model.completing.collectAsStateWithLifecycle()
    completing?.let { (task, open) -> CompleteDialog(task.summary, open, model::complete) }

    LaunchedEffect(Unit) {
        model.messages.collect { (text, change) ->
            val action = if (change != null) tr("Undo") else null
            if (snackbar.showSnackbar(text, action) == SnackbarResult.ActionPerformed) model.undo(change)
        }
    }
    LaunchedEffect(Unit) {
        model.refused.collect { items ->
            for (r in items) {
                val text = tr("“{title}” was not saved on the server: {reason}")
                    .replace("{title}", r.summary).replace("{reason}", r.error)
                snackbar.showSnackbar(text)
            }
        }
    }
    val failed = sync as? SyncState.Failed
    LaunchedEffect(failed) {
        if (failed == null) return@LaunchedEffect
        val names = screen.accounts.associate { it.id to accountName(it) }
        val error = failed.failures.joinToString("; ") { f ->
            f.account?.let { names[it] }?.let { "$it: ${f.error}" } ?: f.error
        }
        snackbar.showSnackbar(tr("Sync failed: {error}").replace("{error}", error))
    }

    ModalNavigationDrawer(
        drawerState = drawer,
        drawerContent = {
            ModalDrawerSheet {
                Drawer(screen) {
                    model.select(it)
                    scope.launch { drawer.close() }
                }
            }
        },
    ) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = {
                        Column {
                            Text(screen.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(
                                ntr("{n} open", "{n} open", screen.open.toLong()).replace("{n}", "${screen.open}"),
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    },
                    navigationIcon = {
                        IconButton(onClick = { scope.launch { drawer.open() } }) {
                            Icon(Icons.Default.Menu, tr("Task lists"))
                        }
                    },
                    actions = {
                        if (sync == SyncState.Syncing) {
                            CircularProgressIndicator(Modifier.padding(12.dp).size(24.dp), strokeWidth = 2.dp)
                        } else {
                            IconButton(onClick = model::sync) { Icon(Icons.Default.Refresh, tr("Sync")) }
                        }
                    },
                )
            },
            snackbarHost = { SnackbarHost(snackbar) },
            bottomBar = { screen.list?.let { AddBar { text -> model.add(it.href, text) } } },
        ) { padding ->
            TaskList(model, screen, padding)
        }
    }
}

/** Whether completing a task completes its `open` subtasks too (`onAnswer(null)`: cancelled). */
@Composable
private fun CompleteDialog(title: String, open: Int, onAnswer: (Boolean?) -> Unit) {
    AlertDialog(
        onDismissRequest = { onAnswer(null) },
        title = { Text(tr("Complete Subtasks Too?")) },
        text = {
            Text(
                ntr("“{title}” has {n} open subtask.", "“{title}” has {n} open subtasks.", open.toLong())
                    .replace("{title}", title).replace("{n}", "$open"),
            )
        },
        confirmButton = { TextButton(onClick = { onAnswer(true) }) { Text(tr("Complete All")) } },
        dismissButton = {
            Row {
                TextButton(onClick = { onAnswer(null) }) { Text(tr("Cancel")) }
                TextButton(onClick = { onAnswer(false) }) { Text(tr("Only This Task")) }
            }
        },
    )
}

@Composable
private fun Drawer(screen: Screen, onSelect: (String) -> Unit) {
    LazyColumn(contentPadding = PaddingValues(vertical = 12.dp)) {
        item {
            Text(
                "Todav",
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(horizontal = 28.dp, vertical = 16.dp),
            )
        }
        items(screen.places, key = { it.key }) { place ->
            place.heading?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 28.dp, top = 16.dp, bottom = 8.dp),
                )
            }
            NavigationDrawerItem(
                label = { Text(place.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                icon = { PlaceIcon(place) },
                badge = { if (place.count > 0) Text("${place.count}") },
                selected = place.key == screen.current,
                onClick = { onSelect(place.key) },
                modifier = Modifier.padding(horizontal = 12.dp),
            )
        }
    }
}

@Composable
private fun PlaceIcon(place: Place) {
    when (place.key) {
        UPCOMING -> Icon(Icons.Outlined.CalendarMonth, null)
        IMPORTANT -> Icon(Icons.Outlined.StarOutline, null)
        else -> Box(Modifier.size(12.dp).background(parseColor(place.color), CircleShape))
    }
}

@Composable
private fun TaskList(model: Main, screen: Screen, padding: PaddingValues) {
    var showCompleted by rememberSaveable { mutableStateOf(false) }
    if (screen.groups.isEmpty() && screen.completed.isEmpty()) {
        Empty(screen, padding)
        return
    }
    LazyColumn(contentPadding = padding, modifier = Modifier.fillMaxSize()) {
        for (group in screen.groups) {
            group.title?.let { title -> item(key = "group:$title") { Heading(title) } }
            items(group.rows, key = { "${group.title}:${it.task.uid}" }) { row -> TaskItem(model, row, screen.now) }
        }
        if (screen.completed.isNotEmpty()) {
            item(key = "completed") {
                Row(
                    Modifier.fillMaxWidth().clickable { showCompleted = !showCompleted }
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        tr("Completed ({n})").replace("{n}", "${screen.completed.size}"),
                        style = MaterialTheme.typography.titleSmall,
                        modifier = Modifier.weight(1f),
                    )
                    Icon(if (showCompleted) Icons.Default.ExpandLess else Icons.Default.ExpandMore, null)
                }
            }
            if (showCompleted) {
                items(screen.completed, key = { "done:${it.task.uid}" }) { row -> TaskItem(model, row, screen.now) }
            }
        }
    }
}

@Composable
private fun Empty(screen: Screen, padding: PaddingValues) {
    val (title, description) = when (screen.current) {
        UPCOMING -> tr("Nothing Scheduled") to tr("Tasks with a due date show up here")
        IMPORTANT -> tr("No Important Tasks") to tr("Tasks of high priority show up here")
        else -> tr("All Done") to null
    }
    Column(
        Modifier.fillMaxSize().padding(padding).padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        description?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable
private fun Heading(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface)
            .padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp),
    )
}

/** A task: its check, title, note or list and due date; swiped away it is deleted (with Undo). */
@Composable
private fun TaskItem(model: Main, row: TaskRow, now: Long) {
    val task = row.task
    val dismiss = rememberSwipeToDismissBoxState()
    SwipeToDismissBox(
        state = dismiss,
        enableDismissFromStartToEnd = false,
        onDismiss = { if (it == SwipeToDismissBoxValue.EndToStart) model.delete(task) },
        backgroundContent = {
            Box(
                Modifier.fillMaxSize().background(MaterialTheme.colorScheme.errorContainer).padding(horizontal = 24.dp),
                Alignment.CenterEnd,
            ) {
                Icon(Icons.Default.Delete, tr("Delete"), tint = MaterialTheme.colorScheme.onErrorContainer)
            }
        },
    ) {
        Row(
            Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface)
                .padding(start = (8 + 24 * row.depth).dp, end = 16.dp, top = 2.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(checked = task.done, onCheckedChange = { model.setDone(task, it) })
            Column(Modifier.weight(1f).padding(vertical = 8.dp)) {
                Text(
                    task.summary,
                    textDecoration = if (task.done) TextDecoration.LineThrough else null,
                    color = if (task.done) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                )
                row.subtitle?.takeIf { it.isNotBlank() }?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            dueLabel(task, now)?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Medium,
                    color = if (overdue(task, now)) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.widthIn(max = 140.dp),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
    HorizontalDivider(Modifier.padding(start = (56 + 24 * row.depth).dp))
}

/** The field adding a task to the list shown. */
@Composable
private fun AddBar(onAdd: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    val add = {
        if (text.isNotBlank()) {
            onAdd(text)
            text = ""
        }
    }
    Surface(tonalElevation = 3.dp) {
        Row(
            Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                placeholder = { Text(tr("Add a task")) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { add() }),
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = add, enabled = text.isNotBlank()) {
                Icon(Icons.AutoMirrored.Filled.Send, tr("Add Task"))
            }
        }
    }
}

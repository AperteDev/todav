package dev.aperte.todav.ui

import android.Manifest
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.outlined.AccountCircle
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.Dns
import androidx.compose.material.icons.outlined.Key
import androidx.compose.material.icons.outlined.Password
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.PhoneAndroid
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import dev.aperte.todav.NextcloudApp
import dev.aperte.todav.Dest
import dev.aperte.todav.Main
import dev.aperte.todav.SignIn
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.caldavServices
import dev.aperte.todav.core.needsAppPassword
import dev.aperte.todav.tr

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun TodavUi(model: Main) {
    val screen by model.screen.collectAsStateWithLifecycle()
    val current = screen
    when {
        current == null -> Box(Modifier.fillMaxSize(), Alignment.Center) { LoadingIndicator() }
        current.accounts.isEmpty() -> Welcome(model)
        else -> NavDisplay(
            backStack = model.stack,
            onBack = model::back,
            entryDecorators = listOf(rememberSaveableStateHolderNavEntryDecorator()),
        ) { dest ->
            NavEntry(dest) {
                when (dest) {
                    // Read inside: Navigation 3 keeps an entry's content from when it was first shown.
                    Dest.Home -> Home(model)
                    is Dest.Task -> TaskPage(model, dest.uid)
                    is Dest.EventEdit -> EventPage(model, dest)
                    Dest.Search -> SearchPage(model)
                    Dest.Settings -> SettingsPage(model)
                    is Dest.SettingsPage -> SettingsSubpage(model, dest.page)
                    is Dest.Account -> AccountPage(model, dest.id)
                    Dest.AddAccount -> AddAccountPage(model)
                    is Dest.WidgetSettings -> dev.aperte.todav.WidgetPage(model.todav, dest.id, model::back)
                }
            }
        }
    }
    if (current != null && current.accounts.isNotEmpty()) ImportDialog(model)
}

// --- welcome ----------------------------------------------------------------------------

/** How to use Todav, then the account to add: as the Linux app's welcome page. */
@Composable
private fun Welcome(model: Main) {
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.statusBarsPadding().navigationBarsPadding().imePadding()
                .verticalScroll(rememberScrollState()).padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Spacer(Modifier.height(24.dp))
            Text(tr("Welcome to Todav"), style = MaterialTheme.typography.headlineMedium)
            AccountChoice(model, welcome = true)
        }
    }
}

/**
 * The kind of account to add, then that account: the Nextcloud app's accounts, a server to
 * sign in to, or lists on this device only. On the welcome page and in Settings → Accounts.
 */
@Composable
fun AccountChoice(model: Main, welcome: Boolean) {
    val context = LocalContext.current
    var server by rememberSaveable { mutableStateOf<String?>(null) }
    var login by rememberSaveable { mutableStateOf("") }
    // Another CalDAV server than Nextcloud: a password only.
    var caldav by rememberSaveable { mutableStateOf(false) }
    val connect = { s: String, l: String, p: String -> model.connect(s, l, p, caldav) }
    // An account of the Nextcloud app: its server and login name, then the browser at once.
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        NextcloudApp.server(r.data)?.let { (url, user) ->
            server = url
            login = user
            model.signInWithBrowser(url, connect, user)
        }
    }
    // The system's calendars: allowed to read them, the account comes, and its page to turn them on.
    val readCalendars = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            runCatching { model.core.addAccount(AccountKind.SYSTEM, "", "", "") }.onSuccess { id ->
                model.todav.changed()
                model.todav.systemCalendarsChanged()
                // They hold events only: the calendar behind the page.
                model.show(dev.aperte.todav.View.CALENDAR)
                model.back()
                model.open(Dest.Account(id))
            }
        }
    }
    val entered = server
    if (entered == null) {
        Text(tr("Tasks and calendar synced with your Nextcloud or another CalDAV server, or kept on this device"))
        Group {
            if (remember { NextcloudApp.installed(context) }) {
                Choice(
                    tr("Account of the Nextcloud App"),
                    tr("Sign in to the server of an account the Nextcloud app on this device has"),
                    Icons.Outlined.AccountCircle,
                ) { pick.launch(NextcloudApp.chooser(context)) }
            }
            Choice(tr("Nextcloud Server"), tr("Sign in to your Nextcloud; Todav gets an app password of its own"), Icons.Outlined.Cloud) {
                caldav = false
                server = ""
            }
            Choice(
                tr("CalDAV Server"),
                tr("iCloud, Fastmail, mailbox.org, or any server with a login name and password"),
                Icons.Outlined.Dns,
            ) {
                caldav = true
                server = ""
            }
            if (model.core.accounts().none { it.kind == AccountKind.SYSTEM }) {
                Choice(
                    tr("System Calendars"),
                    tr("Calendars of the accounts on this device, such as Google, shown read-only"),
                    Icons.Outlined.CalendarMonth,
                ) { readCalendars.launch(Manifest.permission.READ_CALENDAR) }
            }
            if (model.core.accounts().none { it.kind == AccountKind.LOCAL }) {
                Choice(tr("On This Device Only"), tr("Lists and calendars that stay on this device"), Icons.Outlined.PhoneAndroid) {
                    model.useLocally()
                    if (!welcome) model.back()
                }
            }
        }
        if (welcome) {
            Text(
                tr("More accounts can be added later in Settings"),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    } else {
        BackHandler {
            model.cancelSignIn()
            server = null
            login = ""
        }
        SignInForm(model, entered, { server = it }, login, connect, caldav, pick = caldav)
    }
}

@Composable
private fun Choice(title: String, description: String, icon: ImageVector, onClick: () -> Unit) = Item(
    headlineContent = { Text(title) },
    supportingContent = { Text(description) },
    leadingContent = { Icon(icon, null) },
    trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
    modifier = Modifier.clickable(onClick = onClick),
)

/**
 * A server to sign in to, with the browser or an app password (`caldav`: a CalDAV server other
 * than Nextcloud, with a password only); `onDone` gets the credentials.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun SignInForm(
    model: Main,
    server: String,
    onServer: (String) -> Unit,
    /** The login name to fill in (from the Nextcloud app, or the account signing in again). */
    login: String,
    onDone: (String, String, String) -> Unit,
    caldav: Boolean = false,
    /** For a CalDAV account being added: a service by name, "Other" asking for the address. */
    pick: Boolean = false,
) {
    val state by model.signIn.collectAsStateWithLifecycle()
    var password by remember { mutableStateOf(false) }
    val services = remember { if (pick) caldavServices() else emptyList() }
    var choice by rememberSaveable { mutableIntStateOf(0) }
    val other = choice >= services.size
    LaunchedEffect(choice) {
        val service = services.getOrNull(choice)
        if (service != null) onServer(service.url) else if (services.any { it.url == server }) onServer("")
    }
    val context = LocalContext.current
    val s = state
    if (s is SignIn.Browser) {
        LaunchedEffect(s.url) { CustomTabsIntent.Builder().build().launchUrl(context, s.url.toUri()) }
    }
    Text(
        if (pick) tr("Pick your service, or enter the address of any server with CalDAV")
        else if (caldav) tr("Sign in with a login name and password")
        else tr("Sign in with the browser or an app password"),
    )
    Group {
        if (pick) {
            var menu by remember { mutableStateOf(false) }
            val names = services.map { it.name } + tr("Other")
            Box {
                Item(
                    headlineContent = { Text(tr("Service")) },
                    supportingContent = { Text(names[choice.coerceIn(0, names.size - 1)]) },
                    leadingContent = { Icon(Icons.Outlined.Cloud, null) },
                    modifier = Modifier.clickable { menu = true },
                )
                PickMenu(menu, { menu = false }, names, choice) { choice = it }
            }
        }
        if (!pick || other) FieldItem(
            server,
            onServer,
            if (caldav) tr("CalDAV server") else tr("Nextcloud server"),
            icon = if (caldav) Icons.Outlined.Dns else Icons.Outlined.Cloud,
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(
                onGo = { if (caldav) password = true else model.signInWithBrowser(server, onDone, login) },
            ),
        )
    }
    if (caldav) {
        Text(
            tr("Task lists and calendars sync with any CalDAV server. Changes made elsewhere are fetched every minute while Todav is in front, and in the background as often as the account says (every 15 minutes at first); settings stay on this device."),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    when (s) {
        SignIn.Working, is SignIn.Browser -> Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            LoadingIndicator(Modifier.size(40.dp))
            Text(tr("Waiting for Sign-In…"), Modifier.weight(1f))
            TextButton(onClick = model::cancelSignIn) { Text(tr("Cancel")) }
        }
        else -> {
            if (s is SignIn.Failed) Text(s.message, color = MaterialTheme.colorScheme.error)
            if (caldav) {
                Button(onClick = { password = true }, Modifier.fillMaxWidth()) { Text(tr("Sign In…")) }
            } else {
                Button(onClick = { model.signInWithBrowser(server, onDone, login) }, Modifier.fillMaxWidth()) {
                    Text(tr("Sign In with Browser"))
                }
                OutlinedButton(onClick = { password = true }, Modifier.fillMaxWidth()) {
                    Text(tr("Sign In with Password…"))
                }
            }
        }
    }
    if (password) PasswordDialog(login, caldav, caldav && needsAppPassword(server), onDismiss = { password = false }) { name, secret ->
        password = false
        model.signInWithPassword(server, name, secret, onDone, caldav)
    }
}

@Composable
private fun PasswordDialog(initial: String, caldav: Boolean, appPassword: Boolean, onDismiss: () -> Unit, onConnect: (String, String) -> Unit) {
    var login by rememberSaveable { mutableStateOf(initial) }
    var secret by remember { mutableStateOf("") }
    var shown by remember { mutableStateOf(false) }
    val ready = login.isNotBlank() && secret.isNotEmpty()
    val field = MaterialTheme.colorScheme.surfaceContainerHighest
    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Outlined.Key, null) },
        title = { Text(tr("Sign In with Password")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(
                    if (appPassword) tr("This service takes an app-specific password, made in your account's security settings, not your usual one")
                    else if (caldav) tr("The login name and password of your account; some providers ask for an app password instead")
                    else tr("Create an app password in Nextcloud under Settings → Security"),
                )
                Group {
                    FieldItem(
                        login,
                        { login = it },
                        tr("Login name"),
                        icon = Icons.Outlined.Person,
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
                        containerColor = field,
                    )
                    FieldItem(
                        secret,
                        { secret = it },
                        if (caldav) tr("Password") else tr("App password"),
                        icon = Icons.Outlined.Password,
                        trailing = {
                            val label = if (shown) tr("Hide Password") else tr("Show Password")
                            Tip(label) {
                                IconButton(onClick = { shown = !shown }) {
                                    Icon(if (shown) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility, label)
                                }
                            }
                        },
                        singleLine = true,
                        visualTransformation = if (shown) VisualTransformation.None else PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { if (ready) onConnect(login, secret) }),
                        containerColor = field,
                    )
                }
            }
        },
        confirmButton = {
            Button(onClick = { onConnect(login, secret) }, enabled = ready) { Text(tr("Connect")) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(tr("Cancel")) } },
    )
}

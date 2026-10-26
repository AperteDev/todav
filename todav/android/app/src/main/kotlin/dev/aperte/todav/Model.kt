package dev.aperte.todav

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.aperte.todav.core.Account
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.LoginFlow
import dev.aperte.todav.core.SidebarPage
import dev.aperte.todav.core.Starter
import dev.aperte.todav.core.SubtaskCompletion
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.listColor
import dev.aperte.todav.core.listHeadings
import dev.aperte.todav.core.loginFlowPoll
import dev.aperte.todav.core.loginFlowStart
import dev.aperte.todav.core.passwordLogin
import dev.aperte.todav.core.section
import dev.aperte.todav.core.trees
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** The pages above the lists, by the key `last_list` keeps for them (as the Linux app). */
const val UPCOMING = "upcoming"
const val IMPORTANT = "important"

/** A row of the drawer: a page or a list, with its count; `heading` over the first of an account's. */
data class Place(
    val key: String,
    val name: String,
    val count: Int,
    val color: String?,
    val heading: String?,
)

/** A task row: how deep it is (0 top-level) and what shows under its title. */
data class Row(val task: TodavTask, val depth: Int, val subtitle: String?)

/** A group of rows under a heading (a category, an Upcoming day, a list). */
data class Group(val title: String?, val rows: List<Row>)

/** What the main screen shows. */
data class Screen(
    val accounts: List<Account>,
    val places: List<Place>,
    /** The page or list shown (its key). */
    val current: String?,
    /** The list shown, when it is one (not a page): tasks can be added to it. */
    val list: TodavList?,
    val title: String,
    val open: Int,
    val groups: List<Group>,
    /** Completed tasks shown apart, under "Completed". */
    val completed: List<Row>,
    val now: Long,
)

/** Signing in to a Nextcloud server. */
sealed interface SignIn {
    data object Idle : SignIn
    data object Working : SignIn
    /** The browser is to open `url`; the flow is polled meanwhile. */
    data class Browser(val url: String) : SignIn
    data class Failed(val message: String) : SignIn
}

class Main(app: Application) : AndroidViewModel(app) {
    private val todav = app as TodavApp
    private val core get() = todav.core

    val ready: StateFlow<Boolean> = todav.ready
    val syncState: StateFlow<SyncState> = todav.syncState
    val refused = todav.refused

    private val selected = MutableStateFlow<String?>(null)

    val screen: StateFlow<Screen?> = combine(todav.ready, todav.changes, selected) { ready, _, key ->
        if (ready) load(key) else null
    }.flowOn(Dispatchers.IO).stateIn(viewModelScope, SharingStarted.Eagerly, null)

    /** Messages for a snackbar, with the change an "Undo" reverts (if any). */
    val messages = MutableSharedFlow<Pair<String, ULong?>>(extraBufferCapacity = 8)

    /** A task being completed with this many open subtasks, while the user picks whether they go too. */
    val completing = MutableStateFlow<Pair<TodavTask, Int>?>(null)

    val signIn = MutableStateFlow<SignIn>(SignIn.Idle)
    private var polling: Job? = null

    private fun load(key: String?): Screen {
        val accounts = core.accounts()
        val lists = core.byPlace(core.lists())
        val now = core.localNow()
        val pages = core.pages()
        val names = accounts.associate { it.id to accountName(it) }
        val headings = listHeadings(lists)
        val places = pages.map {
            when (it.page) {
                SidebarPage.UPCOMING -> Place(UPCOMING, tr("Upcoming"), it.count.toInt(), null, null)
                SidebarPage.IMPORTANT -> Place(IMPORTANT, tr("Important"), it.count.toInt(), null, null)
            }
        } + lists.mapIndexed { i, l ->
            Place(l.href, l.displayName, l.openCount.toInt(), listColor(l), headings[i]?.let { names[it] })
        }
        // The page or list shown: the one picked, else the last used, else the first.
        val current = (key ?: core.setting("last_list"))
            ?.takeIf { k -> places.any { it.key == k } }
            ?: places.firstOrNull { it.key != UPCOMING && it.key != IMPORTANT }?.key
            ?: places.firstOrNull()?.key
        val listNames = lists.associate { it.href to it.displayName }
        return when (current) {
            UPCOMING -> {
                val tasks = core.upcoming()
                val groups = tasks.groupBy { sectionTitle(section(it, now)) }
                    .map { (title, ts) -> Group(title, ts.map { Row(it, 0, listNames[it.listHref]) }) }
                Screen(accounts, places, current, null, tr("Upcoming"), tasks.size, groups, emptyList(), now)
            }
            IMPORTANT -> {
                val tasks = core.important()
                val groups = lists.mapNotNull { l ->
                    val here = tasks.filter { it.listHref == l.href }
                    if (here.isEmpty()) null else Group(l.displayName, here.map { Row(it, 0, null) })
                }
                Screen(accounts, places, current, null, tr("Important"), tasks.size, groups, emptyList(), now)
            }
            else -> {
                val list = lists.firstOrNull { it.href == current }
                if (list == null) {
                    Screen(accounts, places, current, null, "Todav", 0, emptyList(), emptyList(), now)
                } else {
                    val order = core.order()
                    val view = core.doneView()
                    val groups = core.groupedBy(list.href, order.tasks, order.subtasks, view).map { g ->
                        Group(g.name ?: ptr("category", "Other"), rows(g.tasks))
                    }.filter { it.rows.isNotEmpty() }
                    val completed = if (view.inPlace) emptyList() else rows(core.finished(list.href, view))
                    Screen(
                        accounts, places, current, list, list.displayName, list.openCount.toInt(),
                        groups, completed, now,
                    )
                }
            }
        }
    }

    private fun rows(tasks: List<TodavTask>): List<Row> = trees(tasks).flatMap { t ->
        listOf(Row(t.task, 0, t.task.description?.lineSequence()?.firstOrNull())) +
            t.subtasks.map { Row(it.task, it.depth.toInt(), null) }
    }

    /** Run `f` on the core's thread pool; a failure becomes a message. */
    private fun act(f: () -> Unit) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                f()
            } catch (e: Exception) {
                messages.tryEmit(reason(e) to null)
            }
        }
    }

    fun select(key: String) {
        selected.value = key
        act { core.setSetting("last_list", key) }
    }

    fun sync() = act { core.syncNow() }

    fun add(list: String, summary: String) = act {
        val text = summary.trim()
        if (text.isNotEmpty()) {
            core.addTask(list, text, null, null, core.order().taskStart)
            core.requestSync()
        }
    }

    /** Complete or reopen a task; its open subtasks go along, stay open or the user is asked. */
    fun setDone(task: TodavTask, done: Boolean) = act {
        val mode = core.subtaskCompletion()
        val open = if (done && mode == SubtaskCompletion.ASK) core.openSubtasks(task.uid).size else 0
        if (open > 0) {
            completing.value = task to open
        } else {
            core.setDoneTree(task.uid, done, mode == SubtaskCompletion.COMPLETE)
            core.requestSync()
        }
    }

    /** The answer to `completing`: with its subtasks or alone; null: cancelled. */
    fun complete(subtasks: Boolean?) {
        val task = completing.value?.first ?: return
        completing.value = null
        if (subtasks != null) act {
            core.setDoneTree(task.uid, true, subtasks)
            core.requestSync()
        }
    }

    fun delete(task: TodavTask) = act {
        core.deleteTask(task.uid)
        core.requestSync()
        messages.tryEmit(tr("“{title}” deleted").replace("{title}", task.summary) to core.lastChange())
    }

    fun undo(change: ULong?) = act {
        core.undo(change)
        core.requestSync()
    }

    /** Lists on this device only: a task list with a task to start from and an empty calendar. */
    fun useLocally() = act {
        if (core.accounts().none { it.kind == AccountKind.LOCAL }) {
            val id = core.addAccount(AccountKind.LOCAL, "", "", "")
            val starter = Starter(
                tr("Tasks"),
                tr("Welcome to Todav"),
                tr("Tick tasks off when done; open one to add a due date, reminders or subtasks."),
                tr("Calendar"),
            )
            core.startLocal(id, starter)
        }
        core.accountAdded(false)
        todav.changed()
    }

    /** Sign in with the browser (Login Flow v2): `SignIn.Browser` asks the UI to open it. */
    fun signInWithBrowser(server: String) {
        if (server.isBlank()) {
            signIn.value = SignIn.Failed(tr("Enter the Nextcloud server first"))
            return
        }
        polling?.cancel()
        signIn.value = SignIn.Working
        polling = viewModelScope.launch(Dispatchers.IO) {
            val flow: LoginFlow = try {
                loginFlowStart(server.trim())
            } catch (e: Exception) {
                signIn.value = SignIn.Failed(tr("Cannot start login: {error}").replace("{error}", reason(e)))
                return@launch
            }
            signIn.value = SignIn.Browser(flow.loginUrl)
            // Nextcloud keeps a login flow for 20 minutes.
            repeat(20 * 60 / 2) {
                delay(2000)
                val credentials = try {
                    loginFlowPoll(flow)
                } catch (e: Exception) {
                    signIn.value = SignIn.Failed(tr("Login failed: {error}").replace("{error}", reason(e)))
                    return@launch
                }
                if (credentials != null) {
                    connect(credentials.server, credentials.loginName, credentials.appPassword)
                    return@launch
                }
            }
            signIn.value = SignIn.Failed(tr("Login timed out"))
        }
    }

    /** Sign in with a login name and a password or app password. */
    fun signInWithPassword(server: String, login: String, password: String) {
        if (server.isBlank()) {
            signIn.value = SignIn.Failed(tr("Enter the Nextcloud server first"))
            return
        }
        polling?.cancel()
        signIn.value = SignIn.Working
        polling = viewModelScope.launch(Dispatchers.IO) {
            try {
                val c = passwordLogin(server.trim(), login.trim(), password)
                connect(c.server, c.loginName, c.appPassword)
            } catch (e: Exception) {
                signIn.value = SignIn.Failed(tr("Sign-in failed: {error}").replace("{error}", reason(e)))
            }
        }
    }

    fun cancelSignIn() {
        polling?.cancel()
        signIn.value = SignIn.Idle
    }

    /** Add the account; settings sync through the first one. */
    private fun connect(server: String, user: String, password: String) {
        try {
            val settings = core.settingsAccount() == null
            core.addNextcloud(todav.passwords, server, user, password, settings)
            core.accountAdded(false)
            signIn.value = SignIn.Idle
            todav.changed()
        } catch (e: Exception) {
            signIn.value = SignIn.Failed(reason(e))
        }
    }
}

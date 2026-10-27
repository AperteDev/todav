package dev.aperte.todav

import android.app.Application
import android.util.Log
import androidx.compose.runtime.mutableStateListOf
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.aperte.todav.core.Account
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.Changes
import dev.aperte.todav.core.LoginFlow
import dev.aperte.todav.core.SidebarPage
import dev.aperte.todav.core.Slot
import dev.aperte.todav.core.Starter
import dev.aperte.todav.core.SubtaskCompletion
import dev.aperte.todav.core.SubtaskCount
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.TaskPatch
import dev.aperte.todav.core.TodavList
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.listColor
import dev.aperte.todav.core.listHeadings
import dev.aperte.todav.core.loginFlowPoll
import dev.aperte.todav.core.loginFlowStart
import dev.aperte.todav.core.midnight
import dev.aperte.todav.core.sameList
import dev.aperte.todav.core.passwordLogin
import dev.aperte.todav.core.section
import dev.aperte.todav.core.subtaskCounts
import dev.aperte.todav.core.trees
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** The pages above the lists, by the key `last_list` keeps for them (as the Linux app). */
const val UPCOMING = "upcoming"
const val IMPORTANT = "important"

/** Where the user is: the back stack holds these (Navigation 3 keys). */
sealed interface Dest {
    data object Home : Dest
    data class Task(val uid: String) : Dest
    /** An event to edit (`uid`), or a new one (`uid` null) starting at `start` (0: now). */
    data class EventEdit(
        val list: String?,
        val uid: String?,
        val recurrenceId: Long?,
        val start: Long,
        val allDay: Boolean = false,
    ) : Dest
    data object Search : Dest
    data object Settings : Dest
    data class SettingsPage(val page: dev.aperte.todav.ui.Page) : Dest
    data class Account(val id: Long) : Dest
    data object AddAccount : Dest
}

/** The calendar's views, as the Linux app's `calendar_mode` names them. */
enum class CalMode { MONTH, WEEK, AGENDA }

/** Tasks or Calendar, switched by the navigation bar (both shown on wide screens). */
enum class View { TASKS, CALENDAR }

/** A row of the drawer: a page or a list, with its count; `heading` over the first of an account's. */
data class Place(
    val key: String,
    val name: String,
    val count: Int,
    val color: String?,
    val heading: String?,
)

/**
 * A task row: how deep it is (0 top-level) and what shows under its title; a top-level task
 * with subtasks has their `counts` and shows them only when `expanded`. `tag`: its category,
 * when categories show as tags.
 */
data class Row(
    val task: TodavTask,
    val depth: Int,
    val subtitle: String?,
    val counts: SubtaskCount? = null,
    val expanded: Boolean = false,
    val tag: String? = null,
    /** Where it sits, for drag and drop; null where tasks do not move (Upcoming, Important, Completed). */
    val slot: Slot? = null,
)

/** A list or calendar in the calendar's drawer, shown on the calendar or not. */
data class CalRow(
    val href: String,
    val name: String,
    val color: String?,
    val shown: Boolean,
    val heading: String?,
)

/**
 * A group of rows under a heading (a category, an Upcoming day, a list); `drop`: a task dropped
 * on the heading takes its `category`.
 */
data class Group(val title: String?, val rows: List<Row>, val category: String? = null, val drop: Boolean = false)

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
    /** The categories of the list shown, for its filter, and the one picked. */
    val categories: List<String>,
    val filter: String?,
    val stars: Boolean,
    val pins: Boolean,
)

/** A task being completed (`done`) or reopened, with the subtasks that would stay behind. */
data class Ticking(val task: TodavTask, val done: Boolean, val subtasks: Int)

/** Signing in to a Nextcloud server. */
sealed interface SignIn {
    data object Idle : SignIn
    data object Working : SignIn
    /** The browser is to open `url`; the flow is polled meanwhile. */
    data class Browser(val url: String) : SignIn
    data class Failed(val message: String) : SignIn
}

class Main(app: Application) : AndroidViewModel(app) {
    val todav = app as TodavApp
    val core get() = todav.core

    val ready: StateFlow<Boolean> = todav.ready
    val syncState: StateFlow<SyncState> = todav.syncState
    val refused = todav.refused

    /** The back stack; kept here so a fold, a rotation or a resize keeps it. */
    val stack = mutableStateListOf<Dest>(Dest.Home)

    fun open(d: Dest) {
        stack.add(d)
    }

    fun back() {
        if (stack.size > 1) stack.removeAt(stack.lastIndex)
    }

    private val selected = MutableStateFlow<String?>(null)
    private val expanded = MutableStateFlow<Set<String>>(emptySet())
    private val filter = MutableStateFlow<Pair<String, String>?>(null)

    val view = MutableStateFlow(View.TASKS)

    /** Text to add as a task (shared with Todav, or "" from the "New task" shortcut): the add sheet opens. */
    val sharedText = MutableStateFlow<String?>(null)

    // A read failing keeps what was shown: an exception would end the flow for good.
    val screen: StateFlow<Screen?> = combine(todav.ready, todav.changes, selected, expanded, filter) { ready, _, key, open, f ->
        if (ready) runCatching { load(key, open, f) }.onFailure { Log.w("Todav", "load", it) }.getOrNull() else null
    }.filterNotNull().flowOn(Dispatchers.IO).stateIn(viewModelScope, SharingStarted.Eagerly, null)

    /** Calendars and task lists for the calendar's drawer, by account. */
    val calendarLists: StateFlow<List<CalRow>> = combine(todav.ready, todav.changes) { ready, _ ->
        if (!ready) return@combine emptyList()
        runCatching { calendarRows() }.getOrNull()
    }.filterNotNull().flowOn(Dispatchers.IO).stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    private fun calendarRows(): List<CalRow> {
        val accounts = core.accounts().associate { it.id to accountName(it) }
        val all = core.byPlace((core.calendars() + core.lists()).distinctBy { it.href })
        val hidden = core.hidden().toSet()
        val headings = listHeadings(all)
        return all.mapIndexed { i, l ->
            CalRow(l.href, l.displayName, listColor(l), hidden.none { sameList(it, l.href) }, headings[i]?.let { accounts[it] })
        }
    }

    fun setShown(href: String, shown: Boolean) = act {
        core.setHidden(listOf(href), !shown)
        todav.changed()
    }

    val calMode = MutableStateFlow(CalMode.MONTH)

    fun setMode(m: CalMode) {
        calMode.value = m
        act { core.setSetting("calendar_mode", m.name.lowercase()) }
    }

    /** A task from "To Plan" waiting for an hour in the week view to be planned into. */
    val planning = MutableStateFlow<TodavTask?>(null)

    /** The day the calendar shows (local midnight), set by a reminder, an event pill or Today. */
    val day = MutableStateFlow<Long?>(null)

    /** The calendar on the day of `t`, from wherever the user is. */
    fun showDay(t: Long) {
        day.value = midnight(t)
        while (stack.size > 1) stack.removeAt(stack.lastIndex)
        show(View.CALENDAR)
    }

    /** Messages for a snackbar, with the change an "Undo" reverts (if any). */
    val messages = MutableSharedFlow<Pair<String, ULong?>>(extraBufferCapacity = 8)

    /** A task being completed or reopened, while the user picks whether its subtasks go along. */
    val ticking = MutableStateFlow<Ticking?>(null)

    val signIn = MutableStateFlow<SignIn>(SignIn.Idle)
    private var polling: Job? = null

    fun show(v: View) {
        view.value = v
        act { core.setSetting("last_view", if (v == View.CALENDAR) "calendar" else "tasks") }
    }

    private fun load(key: String?, open: Set<String>, f: Pair<String, String>?): Screen {
        val accounts = core.accounts()
        val lists = core.byPlace(core.lists().filter { it.holdsTasks })
        val now = core.localNow()
        val pages = core.pages()
        val names = accounts.associate { it.id to accountName(it) }
        val headings = listHeadings(lists)
        val stars = core.stars()
        val pins = core.pins()
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
        fun screen(list: TodavList?, title: String, open: Int, groups: List<Group>, done: List<Row> = emptyList(),
                   categories: List<String> = emptyList(), filter: String? = null) =
            Screen(accounts, places, current, list, title, open, groups, done, now, categories, filter, stars, pins)
        return when (current) {
            UPCOMING -> {
                val tasks = core.upcoming()
                val groups = tasks.groupBy { sectionTitle(section(it, now)) }
                    .map { (title, ts) -> Group(title, ts.map { Row(it, 0, listNames[it.listHref]) }) }
                screen(null, tr("Upcoming"), tasks.size, groups)
            }
            IMPORTANT -> {
                val tasks = core.important()
                val groups = lists.mapNotNull { l ->
                    val here = tasks.filter { it.listHref == l.href }
                    if (here.isEmpty()) null else Group(l.displayName, here.map { Row(it, 0, null) })
                }
                screen(null, tr("Important"), tasks.size, groups)
            }
            else -> {
                val list = lists.firstOrNull { it.href == current }
                    ?: return screen(null, "Todav", 0, emptyList())
                listScreen(list, open, f?.takeIf { it.first == list.href }?.second, ::screen)
            }
        }
    }

    /** A list's tasks as the Linux app shows them: Pinned, then by category (or as tags, or not at all). */
    private fun listScreen(
        list: TodavList,
        open: Set<String>,
        wanted: String?,
        screen: (TodavList?, String, Int, List<Group>, List<Row>, List<String>, String?) -> Screen,
    ): Screen {
        val order = core.order()
        val view = core.doneView()
        val categoryView = core.setting("category_view") ?: "group"
        val grouping = categoryView != "tags" && categoryView != "none"
        val pins = core.pins()
        val all = core.ungroupedBy(list.href, order.tasks, order.subtasks, view)
        val groups = if (grouping) {
            core.groupedBy(list.href, order.tasks, order.subtasks, view).map { it.name to it.tasks }
        } else {
            listOf(null to all)
        }
        val names = core.categories(list.href).map { it.name }
        val filter = wanted?.takeIf { it in names }
        val shown = { c: String? -> filter == null || c == filter }
        val counts = subtaskCounts(all)
        val tag = { t: TodavTask -> if (categoryView == "tags") t.category else null }
        // `group`: the category of the group the rows are in; null for Pinned, where each is in its own.
        fun rows(tasks: List<TodavTask>, pinned: Boolean, group: String?, keep: (TodavTask) -> Boolean): List<Row> {
            val roots = trees(tasks).filter { keep(it.task) }
            return roots.flatMapIndexed { i, t ->
                val c = counts[t.task.uid]?.takeIf { t.subtasks.isNotEmpty() }
                val isOpen = t.task.uid in open
                val g = if (pinned) t.task.category else group
                val slot = Slot(g, pinned, true, roots.getOrNull(i + 1)?.task?.uid)
                val subs = t.subtasks.mapIndexed { j, s ->
                    // The next sibling: the next subtask at its depth before one shallower.
                    val next = t.subtasks.drop(j + 1).takeWhile { it.depth >= s.depth }.firstOrNull { it.depth == s.depth }
                    Row(s.task, s.depth.toInt(), null, tag = tag(s.task), slot = Slot(g, pinned, false, next?.task?.uid))
                }
                listOf(Row(t.task, 0, t.task.description?.lineSequence()?.firstOrNull(), c, isOpen, tag(t.task), slot)) +
                    if (isOpen) subs else emptyList()
            }
        }
        val pinned = rows(all, true, null) { pins && it.pinned && shown(it.category) }
        val sections = groups.map { (name, tasks) ->
            name to rows(tasks, false, name) { !(pins && it.pinned) && shown(it.category) }
        }.filter { it.second.isNotEmpty() }
        // A list without categories is one plain list; otherwise every group has a header.
        val titled = grouping && names.isNotEmpty()
        val result = mutableListOf<Group>()
        if (pinned.isNotEmpty()) result += Group(tr("Pinned"), pinned)
        for ((name, rs) in sections) {
            val title = when {
                titled -> name ?: ptr("category", "Other")
                pinned.isNotEmpty() -> tr("Tasks")
                else -> null
            }
            result += Group(title, rs, name, grouping && titled)
        }
        val completed = if (view.inPlace) {
            emptyList()
        } else {
            core.finished(list.href, view).filter { shown(it.category) }
                .sortedByDescending { it.completedAt }
                .map { Row(it, 0, null, tag = tag(it)) }
        }
        return screen(list, list.displayName, list.openCount.toInt(), result, completed, names, filter)
    }

    /** Run `f` on the core's thread pool; a failure becomes a message. */
    fun act(f: () -> Unit) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                f()
            } catch (e: Exception) {
                messages.tryEmit(reason(e) to null)
            }
        }
    }

    /** Ask for a sync when something waits for the server (edits in local lists never do). */
    fun synced() {
        if (core.pending(null)) core.requestSync()
    }

    fun select(key: String) {
        selected.value = key
        act { core.setSetting("last_list", key) }
    }

    fun toggle(uid: String) {
        expanded.value = expanded.value.let { if (uid in it) it - uid else it + uid }
    }

    fun filterBy(list: String, category: String?) {
        filter.value = category?.let { list to it }
    }

    fun sync() = act { core.syncNow() }

    /** Add a task (to `parent` as a subtask) with what the add sheet set; returns at once. */
    fun add(list: String, summary: String, parent: String? = null, category: String? = null, patch: TaskPatch? = null) = act {
        val text = summary.trim()
        if (text.isEmpty()) return@act
        val order = core.order()
        if (parent != null) expanded.value += parent
        core.together(object : Changes {
            override fun run() {
                val t = core.addTask(list, text, category, parent, if (parent == null) order.taskStart else order.subStart)
                if (patch != null) core.updateTask(t.uid, patch)
            }
        })
        synced()
    }

    fun update(uid: String, patch: TaskPatch) = act {
        core.updateTask(uid, patch)
        synced()
    }

    /** Several changes as one undo step; `message` then says so, with Undo. */
    fun together(message: String? = null, f: () -> Unit) = act {
        core.together(object : Changes {
            override fun run() = f()
        })
        synced()
        message?.let { messages.tryEmit(it to core.lastChange()) }
    }

    /**
     * Complete or reopen a task. Completed, its open subtasks go along, stay open or the user is
     * asked, as the settings say; reopened, the user is asked whether its completed ones reopen
     * too, every time.
     */
    fun setDone(task: TodavTask, done: Boolean) = act {
        val mode = core.subtaskCompletion()
        // The subtasks left behind otherwise: open ones when completing, completed ones when reopening.
        val n = core.subtasks(task.uid, !done).size
        if (n > 0 && (!done || mode == SubtaskCompletion.ASK)) {
            ticking.value = Ticking(task, done, n)
        } else {
            tick(task, done, done && mode == SubtaskCompletion.COMPLETE)
        }
    }

    /** The answer to `ticking`: with its subtasks or alone; null: cancelled. */
    fun answer(subtasks: Boolean?) {
        val t = ticking.value ?: return
        ticking.value = null
        if (subtasks != null) act { tick(t.task, t.done, subtasks) }
    }

    private fun tick(task: TodavTask, done: Boolean, subtasks: Boolean) {
        // Completing them all, reopening one reopens its completed parents.
        val parents = !done && core.subtaskCompletion() == SubtaskCompletion.COMPLETE
        core.setDoneTree(task.uid, done, subtasks, parents)
        synced()
        if (done) messages.tryEmit(tr("“{title}” completed").replace("{title}", task.summary) to core.lastChange())
    }

    fun delete(task: TodavTask) = act {
        core.deleteTask(task.uid)
        synced()
        messages.tryEmit(tr("“{title}” deleted").replace("{title}", task.summary) to core.lastChange())
    }

    fun move(task: TodavTask, list: String) = act {
        core.moveTask(task.uid, list)
        synced()
        val name = core.lists().firstOrNull { it.href == list }?.displayName ?: ""
        messages.tryEmit(tr("Moved to “{list}”").replace("{list}", name) to core.lastChange())
    }

    fun undo(change: ULong?) = act {
        core.undo(change)
        synced()
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
    fun signInWithBrowser(server: String, then: (String, String, String) -> Unit = ::connect) {
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
                    then(credentials.server, credentials.loginName, credentials.appPassword)
                    return@launch
                }
            }
            signIn.value = SignIn.Failed(tr("Login timed out"))
        }
    }

    /** Sign in with a login name and a password or app password. */
    fun signInWithPassword(server: String, login: String, password: String, then: (String, String, String) -> Unit = ::connect) {
        if (server.isBlank()) {
            signIn.value = SignIn.Failed(tr("Enter the Nextcloud server first"))
            return
        }
        polling?.cancel()
        signIn.value = SignIn.Working
        polling = viewModelScope.launch(Dispatchers.IO) {
            try {
                val c = passwordLogin(server.trim(), login.trim(), password)
                then(c.server, c.loginName, c.appPassword)
            } catch (e: Exception) {
                signIn.value = SignIn.Failed(tr("Sign-in failed: {error}").replace("{error}", reason(e)))
            }
        }
    }

    fun cancelSignIn() {
        polling?.cancel()
        signIn.value = SignIn.Idle
    }

    /** An account of the Nextcloud app, granted to Todav: its requests go through the app. */
    fun addNextcloudApp(g: NextcloudApp.Granted) = viewModelScope.launch(Dispatchers.IO) {
        try {
            val first = core.accounts().isEmpty()
            val error = todav.passwords.store(g.server, g.user, g.token)
            if (error != null) throw IllegalStateException(error)
            val account = dev.aperte.todav.core.Account(0, AccountKind.NEXTCLOUD, "", g.server, g.user, NEXTCLOUD_APP + g.name, true)
            val client = NextcloudApp.client(todav, account) ?: return@launch
            core.addNextcloudApp(g.server, g.user, NEXTCLOUD_APP + g.name, client, first && core.settingsAccount() == null)
            core.accountAdded(false)
            todav.changed()
            todav.pushSetup()
            if (!first) launch(Dispatchers.Main) { back() }
        } catch (e: Exception) {
            messages.tryEmit(reason(e) to null)
        }
    }

    /** The Nextcloud app granted access again (a new token) for `account`. */
    fun reconnectNextcloudApp(account: Account, g: NextcloudApp.Granted) = act {
        todav.passwords.store(account.url, account.user, g.token)
        NextcloudApp.connect(todav, account)
        core.syncNow()
        todav.changed()
    }

    /** Add the account; on the welcome page settings sync through it (`settings`). */
    fun connect(server: String, user: String, password: String) {
        try {
            val first = core.accounts().isEmpty()
            core.addNextcloud(todav.passwords, server, user, password, first && core.settingsAccount() == null)
            core.accountAdded(false)
            signIn.value = SignIn.Idle
            todav.changed()
            todav.pushSetup()
            if (!first) viewModelScope.launch(Dispatchers.Main) { back() }
        } catch (e: Exception) {
            signIn.value = SignIn.Failed(reason(e))
        }
    }

    init {
        viewModelScope.launch(Dispatchers.IO) {
            todav.ready.collect { r ->
                if (!r) return@collect
                val start = core.setting("start_view") ?: "last"
                val v = if (start == "last") core.setting("last_view") else start
                view.value = if (v == "calendar") View.CALENDAR else View.TASKS
                val m = (core.setting("start_calendar_view") ?: "last").let {
                    if (it == "last") core.setting("calendar_mode") else it
                }
                calMode.value = CalMode.entries.firstOrNull { it.name.lowercase() == m } ?: CalMode.MONTH
            }
        }
    }
}

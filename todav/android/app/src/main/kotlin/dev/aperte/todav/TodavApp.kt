package dev.aperte.todav

import android.app.Application
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.Listener
import dev.aperte.todav.core.Refused
import dev.aperte.todav.core.ServiceShell
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.Todav
import java.time.ZoneId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * The process: the core (database, accounts, sync service) and what the UI follows of it. Core
 * calls block, so they run on [Dispatchers.IO]; its callbacks come from its own threads.
 */
class TodavApp : Application() {
    lateinit var core: Todav
        private set
    lateinit var passwords: Passwords
        private set
    lateinit var push: Push
        private set

    /** Bumped on every change to the data, here or synced: screens read again. */
    val changes = MutableStateFlow(0L)
    val syncState = MutableStateFlow<SyncState>(SyncState.Idle)
    /** Items the server refused in a sync that otherwise went through. */
    val refused = MutableSharedFlow<List<Refused>>(extraBufferCapacity = 8)
    /** The accounts are connected and the service runs: the first screen can show. */
    val ready = MutableStateFlow(false)

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @OptIn(FlowPreview::class)
    override fun onCreate() {
        super.onCreate()
        I18n.load(this)
        passwords = Passwords(this)
        push = Push(this)
        core = Todav.open(filesDir.resolve("todav").path)
        core.setLocalZone(ZoneId.systemDefault().id)
        core.subscribe(object : Listener {
            override fun changed(listHref: String, uids: List<String>) = changed()
            override fun settingsChanged() = changed()
        })
        Reminders.channel(this)
        scope.launch {
            connectAccounts()
            core.startService(object : ServiceShell {
                override fun state(state: SyncState) {
                    syncState.value = state
                }

                override fun refused(items: List<Refused>) {
                    refused.tryEmit(items)
                }
            }, false)
            ready.value = true
            // Reminders missed while the process was gone (and the first time, the mark to count from).
            runCatching { Reminders.show(this@TodavApp) }
            pushSetup()
            SyncWorker.start(this@TodavApp)
            // The next reminder's alarm and the widgets, again after changes settle.
            changes.debounce(1_000).collect {
                runCatching { Reminders.schedule(this@TodavApp) }
                TasksWidget.refresh(this@TodavApp)
            }
        }
    }

    fun changed() = changes.update { it + 1 }

    suspend fun awaitReady() {
        ready.first { it }
    }

    /** Register for pushes through UnifiedPush, when a distributor is installed and some account syncs. */
    fun pushSetup() {
        if (core.canSync()) push.register()
    }

    /** Connect the Nextcloud accounts signed in here with their saved app passwords. */
    private fun connectAccounts() {
        for (a in core.accounts()) {
            if (a.kind != AccountKind.NEXTCLOUD || a.source.isNotEmpty()) continue
            val password = passwords.load(a.url, a.user) ?: continue
            runCatching { core.connect(a.id, password) }
        }
    }
}

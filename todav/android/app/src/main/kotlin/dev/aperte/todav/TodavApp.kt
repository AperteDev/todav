package dev.aperte.todav

import android.app.Application
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.Listener
import dev.aperte.todav.core.Refused
import dev.aperte.todav.core.ServiceShell
import dev.aperte.todav.core.SyncState
import dev.aperte.todav.core.Todav
import dev.aperte.todav.core.TodavException
import dev.aperte.todav.core.setTrustedCertificates
import java.security.KeyStore
import java.time.ZoneId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * The process: the core (database, accounts, sync service) and what the UI follows of it. Core
 * calls block, so they run on [Dispatchers.IO]; its callbacks come from its own threads.
 */
class TodavApp : Application() {
    companion object {
        /** The process's app, for what renders without one at hand (`Item.struck`). */
        lateinit var app: TodavApp
            private set
    }

    lateinit var core: Todav
        private set
    lateinit var passwords: Passwords
        private set
    lateinit var push: Push
        private set
    /** Why the data could not be opened (left by a newer Todav, say): nothing else starts, and
     * the app shows only that. */
    var failed: Throwable? = null
        private set

    /** Bumped on every change to the data, here or synced: screens read again. */
    val changes = MutableStateFlow(0L)
    val syncState = MutableStateFlow<SyncState>(SyncState.Idle)
    /** Items the server refused in a sync that otherwise went through. */
    val refused = MutableSharedFlow<List<Refused>>(extraBufferCapacity = 8)
    /** The accounts are connected and the service runs: the first screen can show. */
    val ready = MutableStateFlow(false)

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** The system's calendars changed, or which are turned on: copied again once that settles. */
    private val systemChanged = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    fun systemCalendarsChanged() {
        systemChanged.tryEmit(Unit)
    }

    @OptIn(FlowPreview::class)
    override fun onCreate() {
        super.onCreate()
        app = this
        trustCertificates()
        I18n.load(this)
        passwords = Passwords(this)
        push = Push(this)
        core = try {
            Todav.open(filesDir.resolve("todav").path)
        } catch (e: TodavException) {
            failed = e
            return
        }
        core.setLocalZone(ZoneId.systemDefault().id)
        core.subscribe(object : Listener {
            override fun changed(listHref: String, uids: List<String>) = changed()
            override fun settingsChanged() = changed()
        })
        Reminders.channel(this)
        scope.launch {
            // Watched once reading them is allowed (watching needs it too).
            var watching = false
            systemChanged.onStart { emit(Unit) }.debounce(500).collect {
                if (!watching && SystemCalendars.allowed(this@TodavApp)) {
                    watching = runCatching { SystemCalendars.watch(this@TodavApp, ::systemCalendarsChanged) }.isSuccess
                }
                runCatching { SystemCalendars.import(this@TodavApp, core) }
            }
        }
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
                WidgetKind.refreshAll(this@TodavApp)
                WidgetClock.schedule(this@TodavApp)
            }
        }
    }

    fun changed() = changes.update { it + 1 }

    /** Fails at once when the data could not be opened: receivers, the sync worker and widgets stop there. */
    suspend fun awaitReady() {
        failed?.let { throw IllegalStateException("data not opened", it) }
        ready.first { it }
    }

    /** Register for pushes through UnifiedPush, when a distributor is installed and a Nextcloud account syncs (DAV Push is Nextcloud's). */
    fun pushSetup() {
        if (core.accounts().any { it.kind == AccountKind.NEXTCLOUD && core.syncing(it) }) push.register()
    }

    /** Connect the Nextcloud and CalDAV accounts signed in here with their saved passwords. */
    private fun connectAccounts() {
        for (a in core.accounts()) {
            if ((a.kind != AccountKind.NEXTCLOUD && a.kind != AccountKind.CAL_DAV) || a.source.isNotEmpty()) continue
            val password = passwords.load(a.url, a.user) ?: continue
            runCatching { core.connect(a.id, password) }
        }
    }

    /** HTTPS trusts what Android does, certificate authorities the user installed too (a
     * self-hosted server's own CA), not only the core's built-in ones. Read once per process:
     * one added later counts from the next start. */
    private fun trustCertificates() {
        runCatching {
            val store = KeyStore.getInstance("AndroidCAStore").apply { load(null) }
            setTrustedCertificates(store.aliases().toList().mapNotNull { store.getCertificate(it)?.encoded })
        }
    }
}

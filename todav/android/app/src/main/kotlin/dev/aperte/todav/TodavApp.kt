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
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
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

    /** Bumped on every change to the data, here or synced: screens read again. */
    val changes = MutableStateFlow(0L)
    val syncState = MutableStateFlow<SyncState>(SyncState.Idle)
    /** Items the server refused in a sync that otherwise went through. */
    val refused = MutableSharedFlow<List<Refused>>(extraBufferCapacity = 8)
    /** The accounts are connected (their passwords restored): the first screen can show. */
    val ready = MutableStateFlow(false)

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        I18n.load(this)
        passwords = Passwords(this)
        core = Todav.open(filesDir.resolve("todav").path)
        core.setLocalZone(ZoneId.systemDefault().id)
        core.subscribe(object : Listener {
            override fun changed(listHref: String, uids: List<String>) = changed()
            override fun settingsChanged() = changed()
        })
        scope.launch {
            connectAccounts()
            ready.value = true
            core.startService(object : ServiceShell {
                override fun state(state: SyncState) {
                    syncState.value = state
                }

                override fun refused(items: List<Refused>) {
                    refused.tryEmit(items)
                }
            }, false)
        }
    }

    fun changed() = changes.update { it + 1 }

    /** Connect the Nextcloud accounts signed in here with their saved app passwords. */
    private fun connectAccounts() {
        for (a in core.accounts()) {
            if (a.kind != AccountKind.NEXTCLOUD || a.source.isNotEmpty()) continue
            val password = passwords.load(a.url, a.user) ?: continue
            runCatching { core.connect(a.id, password) }
        }
    }
}

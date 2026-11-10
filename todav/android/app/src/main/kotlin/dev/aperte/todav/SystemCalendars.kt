package dev.aperte.todav

import android.Manifest
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.CalendarContract
import android.provider.CalendarContract.Calendars
import android.provider.CalendarContract.Events
import android.provider.CalendarContract.Instances
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.SystemCalendar
import dev.aperte.todav.core.SystemChange
import dev.aperte.todav.core.SystemEvent
import dev.aperte.todav.core.Todav

/**
 * The system's calendars (Android's calendar store, filled by the device's accounts: Google,
 * Exchange, DAVx⁵…). Those turned on in the system account's page (the
 * device-only `system_calendars` setting, none at first) are copied into it, their occurrences
 * as the system works them out (repeats, exceptions and time zones included). Events that do not
 * repeat, in a calendar its account lets the user change, can be added, edited and deleted in
 * Todav: the change is made in the system's store (the account's sync, DAVx⁵'s or Google's,
 * uploads it), and the copy comes back from there.
 */
object SystemCalendars {
    /** How the system account's lists are keyed: this and the calendar's id. */
    const val PREFIX = "content://com.android.calendar/calendars/"

    // ponytail: occurrences from two years back to three ahead only; read further on demand if
    // browsing past them matters.
    private const val YEAR = 365L * 86_400_000L

    /** Whether `writable` calendars can be changed by the user (the account's access level). */
    data class Cal(val id: Long, val name: String, val account: String, val color: Int, val writable: Boolean)

    fun allowed(context: Context): Boolean =
        context.checkSelfPermission(Manifest.permission.READ_CALENDAR) == PackageManager.PERMISSION_GRANTED

    /** May change events: allowed to write the system's calendars. */
    fun mayWrite(context: Context): Boolean =
        allowed(context) && context.checkSelfPermission(Manifest.permission.WRITE_CALENDAR) == PackageManager.PERMISSION_GRANTED

    /** Every calendar of the system's, by account, then name. */
    fun all(context: Context): List<Cal> {
        if (!allowed(context)) return emptyList()
        val columns = arrayOf(
            Calendars._ID, Calendars.CALENDAR_DISPLAY_NAME, Calendars.ACCOUNT_NAME, Calendars.CALENDAR_COLOR,
            Calendars.CALENDAR_ACCESS_LEVEL,
        )
        val out = mutableListOf<Cal>()
        context.contentResolver.query(Calendars.CONTENT_URI, columns, null, null, null)?.use { c ->
            while (c.moveToNext()) {
                val writable = c.getInt(4) >= Calendars.CAL_ACCESS_CONTRIBUTOR
                out += Cal(c.getLong(0), c.getString(1) ?: "", c.getString(2) ?: "", c.getInt(3), writable)
            }
        }
        return out.sortedWith(compareBy({ it.account.lowercase() }, { it.name.lowercase() }))
    }

    /**
     * Calendars Todav already syncs itself (DAVx⁵'s of the same server, say), by id: the name of
     * Todav's own. Told by their events' UIDs, as the system keeps no server URL for them.
     */
    fun inTodav(context: Context, core: Todav): Map<Long, String> {
        if (!allowed(context)) return emptyMap()
        val names = core.calendars().associate { it.href to it.displayName }
        val tried = mutableMapOf<Long, Int>()
        val out = mutableMapOf<Long, String>()
        val columns = arrayOf(Events.CALENDAR_ID, Events.UID_2445)
        val where = "${Events.UID_2445} IS NOT NULL AND ${Events.DELETED} = 0 AND ${Events.ORIGINAL_ID} IS NULL"
        context.contentResolver.query(Events.CONTENT_URI, columns, where, null, null)?.use { c ->
            while (c.moveToNext()) {
                val cal = c.getLong(0)
                // ponytail: the first 20 events of each decide; enough unless one mixes both.
                val n = tried.getOrDefault(cal, 0)
                if (cal in out || n >= 20) continue
                tried[cal] = n + 1
                core.event(c.getString(1))?.listHref?.let { names[it] }?.let { out[cal] = it }
            }
        }
        return out
    }

    fun enabled(core: Todav): Set<Long> =
        core.setting("system_calendars").orEmpty().split(',').mapNotNull { it.toLongOrNull() }.toSet()

    fun setEnabled(core: Todav, id: Long, on: Boolean) {
        val ids = if (on) enabled(core) + id else enabled(core) - id
        core.setSetting("system_calendars", ids.joinToString(","))
    }

    /** Copy the calendars turned on, with their occurrences, into the system account. Blocks. */
    fun import(context: Context, core: Todav) {
        val account = core.accounts().firstOrNull { it.kind == AccountKind.SYSTEM } ?: return
        if (!allowed(context)) return
        push(context, core)
        val on = enabled(core)
        val cals = all(context).filter { it.id in on }
        val write = mayWrite(context)
        core.setSystemCalendars(
            account.id,
            cals.map { SystemCalendar(PREFIX + it.id, it.name, hex(it.color), it.writable && write) },
        )
        val now = System.currentTimeMillis()
        for (cal in cals) core.setSystemEvents(PREFIX + cal.id, occurrences(context, cal.id, now - 2 * YEAR, now + 3 * YEAR))
    }

    /**
     * Make the changes made in Todav to the system calendars' events in the system's store, which
     * marks them for the account's sync. A change to a series, or to an event that is gone, is
     * dropped (its copy comes back as the system has it). Blocks.
     */
    fun push(context: Context, core: Todav) {
        if (!mayWrite(context)) return
        for (change in core.systemChanges()) {
            runCatching { apply(context, change) }
            core.systemApplied(change.listHref, change.event.uid)
        }
    }

    private fun apply(context: Context, change: SystemChange) {
        val resolver = context.contentResolver
        val e = change.event
        val id = e.uid.removePrefix("android-").substringBefore('-').toLongOrNull().takeIf { e.uid.startsWith("android-") }
        if (id != null) {
            val uri = ContentUris.withAppendedId(Events.CONTENT_URI, id)
            // Only an event that does not repeat: a series is changed in its own app.
            if (repeats(context, id)) return
            if (change.deleted) {
                resolver.delete(uri, null, null)
            } else {
                resolver.update(uri, values(e, timeZone = null), null, null)
            }
        } else if (!change.deleted) {
            val calendar = change.listHref.removePrefix(PREFIX).toLongOrNull() ?: return
            val v = values(e, timeZone = java.util.TimeZone.getDefault().id)
            v.put(Events.CALENDAR_ID, calendar)
            resolver.insert(Events.CONTENT_URI, v)
        }
    }

    /** The values of an event: all-day ones in UTC, as the system keeps them. */
    private fun values(e: dev.aperte.todav.core.SystemEvent, timeZone: String?): ContentValues = ContentValues().apply {
        put(Events.TITLE, e.title)
        put(Events.DESCRIPTION, e.description.ifEmpty { null })
        put(Events.EVENT_LOCATION, e.location.ifEmpty { null })
        put(Events.ALL_DAY, if (e.allDay) 1 else 0)
        put(Events.DTSTART, e.start * 1000)
        put(Events.DTEND, e.end * 1000)
        // Set for a new event; an edited one keeps its zone, except when it became all-day.
        if (e.allDay) put(Events.EVENT_TIMEZONE, "UTC") else if (timeZone != null) put(Events.EVENT_TIMEZONE, timeZone)
        putNull(Events.DURATION)
    }

    /**
     * Whether event `uid` of list `listHref` cannot be changed in Todav though its calendar can: it
     * is an occurrence of a series (changed in the app of its account, for now).
     */
    fun locked(context: Context, listHref: String, uid: String): Boolean {
        if (!listHref.startsWith(PREFIX) || !uid.startsWith("android-")) return false
        val id = uid.removePrefix("android-").substringBefore('-').toLongOrNull() ?: return false
        return runCatching { repeats(context, id) }.getOrDefault(false)
    }

    /** Whether event `id` is a series (or an exception of one): it has a rule, dates or an original. */
    fun repeats(context: Context, id: Long): Boolean {
        val columns = arrayOf(Events.RRULE, Events.RDATE, Events.ORIGINAL_ID, Events.ORIGINAL_SYNC_ID)
        return context.contentResolver.query(ContentUris.withAppendedId(Events.CONTENT_URI, id), columns, null, null, null)
            ?.use { c -> c.moveToFirst() && (0..3).any { !c.getString(it).isNullOrEmpty() } } ?: false
    }

    private fun occurrences(context: Context, calendar: Long, from: Long, to: Long): List<SystemEvent> {
        val uri = Instances.CONTENT_URI.buildUpon().also {
            ContentUris.appendId(it, from)
            ContentUris.appendId(it, to)
        }.build()
        val columns = arrayOf(
            Instances.EVENT_ID, Instances.BEGIN, Instances.END, Instances.ALL_DAY, Instances.TITLE,
            Instances.DESCRIPTION, Instances.EVENT_LOCATION, Instances.STATUS, Instances.EVENT_COLOR,
        )
        val out = mutableListOf<SystemEvent>()
        context.contentResolver.query(uri, columns, "${Instances.CALENDAR_ID} = ?", arrayOf("$calendar"), null)?.use { c ->
            while (c.moveToNext()) {
                if (!c.isNull(7) && c.getInt(7) == Events.STATUS_CANCELED) continue
                val begin = c.getLong(1)
                out += SystemEvent(
                    uid = "android-${c.getLong(0)}-$begin",
                    start = begin / 1000,
                    end = c.getLong(2) / 1000,
                    allDay = c.getInt(3) == 1,
                    title = c.getString(4) ?: "",
                    description = c.getString(5) ?: "",
                    location = c.getString(6) ?: "",
                    // Its own colour, when it has one (Google Calendar's event colours).
                    color = if (c.isNull(8) || c.getInt(8) == 0) "" else hex(c.getInt(8)),
                )
            }
        }
        return out
    }

    /** When event `uri` (`content://com.android.calendar/events/<id>`) starts, if it can be read. */
    fun start(context: Context, uri: android.net.Uri): Long? {
        if (!allowed(context)) return null
        return runCatching {
            context.contentResolver.query(uri, arrayOf(Events.DTSTART), null, null, null)
                ?.use { c -> if (c.moveToFirst()) c.getLong(0) else null }
        }.getOrNull()
    }

    /** Call `onChange` whenever the system's calendars change (their accounts synced, say). */
    fun watch(context: Context, onChange: () -> Unit) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) = onChange()
        }
        context.contentResolver.registerContentObserver(CalendarContract.CONTENT_URI, true, observer)
    }

    private fun hex(color: Int) = "#%06X".format(color and 0xFFFFFF)
}

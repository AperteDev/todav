package dev.aperte.todav

import android.Manifest
import android.content.ContentUris
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
import dev.aperte.todav.core.SystemEvent
import dev.aperte.todav.core.Todav

/**
 * The system's calendars (Android's calendar store, filled by the device's accounts: Google,
 * Exchange, DAVx⁵…), read-only in Todav. Those turned on in the system account's page (the
 * device-only `system_calendars` setting, none at first) are copied into it, their occurrences
 * as the system works them out (repeats, exceptions and time zones included).
 */
object SystemCalendars {
    /** How the system account's lists are keyed: this and the calendar's id. */
    const val PREFIX = "content://com.android.calendar/calendars/"

    // ponytail: occurrences from two years back to three ahead only; read further on demand if
    // browsing past them matters.
    private const val YEAR = 365L * 86_400_000L

    data class Cal(val id: Long, val name: String, val account: String, val color: Int)

    fun allowed(context: Context): Boolean =
        context.checkSelfPermission(Manifest.permission.READ_CALENDAR) == PackageManager.PERMISSION_GRANTED

    /** Every calendar of the system's, by account, then name. */
    fun all(context: Context): List<Cal> {
        if (!allowed(context)) return emptyList()
        val columns = arrayOf(Calendars._ID, Calendars.CALENDAR_DISPLAY_NAME, Calendars.ACCOUNT_NAME, Calendars.CALENDAR_COLOR)
        val out = mutableListOf<Cal>()
        context.contentResolver.query(Calendars.CONTENT_URI, columns, null, null, null)?.use { c ->
            while (c.moveToNext()) {
                out += Cal(c.getLong(0), c.getString(1) ?: "", c.getString(2) ?: "", c.getInt(3))
            }
        }
        return out.sortedWith(compareBy({ it.account.lowercase() }, { it.name.lowercase() }))
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
        val on = enabled(core)
        val cals = all(context).filter { it.id in on }
        core.setSystemCalendars(account.id, cals.map { SystemCalendar(PREFIX + it.id, it.name, hex(it.color)) })
        val now = System.currentTimeMillis()
        for (cal in cals) core.setSystemEvents(PREFIX + cal.id, occurrences(context, cal.id, now - 2 * YEAR, now + 3 * YEAR))
    }

    private fun occurrences(context: Context, calendar: Long, from: Long, to: Long): List<SystemEvent> {
        val uri = Instances.CONTENT_URI.buildUpon().also {
            ContentUris.appendId(it, from)
            ContentUris.appendId(it, to)
        }.build()
        val columns = arrayOf(
            Instances.EVENT_ID, Instances.BEGIN, Instances.END, Instances.ALL_DAY, Instances.TITLE,
            Instances.DESCRIPTION, Instances.EVENT_LOCATION, Instances.STATUS,
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
                )
            }
        }
        return out
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

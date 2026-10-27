package dev.aperte.todav

import dev.aperte.todav.core.Account
import dev.aperte.todav.core.AccountKind
import dev.aperte.todav.core.Day
import dev.aperte.todav.core.Section
import dev.aperte.todav.core.TodavException
import dev.aperte.todav.core.TodavTask
import dev.aperte.todav.core.accountHost
import dev.aperte.todav.core.dayName
import java.time.LocalDateTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale

// What the core returns, in words: the same as the Linux app's (todav/linux/app/src/dates.rs and
// main.rs), so one translation fits both.

/** What an account is called: the name given to it, else "user@host", or "On This Device". */
fun accountName(a: Account): String = when {
    a.name.isNotEmpty() -> a.name
    a.kind == AccountKind.LOCAL -> tr("On This Device")
    else -> "${a.user}@${accountHost(a)}"
}

/** A local wall-clock time of the core (seconds, read as if UTC) as a date and time. */
fun fields(t: Long): LocalDateTime = LocalDateTime.ofEpochSecond(t, 0, ZoneOffset.UTC)

fun format(t: Long, pattern: String): String =
    fields(t).format(DateTimeFormatter.ofPattern(pattern, Locale.getDefault()))

/** The system's 24-hour setting (MainActivity reads it). */
@Volatile
var hours24 = true

/** A time of day as the system shows it. */
fun time(t: Long): String = format(t, if (hours24) "HH:mm" else "h:mm a")

/** An hour of the day for the week view's side ("08:00", "8 AM"). */
fun hourLabel(h: Int): String = format(h * 3600L, if (hours24) "HH:mm" else "h a")

/** A date in the locale's medium form ("27 Oct 2026"). */
fun date(t: Long): String =
    fields(t).format(DateTimeFormatter.ofLocalizedDate(java.time.format.FormatStyle.MEDIUM).withLocale(Locale.getDefault()))

/** Labels of the reminder presets (`Todav.reminderPresets`), as the Linux app's (dates.rs). */
fun reminderPresets(offsets: List<Long>, allDay: Boolean, event: Boolean, allDayTime: Long): List<Pair<String, Long>> {
    if (!allDay) {
        val labels = listOf(
            if (event) tr("At start") else tr("At due time"),
            tr("5 minutes before"),
            tr("15 minutes before"),
            tr("30 minutes before"),
            tr("1 hour before"),
            tr("1 day before"),
        )
        return labels.zip(offsets)
    }
    val at = String.format(Locale.ROOT, "%d:%02d", allDayTime / 3600, allDayTime / 60 % 60)
    return listOf(
        tr("On the day at {time}").replace("{time}", at),
        tr("The day before at {time}").replace("{time}", at),
    ).zip(offsets)
}

/** The units of a custom reminder, as `todav_app::dates::UNITS`. */
fun reminderUnits() = listOf(ptr("reminder", "minutes"), ptr("reminder", "hours"), ptr("reminder", "days"), ptr("reminder", "weeks"))

/** Before or after the due time; for a date alone, before the day or into it. */
fun reminderDirections(allDay: Boolean) = if (allDay) {
    listOf(ptr("reminder", "before the day"), ptr("reminder", "into the day"))
} else {
    listOf(ptr("reminder", "before"), ptr("reminder", "after"))
}

/** "Today", "Tomorrow", "Yesterday", a weekday in the week ahead, else the date; then the time. */
fun dayLabel(t: Long, allDay: Boolean, now: Long): String {
    val day = when (dayName(t, now)) {
        Day.TODAY -> tr("Today")
        Day.TOMORROW -> tr("Tomorrow")
        Day.YESTERDAY -> tr("Yesterday")
        Day.WEEKDAY -> format(t, "EEEE")
        Day.THIS_YEAR -> format(t, "d MMMM")
        Day.DATE -> format(t, "d MMMM yyyy")
    }
    return if (allDay) day else "$day ${time(t)}"
}

/** A task's due date, as its row shows it. */
fun dueLabel(t: TodavTask, now: Long): String? = t.due?.let { dayLabel(it, t.dueAllDay, now) }

/** The heading of an Upcoming section. */
fun sectionTitle(s: Section): String = when (s) {
    Section.Overdue -> tr("Overdue")
    Section.Today -> tr("Today")
    Section.Tomorrow -> tr("Tomorrow")
    is Section.Weekday -> format(s.day, "EEEE").replaceFirstChar { it.titlecase() }
    Section.Later -> tr("Later")
}

/** Why a core call failed, for a message. */
fun reason(e: Throwable): String = when (e) {
    is TodavException.Auth -> tr("Wrong login name or password")
    is TodavException.Exists -> tr("This account is already added")
    is TodavException.NotFound -> e.reason
    is TodavException.Http -> "HTTP ${e.code}: ${e.reason}"
    is TodavException.Net -> e.reason
    is TodavException.Parse -> e.reason
    is TodavException.Db -> e.reason
    is TodavException.SavePassword -> tr("Cannot save password: {error}").replace("{error}", e.reason)
    is TodavException.Upload ->
        tr("Cannot upload pending changes first: {error}").replace("{error}", e.reason)
    is TodavException.Unsupported ->
        tr("Not possible for an account signed in through the Nextcloud app")
    is TodavException.NotUploaded ->
        tr("It has changes not uploaded yet: sign in again to upload them")
    else -> e.message ?: e.toString()
}

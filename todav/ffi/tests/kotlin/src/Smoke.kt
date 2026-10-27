// A run through the bindings on a fresh database: a local account, tasks, a patch that sets and
// clears a date, undo of a grouped change, the calendar, the pure rules, errors and callbacks.

import dev.aperte.todav.core.*
import kotlin.io.path.createTempDirectory

fun main() {
    val dir = createTempDirectory("todav-ffi").toFile()
    val t = Todav.open(dir.path)
    var changes = 0
    t.subscribe(object : Listener {
        override fun changed(listHref: String, uids: List<String>) { changes++ }
        override fun settingsChanged() {}
    })

    val account = t.addAccount(AccountKind.LOCAL, "", "", "")
    t.startLocal(account, Starter("Tasks", "Welcome", "A note", "Calendar"))
    val list = t.lists().single()
    check(list.displayName == "Tasks" && list.local) { "lists: ${t.lists()}" }
    check(t.calendars().any { it.displayName == "Calendar" })

    // A patch sets a date, then clears it: None and Some(None) stay apart.
    val task = t.addTask(list.href, "Buy milk", null, null, false)
    val day = midnightOn(2026, 10, 30)
    t.updateTask(task.uid, TaskPatch(due = DueValue(Due(day, true))))
    check(t.task(task.uid)!!.due == day)
    t.updateTask(task.uid, TaskPatch(summary = "Buy milk!"))
    check(t.task(task.uid)!!.due == day) { "a patch without a date keeps it" }
    t.updateTask(task.uid, TaskPatch(due = DueValue(null)))
    check(t.task(task.uid)!!.due == null)

    // Two changes, one undo step.
    t.together(object : Changes {
        override fun run() {
            t.setDone(task.uid, true)
            t.updateTask(task.uid, TaskPatch(summary = "Buy oat milk"))
        }
    })
    check(t.undo(null))
    val back = t.task(task.uid)!!
    check(!back.done && back.summary == "Buy milk!") { "undo: $back" }

    // Subtasks as trees.
    val sub = t.addTask(list.href, "Oat", null, task.uid, false)
    val tree = trees(t.tasks(list.href, false)).single { it.task.uid == task.uid }
    check(tree.subtasks.single().task.uid == sub.uid && tree.subtasks.single().depth == 1u)
    check(subtaskCounts(t.tasks(list.href, true))[task.uid]?.open == 1u)

    // The calendar: an event, the agenda, a drop on another day.
    val cal = t.calendars().single()
    val at = day + 10 * 3600
    t.addEvent(cal.href, "Dentist", EventTime(at, at + 3600, false), listOf(-900L))
    val agenda = t.agenda(day, 1, null)
    check(agenda.any { (it.item as? Item.Event)?.event?.summary == "Dentist" && it.time == Time.At(at) })
    val event = t.events(day, day + 86400).single()
    val moved = dayDrop(Item.Event(event), day + 86400, false) as Change.Event
    check(moved.time.start == at + 86400)

    // Pure rules.
    val laid = lanes(listOf(listOf(0u, 1u, 2u), listOf(1u)), 3u)
    check(laid.pieces.size == 2 && laid.more.size == 7)
    check(dayName(day, day + 3600) == Day.TODAY)
    check(reminderOffset(reminderSpan(-5400)) == -5400L)
    check(weekStart(midnightOn(2026, 10, 14), 1) == midnightOn(2026, 10, 12))

    // Errors come as TodavException.
    try {
        t.addAccount(AccountKind.LOCAL, "", "", "")
        error("a second local account")
    } catch (e: TodavException.Exists) {
    }

    // The service, without an account to sync: idle, and reminders since the last call.
    t.startService(object : ServiceShell {
        override fun state(state: SyncState) {}
        override fun refused(items: List<Refused>) {}
    }, false)
    check(!t.canSync() && t.syncState() == SyncState.Idle)
    t.dueReminders()
    t.nextReminder()

    // Translations from the Polish catalog (made by scripts/bindings.sh).
    val pl = Translations(java.io.File("../../target/bindings/pl.mo").readBytes())
    check(pl.gettext("Today") == "Dzisiaj")
    check(pl.ngettext("{n} event", "{n} events", 5uL) == "{n} wydarzeń")
    check(pl.pgettext("category", "Other") == "Inne")
    check(Translations.english().ngettext("{n} event", "{n} events", 1uL) == "{n} event")

    check(changes > 0) { "no change events" }
    println("Kotlin bindings: ok ($changes change events)")
    dir.deleteRecursively()
}

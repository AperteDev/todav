// A run through the bindings on a fresh database: a local account, tasks, a patch that sets and
// clears a date, undo of a grouped change, the calendar, the pure rules, errors and callbacks.

using Aperte.Todav.Core;

static void Check(bool ok, string what)
{
    if (!ok) throw new Exception($"failed: {what}");
}

var dir = Directory.CreateTempSubdirectory("todav-ffi").FullName;
var t = Todav.Open(dir);
var listener = new Counter();
t.Subscribe(listener);

var account = t.AddAccount(AccountKind.Local, "", "", "");
t.StartLocal(account, new Starter("Tasks", "Welcome", "A note", "Calendar"));
var list = t.Lists().Single();
Check(list.DisplayName == "Tasks" && list.Local, "lists");
Check(t.Calendars().Any(c => c.DisplayName == "Calendar"), "calendars");

// A patch sets a date, then clears it: None and Some(None) stay apart.
var task = t.AddTask(list.Href, "Buy milk", null, null, false);
var day = TodavCore.MidnightOn(2026, 10, 30);
t.UpdateTask(task.Uid, new TaskPatch(Due: new DueValue(new Due(day, true))));
Check(t.Task(task.Uid)!.Due == day, "due set");
t.UpdateTask(task.Uid, new TaskPatch(Summary: "Buy milk!"));
Check(t.Task(task.Uid)!.Due == day, "a patch without a date keeps it");
t.UpdateTask(task.Uid, new TaskPatch(Due: new DueValue(null)));
Check(t.Task(task.Uid)!.Due == null, "due cleared");

// Two changes, one undo step.
t.Together(new Grouped(() =>
{
    t.SetDone(task.Uid, true);
    t.UpdateTask(task.Uid, new TaskPatch(Summary: "Buy oat milk"));
}));
Check(t.Undo(null), "undo");
var back = t.Task(task.Uid)!;
Check(!back.Done && back.Summary == "Buy milk!", $"undo: {back}");

// Subtasks as trees.
var sub = t.AddTask(list.Href, "Oat", null, task.Uid, false);
var tree = TodavCore.Trees(t.Tasks(list.Href, false)).Single(x => x.Task.Uid == task.Uid);
Check(tree.Subtasks.Single().Task.Uid == sub.Uid && tree.Subtasks.Single().Depth == 1, "trees");
Check(TodavCore.SubtaskCounts(t.Tasks(list.Href, true))[task.Uid].Open == 1, "counts");

// The calendar: an event, the agenda, a drop on another day.
var cal = t.Calendars().Single();
var at = day + 10 * 3600;
t.AddEvent(cal.Href, "Dentist", new EventTime(at, at + 3600, false), new long[] { -900 });
var agenda = t.Agenda(day, 1);
Check(agenda.Any(e => e.Item is Item.Event ev && ev.EventValue.Summary == "Dentist"
    && e.Time is Time.At a && a.Time == at), "agenda");
var ev = t.Events(day, day + 86400).Single();
var moved = (Change.Event)TodavCore.DayDrop(new Item.Event(ev), day + 86400, false);
Check(moved.Time.Start == at + 86400, "day drop");

// Pure rules.
var laid = TodavCore.Lanes(new[] { new uint[] { 0, 1, 2 }, new uint[] { 1 } }, 3);
Check(laid.Pieces.Length == 2 && laid.More.Length == 7, "lanes");
Check(TodavCore.DayName(day, day + 3600) == Day.Today, "day name");
Check(TodavCore.ReminderOffset(TodavCore.ReminderSpan(-5400)) == -5400, "reminder span");
Check(TodavCore.WeekStart(TodavCore.MidnightOn(2026, 10, 14), 1) == TodavCore.MidnightOn(2026, 10, 12), "week start");

// Errors come as TodavException.
try
{
    t.AddAccount(AccountKind.Local, "", "", "");
    throw new Exception("a second local account");
}
catch (TodavException.Exists)
{
}

// The service, without an account to sync: idle, and reminders since the last call.
t.StartService(new Shell(), false);
Check(!t.CanSync() && t.SyncState() is SyncState.Idle, "service");
t.DueReminders();

Check(listener.Changes > 0, "change events");
Console.WriteLine($"C# bindings: ok ({listener.Changes} change events)");
Directory.Delete(dir, true);

class Counter : Listener
{
    public int Changes;
    public void Changed(string listHref, string[] uids) => Interlocked.Increment(ref Changes);
    public void SettingsChanged() { }
}

class Grouped(Action action) : Changes
{
    public void Run() => action();
}

class Shell : ServiceShell
{
    public void State(SyncState state) { }
    public void Refused(Refused[] items) { }
}

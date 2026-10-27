//! The records and enums the bindings carry. Core and todav-app types are mirrored with
//! `uniffi::remote` (the fields must be theirs exactly; the compiler checks); the rest are this
//! crate's own, where the original does not cross the boundary: patches (`Option<Option<T>>` is
//! one optional in Kotlin), tuples, `usize`, enums with unnamed fields.

use todav_app::calendar as cal;
use todav_core::rrule::Freq;
use todav_core::{
    Account, AccountKind, CategoryGroup, CategoryMeta, Credentials, DoneView, Due, Event,
    EventTime, LoginFlow, PushRegistration, PushStatus, Refused, Repeat, Scope, ServerInfo, Sort,
    Task,
};

// --- mirrored from the core --------------------------------------------------------

/// A task list or a calendar (a CalDAV collection, or one on this device).
pub type TodavList = todav_core::List;

#[uniffi::remote(Record)]
pub struct TodavList {
    pub href: String,
    pub display_name: String,
    pub color: Option<String>,
    pub open_count: u32,
    pub holds_tasks: bool,
    pub holds_events: bool,
    pub writable: bool,
    pub local: bool,
    pub account: i64,
}

/// A task. Not `Task`, which C# code has in scope as System.Threading.Tasks.Task.
pub type TodavTask = todav_core::Task;

#[uniffi::remote(Record)]
pub struct TodavTask {
    pub uid: String,
    pub list_href: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub done: bool,
    pub completed_at: Option<i64>,
    pub category: Option<String>,
    pub parent_uid: Option<String>,
    pub priority: Option<i64>,
    pub due: Option<i64>,
    pub due_all_day: bool,
    pub reminders: Vec<i64>,
    pub sort_order: Option<i64>,
    pub created: Option<i64>,
    pub last_modified: Option<i64>,
    pub start: Option<i64>,
    pub start_all_day: bool,
    pub event_uid: Option<String>,
    pub sync_error: Option<String>,
    pub pinned: bool,
}

#[uniffi::remote(Record)]
pub struct Due {
    pub at: i64,
    pub all_day: bool,
}

#[uniffi::remote(Record)]
pub struct ServerInfo {
    pub product: String,
    pub version: String,
    pub maintenance: bool,
}

#[uniffi::remote(Record)]
pub struct Account {
    pub id: i64,
    pub kind: AccountKind,
    pub name: String,
    pub url: String,
    pub user: String,
    pub source: String,
    pub enabled: bool,
}

#[uniffi::remote(Enum)]
pub enum AccountKind {
    Nextcloud,
    Local,
    Google,
}

#[uniffi::remote(Record)]
pub struct CategoryMeta {
    pub name: String,
    pub icon: Option<String>,
}

#[uniffi::remote(Record)]
pub struct CategoryGroup {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub tasks: Vec<Task>,
}

#[uniffi::remote(Enum)]
pub enum Sort {
    Manual,
    Newest,
    Oldest,
    DueSoonest,
    DueLatest,
    Priority,
}

#[uniffi::remote(Record)]
pub struct DoneView {
    pub in_place: bool,
    pub tasks_last: bool,
    pub hide_subtasks: bool,
    pub subtasks_last: bool,
}

#[uniffi::remote(Record)]
pub struct Event {
    pub uid: String,
    pub list_href: String,
    pub recurrence_id: Option<i64>,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub recurring: bool,
    pub cancelled: bool,
    pub reminders: Vec<i64>,
    pub rule: Option<String>,
}

#[uniffi::remote(Record)]
pub struct EventTime {
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

#[uniffi::remote(Record)]
pub struct Repeat {
    pub freq: Freq,
    pub interval: i64,
    pub until: Option<i64>,
}

#[uniffi::remote(Enum)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[uniffi::remote(Enum)]
pub enum Scope {
    This,
    Following,
    All,
}

#[uniffi::remote(Record)]
pub struct Credentials {
    pub server: String,
    pub login_name: String,
    pub app_password: String,
}

#[uniffi::remote(Record)]
pub struct LoginFlow {
    pub login_url: String,
    pub endpoint: String,
    pub token: String,
}

#[uniffi::remote(Record)]
pub struct PushRegistration {
    pub list_href: String,
    pub registration_href: String,
    pub expires: i64,
}

#[uniffi::remote(Record)]
pub struct PushStatus {
    pub calendars: u32,
    pub supported: u32,
    pub registered: u32,
}

#[uniffi::remote(Record)]
pub struct Refused {
    pub task: bool,
    pub uid: String,
    pub list_href: String,
    pub summary: String,
    pub deleted: bool,
    pub error: String,
}

// --- mirrored from todav-app ----------------------------------------------------------

#[uniffi::remote(Record)]
pub struct Order {
    pub tasks: Sort,
    pub subtasks: Sort,
    pub task_start: bool,
    pub sub_start: bool,
}
pub type Order = todav_app::prefs::Order;

#[uniffi::remote(Enum)]
pub enum Day {
    Today,
    Tomorrow,
    Yesterday,
    Weekday,
    ThisYear,
    Date,
}
pub type Day = todav_app::dates::Day;

#[uniffi::remote(Enum)]
pub enum SubtaskCompletion {
    Complete,
    Keep,
    Ask,
}
/// What completing a task with open subtasks does.
pub type SubtaskCompletion = todav_app::prefs::SubtaskCompletion;

#[uniffi::remote(Enum)]
pub enum SwipeAction {
    Off,
    Complete,
    Star,
    Pin,
    Delete,
}
/// What swiping a task right or left does.
pub type SwipeAction = todav_app::prefs::SwipeAction;

#[uniffi::remote(Enum)]
pub enum SidebarPage {
    Upcoming,
    Important,
}
/// A page above the lists in the sidebar.
pub type SidebarPage = todav_app::tasks::Page;

#[uniffi::remote(Record)]
pub struct Slot {
    pub group: Option<String>,
    pub pinned: bool,
    pub root: bool,
    pub next: Option<String>,
}
pub type Slot = todav_app::tasks::Slot;

#[uniffi::remote(Record)]
pub struct Starter {
    pub tasks: String,
    pub task: String,
    pub note: String,
    pub calendar: String,
}
pub type Starter = todav_app::lists::Starter;

#[uniffi::remote(Record)]
pub struct Failure {
    pub account: Option<i64>,
    pub error: String,
}
pub type Failure = todav_app::service::Failure;

// --- this crate's own -------------------------------------------------------------------

/// What a sync or a push did.
#[derive(uniffi::Record)]
pub struct SyncReport {
    pub pushed: u32,
    pub pulled: u32,
    pub deleted: u32,
    pub conflicts: u32,
    /// What the server refused this time (not again until edited); the rest synced.
    pub refused: Vec<Refused>,
    /// Accounts that did not sync, and why.
    pub failed: Vec<Failure>,
}

impl From<todav_core::SyncReport> for SyncReport {
    fn from(r: todav_core::SyncReport) -> Self {
        SyncReport {
            pushed: r.pushed,
            pulled: r.pulled,
            deleted: r.deleted,
            conflicts: r.conflicts,
            refused: r.refused,
            failed: r
                .failed
                .into_iter()
                .map(|(id, e)| Failure {
                    account: Some(id),
                    error: e.to_string(),
                })
                .collect(),
        }
    }
}

/// A due date or start to set, or to clear (`due: None`).
#[derive(uniffi::Record)]
pub struct DueValue {
    pub due: Option<Due>,
}

/// A text to set, or to clear (`text: None`).
#[derive(uniffi::Record)]
pub struct TextValue {
    pub text: Option<String>,
}

/// A priority to set (1 highest to 9), or to clear (`priority: None`).
#[derive(uniffi::Record)]
pub struct PriorityValue {
    pub priority: Option<i64>,
}

/// A repeat rule to set, or to clear (`repeat: None`).
#[derive(uniffi::Record)]
pub struct RepeatValue {
    pub repeat: Option<Repeat>,
}

/// What to change on a task; None leaves a field as it is.
#[derive(uniffi::Record, Default)]
pub struct TaskPatch {
    #[uniffi(default = None)]
    pub summary: Option<String>,
    #[uniffi(default = None)]
    pub description: Option<String>,
    /// The empty string: no category.
    #[uniffi(default = None)]
    pub category: Option<String>,
    #[uniffi(default = None)]
    pub due: Option<DueValue>,
    /// Seconds after the due time (negative: before).
    #[uniffi(default = None)]
    pub reminders: Option<Vec<i64>>,
    #[uniffi(default = None)]
    pub start: Option<DueValue>,
    /// The UID of the event the task is for.
    #[uniffi(default = None)]
    pub event: Option<TextValue>,
    #[uniffi(default = None)]
    pub priority: Option<PriorityValue>,
    #[uniffi(default = None)]
    pub pinned: Option<bool>,
}

impl From<TaskPatch> for todav_core::TaskPatch {
    fn from(p: TaskPatch) -> Self {
        todav_core::TaskPatch {
            summary: p.summary,
            description: p.description,
            category: p.category,
            due: p.due.map(|v| v.due),
            reminders: p.reminders,
            start: p.start.map(|v| v.due),
            event: p.event.map(|v| v.text),
            priority: p.priority.map(|v| v.priority),
            pinned: p.pinned,
        }
    }
}

impl From<todav_core::TaskPatch> for TaskPatch {
    fn from(p: todav_core::TaskPatch) -> Self {
        TaskPatch {
            summary: p.summary,
            description: p.description,
            category: p.category,
            due: p.due.map(|due| DueValue { due }),
            reminders: p.reminders,
            start: p.start.map(|due| DueValue { due }),
            event: p.event.map(|text| TextValue { text }),
            priority: p.priority.map(|priority| PriorityValue { priority }),
            pinned: p.pinned,
        }
    }
}

/// What to change on an event; None leaves a field as it is.
#[derive(uniffi::Record, Default)]
pub struct EventPatch {
    #[uniffi(default = None)]
    pub summary: Option<String>,
    #[uniffi(default = None)]
    pub location: Option<String>,
    #[uniffi(default = None)]
    pub description: Option<String>,
    #[uniffi(default = None)]
    pub time: Option<EventTime>,
    /// Seconds after the start (negative: before).
    #[uniffi(default = None)]
    pub reminders: Option<Vec<i64>>,
    #[uniffi(default = None)]
    pub repeat: Option<RepeatValue>,
}

impl From<EventPatch> for todav_core::EventPatch {
    fn from(p: EventPatch) -> Self {
        todav_core::EventPatch {
            summary: p.summary,
            location: p.location,
            description: p.description,
            time: p.time,
            reminders: p.reminders,
            repeat: p.repeat.map(|v| v.repeat),
        }
    }
}

/// A section of the Upcoming page.
#[derive(uniffi::Enum)]
pub enum Section {
    Overdue,
    Today,
    Tomorrow,
    /// A day in the week ahead (its midnight), by its weekday.
    Weekday {
        day: i64,
    },
    Later,
}

impl From<todav_app::dates::Section> for Section {
    fn from(s: todav_app::dates::Section) -> Self {
        use todav_app::dates::Section as S;
        match s {
            S::Overdue => Section::Overdue,
            S::Today => Section::Today,
            S::Tomorrow => Section::Tomorrow,
            S::Weekday(day) => Section::Weekday { day },
            S::Later => Section::Later,
        }
    }
}

/// A date's fields.
#[derive(uniffi::Record)]
pub struct Date {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

/// A reminder offset as an amount of `dates::UNITS` (0 minutes, 1 hours, 2 days, 3 weeks).
#[derive(uniffi::Record)]
pub struct ReminderSpan {
    pub amount: i64,
    pub unit: u32,
    pub before: bool,
}

/// A page above the lists, with its count.
#[derive(uniffi::Record)]
pub struct PageCount {
    pub page: SidebarPage,
    pub count: u32,
}

/// A subtask in a tree, depth-first: how deep it is (1 under the top-level task).
#[derive(uniffi::Record)]
pub struct Subtask {
    pub depth: u32,
    pub task: Task,
}

/// A top-level task with its subtasks.
#[derive(uniffi::Record)]
pub struct Tree {
    pub task: Task,
    pub subtasks: Vec<Subtask>,
}

/// Open and done direct subtasks of a task.
#[derive(uniffi::Record)]
pub struct SubtaskCount {
    pub open: u32,
    pub done: u32,
}

/// Where a subtask's "Add subtask" entry goes among a tree's subtasks: in front of `before`
/// (their count: last); `index` is the subtask's own.
#[derive(uniffi::Record)]
pub struct EntryAt {
    pub before: u32,
    pub index: u32,
}

/// Something on the calendar.
#[derive(uniffi::Enum, Clone)]
pub enum Item {
    Event { event: Event },
    Task { task: Task },
}

impl From<cal::Item> for Item {
    fn from(i: cal::Item) -> Self {
        match i {
            cal::Item::Event(event) => Item::Event { event },
            cal::Item::Task(task) => Item::Task { task },
        }
    }
}

impl From<Item> for cal::Item {
    fn from(i: Item) -> Self {
        match i {
            Item::Event { event } => cal::Item::Event(event),
            Item::Task { task } => cal::Item::Task(task),
        }
    }
}

/// A task's place on the calendar: its start, else its due date; `all_day`: a date.
#[derive(uniffi::Record)]
pub struct When {
    pub at: i64,
    pub all_day: bool,
}

/// When an agenda row is on its day.
#[derive(uniffi::Enum)]
pub enum Time {
    /// All of it: an all-day event, or a day inside a longer one.
    AllDay,
    /// From the day's start until `end`: the last day of a longer event.
    Until {
        end: i64,
    },
    At {
        time: i64,
    },
    /// A task due that day, without a time.
    Date,
}

/// An agenda row.
#[derive(uniffi::Record)]
pub struct AgendaEntry {
    /// Midnight of its day.
    pub day: i64,
    pub time: Time,
    pub item: Item,
    pub color: String,
}

impl From<cal::Entry> for AgendaEntry {
    fn from(e: cal::Entry) -> Self {
        AgendaEntry {
            day: e.day,
            time: match e.time {
                cal::Time::AllDay => Time::AllDay,
                cal::Time::Until(end) => Time::Until { end },
                cal::Time::At(time) => Time::At { time },
                cal::Time::Date => Time::Date,
            },
            item: e.item.into(),
            color: e.color,
        }
    }
}

/// What a drop does: a task's new dates, or an event's new time (for a repeating event, ask
/// which occurrences it applies to).
#[derive(uniffi::Enum)]
pub enum Change {
    Task { patch: TaskPatch },
    Event { time: EventTime },
}

impl From<cal::Change> for Change {
    fn from(c: cal::Change) -> Self {
        match c {
            cal::Change::Task(p) => Change::Task { patch: p.into() },
            cal::Change::Event(time) => Change::Event { time },
        }
    }
}

/// When a timed item takes hours in the week view.
#[derive(uniffi::Record)]
pub struct Hours {
    pub from: i64,
    pub to: i64,
}

/// A timed item's part within one day of a week: the day (0 the week's first), from and to in
/// minutes into the day.
#[derive(uniffi::Record)]
pub struct Segment {
    pub day: i64,
    pub from: i64,
    pub to: i64,
}

/// A block of one day of the week view, to lay out: an index of the caller's, from and to.
#[derive(uniffi::Record)]
pub struct DayBlock {
    pub index: u32,
    pub from: i64,
    pub to: i64,
}

/// A block's column among the ones it overlaps (`columns` of them).
#[derive(uniffi::Record)]
pub struct BlockColumn {
    pub index: u32,
    pub column: u32,
    pub columns: u32,
}

/// One piece of an item on a week's row: its lane and the days (0–6) it covers.
#[derive(uniffi::Record)]
pub struct Piece {
    pub item: u32,
    pub lane: u32,
    pub from: u32,
    pub to: u32,
}

/// A week's row laid out: the pieces, and per day the items in its "+N more".
#[derive(uniffi::Record)]
pub struct Lanes {
    pub pieces: Vec<Piece>,
    pub more: Vec<Vec<u32>>,
}

/// What syncing is doing.
#[derive(uniffi::Enum)]
pub enum SyncState {
    Idle,
    Syncing,
    Failed { failures: Vec<Failure> },
}

impl From<todav_app::service::State> for SyncState {
    fn from(s: todav_app::service::State) -> Self {
        use todav_app::service::State;
        match s {
            State::Idle => SyncState::Idle,
            State::Syncing => SyncState::Syncing,
            State::Failed(failures) => SyncState::Failed { failures },
        }
    }
}

/// Reminders that went off.
#[derive(uniffi::Record)]
pub struct DueReminders {
    pub tasks: Vec<Task>,
    pub events: Vec<Event>,
}

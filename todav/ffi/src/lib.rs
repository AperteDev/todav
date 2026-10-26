//! Todav's core and shared front-end layer for the apps that are not in Rust: Android (Kotlin),
//! Windows (C#), later iOS and macOS (Swift), through UniFFI.
//!
//! One object, `Todav`, holds the database (`todav_core::Client`) and the background service
//! (`todav_app::service::Service`); pure rules are plain functions. Everything is synchronous
//! and may block on the network: call it off the UI thread. Callbacks (`Listener`,
//! `ServiceShell`) come from Todav's own threads. Times are local wall-clock seconds, as in the
//! core (`Todav::local_now`).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use todav_app::service::Service;
use todav_core::{
    Account, AccountKind, CategoryGroup, CategoryMeta, Client, Credentials, DoneView, Event,
    EventTime, LoginFlow, PushRegistration, PushStatus, Refused, Scope, ServerInfo, Sort, Task,
};

mod error;
mod types;

pub use error::{Result, TodavError};
pub use types::*;

uniffi::setup_scaffolding!();

// --- callbacks -------------------------------------------------------------------------

/// Told about changes to the data, from Todav's threads.
#[uniffi::export(callback_interface)]
pub trait Listener: Send + Sync {
    /// Tasks or events of `list_href` changed (`uids`; empty: any).
    fn changed(&self, list_href: String, uids: Vec<String>);
    /// Settings changed by another device arrived.
    fn settings_changed(&self);
}

/// The front-end's side of the background service, told from its threads.
#[uniffi::export(callback_interface)]
pub trait ServiceShell: Send + Sync {
    fn state(&self, state: SyncState);
    /// Items the server refused in a sync that otherwise went through.
    fn refused(&self, items: Vec<Refused>);
}

/// Where the front-end keeps the app passwords of accounts signed in in Todav, by server and
/// user.
#[uniffi::export(callback_interface)]
pub trait Keyring: Send + Sync {
    /// None when stored, else why not.
    fn store(&self, url: String, user: String, password: String) -> Option<String>;
    fn clear(&self, url: String, user: String);
}

/// Changes to undo together: see `Todav::together`.
#[uniffi::export(callback_interface)]
pub trait Changes: Send + Sync {
    fn run(&self);
}

/// Which events a search finds: see `Todav::search_events`.
#[uniffi::export(callback_interface)]
pub trait EventFilter: Send + Sync {
    fn hit(&self, event: Event) -> bool;
}

struct ListenerAdapter(Box<dyn Listener>);

impl todav_core::ChangeListener for ListenerAdapter {
    fn changed(&self, list_href: String, uids: Vec<String>) {
        self.0.changed(list_href, uids);
    }

    fn settings_changed(&self) {
        self.0.settings_changed();
    }
}

struct ShellAdapter(Box<dyn ServiceShell>);

impl todav_app::service::Shell for ShellAdapter {
    fn state(&self, state: todav_app::service::State) {
        self.0.state(state.into());
    }

    fn refused(&self, items: Vec<Refused>) {
        self.0.refused(items);
    }
}

struct KeyringAdapter(Box<dyn Keyring>);

impl todav_app::accounts::Keyring for KeyringAdapter {
    fn store(&self, url: &str, user: &str, password: &str) -> std::result::Result<(), String> {
        match self.0.store(url.into(), user.into(), password.into()) {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }

    fn clear(&self, url: &str, user: &str) {
        self.0.clear(url.into(), user.into());
    }
}

// --- the object ------------------------------------------------------------------------

/// The local database, the accounts and the background service.
#[derive(uniffi::Object)]
pub struct Todav {
    client: Arc<Client>,
    service: OnceLock<Arc<Service>>,
}

impl Todav {
    fn c(&self) -> &Client {
        &self.client
    }

    fn service(&self) -> Option<&Arc<Service>> {
        self.service.get()
    }
}

#[uniffi::export]
impl Todav {
    /// Open (or create) the database in `data_dir`.
    #[uniffi::constructor]
    pub fn open(data_dir: String) -> Result<Arc<Self>> {
        Ok(Arc::new(Todav {
            client: Arc::new(Client::open(data_dir)?),
            service: OnceLock::new(),
        }))
    }

    /// The system's time zone ("Europe/Warsaw"), for new event times.
    pub fn set_local_zone(&self, tzid: String) {
        self.c().set_local_zone(tzid)
    }

    pub fn subscribe(&self, listener: Box<dyn Listener>) {
        self.c().subscribe(Box::new(ListenerAdapter(listener)))
    }

    /// Now, in local wall-clock seconds.
    pub fn local_now(&self) -> i64 {
        self.c().local_now()
    }

    // --- accounts

    pub fn accounts(&self) -> Vec<Account> {
        self.c().accounts()
    }

    pub fn account(&self, id: i64) -> Option<Account> {
        self.c().account(id)
    }

    /// Add an account without a password to store: the local one, or a Nextcloud account whose
    /// password is the platform's (`source` says where). Else `add_nextcloud`.
    pub fn add_account(
        &self,
        kind: AccountKind,
        url: String,
        user: String,
        source: String,
    ) -> Result<i64> {
        Ok(self.c().add_account(kind, url, user, source)?)
    }

    /// Add a Nextcloud account signed in in Todav, its app password in `keyring`, and connect
    /// it; with `settings`, settings sync through it.
    pub fn add_nextcloud(
        &self,
        keyring: Box<dyn Keyring>,
        url: String,
        user: String,
        password: String,
        settings: bool,
    ) -> Result<i64> {
        let k = KeyringAdapter(keyring);
        let c = self.c();
        let id = todav_app::accounts::add(c, &k, &url, &user, password, String::new(), settings)?;
        Ok(id)
    }

    /// Account `id` at `url` with a new app password, or the same user on a moved server.
    pub fn reconnect(
        &self,
        keyring: Box<dyn Keyring>,
        id: i64,
        url: String,
        password: String,
    ) -> Result<()> {
        let k = KeyringAdapter(keyring);
        Ok(todav_app::accounts::reconnect(
            self.c(),
            &k,
            id,
            &url,
            password,
        )?)
    }

    /// Remove account `id` with its saved password; edits waiting for its server go up first,
    /// unless `force` (then they are lost).
    pub fn remove_account(&self, keyring: Box<dyn Keyring>, id: i64, force: bool) -> Result<()> {
        let k = KeyringAdapter(keyring);
        todav_app::accounts::remove(self.c(), &k, id, force)?;
        if let Some(s) = self.service() {
            s.account_removed(id);
        }
        Ok(())
    }

    pub fn reorder_accounts(&self, ids: Vec<i64>) -> Result<()> {
        Ok(self.c().reorder_accounts(ids)?)
    }

    pub fn rename_account(&self, id: i64, name: String) -> Result<()> {
        Ok(self.c().rename_account(id, name)?)
    }

    pub fn set_account_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        Ok(self.c().set_account_enabled(id, enabled)?)
    }

    /// Connect account `id` with its password (at start, from the keyring).
    pub fn connect(&self, id: i64, password: String) -> Result<()> {
        Ok(self.c().connect(id, password)?)
    }

    pub fn connected(&self, id: i64) -> bool {
        self.c().connected(id)
    }

    /// Switched on and connected.
    pub fn syncing(&self, account: Account) -> bool {
        todav_app::accounts::syncing(self.c(), &account)
    }

    pub fn settings_account(&self) -> Option<i64> {
        self.c().settings_account()
    }

    pub fn set_settings_account(&self, id: Option<i64>) -> Result<()> {
        Ok(self.c().set_settings_account(id)?)
    }

    pub fn server_info(&self, id: i64) -> Result<ServerInfo> {
        Ok(self.c().server_info(id)?)
    }

    // --- lists and calendars

    pub fn lists(&self) -> Vec<TodavList> {
        self.c().lists()
    }

    pub fn calendars(&self) -> Vec<TodavList> {
        self.c().calendars()
    }

    /// `lists` grouped by account, in the user's order of the accounts.
    pub fn by_place(&self, lists: Vec<TodavList>) -> Vec<TodavList> {
        todav_app::lists::by_place(self.c(), lists)
    }

    /// The accounts new lists can go to now.
    pub fn places(&self) -> Vec<i64> {
        todav_app::lists::places(self.c())
    }

    /// Returns the new list's href.
    pub fn create_list(
        &self,
        account: i64,
        name: String,
        color: String,
        events: bool,
    ) -> Result<String> {
        Ok(self.c().create_list(account, name, color, events)?)
    }

    pub fn edit_list(&self, href: String, name: String, color: String) -> Result<()> {
        Ok(self.c().edit_list(href, name, color)?)
    }

    pub fn delete_list(&self, href: String) -> Result<()> {
        Ok(self.c().delete_list(href)?)
    }

    /// A new local account's lists: a task list with a task and an empty calendar.
    pub fn start_local(&self, account: i64, starter: Starter) -> Result<()> {
        Ok(todav_app::lists::start_local(self.c(), account, starter)?)
    }

    pub fn categories(&self, list_href: String) -> Vec<CategoryMeta> {
        self.c().categories(list_href)
    }

    // --- tasks

    pub fn tasks(&self, list_href: String, include_done: bool) -> Vec<Task> {
        self.c().tasks(list_href, include_done)
    }

    pub fn task(&self, uid: String) -> Option<Task> {
        self.c().task(uid)
    }

    /// Open tasks of a list by category, sorted and with completed ones as `done` says.
    pub fn grouped_by(
        &self,
        list_href: String,
        tasks: Sort,
        subtasks: Sort,
        done: DoneView,
    ) -> Vec<CategoryGroup> {
        self.c().grouped_by(list_href, tasks, subtasks, done)
    }

    pub fn ungrouped_by(
        &self,
        list_href: String,
        tasks: Sort,
        subtasks: Sort,
        done: DoneView,
    ) -> Vec<Task> {
        self.c().ungrouped_by(list_href, tasks, subtasks, done)
    }

    /// The completed tasks shown apart.
    pub fn finished(&self, list_href: String, done: DoneView) -> Vec<Task> {
        self.c().finished(list_href, done)
    }

    pub fn upcoming(&self) -> Vec<Task> {
        self.c().upcoming()
    }

    pub fn important(&self) -> Vec<Task> {
        self.c().important()
    }

    /// The pages shown above the lists, with their counts.
    pub fn pages(&self) -> Vec<PageCount> {
        todav_app::tasks::pages(self.c())
            .into_iter()
            .map(|(page, n)| PageCount {
                page,
                count: n as u32,
            })
            .collect()
    }

    pub fn add_task(
        &self,
        list_href: String,
        summary: String,
        category: Option<String>,
        parent_uid: Option<String>,
        at_start: bool,
    ) -> Result<Task> {
        Ok(self
            .c()
            .add_task(list_href, summary, category, parent_uid, at_start)?)
    }

    pub fn set_done(&self, uid: String, done: bool) -> Result<()> {
        Ok(self.c().set_done(uid, done)?)
    }

    /// As `set_done`, one change for undo. With `tree`, completing also completes the task's
    /// open subtasks and reopening reopens its completed parents (`SubtaskCompletion::Complete`).
    pub fn set_done_tree(&self, uid: String, done: bool, tree: bool) -> Result<()> {
        Ok(todav_app::tasks::set_done(self.c(), &uid, done, tree)?)
    }

    /// The task's open subtasks, at any depth.
    pub fn open_subtasks(&self, uid: String) -> Vec<Task> {
        todav_app::tasks::open_subtasks(self.c(), &uid)
    }

    pub fn update_task(&self, uid: String, patch: TaskPatch) -> Result<()> {
        Ok(self.c().update_task(uid, patch.into())?)
    }

    pub fn delete_task(&self, uid: String) -> Result<()> {
        Ok(self.c().delete_task(uid)?)
    }

    /// Move `uid` in front of `before_uid` (None: last) in manual order.
    pub fn reorder(&self, uid: String, before_uid: Option<String>) -> Result<()> {
        Ok(self.c().reorder(uid, before_uid)?)
    }

    pub fn promote(&self, uid: String) -> Result<()> {
        Ok(self.c().promote(uid)?)
    }

    pub fn set_parent(&self, uid: String, parent_uid: String) -> Result<()> {
        Ok(self.c().set_parent(uid, parent_uid)?)
    }

    /// Whether `uid` is `ancestor` or in its tree.
    pub fn within(&self, uid: String, ancestor: String) -> bool {
        self.c().within(uid, ancestor)
    }

    /// Move a task (with its subtasks) to another list.
    pub fn move_task(&self, uid: String, to: String) -> Result<()> {
        Ok(self.c().move_task(uid, to)?)
    }

    pub fn can_adopt(&self, dragged: Task, parent: Task) -> bool {
        todav_app::tasks::can_adopt(self.c(), &dragged, &parent)
    }

    pub fn can_join(&self, dragged: Task, parent: Task) -> bool {
        todav_app::tasks::can_join(self.c(), &dragged, &parent)
    }

    /// Make `dragged` a subtask of `parent`.
    pub fn adopt(&self, dragged: Task, parent: Task) -> Result<()> {
        Ok(todav_app::tasks::adopt(self.c(), &dragged, &parent)?)
    }

    pub fn can_drop(&self, dragged: Task, from: Slot, target: Task, to: Slot) -> bool {
        todav_app::tasks::can_drop(self.c(), &dragged, &from, &target, &to)
    }

    /// Move `dragged` above or below `target`.
    pub fn drop_task(
        &self,
        dragged: Task,
        from: Slot,
        target: Task,
        to: Slot,
        above: bool,
    ) -> Result<()> {
        let c = self.c();
        Ok(todav_app::tasks::drop_task(
            c, &dragged, &from, &target, &to, above,
        )?)
    }

    /// A task dropped on a category (None: "Other").
    pub fn drop_on_category(
        &self,
        dragged: Task,
        from: Slot,
        category: Option<String>,
    ) -> Result<()> {
        let c = self.c();
        Ok(todav_app::tasks::drop_on_category(
            c, &dragged, &from, category,
        )?)
    }

    // --- undo

    /// What `changes` does is undone together, as one step.
    pub fn together(&self, changes: Box<dyn Changes>) {
        self.c().together(|| changes.run())
    }

    /// The latest change, to undo just that one later (a snackbar's Undo).
    pub fn last_change(&self) -> Option<u64> {
        self.c().last_change()
    }

    /// Put back what change `id` (else the latest) replaced; false when there is none.
    pub fn undo(&self, id: Option<u64>) -> Result<bool> {
        Ok(self.c().undo(id)?)
    }

    // --- events and the calendar

    pub fn events(&self, from: i64, to: i64) -> Vec<Event> {
        self.c().events(from, to)
    }

    pub fn event(&self, uid: String) -> Option<Event> {
        self.c().event(uid)
    }

    /// Tasks for an event.
    pub fn event_tasks(&self, event_uid: String) -> Vec<Task> {
        self.c().event_tasks(event_uid)
    }

    /// Dated tasks in `[from, to)`.
    pub fn scheduled(&self, from: i64, to: i64) -> Vec<Task> {
        self.c().scheduled(from, to)
    }

    /// Open tasks without a time slot, undated or due by `until`.
    pub fn unscheduled(&self, until: i64) -> Vec<Task> {
        self.c().unscheduled(until)
    }

    /// Returns the new event's UID.
    pub fn add_event(
        &self,
        list_href: String,
        summary: String,
        time: EventTime,
        reminders: Vec<i64>,
    ) -> Result<String> {
        Ok(self.c().add_event(list_href, summary, time, reminders)?)
    }

    pub fn update_event(
        &self,
        list_href: String,
        uid: String,
        recurrence_id: Option<i64>,
        scope: Scope,
        patch: EventPatch,
    ) -> Result<()> {
        let c = self.c();
        Ok(c.update_event(list_href, uid, recurrence_id, scope, patch.into())?)
    }

    pub fn delete_event(
        &self,
        list_href: String,
        uid: String,
        recurrence_id: Option<i64>,
        scope: Scope,
    ) -> Result<()> {
        Ok(self
            .c()
            .delete_event(list_href, uid, recurrence_id, scope)?)
    }

    /// Move an event (the whole series) to another calendar.
    pub fn move_event(&self, from: String, uid: String, to: String) -> Result<()> {
        Ok(self.c().move_event(from, uid, to)?)
    }

    /// Events and dated tasks in `[from, to)` in the collections shown.
    pub fn items(&self, from: i64, to: i64) -> Vec<Item> {
        let items = todav_app::calendar::items(self.c(), from, to);
        items.into_iter().map(Into::into).collect()
    }

    /// Every collection's colour, by href.
    pub fn colors(&self) -> HashMap<String, String> {
        todav_app::calendar::colors(self.c())
    }

    /// For each of `days` days from `from`, the colours of the collections with something on it
    /// (three at most).
    pub fn day_colors(&self, from: i64, days: i64) -> Vec<Vec<String>> {
        todav_app::calendar::day_colors(self.c(), from, days)
    }

    /// What is on each of `days` days from `from`.
    pub fn agenda(&self, from: i64, days: i64) -> Vec<AgendaEntry> {
        let entries = todav_app::calendar::agenda(self.c(), from, days);
        entries.into_iter().map(Into::into).collect()
    }

    /// Today's events in the calendars shown.
    pub fn today_events(&self) -> Vec<Event> {
        todav_app::calendar::today_events(self.c())
    }

    /// Events `filter` takes within a year of now: each series once, the coming ones first.
    pub fn search_events(&self, filter: Box<dyn EventFilter>) -> Vec<Event> {
        let now = self.c().local_now();
        todav_app::calendar::search(self.c(), now, |e| filter.hit(e.clone()))
    }

    /// What dropping `item` on an hour slot starting at `at` does.
    pub fn slot_drop(&self, item: Item, at: i64) -> Change {
        todav_app::calendar::slot_drop(self.c(), &item.into(), at).into()
    }

    /// How a task is planned into the slot starting at `at`.
    pub fn plan_at(&self, task: Task, at: i64) -> TaskPatch {
        todav_app::calendar::plan_at(self.c(), &task, at).into()
    }

    /// Move `item` to the list or calendar `href`.
    pub fn move_to_list(&self, item: Item, href: String) -> Result<()> {
        Ok(todav_app::calendar::move_to_list(
            self.c(),
            &item.into(),
            href,
        )?)
    }

    /// Minutes a dropped item lasts.
    pub fn drop_length(&self, item: Item) -> i64 {
        todav_app::calendar::drop_length(self.c(), &item.into())
    }

    /// When `item` takes hours in the week view; None for what goes above them.
    pub fn hours(&self, item: Item) -> Option<Hours> {
        todav_app::calendar::span(self.c(), &item.into()).map(|(from, to)| Hours { from, to })
    }

    /// `item`'s part within each day of the week starting at `start`.
    pub fn segments(&self, item: Item, start: i64) -> Vec<Segment> {
        todav_app::calendar::segments(self.c(), &item.into(), start)
            .into_iter()
            .map(|(day, from, to)| Segment { day, from, to })
            .collect()
    }

    // --- settings

    pub fn setting(&self, key: String) -> Option<String> {
        self.c().setting(key)
    }

    /// Stores `value`; it syncs unless `key` is device-only.
    pub fn set_setting(&self, key: String, value: String) -> Result<()> {
        Ok(self.c().set_setting(key, value)?)
    }

    pub fn order(&self) -> Order {
        todav_app::prefs::order(self.c())
    }

    pub fn done_view(&self) -> DoneView {
        todav_app::prefs::done_view(self.c())
    }

    /// Where `key` ("task_sort", "sub_sort") is in `sort_values`.
    pub fn sort_index(&self, key: String) -> u32 {
        todav_app::prefs::sort_index(self.c(), &key) as u32
    }

    pub fn stars(&self) -> bool {
        todav_app::prefs::stars(self.c())
    }

    pub fn pins(&self) -> bool {
        todav_app::prefs::pins(self.c())
    }

    pub fn mini_dots(&self) -> bool {
        todav_app::prefs::mini_dots(self.c())
    }

    pub fn subtask_completion(&self) -> SubtaskCompletion {
        todav_app::prefs::subtask_completion(self.c())
    }

    /// Whether a click on a task's title edits it in place (mouse and touchpad only).
    pub fn inline_edit(&self) -> bool {
        todav_app::prefs::inline_edit(self.c())
    }

    pub fn double_click_edit(&self) -> bool {
        todav_app::prefs::double_click_edit(self.c())
    }

    /// Seconds a planned task's slot lasts.
    pub fn task_duration(&self) -> i64 {
        todav_app::prefs::task_duration(self.c())
    }

    /// Seconds a new event lasts.
    pub fn event_duration(&self) -> i64 {
        todav_app::prefs::event_duration(self.c())
    }

    /// When reminders of tasks without a time go off, in seconds into the day.
    pub fn all_day_time(&self) -> i64 {
        todav_app::prefs::all_day_time(self.c())
    }

    pub fn default_reminder(&self, all_day: bool) -> i64 {
        todav_app::prefs::default_reminder(self.c(), all_day)
    }

    /// Reminder choices for a due time or (`all_day`) a date, in seconds after it.
    pub fn reminder_presets(&self, all_day: bool) -> Vec<i64> {
        todav_app::dates::presets(self.c(), all_day)
    }

    /// Collections hidden on the calendar.
    pub fn hidden(&self) -> Vec<String> {
        let mut h: Vec<String> = todav_app::prefs::hidden(self.c()).into_iter().collect();
        h.sort();
        h
    }

    pub fn set_hidden(&self, hrefs: Vec<String>, hide: bool) {
        todav_app::prefs::set_hidden(self.c(), hrefs.iter().map(String::as_str), hide)
    }

    // --- sync and push

    /// Push and pull every account now (the service does this by itself).
    pub fn sync(&self) -> SyncReport {
        self.c().sync().into()
    }

    pub fn sync_account(&self, id: i64) -> Result<SyncReport> {
        Ok(self.c().sync_account(id)?.into())
    }

    /// Pull the list a push message names (`push_decode`).
    pub fn sync_topic(&self, topic: String) -> Result<SyncReport> {
        Ok(self.c().sync_topic(topic)?.into())
    }

    /// When account `id` last synced, in UTC seconds.
    pub fn last_sync(&self, id: i64) -> Option<i64> {
        self.c().last_sync(id)
    }

    /// Edits wait for the server (of account `id`, else any).
    pub fn pending(&self, id: Option<i64>) -> bool {
        self.c().pending(id)
    }

    /// Register WebDAV-Push for every list that has it, to `push_resource` (a UnifiedPush
    /// endpoint).
    pub fn push_register(
        &self,
        push_resource: String,
        expires_in_secs: u64,
    ) -> Result<Vec<PushRegistration>> {
        Ok(self.c().push_register(push_resource, expires_in_secs)?)
    }

    pub fn push_unregister(&self) -> Result<()> {
        Ok(self.c().push_unregister()?)
    }

    /// The topics a push message names (decrypted if need be).
    pub fn push_decode(&self, body: Vec<u8>) -> Result<Vec<String>> {
        Ok(self.c().push_decode(body)?)
    }

    pub fn push_status(&self, account: i64) -> PushStatus {
        self.c().push_status(account)
    }

    // --- the background service

    /// Start the service: the sync worker and (`listen`, where the platform has no push service
    /// of its own) the ntfy listener, and sync. True the first time; later calls just sync.
    pub fn start_service(&self, shell: Box<dyn ServiceShell>, listen: bool) -> bool {
        let shell = Box::new(ShellAdapter(shell));
        let s = self
            .service
            .get_or_init(|| Service::new(self.client.clone(), shell));
        s.start(listen)
    }

    /// Ask for a full sync; requests arriving while one runs are coalesced.
    pub fn sync_now(&self) {
        if let Some(s) = self.service() {
            s.sync_now()
        }
    }

    /// After an edit: sync if it left something for the server.
    pub fn request_sync(&self) {
        if let Some(s) = self.service() {
            s.request_sync()
        }
    }

    /// Some account syncs: the sync buttons work.
    pub fn can_sync(&self) -> bool {
        self.service().is_some_and(|s| s.can_sync())
    }

    pub fn sync_state(&self) -> SyncState {
        self.service().map_or(SyncState::Idle, |s| s.state().into())
    }

    /// Why account `id`'s last sync failed.
    pub fn sync_error(&self, id: i64) -> Option<String> {
        self.service()?.sync_error(id)
    }

    pub fn push_error(&self) -> Option<String> {
        self.service()?.push_error()
    }

    /// The ntfy server changed here: listen to it.
    pub fn restart_push(&self) {
        if let Some(s) = self.service() {
            s.restart_push()
        }
    }

    /// An account came or got its password: sync it (and register push, with `listen`).
    pub fn account_added(&self, listen: bool) {
        if let Some(s) = self.service() {
            s.account_added(listen)
        }
    }

    /// Call once a minute; `active`: the app is in front.
    pub fn tick(&self, active: bool) {
        if let Some(s) = self.service() {
            s.tick(active)
        }
    }

    /// When the next reminder goes off (local time), within a week: set an alarm for it, and
    /// again after a sync or a change. Works without the service.
    pub fn next_reminder(&self) -> Option<i64> {
        let now = self.c().local_now();
        todav_app::service::next_reminder(self.c(), now)
    }

    /// Reminders that went off since the last call.
    pub fn due_reminders(&self) -> DueReminders {
        match self.service() {
            Some(s) => {
                let (tasks, events) = s.due_reminders();
                DueReminders { tasks, events }
            }
            None => DueReminders {
                tasks: Vec::new(),
                events: Vec::new(),
            },
        }
    }
}

// --- translations ---------------------------------------------------------------------

/// One language's messages, from the `todav` gettext catalogs: what the Linux app shows, with
/// the same contexts and plural rules. Placeholders are named (`{n}`); fill them after.
#[derive(uniffi::Object)]
pub struct Translations(todav_app::i18n::Catalog);

#[uniffi::export]
impl Translations {
    /// A language's catalog: the `.mo` file msgfmt makes of `po/<language>.po`.
    #[uniffi::constructor]
    pub fn new(mo: Vec<u8>) -> Result<Arc<Self>> {
        match todav_app::i18n::Catalog::parse(&mo) {
            Some(c) => Ok(Arc::new(Translations(c))),
            None => Err(TodavError::Parse {
                reason: "not a .mo file".into(),
            }),
        }
    }

    /// English: the messages as written.
    #[uniffi::constructor]
    pub fn english() -> Arc<Self> {
        Arc::new(Translations(todav_app::i18n::Catalog::english()))
    }

    pub fn gettext(&self, msgid: String) -> String {
        self.0.gettext(&msgid)
    }

    /// The form for `n` of a counted message.
    pub fn ngettext(&self, one: String, many: String, n: u64) -> String {
        self.0.ngettext(&one, &many, n)
    }

    /// A message with a context, for a short word that could translate two ways.
    pub fn pgettext(&self, context: String, msgid: String) -> String {
        self.0.pgettext(&context, &msgid)
    }
}

// --- sign-in -------------------------------------------------------------------------------

/// Start Nextcloud's Login Flow v2: open `login_url` in a browser, then poll.
#[uniffi::export]
pub fn login_flow_start(server: String) -> Result<LoginFlow> {
    Ok(todav_core::login_flow_start(server)?)
}

/// The credentials once the user granted access, None while waiting.
#[uniffi::export]
pub fn login_flow_poll(flow: LoginFlow) -> Result<Option<Credentials>> {
    Ok(todav_core::login_flow_poll(&flow)?)
}

/// Sign in with a login name and a password (or app password): an app password for Todav.
#[uniffi::export]
pub fn password_login(server: String, login_name: String, password: String) -> Result<Credentials> {
    Ok(todav_core::password_login(server, login_name, password)?)
}

/// Whether an ntfy server answers.
#[uniffi::export]
pub fn ntfy_check(ntfy_url: String) -> Result<()> {
    Ok(todav_core::ntfy_check(ntfy_url)?)
}

/// The host of a Nextcloud account's server, for its default name ("user@host").
#[uniffi::export]
pub fn account_host(account: Account) -> String {
    todav_app::accounts::host(&account).into()
}

// --- lists -----------------------------------------------------------------------------

/// A list's colour as the app shows it ("#rrggbb").
#[uniffi::export]
pub fn list_color(list: TodavList) -> String {
    todav_app::lists::color(&list)
}

/// Nextcloud's colour presets, in its order.
#[uniffi::export]
pub fn color_presets() -> Vec<String> {
    todav_app::lists::PRESETS
        .iter()
        .map(|p| p.to_string())
        .collect()
}

/// The account heading each of `lists` (as `Todav::by_place` orders them), when they are from
/// more than one.
#[uniffi::export]
pub fn list_headings(lists: Vec<TodavList>) -> Vec<Option<i64>> {
    todav_app::lists::headings(&lists)
}

/// Whether two hrefs are the same list.
#[uniffi::export]
pub fn same_list(stored: String, href: String) -> bool {
    todav_core::same_list(&stored, &href)
}

// --- dates -----------------------------------------------------------------------------

/// Days from the day of `now` to the day of `t`.
#[uniffi::export]
pub fn days_between(t: i64, now: i64) -> i64 {
    todav_app::dates::days(t, now)
}

/// Midnight of the day holding `t`.
#[uniffi::export]
pub fn midnight(t: i64) -> i64 {
    todav_app::dates::midnight(t)
}

#[uniffi::export]
pub fn date_of(t: i64) -> Date {
    let (year, month, day) = todav_app::dates::date(t);
    Date { year, month, day }
}

/// Midnight of a date; months past 12 or below 1 go on into other years.
#[uniffi::export]
pub fn midnight_on(year: i64, month: i64, day: i64) -> i64 {
    todav_app::dates::on(year, month, day)
}

#[uniffi::export]
pub fn month_first(t: i64) -> i64 {
    todav_app::dates::month_first(t)
}

/// Midnight of the first day of the week holding `t`, weeks starting on `first` (0 Sunday).
#[uniffi::export]
pub fn week_start(t: i64, first: i64) -> i64 {
    todav_app::dates::week_start(t, first)
}

/// How a day is named, seen from `now`.
#[uniffi::export]
pub fn day_name(t: i64, now: i64) -> Day {
    todav_app::dates::day(t, now)
}

#[uniffi::export]
pub fn overdue(task: Task, now: i64) -> bool {
    todav_app::dates::overdue(&task, now)
}

/// Due today or earlier.
#[uniffi::export]
pub fn pressing(task: Task, now: i64) -> bool {
    todav_app::dates::pressing(&task, now)
}

/// The Upcoming section of an open task with a due date.
#[uniffi::export]
pub fn section(task: Task, now: i64) -> Section {
    todav_app::dates::section(&task, now).into()
}

/// Reminder offsets for a due time: at it, 5, 15 and 30 minutes, an hour and a day before.
#[uniffi::export]
pub fn timed_reminders() -> Vec<i64> {
    todav_app::dates::TIMED.to_vec()
}

/// A reminder offset in the largest unit it fits exactly.
#[uniffi::export]
pub fn reminder_span(offset: i64) -> ReminderSpan {
    let (amount, unit, before) = todav_app::dates::span(offset);
    ReminderSpan {
        amount,
        unit,
        before,
    }
}

#[uniffi::export]
pub fn reminder_offset(span: ReminderSpan) -> i64 {
    todav_app::dates::offset(span.amount, span.unit, span.before)
}

// --- task trees ------------------------------------------------------------------------

/// A group's depth-first tasks as top-level tasks with their subtasks.
#[uniffi::export]
pub fn trees(tasks: Vec<Task>) -> Vec<Tree> {
    todav_app::tasks::trees(&tasks)
        .into_iter()
        .map(|(task, subs)| Tree {
            task: task.clone(),
            subtasks: subs
                .into_iter()
                .map(|(depth, task)| Subtask {
                    depth: depth as u32,
                    task: task.clone(),
                })
                .collect(),
        })
        .collect()
}

/// Open and done direct subtasks of each task, by UID.
#[uniffi::export]
pub fn subtask_counts(tasks: Vec<Task>) -> HashMap<String, SubtaskCount> {
    todav_app::tasks::subtask_counts(&tasks)
        .into_iter()
        .map(|(uid, (open, done))| {
            let n = SubtaskCount {
                open: open as u32,
                done: done as u32,
            };
            (uid, n)
        })
        .collect()
}

/// Where subtask `uid`'s "Add subtask" entry goes in a tree: first among its subtasks with
/// `start`, else after them.
#[uniffi::export]
pub fn entry_at(subtasks: Vec<Subtask>, uid: String, start: bool) -> Option<EntryAt> {
    let subs: Vec<(usize, &Task)> = subtasks
        .iter()
        .map(|s| (s.depth as usize, &s.task))
        .collect();
    let (before, index) = todav_app::tasks::entry_at(&subs, &uid, start)?;
    Some(EntryAt {
        before: before as u32,
        index: index as u32,
    })
}

/// Whether dropping a task above or below another would leave it where it is.
#[uniffi::export]
pub fn stays_put(dragged: Task, from: Slot, target: Task, to: Slot, above: bool) -> bool {
    todav_app::tasks::stays_put(&dragged, &from, &target, &to, above)
}

// --- the calendar ----------------------------------------------------------------------

/// A task's place on the calendar.
#[uniffi::export]
pub fn task_when(task: Task) -> When {
    let (at, all_day) = todav_app::calendar::when(&task);
    When { at, all_day }
}

/// Whether `item` is on the day starting at `day`.
#[uniffi::export]
pub fn on_day(item: Item, day: i64) -> bool {
    todav_app::calendar::on_day(&item.into(), day)
}

/// The collections with something on `day` among `items`, three at most.
#[uniffi::export]
pub fn day_lists(items: Vec<Item>, day: i64) -> Vec<String> {
    let all: Vec<todav_app::calendar::Item> = items.into_iter().map(Into::into).collect();
    let lists = todav_app::calendar::day_lists(&all, day);
    lists.into_iter().map(String::from).collect()
}

/// What dropping `item` on the day starting at `day` does; with `all_day`, into the day's
/// top row.
#[uniffi::export]
pub fn day_drop(item: Item, day: i64, all_day: bool) -> Change {
    todav_app::calendar::day_drop(&item.into(), day, all_day).into()
}

/// `item` resized in the week view to `[from, to)` by its top edge or its bottom.
#[uniffi::export]
pub fn resize(item: Item, from: i64, to: i64, top: bool) -> Change {
    todav_app::calendar::resize(&item.into(), from, to, top).into()
}

/// A task moved to the day starting at `day`, keeping its times of day.
#[uniffi::export]
pub fn move_to_day(task: Task, day: i64) -> TaskPatch {
    todav_app::calendar::move_to_day(&task, day).into()
}

/// Where a planned task's slot ends.
#[uniffi::export]
pub fn slot_end(task: Task) -> Option<i64> {
    todav_app::calendar::slot_end(&task)
}

/// Whether a task has a time slot.
#[uniffi::export]
pub fn planned(task: Task) -> bool {
    todav_app::calendar::planned(&task)
}

/// A task back to "To Plan"; None when it has no slot.
#[uniffi::export]
pub fn unplan(task: Task) -> Option<TaskPatch> {
    todav_app::calendar::unplan(&task).map(Into::into)
}

/// Columns for the blocks of one day: overlapping ones side by side.
#[uniffi::export]
pub fn columns(blocks: Vec<DayBlock>) -> Vec<BlockColumn> {
    let segs = blocks
        .into_iter()
        .map(|b| (b.index as usize, b.from, b.to))
        .collect();
    todav_app::calendar::columns(segs)
        .into_iter()
        .map(|(index, column, columns)| BlockColumn {
            index: index as u32,
            column: column as u32,
            columns: columns as u32,
        })
        .collect()
}

/// Lanes for items covering the days `days[i]` (0–6, ascending) of a week's row, at most
/// `max` of them.
#[uniffi::export]
pub fn lanes(days: Vec<Vec<u32>>, max: u32) -> Lanes {
    let days: Vec<Vec<usize>> = days
        .into_iter()
        .map(|d| {
            d.into_iter()
                .map(|x| x as usize)
                .filter(|x| *x < 7)
                .collect()
        })
        .collect();
    let (pieces, more) = todav_app::calendar::lanes(&days, max as usize);
    Lanes {
        pieces: pieces
            .into_iter()
            .map(|p| Piece {
                item: p.item as u32,
                lane: p.lane as u32,
                from: p.from as u32,
                to: p.to as u32,
            })
            .collect(),
        more: more
            .into_iter()
            .map(|m| m.into_iter().map(|i| i as u32).collect())
            .collect(),
    }
}

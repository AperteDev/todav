# Todav — Features

What Todav can do, in short. The details of each release are in the
[changelog](CHANGELOG.md), the plans in the [roadmap](ROADMAP.md).

## Now

**Tasks**

- Task lists from your Nextcloud calendars; create, rename, recolour and delete them.
- Categories (CalDAV `CATEGORIES`), shown as groups or tags, with a filter in the sidebar.
- Subtasks nested to any depth, and notes.
- Manual order by drag and drop (the order Nextcloud Tasks uses), or sorted by creation date,
  due date or priority.
- Priority on a star, coloured by its level, as Nextcloud Tasks stars tasks; an Important page
  with the high-priority tasks of every list.
- Pinned tasks stay at the top of their list.
- Move a task with its subtasks to another list, by drag and drop or from the task dialog.
- Due dates and start dates, with or without a time.
- Several reminders per task, before or after its due date or its start, as notifications with
  "Mark as Done"; one already past when set goes off at once.
- An Upcoming page across all lists: Overdue, Today, Tomorrow, the coming days and later.
- Search across all lists, ignoring accents.
- Undo with Ctrl+Z or a toast's Undo.

**Calendar**

- Month, Week and Agenda views of your Nextcloud calendars, with a sidebar month and show/hide
  per calendar.
- Create, edit, move and delete events, recurring ones too (this occurrence, this and the
  following, or all), with reminders.
- Priority on events, as on tasks: none, low, medium or high (the one Thunderbird sets).
- Plan a task by dropping it from "To Plan" on an hour (a time slot) or a day (due that day),
  with a preview in 15-minute steps; drop it back to unplan it.
- Move events and planned tasks by drag and drop; resize them in the week view by their edges.
- Drop an event on another calendar in the sidebar to move it there.
- An event's or a task's details on a click (subtasks, an event's tasks), editing on a
  double-click.
- Tasks linked to events: an event lists its tasks, a task shows its event.
- Opens iCalendar files (`.ics`), importing their events and tasks.
- Copy Link: a `todav://` link that opens a task or an event, to paste elsewhere.

**Accounts and sync**

- Sign in through GNOME Online Accounts, the browser (Nextcloud Login Flow) or a login name and
  app password; passwords stay in the system keyring.
- Several accounts side by side: Nextcloud on one server or more, a Google account (its
  calendars and Google Tasks) and lists kept on this device; each can be switched off.
- Use Todav without a server: lists and calendars on this device only, alone or next to synced
  ones.
- Local-first: works offline from a local copy; changes go up when the server is reachable.
- Changes from elsewhere within seconds with WebDAV Push and ntfy; otherwise a check every minute
  while the window is focused and every 30 minutes in the background.
- Conflicts resolve per task, and a task ticked done on any device stays done.
- Properties Todav does not understand are kept, so other clients' data survives.
- Preferences synced between devices through `.config/todav/settings.json` in the Nextcloud
  Files of the account you pick, or none.

**GNOME**

- Runs in the background: closing the window hides it; reminders and sync go on. Starts with
  your session.
- A GNOME Shell extension: a count by the clock and a menu with a task list, the month and the
  agenda, which adds tasks and events; or the same in GNOME's clock menu.
- Can be the system's calendar app, opened by the date in GNOME's clock menu.
- Follows the system style; adaptive layout for narrow windows and touchscreens; keyboard
  shortcuts.
- Restart into a new version from a banner, when Todav is updated while it runs.

**Android**

- The same tasks and calendar in Material 3 Expressive: a navigation bar on phones, a rail and a
  resizable sidebar on unfolded foldables and tablets.
- Swipe actions of your choice; drag and drop to reorder tasks, make subtasks, change a category
  and plan tasks into the calendar's hours.
- Month, Week, Work Week, 3 Days, Day and Agenda views.
- Home-screen widgets: a task list, the month, the agenda, a day's hours and a month planner.
- Push through UnifiedPush (the ntfy app, for instance), without Google services.
- Reminders with "Mark as Done" and Snooze; "New Task" from the launcher; text shared as a task.
- Syncs with Nextcloud by itself, no DAVx⁵ needed; the Nextcloud app's accounts fill in the
  sign-in.
- System Calendars: the phone's other calendars (Google, Exchange, DAVx⁵…), read-only.

**Languages**

- English and Polish; German, French, Spanish, Italian and Brazilian Portuguese as drafts.

## Planned

- Advanced notifications, configurable.
- Advanced search, filtering and sorting.
- Settings per list and per calendar, such as how its completed tasks show.
- Export of lists and calendars to iCalendar files.
- More services besides Nextcloud: other CalDAV servers first.
- Google Tasks on Android, with a Google sign-in of its own.
- Google Play and F-Droid for the Android app; Flathub for the Linux one.
- Editing System Calendars' events on Android.
- Moving a whole list to another account.
- A Pebble app, through the Android app.
- A Windows app.
- A linked task's title and state in Notav's notes.

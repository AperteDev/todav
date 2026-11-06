# Todav — Features

What Todav can do, in short. The details of each release are in the
[changelog](CHANGELOG.md), the plans in the [roadmap](ROADMAP.md).

## Now

**Tasks**

- Task lists from your Nextcloud calendars; create, rename, recolour and delete them.
- Categories (CalDAV `CATEGORIES`), shown as groups or tags, with a filter (the sidebar on
  Linux, a menu on Android): add, rename, delete (the tasks stay) and put them in order by
  dragging; the order is kept in the account's Nextcloud Files, so every device follows it.
- Subtasks nested to any depth, and notes.
- Notes in light Markdown, kept as plain text for other apps: bold, italic, headings, lists;
  links and e-mail addresses open with a click, checkboxes (`- [ ]`) tick in place.
- Manual order by drag and drop (the order Nextcloud Tasks uses), or sorted by creation date,
  due date or priority.
- Priority on a star, coloured by its level, as Nextcloud Tasks stars tasks; an Important page
  with the high-priority tasks of every list.
- Pinned tasks stay at the top of their list.
- A task completed where it doesn't stay (into the Completed group, out of Upcoming) shows
  struck through a moment, then fades out: in the app, the GNOME extension and the widget.
- Move a task with its subtasks to another list, by drag and drop or from the task dialog;
  with Ctrl held while dropping, a copy goes there.
- Duplicate a task with its subtasks, or an event, from its dialog or details.
- Due dates and start dates, with or without a time.
- Repeating tasks: daily, weekly, monthly or yearly, every n, until a day, from their dates or
  from when they are done; ticked, one moves on to its next date.
- Several reminders per task, before or after its due date or its start, as notifications with
  "Mark as Done" and Snooze (back in ten minutes); one already past when set goes off at once.
- Urgent reminders for important tasks and events (high priority), for all with a setting:
  they stay on screen until dismissed (on Android: a channel of their own).
- An Upcoming page across all lists: Overdue, Today, Tomorrow, the coming days and later.
- Search across all lists, ignoring accents.
- Undo with Ctrl+Z or a toast's Undo.

**Calendar**

- Month, Week, Work Week, 3 Days, Day and Agenda views of your Nextcloud calendars, with a sidebar month and show/hide
  per calendar.
- Calendars subscribed to by link in Nextcloud (e.g. a work Outlook calendar), read-only.
- Create, edit, move and delete events, recurring ones too (this occurrence, this and the
  following, or all), with reminders.
- Add an event from the month: click a day, or drag across days, for a popover with its title,
  colour and calendar; on Android, hold a day, or hold and move across days.
- Priority on events, as on tasks: none, low, medium or high (the one Thunderbird sets).
- An event's own colour instead of its calendar's, as Nextcloud Calendar sets it: click the
  colour box by its title, in its details or its editor, to pick another.
- Plan a task by dropping it from "To Plan" on an hour (a time slot) or a day (due that day),
  with a preview in 15-minute steps; drop it back to unplan it.
- Move events and planned tasks by drag and drop; resize them in the week view by their edges.
- Drop an event on another calendar in the sidebar to move it there, or with Ctrl held, a copy
  (always a copy from a read-only calendar).
- An event's or a task's details on a click (subtasks, an event's tasks), with Edit there.
- Tasks linked to events: an event lists its tasks, a task shows its event; a task added for
  an event goes to its calendar, or, for calendars without tasks, to a list you pick (none by
  default).
- Opens iCalendar files (`.ics`), importing their events and tasks; exports a list or a
  calendar to one, from its dialog.
- Copy Link: a `todav://` link that opens a task or an event, to paste elsewhere.
- Week numbers, by a setting for the app and another for the GNOME extension and the widgets.
- Cancelled events and invitations you declined shown, crossed out (the default) or hidden.
- Join Call: a button for an event's video call (Meet, Zoom, Teams, Jitsi, Webex, Nextcloud
  Talk…), found in its link, place or notes, in its details, its reminder and the extension's
  agenda.

**Accounts and sync**

- Sign in through GNOME Online Accounts, the browser (Nextcloud Login Flow) or a login name and
  app password; passwords stay in the system keyring.
- Other CalDAV servers than Nextcloud with a login name and password: iCloud, Fastmail, Yahoo,
  mailbox.org and Posteo by name, any other by its address, or from GNOME Online Accounts.
  Their task lists and calendars sync, checked every minute (settings and categories' order stay
  on the device; DAV Push and synced settings are Nextcloud's).
- Several accounts side by side: Nextcloud on one server or more, other CalDAV servers, a Google
  account (its calendars and Google Tasks) and lists kept on this device; each can be switched
  off.
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
- A number on the app's icon in docks that show one (Dash to Dock and others), by a setting:
  overdue tasks, tasks due today or earlier, important tasks or today's events; by default the
  dock's own count of notifications.
- Follows the system style; adaptive layout for narrow windows and touchscreens; keyboard
  shortcuts.
- Restart into a new version from a banner, when Todav is updated while it runs.

**Android**

- The same tasks and calendar in Material 3 Expressive: a navigation bar on phones, a rail and a
  resizable sidebar on unfolded foldables and tablets.
- Swipe actions of your choice; drag and drop to reorder tasks, make subtasks, change a category
  and plan tasks into the calendar's hours.
- Month, Week, Work Week, 3 Days, Day and Agenda views.
- Home-screen widgets: a task list, the month, the agenda (its title optional), a day's hours
  and a month planner.
- Push through UnifiedPush (the ntfy app, for instance), without Google services.
- Reminders with "Mark as Done" and Snooze; "New Task" from the launcher; text shared as a task.
- Syncs with Nextcloud by itself, no DAVx⁵ needed; the Nextcloud app's accounts fill in the
  sign-in. Other CalDAV servers sign in with a login name and password.
- System Calendars: the phone's other calendars (Google, Exchange, DAVx⁵…), read-only; their
  name and colour can be changed in Todav.

**Languages**

- English and Polish; German, French, Spanish, Italian and Brazilian Portuguese as drafts.

## Planned

- A Plan view next to Tasks and Calendar: Today, the Week ahead, a Review of overdue and undated
  tasks, a board, estimates, dependencies, a focus timer, progress, and Notav's notes.
- Quick add in plain words ("tomorrow 17:00 #work"), and from anywhere with a system-wide
  shortcut.
- Task status (in progress, cancelled) and percent complete.
- Advanced notifications, configurable: repeating until done, per list.
- Advanced search, filtering and sorting: saved filters, tasks hidden until they start.
- Selecting several tasks to change them at once.
- Duplicating by Ctrl+drag in the calendar and among the tasks; templates.
- A Year view and folded night hours in the calendar.
- An event's own time zone; the weather in the calendar.
- A task's own colour, as events have.
- Attachments to tasks and events.
- Invitations: attendees, sending and answering.
- More choices for the GNOME extension's task list, and a place beside GNOME's calendar.
- Scripting through the running app.
- Nextcloud Deck boards as task lists.
- Settings per list and per calendar, such as how its completed tasks show.
- Export of several lists at once, and automatic backups.
- More of other CalDAV servers: their categories' order and settings synced, sign-in with OAuth.
- Google Tasks on Android, with a Google sign-in of its own.
- Google Play and F-Droid for the Android app; Flathub for the Linux one.
- Editing System Calendars' events on Android.
- Moving a whole list to another account.
- A Pebble app, through the Android app.
- A Windows app.
- A linked task's title and state in Notav's notes.

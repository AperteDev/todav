# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Android: a full Material 3 Expressive app. Tasks and Calendar switched by a navigation bar on
  phones and a rail on wider screens, whose menu button shows the lists and calendars: a page
  of its own on phones, a sidebar that can be hidden and resized on unfolded foldables and tablets; a task page with everything the Linux edit
  dialog has; swipe actions picked in Settings for each direction (complete, star, pin or
  delete with Undo); drag tasks to reorder
  them, make them subtasks or change their category; month, week and agenda views where events
  and tasks move by dragging; an event editor with repeating events; "To Plan" for putting tasks
  into hours of the week; search; settings and accounts; reminders; push through UnifiedPush;
  "New Task" from the launcher; adding shared text as a task; keyboard shortcuts.
- Android: home-screen widgets, read-only as the GNOME extension: a task list (or Upcoming,
  Important) by category with its subtasks, the month with a dot per calendar on each day, the
  coming week's agenda, a day's hours, and a month planner with bars across the days. A task or
  an event opens in its editor, a month's or day's title in the calendar. Each has an add button
  and settings (its list, or calendars other than those shown in the app; its add button), in
  the launcher's menu of the widget and in Settings → Widgets, which also sets the background's
  opacity, the corners' radius and a settings button on each widget.
- Android: a launcher icon of its own, the calendar page filling whatever shape the launcher gives
  icons, with a themed version; the app opens on it over the system's accent colour, light or
  dark with it.
- Android: About Todav in Settings has a Support section with the Ko-fi, GitHub Sponsors and
  Liberapay links, as About on Linux.
- Android: an account of the Nextcloud app on the device fills in the server and login name
  of a sign-in.
- Calendar: Work Week, 3 Days and Day views. Work Week shows the days picked in Preferences
  (Monday to Friday at first); 3 Days moves a day at a time; Day shows its
  week above the hours. Ctrl+4, Ctrl+5 and Ctrl+6 open them. On Linux the views are now picked
  from a menu.
- Calendar: an event's or a task's title can be changed right in its details popover: click it,
  type, press Enter.
- Lists and calendars can be put in any order by dragging them in the sidebar, within their
  account; with Nextcloud the order is shared with its Calendar and Tasks apps and other
  devices, lists on this device keep theirs here. On Android, hold a list, then drag it.
- Linux: the sidebar can be hidden with the button at the start of the header or F9; in a
  narrow window it opens over the content instead of in place of it.

### Changed

- Calendar: clicking a task in "To Plan" says how to plan it: drag it onto the calendar;
  right-clicking it offers Edit….
- Calendar: in the agenda, as in the GNOME extension, a task is a ring in its list's colour, with
  a tick when done, and an event a dot; a task is ticked in its details, not in the agenda.

### Fixed

- On a new installation, reminders never went off.
- The calendar no longer highlights the first day of a month or week moved to with the arrows
  as if it had been picked.
- Calendar: a task or event dragged over the events of a day lights the day up, as over its
  free space.

## [1.3.1] - 2026-10-26

### Added

- "Completing a task with open subtasks" in Preferences > Tasks: completing a task completes its
  open subtasks too (the default, as in Nextcloud Tasks; reopening a subtask then reopens its
  task), leaves them open, or asks each time.
- Reopening a task with completed subtasks asks whether they reopen too.
- "Edit titles in place" in Preferences > Tasks: a click on a task's title with a mouse or
  touchpad edits it right in the list. Off by default.
- When Todav is updated while it runs, it restarts into the new version by itself if its window
  is closed, and otherwise offers Restart in a banner.

### Fixed

- The open subtasks of a completed task stay under it among the open tasks instead of showing
  as top-level tasks.
- Undoing a change while a sync was uploading it no longer gets the change back from the server:
  a task reopened that way was completed again a moment later.

## [1.3.0] - 2026-10-26

### Added

- Several accounts side by side: more Nextcloud accounts (through GNOME Online Accounts or
  signed in in Todav, on the same server or others) and the lists kept on this device, each an
  account in Preferences → Accounts. Add one with Add; each has its own page with its name, its
  connection, Server Status and Remove. The sidebars and list pickers group lists by account,
  in the order set by dragging the accounts by their handles.
- An account can be switched off: its lists and calendars are hidden and stop syncing, nothing
  is deleted.
- "Sync settings with" in Preferences → Accounts picks the account settings sync through, or
  none. The welcome page asks whether to sync settings with the account added there.
- A banner over the tasks and the calendar names an account that is not connected (its password
  is gone from the keyring, or its GNOME Online Accounts account), with Sign In.

### Changed

- The lists on this device are an account of their own ("On This Device"), replacing the
  "Local lists and calendars" switch; hidden local lists stay hidden, as a switched-off account.
- An account without its password no longer shows the welcome page: its lists stay usable and
  the banner offers to sign in again.
- Signing in to a server offers its three ways as buttons (with the browser, by a copied
  login link, with a password) instead of a button with a menu; while the browser sign-in
  waits, only Cancel shows.
- Signing in again keeps the account: a different user is added as another account, and a GNOME
  Online Accounts account and a sign-in in Todav are separate accounts rather than one switch.

### Fixed

- A calendar new on the server, or every calendar of an account just added, gets DAV Push at
  the sync that finds it instead of up to an hour later.

## [1.2.2] - 2026-10-25

### Added

- About has a Support page with where to support Todav's development: Ko-fi, GitHub Sponsors
  and Liberapay.
- Server Status in Preferences → Accounts: the Nextcloud version (or why the server cannot
  be reached, or that it is in maintenance mode), when Todav last synced, and whether DAV
  Push works. Without DAV Push on the server, it says what that means and links to the app
  in the Nextcloud App Store; with it but no ntfy server, it asks for one.
- Extension: a settings button in the Extensions app, opening Todav's Preferences on the
  Extension page.
- An RPM spec for Fedora, with the extension as a separate package, built on COPR from
  `.copr/Makefile`.

### Changed

- Todav uses the system's SQLite library instead of a built-in copy; packages now depend on it.
- The app's store listing has screenshots, release notes and brand colours, and says the GNOME
  Shell extension is optional and installed separately.
- A task's subtask count ("1/4") counts only its own subtasks; deeper ones count on their
  subtask's row.
- Extension: with tasks and calendar side by side, the task list and the agenda take the
  height their column has to spare, so the fields for adding a task and an event line up.

### Fixed

- An account whose password is missing no longer reports "Sync failed: no account
  configured" in local mode; Preferences say it is not connected, and signing in again
  reconnects it.
- Extension: hovering over the menu no longer highlights all of it with themes that style
  hovered menu items, such as Blur my Shell's; only the row under the pointer is highlighted.

## [1.2.1] - 2026-10-25

### Changed

- The app ID is now `dev.aperte.Todav` and the extension's UUID `todav@aperte.dev`.
  Todav moved to [AperteDev](https://github.com/AperteDev/todav) on GitHub.

## [1.2.0] - 2026-10-23

### Added

- Use Todav without an account: "On This Device Only" on the welcome page keeps
  your task lists and calendars on this computer, starting with a task list and
  a calendar.
- "Local lists and calendars" in Preferences → Accounts: keep lists and
  calendars on this device only, next to the synced ones. Turning it off hides
  them; nothing is deleted. New lists and calendars ask where to go.
- The sidebars and the event dialog's calendar picker group lists and calendars
  by where they are kept, those on this device last.
- Move tasks to another list and events to another calendar by dropping them on
  it in the sidebar; a task takes its subtasks along.
- A list picker in the task dialog, to move a task to another list.
- A star on every task, coloured by its priority; its menu sets the priority.
  As in Nextcloud Tasks, a starred task is one of high priority.
- An Important page with the high-priority tasks of every list.
- "Priority, highest first" to sort tasks and subtasks.
- Pin tasks to the top of their list, in a Pinned group (shared with Nextcloud
  Tasks' pins).
- "Stars" and "Pins" in Preferences → Tasks turn either off; pinned tasks stay
  pinned for other apps.
- Nested subtasks, any number of levels deep: a subtask's "+" adds subtasks to
  it, and a task dropped on a "+" moves there with its own subtasks; dropped on
  an "Add subtask" field, it goes where a new subtask would. The task details
  on the calendar list a subtask's subtasks too.

### Changed

- The welcome page asks first how to use Todav (GNOME Online Accounts, a
  Nextcloud server or this device only), then for the account.
- The account, push and sync settings moved to a new Accounts page in
  Preferences.
- Times on the calendar are dimmer than titles and come after them, at the end
  of one-line entries, as in GNOME Calendar.
- Upcoming shows in the sidebar only while some task has a due date, as
  Important does while some task has high priority.

### Fixed

- A task dropped on the "Add a task" field no longer types its internal ID
  into it.

## [1.1.0] - 2026-10-22

### Fixed

- The window opens with the size it had when the app quit, and maximized if it was.

## [1.0.0] - 2026-10-22

### Added

- Tasks and events in your Nextcloud over CalDAV, kept in a local copy: the app
  works offline and syncs in the background, and push through WebDAV Push and
  ntfy brings changes from other devices within seconds.
- Sign in through GNOME Online Accounts, in the browser (Nextcloud Login Flow),
  or with a login name and app password; passwords stay in the keyring.
- Task lists: create, rename, recolour and delete them; categories as groups or
  tags with a sidebar filter; subtasks; manual order by drag and drop, shared
  with Nextcloud Tasks; sorting by due date.
- Tasks: due dates and start times, priority, notes, several reminders each as
  desktop notifications with "Mark as Done", and an Upcoming page across lists.
- A calendar with Month, Week and Agenda views: create, edit, move and delete
  events, repeating ones too (this occurrence, the following or all), with
  reminders. Weeks start on the day your region's formats say.
- Time-blocking: drag tasks from "To Plan" onto an hour or a day, move and
  resize blocks in the week, drop them back to unplan.
- Click an event or task on the calendar for its details, where an event's
  tasks and a task's subtasks can be ticked and added; double-click to edit it
  ("Edit on double click" in Preferences turns this off for instant details).
- Tasks linked to events, and today's events at the top of Upcoming.
- Search across tasks and events, undo with Ctrl+Z, keyboard shortcuts.
- Preferences synced between devices through Nextcloud Files.
- Runs in the background and starts with the session.
- Translations into Polish, German, French, Spanish, Italian and Brazilian
  Portuguese, for the app and the extension. Polish is reviewed; the others are
  first drafts.
- Packages for GNOME 49 and newer: a Flatpak bundle, a `.deb` for Ubuntu 26.04
  and newer, a tarball for other distributions (both with the extension), and
  the extension as a zip.
- A GNOME Shell extension: the open-task count in the top bar, and a menu with a
  task list, a month calendar with dots for what each day holds and the coming
  week's agenda; it adds tasks and events and opens the app where you click.
  Its options are in the app's Preferences. Without the app installed, its
  menu says so and links to the download.

[Unreleased]: https://github.com/AperteDev/todav/compare/v1.3.1...HEAD
[1.3.1]: https://github.com/AperteDev/todav/compare/v1.3.0...v1.3.1
[1.3.0]: https://github.com/AperteDev/todav/compare/v1.2.2...v1.3.0
[1.2.2]: https://github.com/AperteDev/todav/compare/v1.2.1...v1.2.2
[1.2.1]: https://github.com/AperteDev/todav/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/AperteDev/todav/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/AperteDev/todav/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/AperteDev/todav/releases/tag/v1.0.0

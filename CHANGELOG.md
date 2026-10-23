# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Use Todav without an account: "On This Device Only" on the welcome page keeps
  your task lists and calendars on this computer, starting with a task list and
  a calendar.
- "Local lists and calendars" in Preferences → Accounts: keep lists and
  calendars on this device only, next to the synced ones. Turning it off hides
  them; nothing is deleted. New lists and calendars ask where to go, and local
  ones carry a broken-chain mark.
- The sidebars and the event dialog's calendar picker group lists and calendars
  by where they are kept, those on this device last.

### Changed

- The welcome page asks first how to use Todav (GNOME Online Accounts, a
  Nextcloud server or this device only), then for the account.
- The account, push and sync settings moved to a new Accounts page in
  Preferences.

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

[Unreleased]: https://github.com/SelfRef/todav/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/SelfRef/todav/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/SelfRef/todav/releases/tag/v1.0.0

<p align="center">
  <img src="todav/linux/data/dev.aperte.Todav.svg" width="128" alt="Todav icon">
</p>

<h1 align="center">Todav</h1>

<p align="center">Tasks and calendar for GNOME, kept in your Nextcloud.</p>

<p align="center">
  <a href="https://github.com/AperteDev/todav/actions/workflows/ci.yml"><img src="https://github.com/AperteDev/todav/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

Todav is a task list and calendar app for the GNOME desktop. Your tasks and events stay in your
own Nextcloud as plain CalDAV, so the Nextcloud Tasks and Calendar web apps, DAVx⁵, Thunderbird
and other CalDAV clients keep seeing the same data. Todav adds a fast desktop app on top: it
works from a local copy, syncs in the background and picks up changes from other devices within
seconds.

Todav is developed in my free time. If it keeps your days in order, you can support its
development:

[![Support me on Ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/N5E229GJPZ)
[![Sponsor on GitHub](https://img.shields.io/badge/Sponsor-EA4AAA?style=for-the-badge&logo=githubsponsors&logoColor=white)](https://github.com/sponsors/SelfRef)
[![Donate using Liberapay](https://liberapay.com/assets/widgets/donate.svg)](https://liberapay.com/SelfRef/donate)

## Screenshots

| Tasks, grouped by category | The month, an event's details |
| --- | --- |
| ![A task list with categories, due dates and an expanded task's subtasks](docs/screenshots/tasks.png) | ![The month view with an event's details open](docs/screenshots/calendar-month.png) |
| **The week, a time-blocked task** | **Editing an event** |
| ![The week view with a planned task's details open](docs/screenshots/calendar-week.png) | ![The Edit Event dialog](docs/screenshots/event-dialog.png) |

<p align="center">
  <img src="docs/screenshots/extension.png" width="480"
    alt="The GNOME Shell extension's menu: a task list, the month with dots and the coming week's agenda">
  <br><em>The GNOME Shell extension</em>
</p>

## Features

**Tasks**

- Task lists from your Nextcloud calendars; create, rename, recolour and delete lists.
- Categories (CalDAV `CATEGORIES`), shown as groups or tags, with a category filter in the
  sidebar.
- Subtasks, nested to any depth, manual order by drag and drop (the same order Nextcloud Tasks uses), sorting by due
  date or priority, and notes.
- Priority on a star, coloured by its level and set from its menu, as Nextcloud Tasks stars
  tasks; an Important page with the high-priority tasks of every list.
- Pinned tasks stay at the top of their list.
- Move a task, with its subtasks, to another list: drop it on the list in the sidebar or pick
  the list in the task dialog.
- Due dates with optional times, start times, and several reminders per task, delivered as
  desktop notifications with "Mark as Done".
- An Upcoming page across all lists: Overdue, Today, Tomorrow, the coming days and later.
- Search across all lists (ignores accents) and undo with Ctrl+Z.

**Calendar**

- Month, Week and Agenda views of your Nextcloud calendars, with a sidebar month and show/hide per
  calendar.
- Create, edit, move and delete events, including recurring ones (this occurrence, this and the
  following, or all), with reminders.
- Drag and drop:
  - **Plan** a task from the "To Plan" list in the sidebar by dropping it on an hour of the week
    view (it gets a time slot) or on a day (it becomes due that day). A preview shows where it
    lands and the times, in 15-minute steps.
  - **Move** events and planned tasks to another hour or day; for a repeating event Todav asks
    whether to move this occurrence, the following ones or all of them.
  - **Resize** a block in the week view by dragging its top or bottom edge to change when it
    starts or ends.
  - **Unplan** a task by dropping it back on "To Plan".
  - **Change calendar** by dropping an event on another calendar in the sidebar (a task on
    another list).
- Click an event or task for its details (a task's subtasks, an event's tasks, both addable
  there); double-click to edit it.
- Tasks linked to events: an event lists its tasks, and a task shows its event.

**GNOME integration**

- Sign in through **GNOME Online Accounts**, in the browser (Nextcloud Login Flow) or with a login
  name and app password. Passwords stay in the system keyring.
- Runs in the background: closing the window hides it, and reminders and sync keep working.
  Starts with your session.
- A **GNOME Shell extension** with the open-task count in the top bar and a menu showing a task
  list, a month calendar and the coming week's agenda; it can add tasks and events and opens the app
  where you click.
- Follows the system style (light/dark), adaptive layout for narrow windows and touchscreens,
  keyboard shortcuts.

**Sync**

- Local-first: everything reads and writes a local database, so the app never waits for the
  network and keeps working offline; changes are sent when the server is reachable again.
- Push: with WebDAV Push and ntfy, changes made elsewhere show up within seconds. Without them,
  Todav checks every minute while its window is focused and every 30 minutes otherwise.
- Conflicts resolve per task, and a task ticked done on any device stays done.
- Properties Todav does not understand are kept as they are, so other clients' data survives.
- Your preferences sync between devices through `.config/todav/settings.json` in your Nextcloud
  Files (can be turned off; server addresses stay on each device). The order of each list's
  categories is kept next to it, in `todav/config.json`.

**On this device only**

- Use Todav without an account: choose "On This Device Only" when it starts, and your task lists
  and calendars stay on this computer.
- With an account too, turn on "Local lists and calendars" (Preferences → Accounts) to keep some
  lists and calendars off the server, next to the synced ones. Turning it off hides them without
  deleting anything.

## Getting started

Todav needs GNOME 49 or newer. The GNOME Shell extension is optional.

**Arch Linux and derivatives** (Manjaro, EndeavourOS, CachyOS…): install from the
[AUR](https://aur.archlinux.org/packages?K=todav) with an AUR helper:

```sh
yay -S todav gnome-shell-extension-todav
```

These build the latest release from source; `todav-bin` and `gnome-shell-extension-todav-bin`
install the prebuilt release instead, and `todav-git` and `gnome-shell-extension-todav-git` build
the latest commit.

**Other distributions:** download a package from the
[latest release](https://github.com/AperteDev/todav/releases/latest):

```sh
# Flatpak, on any distribution (the GNOME runtime comes from Flathub), plus the extension
flatpak install --user dev.aperte.Todav.flatpak
gnome-extensions install todav@aperte.dev.shell-extension.zip

# Ubuntu 26.04 and newer: the app with the extension
sudo apt install ./todav_<version>_amd64.deb

# Other distributions: the same files to unpack into /
sudo tar -xzf todav-<version>-x86_64.tar.gz -C /
```

Log out and back in, then turn the extension on in Extensions (or
`gnome-extensions enable todav@aperte.dev`). Or [build and install](#building-and-installing)
Todav from source. Then:

1. Open Todav and sign in to your Nextcloud: pick a GNOME Online Accounts account, sign in in the
   browser, or use a login name and app password. Or choose "On This Device Only" to start
   without an account.
2. Your task lists and calendars appear, and new ones can be created from the sidebar.
3. For instant updates, install the [DAV Push](https://apps.nextcloud.com/apps/dav_push) app on
   your Nextcloud and enter your ntfy server in Preferences → Accounts → Push. Server Status
   on the same page shows whether the server has DAV Push and whether it works.

## Roadmap

Todav is under active development. Planned:

- **Multiple accounts**
- **Advanced notifications**, configurable
- **Advanced search, filtering and sorting**
- **More services** besides Nextcloud
- **Android app**
- **Pebble app**, through the Android app
- **Windows app**

Ideas, questions and bug reports are welcome: open an
[issue](https://github.com/AperteDev/todav/issues). See [CONTRIBUTING.md](CONTRIBUTING.md) before
working on a pull request.

## Translations

Todav speaks English, Polish, German, French, Spanish, Italian and Brazilian Portuguese, and
follows your GNOME language. English and Polish are reviewed; the others are first drafts, so
corrections from native speakers are very welcome. Missing your language? Ask for it in an
[issue](https://github.com/AperteDev/todav/issues/new?template=translation.yml), or translate it
yourself (see [CONTRIBUTING.md](CONTRIBUTING.md#translations)).

## Development

### Tested setup

Todav is developed and tested mainly on this setup:

- **Nextcloud** with its built-in CalDAV server (the Tasks and Calendar apps are optional, but
  handy as web fallbacks).
- **GNOME** 49 to 51, with the app (GTK 4 + libadwaita) and the optional GNOME Shell extension.
- Optionally, for instant updates: the [DAV Push](https://github.com/bitfireAT/nc_ext_dav_push)
  Nextcloud app (`dav_push`) and an [ntfy](https://ntfy.sh) server.

Todav speaks standard CalDAV, but other servers and desktops are untested for now.

### Building and installing

**Dependencies:** Rust (stable), GTK 4.14+, libadwaita 1.8+, libsecret, SQLite, `blueprint-compiler`,
`glib-compile-resources` (GLib) and gettext's `msgfmt`.

```sh
# The app: binary, desktop file, icons, D-Bus activation and autostart in ~/.local
make -C todav/linux install

# The GNOME Shell extension (log out and back in, then enable it in Extensions)
make -C todav/gnome-extension install
```

A Flatpak manifest is included too: `make -C todav/linux flatpak` (needs `flatpak-builder`).
For Fedora, `todav/linux/rpm/todav.spec` builds the app and the extension as RPMs; COPR builds
it with `.copr/Makefile`, which also works locally:

```sh
make -f .copr/Makefile srpm outdir=$PWD/target/rpm spec=$PWD/todav/linux/rpm/todav.spec
```

Every push is checked and packaged by [CI](.github/workflows/ci.yml); releases are made by the
manual [Release](.github/workflows/release.yml) workflow and listed in the
[changelog](CHANGELOG.md).

## License

[GPL-3.0-or-later](LICENSE)

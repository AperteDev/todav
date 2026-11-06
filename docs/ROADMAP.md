# Todav — Roadmap

Planned work, not yet done. [FEATURES.md](FEATURES.md) lists it publicly; the order is not fixed.

## Planned features

- **Plan view**, a third view next to Tasks and Calendar for planning the day and the week (see
  [Plan view](#plan-view)). Tasks and Calendar stay as they are.
- **More of repeating tasks and events**: weekdays for a weekly repeat, the nth weekday or the
  last day for a monthly one, ending after n times (a `COUNT` is followed and kept now, not
  set), in the task and event dialogs alike (`calendar::dialog::repeat_rows`, `RepeatItems`);
  a repeating task's coming occurrences in the calendar and Upcoming (only its next one shows
  now). Open: whether a done occurrence leaves a done copy behind, under a UID of its own, for
  history and the Plan view's progress (now the task only moves on).
- **Quick add in plain words**: the date, time, repeat, list, category and priority read from
  the title as it is typed ("tomorrow 17:00", "every monday", `#category`, `!` for priority),
  shown as chips before adding and removable. In `todav-app`, so Linux, Android and the
  extension parse alike; the words are translatable per language. Open: what to do with a
  title that only looks like a date.
- **Quick add from anywhere**: a system-wide shortcut (the GlobalShortcuts portal, or a key
  binding in the extension) opening a small window for one task, with the list picked and quick
  add's parsing; the window does not show.
- **Task status and progress**: in progress and cancelled next to open and done (`STATUS`
  `IN-PROCESS`, `CANCELLED`), and percent complete (`PERCENT-COMPLETE`), shown on the task row and
  set in the task dialog; cancelled tasks show with done ones. The Plan view's board uses it.
- **Selecting several tasks** (Ctrl+click, Shift+click, a selection mode on touch and Android) to
  set their date, list, category or priority, tick or delete them at once, as one undo step
  (`Client::together`).
- **Duplicating** by Ctrl+drag onto a day or an hour of the calendar, or among the tasks (onto
  a list or a calendar in the sidebar, Duplicate in the dialogs and details exist);
  **templates**: a task with subtasks, or a list, kept to copy from.
- **More of the calendar**: a Year view, night hours folded away in the week views; week
  numbers in the planner widget too.
- **More of an event**: a time zone for its start and end; declined events of System Calendars
  on Android (the provider's own attendee status, not read yet); the weather of the coming days
  in the calendar's views (open: its source).
- **A task's own colour** (`COLOR` on a VTODO), as events have one: set with Own Color in the
  task dialog and page (`lists::swatches`, `ColorSwatches`, the event presets), shown on its
  chips and blocks in the calendar views and the widgets (`Item::color`). Few task apps set or
  show one (Nextcloud Tasks, Tasks.org, Thunderbird do not), so it is mostly Todav's own. Open:
  how a task row in a list shows it (a coloured mark, the check's tint), if at all.
- **Attachments** to tasks and events (`ATTACH`): links, and files uploaded to the account's
  Nextcloud Files; shown and opened from the task dialog and the event's details.
- **Invitations**: attendees on an event, invitations sent and answered through the server's
  scheduling (iTIP), replies shown per attendee. A large piece; after the rest.
- **GNOME extension**: the task list showing only some groups (overdue, today, the next 7 days,
  started), several lists merged; a place beside GNOME's
  calendar and events as well as in place of them. GNOME's designers propose moving
  notifications out of the clock menu into Quick Settings: follow it if it lands.
- **Scripting**: adding, listing and ticking tasks from a shell or other apps through the
  running app's D-Bus interface (`todav/cli` is a debugging tool with data of its own).
- **Nextcloud Deck** boards as task lists. Open: whether it is worth it next to the Plan view's
  board.
- **More of other CalDAV servers** (`AccountKind::CalDav` signs in with a login name and a
  password and checks for changes on a timer, in the Linux and Android apps): so far tried
  against Nextcloud's own CalDAV and iCloud (an app-specific password), so mailbox.org,
  Radicale, Baïkal and Fastmail are to be tried, each with what it needs. Open: where the
  categories' order and icons and the synced settings are kept for a server without Nextcloud
  Files (they stay on the device now; a file in a calendar-less collection, or a property of
  the calendar, if servers take them); sign-in with OAuth for servers that need it (Fastmail's,
  Microsoft's), as Google's; a push, where a server has one besides WebDAV-Push.
- **Google Tasks on Android** (Linux has them, `core/src/gtasks.rs`): Android has no Google
  account of its own yet, so it needs a sign-in first. Google gives a token for the `tasks`
  scope only to an app registered in a Google Cloud project: an OAuth client for the package
  and the SHA-1 of each signing key (release and debug), the Tasks and CalDAV APIs enabled, and
  Google's verification of the app for its sensitive scopes (until then at most 100 test users,
  behind a warning). Then AppAuth or Credential Manager for the token, the account added as
  `AccountKind::Google`, and the task page hiding what Google does not keep, as Linux does
  (`gtasks::is_list`). Open: whether to ask for the calendar scope too, or keep calendars to
  System Calendars.
- **Advanced notifications**, configurable, beyond the reminder notifications shown now
  (urgent ones for important tasks and events, or for all, and an event's Join Call exist):
  a reminder repeating until the task is done (every n minutes, n times), notifications on or
  off per list and per calendar (with the settings per list), a choice of snooze delays; the
  rest to be specified.
  Open: on Android an urgent reminder that stays on screen needs a full-screen intent
  (`USE_FULL_SCREEN_INTENT`), which Android 14+ and Google Play keep for alarm and calling apps.
- **Advanced search, filtering and sorting**: saved filters (smart lists) combining lists,
  categories, dates, priority and status, in the sidebar, the extension and the widgets; tasks
  hidden until their start date, by a setting; the rest to be specified.
- **Settings per list and per calendar**, overriding Preferences for that one: for a task list
  how completed tasks show (`done_tasks`, `done_subtasks`), its sort and where new tasks go,
  completing a task with open subtasks, the default duration and reminders; for a calendar the
  default event duration and reminders. Each option "As in Preferences" until changed, in the
  list's or calendar's edit dialog on Linux and Android. `prefs::order`, `done_view` and the
  like would take the list; Upcoming, Important and search, across lists, keep the defaults.
  Open questions: stored next to the categories' order in the account's `todav/config.json`
  (with the list, for every device on the account; lists on this device only in `kv`) or in the
  synced settings keyed by list; which options are worth it per list; how the GNOME extension
  and the widgets, which show one list, follow them.
- **More of export** (one list or calendar to an `.ics` file exists, `Client::export_ics`):
  several or all at once (open: one file or one per list), then automatic backups: all of it
  exported daily to a folder, the last n kept, restored by importing.
- **Pebble app**, fed by the Android app (see [Pebble](#pebble-watchapp--companion)).
- **Windows app**: a native WinUI 3 front-end (see [Windows](#windows-app)).
- iOS and macOS, later: SwiftUI over UniFFI's Swift bindings from `todav-ffi`.

Smaller items:

- Move a whole list between accounts (single events move with the calendar picker, tasks by
  dropping them on a list).
- Key tasks by list and UID, as events are: a task in a calendar two accounts share shows once
  now, in the account that had it first (`store::task_elsewhere`).
- Show the conflict log (`kv:conflict_log`) in the app's settings.
- Keys for the first nine lists (Ctrl+1 to Ctrl+6 pick the calendar's view there; which keys is
  open).
- Check memory use over a long run (the window open, many syncs and pushes) for widgets kept
  alive by their signal handlers' closures; it stayed near 48 MB in the background so far.
- Google Tasks keeps no UID: a task made here keeps its own, on other devices it has Google's
  id, so a `todav://task/` link to it opens only where it was made. Taking Google's id after
  the first upload would mean renaming the UID everywhere it is used (subtasks, alarms, undo).
- Google's copy of a task is as new as its upload, not its edit: an edit uploaded late (made
  offline) beats a newer one made meanwhile on another device and uploaded after it.

## Plan view

A third view on Linux (next to Tasks and Calendar in the view switcher) and Android (a third
destination in the bar and the rail), for deciding what to do when. Tasks stays the place for
keeping lists, Calendar the one for time; the calendar's "To Plan" pane stays as it is. Plan
reads and writes the same tasks and events, so all it does syncs as usual.

- **Today**: overdue tasks, tasks due or starting today and the ones planned for today, next to
  the day's events and planned tasks in time order; from there a task is ticked, moved to
  tomorrow or a day picked, or dropped on a free hour to plan it.
- **Week**: the coming days side by side, each with its events, its tasks and how much of its
  free time their estimates fill; dragging a task to another day moves its due date, onto
  "Later" clears it.
- **Review**: overdue tasks and tasks without a date, one at a time, to move, plan, tick or
  delete, with keys for each; for starting or ending the day.
- **Board**: the tasks of a list, a category or a saved filter in columns by status, category or
  priority; dragging a task to another column changes it (status from Task status and progress).
- **Estimates**: how long a task takes, set in the task dialog, filling the Week's days and used
  as the length of its slot when planned (instead of `task_duration`). Open: where it is stored
  (no standard VTODO property; `DURATION` means the span from its start).
- **Dependencies**: a task waiting for others (`RELATED-TO;RELTYPE=DEPENDS-ON`, RFC 9253), shown
  as blocked until they are done; Today, Week and the board put blocked tasks aside.
- **Focus**: a timer on a task (work and break intervals, a notification at each end), Today's
  task at hand shown in the extension while it runs. Open: whether the time spent is kept on the
  task, and where.
- **Progress**: tasks done per day and per week against a goal one sets, and the past weeks at a
  glance. Needs the completion times (`COMPLETED`) of done tasks, and of repeating tasks' done
  occurrences.
- **Notav**: its notes and journal next to the plan, above all notes; details to be specified
  (see [Notav integration](#notav-integration)).

Open questions: picking a task for today without changing its due date needs a property other
clients ignore (or a category); whether the extension and the widgets get a Today of their own;
on a phone, whether Week shows as a list of days.

## Android

- Release on F-Droid and Google Play (`USE_EXACT_ALARM` is allowed for a calendar app), with the
  release key the GitHub APK is signed with, so all three update one another:
  - F-Droid: the recipe (`metadata/dev.aperte.todav.yml`, branch `dev.aperte.todav` of the
    user's `fdroiddata` fork) builds and matches our signed 2.0.2 in F-Droid's CI. The merge
    request was closed on 2026-10-28 under the inclusion policy's Quality point 6: at least 3
    months of active development (first commit 2026-10-08). Reopen it from about 2027-01-08,
    with the recipe's build updated to the latest release (version, code, full commit hash).
  - Google Play, as an organisation account (D-U-N-S number requested): `bundleRelease` signed
    in the Release workflow, the key for Play App Signing uploaded with PEPK, the Data safety
    and exact-alarm forms, a privacy policy page.
- Verify push delivery through a real UnifiedPush distributor (ntfy) in the background.
- Move a task to another list by dragging it onto the sidebar (Move to List in its menu does it
  now).
- Drop the material3 pin in `gradle/libs.versions.toml` once the Compose BOM ships 1.5.
- System calendars (`SystemCalendars.kt`, read-only now): editing their events, written back to
  Android's calendar store (whole events with their repeat rules, not the occurrences copied
  now); occurrences outside two years back and three ahead, read when browsed there. Test with
  a Google account on a real phone (the emulator's image has no Google Play services).

## Distribution

- **Flathub**: a manifest building offline from the release tag, with a generated
  `cargo-sources.json`, in its own `flathub/dev.aperte.Todav` repository. Written, opened and
  answered by hand only: Flathub's generative AI policy bans AI-generated or AI-assisted
  content in the manifest, and AI-opened or AI-written PRs, commit messages and replies; the
  app's AI-generated code, docs and translations must be disclosed with their extent (Claude),
  and reviewers may reject on it. Blockers as of 2026-10-28:
  - Project age: the first commit is from 2026-10-08, 1.0.0 from 2026-10-22; Flathub wants a
    meaningful history and real-world use first.
  - Translations: Flathub accepts no machine-generated translations. Only Polish is reviewed;
    de, es, fr, it and pt_BR are drafts and need a review or have to stay out of that build.
  - GNOME Online Accounts has no portal, so its `--talk-name` needs a justification. (The
    keyring needs none: inside Flatpak libsecret keeps passwords through the Secret portal.)
  - The repository's manifest fetches crates over the network (`--share=network`), which
    Flathub's offline build does not allow.
  The metainfo validates; `https://aperte.dev` answers, as the ID's domain check needs, and
  the verification token goes to `/.well-known/org.flathub.VerifiedApps.txt` after acceptance.
- **extensions.gnome.org**: reviews reject code with clear signs of AI generation; the extension
  must be one its author can explain line by line.
- **Fedora's own repositories** (COPR works now) would want Fedora's crates instead of vendored
  ones: all are packaged at our versions except `libsecret` (not packaged) and `rusqlite`
  (Fedora has 0.38, which the core accepts).
- **README badges** (shields.io), added as each source exists: the Flathub and F-Droid versions
  (`flathub/v/dev.aperte.Todav`, `f-droid/v/dev.aperte.todav`; both say "not found" until we are
  listed) and Repology (`repology/v/todav`, once a repository other than ours packages it).
- **Microsoft Store** and winget for the Windows app; check the Store's rules on AI-assisted code
  first, as for Flathub.

## Windows app

What the Linux app does, the Windows way, in C# + WinUI 3 (Windows App SDK) over `todav-ffi`'s
C# bindings (`uniffi-bindgen-cs`):

- One resident process (single instance by `AppInstance.FindOrRegisterForKey`) with a tray icon,
  the window hidden on close; the ntfy listener and the safety-net syncs from `todav-app`'s
  `Service`.
- Reminders as toasts (`AppNotificationManager`) with "Mark as Done"; start with the session (an
  MSIX `StartupTask`); app passwords in the credential locker (`PasswordVault`); the system zone
  for `Client::set_local_zone` from `TimeZoneInfo.TryConvertWindowsIdToIanaId`; data in the
  app's local folder.
- A tray flyout with a list and the agenda, the GNOME extension's counterpart. WinUI 3 has no
  tray icon of its own: a small library (H.NotifyIcon) or the Win32 calls through CsWin32, to be
  chosen.
- The number on the app's icon (`launcher_count`, as on Linux) as the taskbar button's overlay
  badge (`ITaskbarList3::SetOverlayIcon`, with the number drawn into an icon), from
  `todav_app::service::launcher_count`.
- Translations through `Translations` from `todav-ffi`, as on Android; `scripts/i18n.sh` gets
  the C# sources.
- Builds only on Windows: `cargo build --target x86_64-pc-windows-msvc` (and `aarch64`) in
  `todav/ffi`, a Windows machine or VM for development, a `windows-latest` runner in CI.

## Pebble watchapp + companion

The watch is a remote control for the phone's local database: it never talks to the network and
never holds the source of truth. The Android app is the companion and pushes list snapshots over
PebbleKit `AppMessage`.

**Watchapp** (C, the open-source Pebble SDK, pinned in `pebble/README.md`)

- Screens: list picker (`MenuLayer`) → task list grouped by category (section headers are
  categories) → select toggles done. Up/down long-press jumps between sections.
- Keeps the last snapshot in watch storage (`persist_write_data`), so the list opens at once and
  works with the phone out of range; changes made offline are queued and sent when connected.
- Aplite, Basalt, Chalk, Diorite, Emery and the Core Devices models; monochrome-safe (no colour
  as the only signal).

**AppMessage protocol** (keys in `package.json`; strings UTF-8, at most 60 bytes, longer
summaries cut with `…` on the phone)

| Key | Direction | Payload |
| --- | --- | --- |
| `LISTS_BEGIN` | phone → watch | count |
| `LIST_ITEM` | phone → watch | index, short id (uint16), name, open count |
| `TASKS_BEGIN` | phone → watch | list short id, count, snapshot version |
| `TASK_ITEM` | phone → watch | index, short id (uint16), category index, done (0/1), summary |
| `CATEGORY_ITEM` | phone → watch | index, name |
| `END` | phone → watch | snapshot version |
| `REQUEST_LISTS` | watch → phone | — |
| `REQUEST_TASKS` | watch → phone | list short id |
| `SET_DONE` | watch → phone | task short id, done |
| `ACK_DONE` | phone → watch | task short id, snapshot version |

- The phone keeps a short id ↔ UID map per snapshot (no 36-byte UIDs on the wire). A snapshot
  is at most 60 tasks; beyond that, the first 60 open tasks and a trailing "+n more" item.
- One `AppMessage` at a time, paced by acks (`app_message_outbox_sent`); resent on NACK up to 3
  times.
- On `SET_DONE` the phone calls `set_done`, replies `ACK_DONE` and syncs. On a change to the
  shown list, it pushes a fresh snapshot if the watch is connected.

**Companion** (Android, a `pebble` package): `PebbleKit.registerReceivedDataHandler` for the
UUID, a `PebbleBridge` serialising snapshots from the core, started when the watchapp sends
`REQUEST_LISTS`. PebbleKit as a vendored AAR. No watch-side push: the phone's push → sync →
snapshot path updates the watch within seconds while the app is in front, otherwise on the
watch's next request.

## Notav integration

[Notav](https://github.com/AperteDev/notav) is a sibling app for notes and a journal, with its own
repository. The two keep separate configuration and credentials. What Todav provides for it:

- A note links a task as `todav://task/<uid>`. Todav handles the `todav` URI scheme
  (`x-scheme-handler/todav` in its desktop file) and opens the task as `ShowTask` does.
- Notav shows a linked task's title and state through a new D-Bus method, `GetTask` (UID in;
  summary, done and list out).
- Once Notav uses them, the scheme and the method are a public interface: change them only in
  compatible ways.

## Open questions and risks

- **dav\_push stability**: the app-store build may lag GitHub; pin a git commit on the server.
  Fallback if it misbehaves: a small Nextcloud app that listens to
  `CalendarObject{Created,Updated,Deleted}Event` and posts the topic to ntfy directly (the
  core's listener stays the same).
- **ntfy topic security**: topics must be random (22+ characters) and the ntfy server should have
  ACLs so only the owner can subscribe; the app would authenticate with an ntfy token kept in
  the keyring.
- **Pebble SDK toolchain**: confirm the open-source SDK builds for the user's watch and that the
  Core Devices Pebble app forwards PebbleKit intents; otherwise test the companion with the
  legacy Pebble app.

## Ruled out

- **Signing in through the Nextcloud Android app** (Single Sign-On; tried and dropped
  2026-10-27): it forwards no REPORT, MKCALENDAR or PROPPATCH, sends PROPFIND at depth 1 only and
  appends "?" to every URL, which breaks DAV Push registration. Worth another look only if it
  forwards any method and the URL as given.
- **Honor's liquid glass widget background** (looked into 2026-10-28, MagicOS 11): the launcher
  draws it behind widgets whose receiver has `com.hihonor.android.launcher.widget_type`
  `cardstyle` and `card_background` `blur`, but only for the packages in its built-in
  `widgetcard_whitelist` (Honor's own apps and the system UI). Another app's widget stays a
  plain one, and a widget can't blur what is behind it itself.
- **The number on the app's icon on Android** (looked into 2026-11-06): there is no API to set a
  launcher's badge count. Since Android 8 the launcher draws a notification dot for an app with
  a notification, and `Notification.Builder.setNumber` only shows a number in the long-press
  popup; a number on the icon itself comes from vendor broadcasts (Samsung, older Sony and HTC),
  which break between versions. Worth another look only if Android adds a public API.
- **A cross-platform UI toolkit** (Compose Multiplatform, Flutter, Slint): a native UI per
  platform over the shared Rust instead.

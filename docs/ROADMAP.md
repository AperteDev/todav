# Todav — Roadmap

Planned work, not yet done. The README lists it publicly; the order is not fixed.

## Planned features

- **More services** besides Nextcloud: other CalDAV servers first (Radicale, Baïkal, Fastmail,
  iCloud…), which mostly means login without Login Flow v2, settings and categories without
  Nextcloud Files, and polling where there is no WebDAV-Push. Each would be another
  `AccountKind`.
- **Google Tasks** in the Google account (events only now): Google keeps tasks out of CalDAV, so
  they come over the Tasks REST API (`tasks.googleapis.com`), polled. The token GNOME Online
  Accounts gives already has the `tasks` scope; Android has no Google account of its own yet,
  so it needs its own OAuth sign-in first. The API knows title, notes, done, a due date
  without time, subtasks and order only: no priority, categories, reminders or repeats, so the
  app hides or disables those for Google lists.
- **Advanced notifications**, configurable, beyond the reminder notifications shown now; details
  to be specified.
- **Advanced search, filtering and sorting**; details to be specified.
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
- **Export**, the counterpart of the iCalendar import (`core/src/import.rs`): lists and
  calendars (one, some or all) to an `.ics` file, on Linux and Android; details to be
  specified (one file or one per list, done tasks, a date range for events).
- **Pebble app**, fed by the Android app (see [Pebble](#pebble-watchapp--companion)).
- **Windows app**: a native WinUI 3 front-end (see [Windows](#windows-app)).
- iOS and macOS, later: SwiftUI over UniFFI's Swift bindings from `todav-ffi`.

Smaller items:

- Move a whole list between accounts (single events move with the calendar picker, tasks by
  dropping them on a list).
- Key tasks by list and UID, as events are: a task in a calendar two accounts share shows once
  now, in the account that had it first (`store::task_elsewhere`).
- Show the conflict log (`kv:conflict_log`) in the app's settings.

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
- **A cross-platform UI toolkit** (Compose Multiplatform, Flutter, Slint): a native UI per
  platform over the shared Rust instead.

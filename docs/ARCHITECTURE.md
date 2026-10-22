# Todav — Architecture and Plan

Oct 8, 2026 · @Kris · updated Oct 22, 2026

## Overview

Todav is a personal, local-first task list synced through Nextcloud CalDAV (VTODO) with near-instant push via WebDAV-Push (`dav_push`) and a self-hosted ntfy server. One Rust core drives four front-ends: a GTK4/libadwaita app on Linux, a GNOME Shell extension for quick preview, an Android app, and a Pebble watchapp fed by the Android app.

Todav is meant to become a tasks + calendar app (VEVENT next to VTODO, same CalDAV calendars). Notes and journal go in a sibling app, [Notav](#notav-notes--journal-planned), in the same repository.

**Goals**

- Same lists, same behaviour, same categories on every device; changes visible on other devices within seconds.
- Local-first: every UI reads and writes SQLite; the network is never on the UI path.
- Nextcloud Tasks web UI and DAVx5/Tasks.org keep working as fallbacks (no proprietary storage).
- Minimal dependencies: standard library and platform SDKs first; a third-party crate or library only where it replaces real work (HTTP/TLS, SQLite, XML, UniFFI).

**Non-goals (v1)**

- Multi-user sharing, assignees, comments.
- Recurring tasks, attachments.
- iOS, web front-end.

**Principles for the agent**

- The Rust core is the only place with sync, push, parsing and conflict logic. Front-ends are thin: they render state and call commands.
- Every feature lands in the core with tests against a real Nextcloud (the Test account, see [Progress](#progress)) before any UI uses it.
- Push is best-effort by spec; polling with `sync-token` stays as the safety net everywhere.
- When in doubt, do the simpler thing and leave a `TODO(v2)` comment rather than a configuration option.

## Architecture

Nextcloud is the only source of truth and both apps talk to it over plain CalDAV; ntfy only carries "something changed" signals, after which the app runs a sync-token pull. The GNOME extension and the Pebble watch hold no data of their own and go through their host app.

**Sync + push cycle (same on both apps)**

1. User action → core writes SQLite, marks the task dirty, emits `Changed` → UI updates at once.
2. Core pushes dirty tasks (`PUT` with `If-Match`), then pulls each list with `REPORT sync-collection`.
3. Nextcloud's `dav_push` sends a Web Push to the registered ntfy topic; ntfy fans it out: WebSocket to the Linux app, UnifiedPush to the Android app.
4. The receiving app maps the push topic to a calendar and runs step 2 for that calendar only.
5. A periodic sync (every 30 min, or 60 s while a window is focused) covers lost pushes.

## Data model

Storage is plain CalDAV: one calendar per list, one VTODO per task. Categories are `CATEGORIES` tags, not parent tasks. Subtasks use `RELATED-TO`.

| Concept | Where it lives | Notes |
| --- | --- | --- |
| List (Shopping, Personal, Work) | CalDAV calendar collection | Only calendars that advertise `VTODO` in supported-component-set are shown |
| Task | `VTODO` resource (`<uid>.ics`) | `UID`, `SUMMARY`, `STATUS`, `COMPLETED`, `PRIORITY`, `DUE`, `DESCRIPTION` |
| Category (Groceries, Home, Electronics, Mountains, Car) | `CATEGORIES` on the VTODO | Exactly one category per task in v1; multiple are read but the first is used for grouping |
| Subtask | `RELATED-TO;RELTYPE=PARENT:<parent uid>` | Depth 1 only in UI; deeper nesting is read and preserved but flattened |
| Manual order | `X-APPLE-SORT-ORDER` (integer), shared with Nextcloud Tasks | Fallback: `CREATED` timestamp; the old `X-TODAV-ORDER` is read and dropped on the next reorder |
| Event | `VEVENT` resource in a calendar that supports it | See [Calendar](#calendar-in-todav) |
| Category metadata (order, icon) | `todav/config.json` in Nextcloud Files via WebDAV | One file, fetched with `If-None-Match` on sync |
| Client settings | `.config/todav/settings.json` in Nextcloud Files | Merged per key on every sync, newer wins; device-only keys stay local |
| Server change cursor | `sync-token` per calendar | Stored locally; `sync-collection` REPORT (RFC 6578) on every sync |
| Push identity | `DAV:Push` `topic` per calendar | Maps incoming push to a calendar |

**Local schema**: SQLite in the core (`todav/core/src/store.rs`, migrations by `PRAGMA user_version`): `lists`, `tasks`, `events` (raw iCalendar plus derived columns, a dirty flag for the push queue), `alarms` (reminders in local time) and `kv` (config/settings cache, push keys, setting change times).

`raw_ics` keeps the full original VTODO so properties the app does not understand survive a round trip. On write, the core patches the known properties in `raw_ics` and bumps `DTSTAMP`/`LAST-MODIFIED`/`SEQUENCE`.

**config.json** (category metadata only; everything else lives in VTODO)

```json
{
  "version": 1,
  "lists": {
    "shopping": { "categories": [
      { "name": "Groceries", "icon": "basket", "order": 0 },
      { "name": "Home", "icon": "house", "order": 1 }
    ] }
  }
}
```

Tasks whose category is not in config.json are grouped under "Other" at the end, so a missing or stale config never hides data.

## Repository layout

One monorepo, one Cargo workspace at the root. Each app has its own top-level directory with its core and front-ends; `notav/` (see [Notav](#notav-notes--journal-planned)) will sit next to `todav/`. Code both apps need moves into a shared crate at the root when Notav starts, not before.

```text
<repo>/
  Cargo.toml                 # workspace: todav/core, todav/cli, todav/linux/app
  todav/
    core/                    # todav-core (library), flat modules in src/
      src/
        lib.rs               # public API (Client), change events
        caldav.rs            # PROPFIND/REPORT/PUT/DELETE, MKCALENDAR, XML tree + multistatus
        ical.rs              # VTODO/VEVENT reader/writer, unknown properties kept
        rrule.rs, tz.rs      # recurrence expansion, VTIMEZONE offsets
        event.rs             # events, occurrences, event reminders
        store.rs             # SQLite schema, migrations, queries
        sync.rs              # engine: pull (sync-token), push (dirty queue), conflicts
        push.rs              # WebDAV-Push registration, aes128gcm decryption, ntfy listener
        config.rs            # todav/config.json (categories)
        settings.rs          # synced client settings
        login.rs             # Login Flow v2, app password sign-in
        undo.rs              # undo history of local changes
        json.rs              # minimal JSON (no serde)
    cli/                     # todav-cli: thin debugging/scripting front-end
    linux/
      app/                   # GTK4 + libadwaita app (Rust, gtk4-rs); Blueprint views in ui/
      data/                  # .desktop, metainfo, icons, D-Bus service
      flatpak/               # io.github.selfref.Todav.json
    gnome-extension/         # GNOME Shell extension (GJS), talks D-Bus to the Linux app
    android/                 # planned: app (Compose), core (UniFFI module), pebble companion
    pebble/                  # planned: Pebble watchapp (C, Pebble SDK)
  notav/                     # Notav (planned, not started)
```

**Tooling**

- Rust stable; `cargo clippy -D warnings`, `cargo fmt`, `cargo test` (`--features integration` for the tests against the server).
- Linux: `make -C todav/linux install` (per-user) or `make -C todav/linux flatpak`; the build needs `blueprint-compiler` and `glib-compile-resources`. Flathub will need a generated `cargo-sources.json` for an offline build.
- Android (planned): Gradle, Kotlin, `cargo-ndk` to build `.so` for arm64-v8a (and x86\_64 for the emulator); UniFFI bindgen in a Gradle task.
- Pebble (planned): the open-source Pebble SDK toolchain (`pebble build`), pinned version in `pebble/README.md`.
- CI (GitHub Actions, `.github/workflows/`): fmt, clippy, unit tests, the Flatpak bundle and the extension zip on every push; releases by the manual Release workflow (see [Progress](#progress)). Later the Android debug build and Pebble build. No signing in CI.

**Dependency policy** (keep it short)

| Need | Crate / library | Why not std |
| --- | --- | --- |
| HTTP + TLS | `ureq` (sync, rustls) | No async runtime needed; keeps the core small |
| SQLite | `rusqlite` (bundled) | Standard choice, single file |
| XML | `quick-xml` | PROPFIND/REPORT bodies and multistatus responses |
| FFI | `uniffi` | Kotlin bindings generated from the Rust API |
| ntfy stream (Linux push) | — (`ureq`) | ntfy's `/<topic>/json` NDJSON stream over plain HTTP; no WebSocket crate needed |
| Web Push decryption | `ring` (HKDF, AES-128-GCM, RNG; already pulled in by rustls) + `p256` (static-key ECDH) | `dav_push` always encrypts (RFC 8291); `ece` drags in OpenSSL, ring can't load a stored ECDH key |
| iCalendar | hand-written in `todav/core/src/ical.rs` | Full iCal crates are large; VTODO needs \~15 properties, line folding and escaping |
| Linux UI | `gtk4`, `libadwaita` | The platform toolkit; D-Bus, GOA and notifications go through `gio` from the same bindings |
| Linux secrets | `libsecret` | App passwords in the Secret Service |
| Locale (gettext, first weekday) | `libc` (already in the tree) + glibc's own gettext | `setlocale`, `nl_langinfo`; the five gettext calls are declared in `i18n.rs`, no `gettext-rs` |

Anything beyond this table needs a one-line justification in the PR.

## Rust core (`todav-core`)

The core owns all state and all network I/O. Front-ends get a `Client` handle, call commands, and subscribe to change events. The API is synchronous and blocking; callers run it on a background thread (GTK: `gio::spawn_blocking`, Android: coroutine on `Dispatchers.IO`).

**Public API (UniFFI surface)**: the first sketch below; the real API in `todav/core/src/lib.rs` has grown since (events, lists and calendars, settings, login, undo).

```rust
pub struct Client { /* SQLite conn, http agent, account */ }

impl Client {
    pub fn open(data_dir: String) -> Result<Client>;
    pub fn set_account(&self, url: String, user: String, app_password: String) -> Result<()>;
    pub fn account(&self) -> Option<Account>;

    // Read (never touches the network)
    pub fn lists(&self) -> Vec<List>;
    pub fn tasks(&self, list_href: String, include_done: bool) -> Vec<Task>;
    pub fn grouped(&self, list_href: String) -> Vec<CategoryGroup>;  // ordered by config.json

    // Write (local, marks dirty, emits Changed; sync pushes later)
    pub fn add_task(&self, list_href: String, summary: String, category: Option<String>, parent_uid: Option<String>) -> Result<Task>;
    pub fn set_done(&self, uid: String, done: bool) -> Result<()>;
    pub fn update_task(&self, uid: String, patch: TaskPatch) -> Result<()>;
    pub fn delete_task(&self, uid: String) -> Result<()>;
    pub fn reorder(&self, uid: String, before_uid: Option<String>) -> Result<()>;

    // Sync
    pub fn sync(&self) -> Result<SyncReport>;            // push dirty, then pull every list
    pub fn sync_list(&self, list_href: String) -> Result<SyncReport>;
    pub fn sync_topic(&self, topic: String) -> Result<SyncReport>;  // from a push message

    // Push
    pub fn push_register(&self, push_resource: String, expires_in_secs: u64) -> Result<Vec<PushRegistration>>;
    pub fn push_unregister(&self) -> Result<()>;
    pub fn push_decode(&self, body: Vec<u8>) -> Result<Vec<String>>;  // returns topics

    pub fn subscribe(&self, cb: Box<dyn ChangeListener>);  // Changed { list_href, uids }
}
```

**Modules**

1. `caldav` — discovery (`current-user-principal` → `calendar-home-set` → calendars with `VTODO`), `PROPFIND` for `getctag`, `sync-token`, `DAV:Push` `push-transports` and `topic`; `REPORT sync-collection`; `calendar-multiget` for changed hrefs; `PUT` with `If-Match`/`If-None-Match: *`; `DELETE` with `If-Match`.
2. `ical` — parse/serialize VCALENDAR with one VTODO. Handles line folding (75 octets), escaping (`\,` `\;` `\n`), `TZID` and `VALUE=DATE` on `DUE`, `RELATED-TO` with `RELTYPE`, multi-value `CATEGORIES`, unknown properties preserved verbatim. Property-tested against ics files exported from Nextcloud Tasks, Tasks.org and Errands.
3. `store` — rusqlite, migrations by `PRAGMA user_version`, one write transaction per command.
4. `sync` — the engine below.
5. `push` — WebDAV-Push registration per calendar; on Linux also the ntfy listener (`push::listen`, the topic's JSON stream over HTTP); on Android the app receives the message via UnifiedPush and hands bytes to `push_decode`.
6. `config` — fetch `todav/config.json` with `If-None-Match`, cache in `kv`, create it with defaults if 404.

**Sync engine**

1. Push phase: for each `dirty=1` task → `PUT` with `If-Match: <etag>` (or `If-None-Match: *` for new). `412` → fetch the server copy, run conflict rule, retry once. For `dirty=2` → `DELETE`; `404` counts as success.
2. Pull phase per list: `REPORT sync-collection` with the stored `sync-token`. Empty/invalid token (`403 valid-sync-token`) → full `calendar-query` for VTODO and rebuild. For each changed href → `calendar-multiget`, parse, upsert by UID; `404` entries → delete locally (unless `dirty=1`, then keep and re-push as new).
3. Conflict rule: compare `LAST-MODIFIED`; newer wins for the whole VTODO, except `STATUS=COMPLETED` always wins over an older `NEEDS-ACTION` (a tick on any device sticks). Log every conflict to `kv:conflict_log`.
4. Emit one `Changed` event per list with the affected UIDs.
5. `sync_topic(topic)` looks up the list by `push_topic` and runs only step 2 for it; unknown topic → full `sync()`.

**Push registration**

- After discovery, `PROPFIND` each calendar for `DAV:Push` `push-transports` and `topic`. If `web-push` is listed, `POST <calendar href>` with:

```xml
<?xml version="1.0" encoding="utf-8"?>
<push-register xmlns="DAV:Push">
  <expires>Thu, 09 Oct 2026 10:00:00 GMT</expires>
  <subscription>
    <web-push-subscription>
      <push-resource>https://ntfy.example.org/nmr-8f3a...?up=1</push-resource>
      <!-- add client-public-key / auth-secret here if dav_push requires encryption -->
    </web-push-subscription>
  </subscription>
</push-register>
```

- Store the `Location` header as `push_registration_href` and `push_expires`. Re-register when less than 25% of the lifetime is left or when the push resource changes (new UnifiedPush endpoint, new ntfy topic). `DELETE` old registrations.
- Polling fallback stays on: every 30 min in the background, every 60 s while a window is focused; a successful push resets the timer.
- `push_decode` accepts raw body bytes; if the first byte is not `{`/`<` and an `aes128gcm` content-encoding was indicated, decrypt with the stored keys (`ring` + `p256`). Output is the list of topics (one in practice).

**Testing**

- Unit: `ical` round-trips, sync-engine state machine with a fake server trait.
- Integration: against the real instance (see [Progress](#progress)); each test makes or reuses its own `it-<name>` calendar, writes through a second client, asserts the core sees it after `sync()`, then after a push message. Run with `cargo test -p todav-core --features integration`.

**CLI** (`todav`): `lists`, `ls`, `add`, `done`, `undo`, `rm`, `sync`, `push-register`, `listen` (ntfy loop → sync); the account comes from `.env`. Used to validate the core before any GUI exists and as a debugging aid later.

## Linux app (GTK4 + libadwaita)

A single Rust binary (`todav-linux`) that is both the GUI and a background service: it keeps the ntfy connection open, runs sync, and exposes a small D-Bus interface the GNOME extension uses. There is no separate daemon.

**Stack**: `gtk4-rs`, `libadwaita-rs`, `libsecret`; D-Bus, GNOME Online Accounts and notifications through `gio`. Static views are Blueprint files in `ui/`. No GSettings: settings live in the core (`Client::setting`, synced). The app password in the Secret Service; a GOA account's password stays in GOA. State in `$XDG_DATA_HOME/todav/core.sqlite` via the core.

**UI**

- Welcome page to sign in: GNOME Online Accounts, browser (Login Flow v2) or login name + app password; optional ntfy server.
- Tasks | Calendar switcher in the header. Tasks page: `AdwNavigationSplitView` with Upcoming, the lists (colour, open count) and categories in the sidebar; tasks grouped by category (or tags, or ungrouped), subtasks in expander rows, completed ones in a "Completed" expander or at the end.
- Entry at the top of the list: type, Enter → `add_task`; `Ctrl+K` category picker. Edit dialog for dates, reminders, priority, category, event link and notes.
- Drag and drop reorders, moves between categories, makes and promotes subtasks.
- Type to search; `Ctrl+Z` undo; Keyboard Shortcuts dialog. Toasts for sync errors; the sync button turns into a spinner while syncing.
- Calendar page: see [Calendar](#calendar-in-todav).
- Preferences: General (style, start page, account, push, settings sync), Tasks, Calendar, Extension.

**Process model**

- `GtkApplication` with `HOLD` flag so the process stays alive when the window closes (`--gapplication-service` on login via autostart `.desktop` with `X-GNOME-Autostart`). Closing the window hides it; quitting is explicit (menu or `Ctrl+Q`).
- Background thread: `push::listen` loop → on message `sync_topic` → emit `Changed` → UI refreshes on the main thread.
- Fallback timer: every 60 s while the window is focused, every 30 min otherwise → `sync()`. Reminders are checked every 30 s and shown as notifications.

**D-Bus interface** (session bus, name `io.github.selfref.Todav`, path `/io/github/selfref/Todav`)

```xml
<interface name="io.github.selfref.Todav1">
  <method name="GetLists">   <arg direction="out" type="a(ssus)"/> </method>           <!-- href, name, open_count, colour -->
  <method name="GetTasks">   <arg direction="in" name="list" type="s"/>
                             <arg direction="out" type="a(sssbu)"/> </method>          <!-- uid, summary, category, done, depth -->
  <method name="AddTask">    <arg direction="in" type="s"/> <arg direction="in" type="s"/> <arg direction="out" type="s"/> </method>
  <method name="ShowList">   <arg direction="in" type="s"/> </method>                    <!-- raise the window on that list -->
  <method name="ShowTask">   <arg direction="in" type="s"/> </method>                    <!-- the task's list, the task pulsing -->
  <method name="GetDays">    <arg direction="in" type="x"/> <arg direction="in" type="u"/>
                             <arg direction="out" type="aas"/> </method>              <!-- from (local midnight), days: each day's calendar colours, three at most -->
  <method name="GetAgenda">  <arg direction="in" type="u"/>
                             <arg direction="out" type="a(xsssbb)"/> </method>        <!-- days from today: day, time, title, colour, task, done -->
  <method name="ShowDay">    <arg direction="in" type="x"/> </method>                    <!-- the calendar on that day -->
  <method name="NewEvent">   <arg direction="in" type="s"/> </method>                    <!-- the new-event dialog with this title -->
  <method name="GetSettings"> <arg direction="out" type="a{ss}"/> </method>            <!-- the extension's options (ext_*) -->
  <method name="SetSetting"> <arg direction="in" type="s"/> <arg direction="in" type="s"/> </method> <!-- ext_* keys only -->
  <method name="Sync"/>
  <signal name="Changed">    <arg name="list" type="s"/> </signal>
  <property name="SyncState" type="s" access="read"/>                                 <!-- idle | syncing | error:<msg> -->
</interface>
```

The interface returns small tuples, not JSON, so the extension needs no parsing library. D-Bus activation (`DBusActivatable=true` + a service file) lets the extension start the app on demand.

**Packaging**: Flatpak on `org.gnome.Platform` 51 (`todav/linux/flatpak/`). Permissions: `--share=network`, `--talk-name=org.freedesktop.secrets`, `--talk-name=org.gnome.OnlineAccounts`; autostart through the Background portal. Not on Flathub yet.

## GNOME Shell extension

A panel indicator with a read-only view of a list and a month calendar; clicking anything opens the app there. It holds no state and does no networking: everything goes over D-Bus to the Linux app, which means the extension is a few hundred lines of GJS with zero dependencies.

**Behaviour**

- Panel icon (the app's symbolic icon, shipped with the extension) with the open-task count of the shown list.
- Click → a dense two-column menu (or one column): tasks on the left, calendar on the right.
  - Tasks: the list's name heads the column; clicking it swaps the tasks for the lists to pick another. Tasks are grouped by category, subtasks indented; done ones keep the same circle with a check in it. Clicking a task opens it in the app (`ShowTask`: its list, the row pulsing in the accent colour). A field adds a task (`AddTask`, no category).
  - Calendar: a month to browse (a day opens it in the app), with dots in the colours of the calendars that have something that day (`GetDays`, as the sidebar month), then an agenda of the coming week, events and dated tasks under their days (`GetAgenda`; a row opens its day, `ShowDay`), and a field that opens the app's new-event dialog with the title typed (`NewEvent`). Dots and agenda are options, on by default.
  - "Open Todav" → `ShowList(list)`.
- Its options are app settings (`ext_*`, Preferences → Extension), synced between devices like the rest; the list shown (`ext_list`) too. The extension reads them with `GetSettings` each time the menu opens and sets only `ext_list`.
- Subscribes to the `Changed` signal and `SyncState` property; updates the count on change; shows a small warning glyph when `SyncState` starts with `error:`.
- If the app is not running, the first D-Bus call activates it (D-Bus activation); the menu shows "Starting…" for that second. The app exports its interface in `dbus_register` (an `adw::Application` subclass in `main.rs`), before it takes the bus name, so that activating call finds it. The app's window then asks for attention (the shell does not raise a window on a D-Bus call); the extension raises it.
- If the app is not installed (D-Bus knows no such service: `ServiceUnknown`), the menu says so and offers "Get Todav", opening the latest GitHub release.

**Implementation**

- `metadata.json`: `shell-version` 48–51; no GSettings schema.
- `Gio.DBusProxy.makeProxyWrapper(interfaceXml)` from the XML in the Linux section; copy the XML verbatim into `dbus.js`.
- The task list scrolls; at most 100 tasks, then "+ n more".
- Install: `make install` copies to `~/.local/share/gnome-shell/extensions/todav@selfref.dev` (`make link` for development); no extensions.gnome.org submission in v1.

**Non-goals**: ticking or editing tasks (read-only for now), reordering, categories.

## Android app

Kotlin + Jetpack Compose on top of the same core via UniFFI. Push arrives through UnifiedPush with the ntfy app as distributor; no Google services, no own foreground service.

**Dependencies** (AndroidX + Compose + these three): `net.java.dev.jna:jna` (required by UniFFI), `org.unifiedpush.android:connector`, and PebbleKit (vendored AAR, see Pebble section). No Room, Retrofit, OkHttp, Hilt or WorkManager-dependent libraries; the core already has storage and HTTP.

**Structure**

- `android/core`: Gradle library module; a `cargo-ndk` task builds `libtodav_core.so` for `arm64-v8a` and `x86_64`, then runs `uniffi-bindgen` to generate `uniffi/todav/todav.kt`.
- `android/app`: `TodavApp` (holds one `Client`), `MainActivity` (Compose), `SyncScheduler`, `PushReceiver`, `pebble/` package.

**UI (Compose, Material 3)**

- Bottom navigation or a list drawer mirroring the Linux layout; task screen is a `LazyColumn` with sticky category headers; checkbox rows; swipe-to-delete with undo snackbar.
- Quick-add bar at the bottom with a category chip row (from config.json) above the keyboard.
- Home-screen widget (`AppWidgetProvider`, `RemoteViews` list) for the pinned list: ticking an item sends a broadcast → `set_done` → `sync()`. This is the Android counterpart of the GNOME extension.
- State: a `ViewModel` per screen reads the core on `Dispatchers.IO` and re-reads on `ChangeListener` callbacks; no in-memory duplicate of the data.

**Push and sync**

1. On first launch, `UnifiedPush.registerApp(context)`; when `onNewEndpoint(endpoint)` arrives, call `client.push_register(endpoint, 86400)` on IO. On `onUnregistered`, `push_unregister()`.
2. `onMessage(bytes)` → `client.push_decode(bytes)` → for each topic `client.sync_topic(topic)`. UnifiedPush delivers in the background, so this runs in a broadcast receiver with `goAsync()` and a short coroutine; keep it under 10 s.
3. If the connector reports an encrypted Web Push message (UnifiedPush 3 connectors decrypt it before `onMessage`), the bytes are already plaintext; `push_decode` handles both.
4. Safety net: `WorkManager` (part of AndroidX, acceptable) periodic `SyncWorker` every 30 min with network constraint; plus `sync()` on app foreground.
5. Registration refresh: a `SyncWorker` run also re-registers push when `push_expires` is within 6 h.

**Account setup**: Nextcloud Login Flow v2 (`/index.php/login/v2`) in a Custom Tab yields an app password; store it in `EncryptedSharedPreferences` is deprecated, so use the Android Keystore to wrap it and keep the blob in `SharedPreferences`.

**Battery**: the ntfy app keeps the only persistent connection; this app wakes only on push or on the 30-min worker. Ask the user to exempt ntfy (not this app) from battery optimisation.

## Pebble watchapp + companion

The watch is a remote control for the phone's local database: it never talks to the network and never holds the source of truth. The Android app is the companion and pushes list snapshots over PebbleKit `AppMessage`.

**Watchapp (C, Pebble SDK)**

- Screens: list picker (`MenuLayer`) → task list grouped by category (section headers = categories) → select toggles done. Up/down long-press jumps between sections.
- Persists the last snapshot in watch storage (`persist_write_data`) so the list opens instantly and works when the phone is out of range; changes made offline are queued and sent when connected.
- Supports Aplite/Basalt/Chalk/Diorite/Emery and the Core Devices models; monochrome-safe design (no colour as the only signal).

**AppMessage protocol** (keys in `appinfo.json` / `package.json`; all strings UTF-8, ≤ 60 bytes, longer summaries truncated with `…` on the phone)

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

- The phone keeps a `short id ↔ UID` map per snapshot; short ids avoid sending 36-byte UIDs. A snapshot is at most 60 tasks (watch memory); more → the phone sends the first 60 open tasks and a trailing "+n more" item.
- Items are sent one `AppMessage` at a time with ack-driven pacing (`app_message_outbox_sent` callback); the companion resends on NACK up to 3 times.
- On `SET_DONE` the phone calls `set_done`, replies `ACK_DONE`, and triggers `sync()`.
- On any `Changed` event for the currently shown list, the phone pushes a fresh snapshot if the watch is connected.

**Companion (Android, `pebble/` package)**: `PebbleKit.registerReceivedDataHandler` for the UUID; a `PebbleBridge` singleton that serialises snapshots from the core; started lazily when the watchapp sends `REQUEST_LISTS`. Works with the Pebble app from Core Devices (PebbleKit intents are unchanged).

**Timeline/push**: no watch-side push; the phone's push → sync → `Changed` → snapshot path gives the watch updates within a few seconds while the app is in the foreground, and on next watch request otherwise.

## Migration of existing tasks

Dropped: existing tasks are rewritten by hand into categories.

## Notav (notes + journal, planned)

Not started; nothing in the code yet. Notav (*notatka* + DAV) is a sibling app for notes and an Obsidian-style journal, with the same Nextcloud/GOA integration as Todav. It is a separate app, not a Todav view: different protocol, different UI (a Markdown editor), and each app stays small enough to finish.

**Decisions so far**

- Name Notav, app ID `io.github.selfref.Notav`, code in `notav/` (same shape as `todav/`: core, cli, linux, …).
- Storage: plain Markdown files in Nextcloud Files over WebDAV, so the Nextcloud Notes app and Obsidian (through the desktop sync client) can open the same folder. No proprietary format.
- Journal: one daily note per day in a dated folder; `[[wiki links]]`, backlinks and tags are computed locally from the files.
- Own synced settings file (`.config/notav/settings.json`), so the two apps never write the same file.
- Shared code: when Notav starts, move what both apps need out of `todav/core` into a shared crate at the root: login (Login Flow v2, GOA), the HTTP/WebDAV client, synced settings, `json.rs`, push keys. Task and calendar logic stays in `todav/core`.
- Cross-app: a note mentions a task with a `todav://task/<uid>` link; Todav opens it, and Notav shows its title and state through Todav's D-Bus API. No shared database.

**Open questions**

- [ ] Change detection for files: Nextcloud Files has no `sync-collection`; compare folder ETags (they propagate upwards) and only descend into changed folders.
- [ ] Push for files: `dav_push` covers CalDAV/CardDAV only; check whether Nextcloud's `notify_push` (or a small app posting to ntfy) can signal file changes. Polling as the fallback, as in Todav.
- [ ] Folder layout and name of the notes root (reuse the Nextcloud Notes folder or a separate one), daily note format, attachments.
- [ ] Editor: GtkSourceView with Markdown highlighting vs. a rendered/WYSIWYG view.

## Calendar (in Todav)

Events next to tasks in the same app, from the same CalDAV account: a full calendar view like GNOME Calendar, with tasks living on it. Decided 2026-10-20.

**Decisions**

- Sync: our own CalDAV in the core (not evolution-data-server), so Android gets it too. Every calendar collection is synced; a collection is a task list if it supports VTODO and a calendar if it supports VEVENT (Nextcloud's "Personal" is both).
- Events are stored like tasks (raw iCalendar plus derived columns, one row per resource, keyed by calendar and UID), through the same push/pull/conflict code.
- Time zones come from the `VTIMEZONE` inside each object (always present for a `TZID`; covers Windows zone names too), so no tz database dependency. A `TZID` without one is read as local time. Task due times use the same code.
- Recurrence: hand-written `RRULE` subset, enough for real data (Nextcloud, Thunderbird, Outlook exports in `tmp/events/`): `DAILY`/`WEEKLY`/`MONTHLY`/`YEARLY`, `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` (with ordinals), `BYMONTHDAY`, `BYMONTH`, `BYSETPOS`, `WKST`; plus `RDATE`, `EXDATE` and `RECURRENCE-ID` overrides. Other parts (`BYYEARDAY`, `BYWEEKNO`, `BYHOUR`…) show only the first occurrence. Expansion jumps to the requested range (time zone rules start in 1601).
- Occurrences are expanded on demand for the visible range; the table keeps each event's first start and last end so a range query only parses events that can overlap.
- Time-blocking: a task scheduled into a time slot gets `DTSTART` (and `DUE` as its end) on the VTODO itself, no linked event.
- Tasks for an event: `RELATED-TO` on the task pointing at the event's UID.

**UI**: a Tasks | Calendar switcher in the header. Calendar page: sidebar with a mini month, the calendars (colour, show/hide) and unscheduled tasks; Month, Week and Agenda views (custom-drawn grids); an event dialog like the task edit dialog. Tasks with dates show on the calendar, can be ticked there and dragged to another day or into a time slot.

**Steps**

- [x] Core: `VEVENT` objects in `ical`, `RRULE` expansion (`rrule.rs`), `VTIMEZONE` offsets (`tz.rs`; TZID-aware task times too). Tested on the TM account's exports (birthdays, weekly series, Windows zones, overrides)
- [x] Core: event calendars in discovery (with `writable` from the privilege set), `events` table keyed by calendar + UID, events through the same push/pull/conflict code, `events(from, to)` and `event_reminders` (`event.rs`), task `DTSTART`; integration test with a zoned weekly series and a moved occurrence. Push registration skips read-only calendars that refuse it
- [x] Linux: Tasks | Calendar switcher (`Adw.InlineViewSwitcher` over an `Adw.ViewStack`), month view (widgets, not drawn: up to three chips a day, "+N more" popover), agenda (30 days), calendars and task lists sidebar with colours and show/hide (synced `hidden_calendars`), mini month, event details popover, dated tasks tickable on the calendar, event reminders as notifications (open the calendar on that day). `calendar.rs`, `ui/calendar.blp`. Weeks start on the locale's first weekday (LC_TIME, as GTK's calendar: `i18n::first_weekday`)
- [x] Linux: week view (`calendar/week.rs`: hour grid drawn under an overlay of blocks, overlapping blocks side by side, all-day row, current-time line), "To Plan" sidebar (open tasks without a slot, undated or due by the end of the shown range), time-blocking by dragging a task onto an hour (`DTSTART`/`DUE`, an hour or its old length, 15-minute steps, drop preview), rescheduling by dragging to another hour or day (month cells, all-day row), unplanning by dropping on "To Plan". Core: `TaskPatch.start`, `scheduled`, `unscheduled`
- [x] Core + Linux: create, edit and delete events (this / this and following / all occurrences for recurring ones: an override, a split into two series, or the series moved with its overrides and exceptions), move events between calendars, event reminders as notifications. New events are written in the system zone (its VTIMEZONE borrowed from a stored event; floating otherwise); edits keep an event's own zone. Event dialog (`calendar/dialog.rs`, `ui/event-dialog.blp`: title, location, calendar, all day, start/end, plain repeat or a kept custom rule, until, notes, reminders, delete), Edit/Delete in the details popover, "+" / Ctrl+N, double-click a free hour or a day, drag events to another hour or day (asks which occurrences for a repeating one)
- [x] Integration: tasks linked to events (`RELATED-TO;RELTYPE=X-TODAV-EVENT` on the task; the event's popover lists them with done/all and adds new ones due when it starts; a task row shows its event as a pill opening the calendar), today's events at the top of Upcoming, the next event at the top of the GNOME extension's menu (`GetNextEvent` / `ShowDay`; since replaced by the month's dots and an agenda, `GetDays` / `GetAgenda`). Week view: only tasks with a start time take hours; ones just due at a time sit in the day's top row
- [x] Tasks and events alike: a task on the calendar opens a details popover as an event does (tick, dates, list · category, priority, its event, reminders, notes; a task's subtasks with a field adding one, a subtask names its task; Delete, Edit…), built from the same parts as the event's (`details_box`, `checklist`, `details_popover` in `calendar.rs`). A double click on either opens its edit dialog (`on_clicks`: the popover waits out the double-click time, as its grab would take the second click; events are plain buttons now, as tasks). "Edit on double click" (synced `double_click_edit`, Preferences → Calendar, on by default) turned off shows the popover at once instead. The event dialog has the task dialog's two-column layout (dates as expander rows with an inline calendar and time, one column under 560sp). `ShowDay` (the extension, a reminder) pulses the day in the month, week or agenda, as `ShowTask` pulses a task

## Progress

Deviation: no docker stack (`infra/`) for now — integration runs against the real instance (`cloud.aperte.dev`, user `Test`; ntfy at `ntfy.aperte.dev`, anonymous topics). Credentials live in a gitignored `.env`.

- [x] Cargo workspace (`core`, `cli`)
- [x] `todav/core/src/ical.rs` — VTODO reader/writer, folding, escaping, unknown props + VALARM preserved (unit tests)
- [x] `todav/core/src/caldav.rs` — XML tree, multistatus, discovery, `sync-collection` (incl. empty-token full sync), PUT/DELETE/GET with preconditions
- [x] `todav/core/src/store.rs` — schema + migrations by `user_version`
- [x] `todav/core/src/sync.rs` — push dirty queue, pull, conflict rule (unit test), `sync_topic`
- [x] `todav/core/src/push.rs` — p256dh keys in `kv`, registration/refresh/unregister, aes128gcm decryption, ntfy JSON-stream listener
- [x] `todav` CLI: `lists`, `ls`, `add`, `done`, `undo`, `rm`, `sync`, `push-register`, `listen`
- [x] Auth: server is behind Authelia, so DAV needs an app password (in `.env`). Test user has task lists `Shopping` and `Work` (created via MKCALENDAR)
- [x] CLI round-trip against the server (add, done, edit on server → pull)
- [x] Integration tests (`cargo test -p todav-core --features integration`): two-device round trip, foreign props preserved, offline tick-vs-edit conflict → done, remote delete. Each test makes and deletes its own calendar
- [x] Push verified manually: server-side edit → dav_push → ntfy → decrypt → `sync_topic` in < 6 s
- [x] ical round-trip on a real Nextcloud export (318 VTODOs): byte-stable, patching SUMMARY touches only SUMMARY. Exports go in gitignored `tmp/` (test skips when empty). Fixed: empty `DESCRIPTION:`/`RELATED-TO:` read as absent, `PRIORITY:0` = none
- [x] Automated push test + re-registration across expiry; invalid sync-token path test (malformed → 403, and never-issued → Nextcloud answers 207 with no changes; both trigger a full resync)
- [x] `config` module: `todav/config.json` (ETag, created from categories in use if missing), `categories()`, `grouped()` (unknown categories get their own group after configured ones; uncategorised last as "Other")
- [x] Core: Login Flow v2 (`login_flow_start/poll`), `logout`, `setting/set_setting`; hand-written `json.rs` (no serde)
- [x] Integration fixture reuses `it-<name>` calendars (Nextcloud rate-limits MKCALENDAR: 10/h per user) — verified
- [x] ~~M3 migration script~~ — dropped: existing tasks are rewritten by hand
- [x] M4 Linux app (`todav/linux/app`): login via browser (Login Flow v2), app password in Secret Service (libsecret), lists sidebar, grouped tasks, add with category (Ctrl+K), tick, delete, edit dialog, Done expander, F5/Ctrl+Q, hide on close, `--gapplication-service`, sync worker + ntfy listener + 60 s/30 min fallback timer, D-Bus API. `make -C todav/linux install` (desktop file, D-Bus activation, autostart). No GSettings: settings live in the core's kv. Verified via Broadway + D-Bus
- [x] Drag-and-drop reorder, app icon (`todav/linux/data/*.svg`, also the extension's panel icon)
- [x] M4 leftovers: undo for delete (toast; a delete already on the server is re-created), Flatpak (`todav/linux/flatpak/`, GNOME 51, `make -C todav/linux flatpak`; autostart through the Background portal; builds with network, Flathub needs a generated `cargo-sources.json`)
- [x] M5 GNOME extension (`todav/gnome-extension/`, ESM, Shell 48–51): pinned list, category sections, add entry, list switcher, open app, warning on sync error. `make -C todav/gnome-extension install`. Verified in a nested headless Shell. Since reworked into the read-only two-column menu with a month calendar (see [GNOME Shell extension](#gnome-shell-extension)), options in the app's Preferences
- [x] Dates: due date (+ optional time) and reminders (one `VALARM` each, `TRIGGER;RELATED=END`; presets or a custom time span; synced settings for the default new reminder and the time all-day reminders go off; local times in an `alarms` table) in the edit dialog; due tag on rows (red when overdue, bell with a reminder); "Upcoming" sidebar page across lists (Overdue, Today, Tomorrow, weekdays, Later); notifications for each reminder from the background service with "Mark as Done". Times are local wall-clock in the core (`to_local`/`to_utc` through SQLite's `localtime`); TZID times are read as local
- [x] Calendar: events (VEVENT) next to tasks — see the Calendar section for the steps
- [x] Sign-in through GNOME Online Accounts and with a login name + app password; synced client settings (`.config/todav/settings.json`, device-only keys, "Sync settings with server"); undo history (`Client::undo`, `Client::together`) with Ctrl+Z; create, rename, recolour and delete lists and calendars; search; keyboard shortcuts; About dialog from the metainfo; new app and symbolic icons
- [x] README and CONTRIBUTING
- [x] CI and releases (GitHub Actions): every push runs fmt, clippy `-D warnings` and the unit tests on Fedora 45 (GNOME 51), and builds the packages: the Flatpak bundle (Flathub's GNOME 51 builder image), a tarball of the system-wide tree (`make -C todav/linux stage`: app, data, `/etc/xdg/autostart`, the extension under `/usr/share/gnome-shell/extensions`) built on Fedora 43 so its glibc suits newer distributions, a `.deb` for Ubuntu 26.04+ repacked from that tree (`scripts/deb.sh`, dependencies by `dpkg-shlibdeps` plus libadwaita ≥ 1.8) and the extension zip. Lowest supported GNOME: 49 (libadwaita 1.8, `Adw.ShortcutsDialog`). The manual Release workflow builds them with the version set by `scripts/release.py`, then commits `chore(release): X`, tags `vX` and publishes a GitHub release with the changelog section as notes. The AUR has the app and the extension as separate packages, each from the release tag, the release assets (`-bin`) and `master` (`-git`); they are maintained outside this repository. One version for the workspace (`[workspace.package]`), the metainfo and the extension
- [x] Translations: gettext domain `todav` for the app (Rust `i18n.rs` over glibc, Blueprint `_()`), the extension (`gettext-domain`, catalogs in its `locale/`), the desktop file and the metainfo (`msgfmt --desktop/--xml`, rules in `po/its/`, so no AppStream package is needed to build). `scripts/i18n.sh` refreshes `po/todav.pot` and merges it; build.rs compiles the catalogs for a build run (and the translated metainfo for About), `make install-data` and the extension's `stage` for installs. Languages: Polish (reviewed), German, French, Spanish, Italian, Brazilian Portuguese (drafts). Weeks start on the locale's first weekday (`i18n::first_weekday`, glibc's `_NL_TIME_WEEK_1STDAY` + `_NL_TIME_FIRST_WEEKDAY`, as GTK's calendar)
- [ ] `docs/`, Flathub (generated `cargo-sources.json`), extensions.gnome.org
- [ ] M6 Android (+ UniFFI exports) → M7 Pebble

## Roadmap

Public in the README as planned work; order not fixed.

- [ ] **Multiple accounts**: several servers or users side by side; the core holds one account today (`Client::account`), and lists, push registrations and settings would need an account key.
- [ ] **Offline mode**: use Todav without a server (local-only lists), and move them to an account later.
- [ ] **More services** besides Nextcloud: other CalDAV servers first (Radicale, Baïkal, Fastmail, iCloud…), which mostly means login without Login Flow v2, settings/categories without Nextcloud Files, and polling where there is no WebDAV-Push.
- [ ] **Android app**: M6 above.
- [ ] **Nested subtasks**: more than one level; deeper `RELATED-TO` nesting is read and kept today, but the UI shows one level (the calendar's task popover lists a task's subtasks only).
- [ ] **Advanced notifications**: configurable, beyond the reminder notifications the background service shows now; details to be specified.
- [ ] **Starred tasks**: details to be specified.

## Milestones

Each milestone ends with something you can use daily; nothing is started before the previous one is green in CI.

| # | Milestone | Done when |
| --- | --- | --- |
| 0 | Infra | Dropped: tests run against the real instance (see [Progress](#progress)) |
| 1 | Core read/write | `todav ls/add/done/sync` round-trips against the server; `ical` round-trip tests pass on exports from Nextcloud Tasks, Tasks.org, Errands |
| 2 | Core push | `todav listen` registers WebDAV-Push with an ntfy topic and syncs within 5 s of a web edit; registration refresh works across expiry |
| 3 | Migration | Dropped: tasks are rewritten by hand |
| 4 | Linux app | GTK app replaces Errands for daily use; survives window close; D-Bus interface answers `GetTasks`; Flatpak builds |
| 5 | GNOME extension | Popup shows the pinned list and adds; count label updates on `Changed` |
| 6 | Android app | Replaces Tasks.org for daily use; UnifiedPush via ntfy delivers in background; widget works |
| 7 | Pebble | Watch lists, ticks, works offline; snapshot refresh after a push on the phone |
| 8 | Polish | Conflict log visible in app settings; crash-free for 2 weeks; `docs/` complete |

## Open questions and risks

- [x] **dav\_push encryption** — resolved: `dav_push` (v1.0.3) *requires* `subscription-public-key type="p256dh"` + `auth-secret` and always sends aes128gcm. Namespace is `https://bitfire.at/webdav-push` (props `transports`, `topic`), registration is `POST <calendar>` with `push-register`, `201` + `Location`, max expiry 1 week, unregister = `DELETE <Location>`, echo suppression via `Push-Dont-Notify: "<Location>"`. Implemented in `todav/core/src/push.rs`, verified against the RFC 8291 test vector. Original note: the WebDAV-Push draft leaves RFC 8291 message encryption as TODO, while DAVx5 states its pushes are end-to-end encrypted. Before writing `core/push`, read `nc_ext_dav_push`'s registration handler and capture one real DAVx5 ↔ Nextcloud exchange (dav\_push debug log). If keys are required: add `client-public-key`/`auth-secret` to the registration XML, generate P-256 keys in the core, store them in `kv`, and pull in `ece`. If not: skip encryption on this private server and note it in `docs/PUSH.md`.
- [ ] **dav\_push stability**: app-store build may lag GitHub; pin a git commit on the server. Fallback if it misbehaves: a 50-line Nextcloud app that listens to `CalendarObject{Created,Updated,Deleted}Event` and `POST`s the topic to ntfy directly (the core's listener code stays identical).
- [ ] **ntfy topic security**: topics must be random (22+ chars) and the ntfy server should have ACLs so only the owner can subscribe; the Linux app authenticates with an ntfy token stored in the Secret Service.
- [ ] **Pebble SDK toolchain**: confirm the current open-source SDK builds for the user's watch model and that the Core Devices Pebble app forwards PebbleKit intents; otherwise fall back to the legacy Pebble app for the companion test.
- [x] **GNOME Shell version**: the extension targets Shell 48–51 (ESM); GJS APIs for popup menus moved between 45 and 48.
- [x] **Nextcloud Tasks compatibility**: manual order is stored in Nextcloud Tasks' own `X-APPLE-SORT-ORDER`, so both share one order; `X-TODAV-ORDER` is still read and dropped on the next reorder.

**Verification checklist for the agent** (run before marking any milestone done)

- [ ] `cargo test` incl. integration against the server (`--features integration`) is green.
- [ ] A task created in Nextcloud Tasks web appears in the client under test within 5 s with the app in the background (Linux hidden window / Android not running).
- [ ] A task ticked on device A shows ticked on device B within 5 s; ticking the same task on both while offline resolves to "done" after reconnect.
- [ ] Killing the network for 10 min and restoring it produces no duplicates and no lost edits.
- [ ] Round-tripping a VTODO with `VALARM`, `DESCRIPTION` with newlines, and non-ASCII `SUMMARY` through the core changes only the intended properties (diff the `.ics`).
- [ ] No new third-party dependency without a line in the Dependency policy table.

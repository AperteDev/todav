# Todav — Architecture and Plan

Oct 8, 2026 · @Kris · updated Oct 26, 2026

## Overview

Todav is a personal, local-first task list synced through Nextcloud CalDAV (VTODO) with near-instant push via WebDAV-Push (`dav_push`) and a self-hosted ntfy server. One Rust core drives the front-ends: a GTK4/libadwaita app on Linux, a GNOME Shell extension for quick preview, an Android app, a Pebble watchapp fed by the Android app, and a Windows app (planned, see [Platforms](#platforms)).

Todav is meant to become a tasks + calendar app (VEVENT next to VTODO, same CalDAV calendars). Notes and journal go in a sibling app, Notav, with its own repository (see [Notav integration](#notav-integration-planned)).

**Goals**

- Same lists, same behaviour, same categories on every device; changes visible on other devices within seconds.
- Local-first: every UI reads and writes SQLite; the network is never on the UI path.
- Nextcloud Tasks web UI and DAVx5/Tasks.org keep working as fallbacks (no proprietary storage).
- Minimal dependencies: standard library and platform SDKs first; a third-party crate or library only where it replaces real work (HTTP/TLS, SQLite, XML, UniFFI).

**Non-goals (v1)**

- Multi-user sharing, assignees, comments.
- Recurring tasks, attachments.
- iOS, macOS, web front-end (iOS and macOS later, see [Platforms](#platforms)).

**Principles for the agent**

- The Rust core is the only place with sync, push, parsing and conflict logic. Front-ends are thin: they render state and call commands.
- What every front-end needs beyond storage and sync (settings with their defaults, date rules, task trees, calendar layout and drops, the background service) lives in `todav-app`, not in a front-end. A front-end keeps its widgets, its wording and formatting, and its platform services (keyring, notifications, timers).
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
| Account | `accounts` table, this device only | A Nextcloud account or the local one (see [Accounts](#accounts)); every list belongs to one |
| List (Shopping, Personal, Work) | CalDAV calendar collection | Only calendars that advertise `VTODO` in supported-component-set are shown |
| Task | `VTODO` resource (`<uid>.ics`) | `UID`, `SUMMARY`, `STATUS`, `COMPLETED`, `PRIORITY`, `DUE`, `DESCRIPTION` |
| Category (Groceries, Home, Electronics, Mountains, Car) | `CATEGORIES` on the VTODO | Exactly one category per task in v1; multiple are read but the first is used for grouping |
| Subtask | `RELATED-TO;RELTYPE=PARENT:<parent uid>` | Any depth |
| Manual order | `X-APPLE-SORT-ORDER` (integer), shared with Nextcloud Tasks | Fallback: `CREATED` timestamp; the old `X-TODAV-ORDER` is read and dropped on the next reorder |
| Event | `VEVENT` resource in a calendar that supports it | See [Calendar](#calendar-in-todav) |
| Category metadata (order, icon) | `todav/config.json` in each account's Nextcloud Files via WebDAV | One file per account for its lists, fetched with `If-None-Match` on sync |
| Client settings | `.config/todav/settings.json` in the Nextcloud Files of the account chosen for it | Merged per key on every sync, newer wins; device-only keys stay local |
| Server change cursor | `sync-token` per calendar | Stored locally; `sync-collection` REPORT (RFC 6578) on every sync |
| Push identity | `DAV:Push` `topic` per calendar | Maps incoming push to a calendar |

**Local schema**: SQLite in the core (`todav/core/src/store.rs`, migrations by `PRAGMA user_version`): `accounts`, `lists` (each with its account), `tasks`, `events` (raw iCalendar plus derived columns, a dirty flag for the push queue), `alarms` (reminders in local time), `moved` (server copies of tasks moved to another list, deleted on the next push) and `kv` (config/settings cache, per account as `a<id>_…`; push keys, setting change times).

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

One Cargo workspace at the root, the app's core and front-ends under `todav/`. Notav has its own repository.

```text
<repo>/
  Cargo.toml                 # workspace: todav/core, todav/app, todav/cli, todav/linux/app
  todav/
    core/                    # todav-core (library), flat modules in src/
      src/
        lib.rs               # public API (Client), change events
        account.rs           # accounts: add, connect, switch off, remove
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
    app/                     # todav-app: what every front-end does over the core, no toolkit, no text
      src/
        prefs.rs             # synced settings the app reads, with their defaults
        dates.rs             # day arithmetic, how a day is named, Upcoming sections, reminder offsets
        tasks.rs             # subtask trees, sidebar pages, drag and drop of tasks
        calendar.rs          # items by day, agenda, search, drops on days/hours/lists, week and month layout
        lists.rs             # colours, order by account, a new local account's lists
        accounts.rs          # add, reconnect, remove over the platform's keyring (`Keyring`)
        service.rs           # sync worker, ntfy listener, safety-net syncs, due reminders (`Shell`)
    ffi/                     # todav-ffi, a workspace of its own: core + app exported with UniFFI
      src/lib.rs             # the `Todav` object, callback interfaces, plain functions
      src/types.rs           # records and enums: core and app types mirrored, FFI-only forms
      tests/                 # smoke tests of the generated bindings (kotlin/, csharp/)
    cli/                     # todav-cli: thin debugging/scripting front-end
    linux/
      app/                   # GTK4 + libadwaita app (Rust, gtk4-rs); Blueprint views in ui/
      data/                  # .desktop, metainfo, icons, D-Bus service
      flatpak/               # dev.aperte.Todav.json
    gnome-extension/         # GNOME Shell extension (GJS), talks D-Bus to the Linux app
    android/                 # Gradle: core/ (todav-ffi, bindings, catalogs), app/ (Compose); sdk.sh, Makefile
    pebble/                  # planned: Pebble watchapp (C, Pebble SDK)
    windows/                 # planned: WinUI 3 app (C#)
```

**Tooling**

- Rust stable; `cargo clippy -D warnings`, `cargo fmt`, `cargo test` (`--features integration` for the tests against the server).
- Linux: `make -C todav/linux install` (per-user) or `make -C todav/linux flatpak`; the build needs `blueprint-compiler` and `glib-compile-resources`. Flathub will need a generated `cargo-sources.json` for an offline build.
- Android: `make -C todav/android setup|run|logs|build` (AGP 9, Kotlin, Compose; Gradle wrapper on JDK 21). The SDK is shared in `~/Android/Sdk`, set up by `todav/android/sdk.sh` with the versions in `todav/android/gradle.properties`; the emulator image is the one Noogle uses (see AGENTS.md).
- Bindings: `scripts/bindings.sh` (see [Platforms](#platforms)); needs Gradle with a JDK 17 and the .NET 8 SDK for the smoke tests.
- Windows (planned): .NET and the Windows App SDK (WinUI 3), `cargo build --target x86_64-pc-windows-msvc` (and `aarch64`) in `todav/ffi`; `uniffi-bindgen-cs` for the C# bindings. Builds and runs only on Windows: a Windows machine or VM for development, a `windows-latest` runner in CI.
- Pebble (planned): the open-source Pebble SDK toolchain (`pebble build`), pinned version in `pebble/README.md`.
- CI (GitHub Actions, `.github/workflows/`): fmt, clippy, unit tests, the bindings (`scripts/bindings.sh`), the Flatpak bundle and the extension zip and the Android debug APK on every push; releases by the manual Release workflow (see [Progress](#progress)). Later the Pebble build. No signing in CI.

**Dependency policy** (keep it short)

| Need | Crate / library | Why not std |
| --- | --- | --- |
| HTTP + TLS | `ureq` (sync, rustls) | No async runtime needed; keeps the core small |
| SQLite | `rusqlite` (0.38 to 0.40), the system SQLite | Standard choice, single file; not bundled, as distributions (Fedora) require, and 0.38 is what Fedora packages. Android and Windows will need its `bundled` feature (behind a feature of the core, off for Linux) |
| XML | `quick-xml` | PROPFIND/REPORT bodies and multistatus responses |
| FFI | `uniffi` (in `todav-ffi` only) + `uniffi-bindgen-cs` (NordSecurity, a build tool) | Kotlin (later Swift) bindings from UniFFI itself, C# from the third-party generator; UniFFI pinned to the version both support (0.31 as of Oct 2026) |
| ntfy stream (Linux push) | — (`ureq`) | ntfy's `/<topic>/json` NDJSON stream over plain HTTP; no WebSocket crate needed |
| Web Push decryption | `ring` (HKDF, AES-128-GCM, RNG; already pulled in by rustls) + `p256` (static-key ECDH) | `dav_push` always encrypts (RFC 8291); `ece` drags in OpenSSL, ring can't load a stored ECDH key |
| iCalendar | hand-written in `todav/core/src/ical.rs` | Full iCal crates are large; VTODO needs \~15 properties, line folding and escaping |
| Linux UI | `gtk4`, `libadwaita` | The platform toolkit; D-Bus, GOA and notifications go through `gio` from the same bindings |
| Linux secrets | `libsecret` | App passwords in the Secret Service |
| Locale (gettext, first weekday) | `libc` (already in the tree) + glibc's own gettext | `setlocale`, `nl_langinfo`; the five gettext calls are declared in `i18n.rs`, no `gettext-rs` |
| Android UI | AndroidX: Compose Material 3, Navigation 3, WorkManager, Glance | The platform's own libraries: screens with predictive back, the 30-minute safety-net sync, the home-screen widget |
| Android push | — (UnifiedPush AND\_3 spoken directly, `Background.kt`) | The connector library brings Google's Tink to decrypt Web Push, which the core already does |

Anything beyond this table needs a one-line justification in the PR.

## Rust core (`todav-core`)

The core owns all state and all network I/O. Front-ends get a `Client` handle, call commands, and subscribe to change events. The API is synchronous and blocking; callers run it on a background thread (GTK: `gio::spawn_blocking`, Android: coroutine on `Dispatchers.IO`).

**Public API (UniFFI surface)**: the first sketch below; the real API in `todav/core/src/lib.rs` has grown since (accounts, events, lists and calendars, settings, login, undo). `set_account`/`account` became `add_account`, `connect`, `accounts` and the rest of [Accounts](#accounts).

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
    pub fn last_sync(&self) -> Option<i64>;              // when a sync last went through
    pub fn server_info(&self) -> Result<ServerInfo>;     // status.php: product, version, maintenance

    // Push
    pub fn push_register(&self, push_resource: String, expires_in_secs: u64) -> Result<Vec<PushRegistration>>;
    pub fn push_unregister(&self) -> Result<()>;
    pub fn push_decode(&self, body: Vec<u8>) -> Result<Vec<String>>;  // returns topics
    pub fn push_status(&self) -> PushStatus;  // calendars, with a push topic, registered

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

## Shared front-end layer (`todav-app`)

Code every front-end would otherwise write again, on top of `Client`; the Linux app uses it directly, the others through `todav-ffi`. It holds no state of its own beyond the service's, has no dependency but the core, and returns no text: what to show comes back as enums, times and ids (`dates::Day`, `dates::Section`, `calendar::Time`, `service::State`, `accounts::Error`), and the front-end words and formats it in its locale.

- `prefs`: the settings the app reads with their defaults (sorts, completed tasks, stars, pins, durations, reminder defaults, hidden calendars), so a missing key means the same everywhere.
- `dates`, `tasks`, `calendar`, `lists`: pure rules over core data (trees, Upcoming, items by day, lanes and columns of the views, what a drop does). A drop that makes several core calls is one undo step (`Client::together` inside).
- `accounts`: add, reconnect and remove, with the platform's keyring behind the `Keyring` trait; a password from elsewhere (GNOME Online Accounts) stays the front-end's (`Account::source`).
- `service`: one `Service` per app: the sync worker (requests coalesced, 300 ms to let a burst settle), the ntfy listener (`start(listen)`; Android gets pushes from UnifiedPush instead), `tick` once a minute for the safety-net syncs, `due_reminders` since the last call. It reports to the front-end's `Shell` from its own threads.

## Platforms

Decided 2026-10-26: a native UI per platform over the shared Rust, not a cross-platform UI toolkit.

| Platform | UI | Bindings |
| --- | --- | --- |
| Linux | GTK4 + libadwaita (Rust) | `todav-core` and `todav-app` directly |
| Android | Kotlin + Jetpack Compose (Material 3) | UniFFI (Kotlin), from `todav-ffi` |
| Windows | C# + WinUI 3 (Windows App SDK) | `uniffi-bindgen-cs`, from `todav-ffi` |
| iOS, macOS (later) | SwiftUI | UniFFI (Swift), from `todav-ffi` |

Why: most of each app's own work is the platform's services (Android: UnifiedPush, widgets, the Pebble companion, notification actions; Windows: tray, start with the session, toasts, the credential locker), and with `todav-app` the part written per platform is mostly views. Compose Multiplatform (one Kotlin UI for Android and Windows) was ruled out: Material on Windows and a resident JVM for an app that runs all day for push and reminders. Flutter and Slint likewise give neither platform its own look, and Android's services would still be Kotlin.

**Bindings** (`todav-ffi`): a cdylib (and staticlib) exporting the core's and `todav-app`'s API with UniFFI proc-macros. It is a Cargo workspace of its own (excluded from the root one), so UniFFI and SQLite's `bundled` feature, which Android and Windows need, stay out of the Linux builds and their packaging; the release script sets its version too.

- One object, `Todav`, holds the `Client` and the `Service` (started by `start_service`; without it the service calls do nothing); pure rules are plain functions (Kotlin: top-level, C#: static methods of `TodavCore`). The API is synchronous with sync callback interfaces (`Listener`, `ServiceShell`, `Keyring`, `Changes` for `together`, `EventFilter`), which both generators handle; the C# one has open bugs with async callbacks.
- Core and app types are mirrored with `uniffi::remote` (the compiler checks the fields), so neither crate depends on UniFFI. The bindings' own types cover what does not cross as it is: patches (`Option<Option<T>>` is a single optional in Kotlin, so a field to clear is `DueValue { due: None }`), tuples, `usize`, enums with unnamed fields, `SyncReport`'s errors. One error type, `TodavError` (Kotlin `TodavException`, C# the same name).
- Names that would clash on a platform are avoided at the source: `TodavList` and `TodavTask` (Kotlin's `List`, C#'s `Task`), `Changes` (C#'s `Action`), `ReminderSpan` (`Span`), `SidebarPage` and `DayBlock` (WinUI's `Page` and `Block`); a variant's field never has the variant's name (C#). The C# namespace is `Aperte.Todav.Core` (a `Todav.…` namespace would be shadowed by the `Todav` class), the Kotlin package `dev.aperte.todav.core`; C# symbols are `internal`, the bindings file compiled into the app.
- `scripts/bindings.sh` builds the library for the machine it runs on, generates both bindings (Kotlin with UniFFI's own generator, a binary of the crate; C# with `uniffi-bindgen-cs`, built into `target/tools` at the version matching UniFFI 0.31) and runs the smoke tests in `todav/ffi/tests/` against them: Gradle with JNA, and .NET 8. CI runs it, so an API change that does not export, or breaks the bindings, fails there.

**Translations** (decided 2026-10-26): gettext everywhere, `po/` the one source. Android and Windows read the same compiled catalogs (`.mo`, made by msgfmt at build time and shipped with the app) through `todav_app::i18n::Catalog`, exported as `Translations` (`gettext`, `ngettext`, `pgettext`; `Translations.english()` without a catalog): the same messages, contexts and Polish plurals as the Linux app, which keeps glibc's gettext (its Blueprint files need it). The front-end picks the language (Android: the app's locale, so the per-app language setting works if the app declares its languages; Windows: the user's languages) and loads that file. xgettext reads C# itself and Kotlin with its Java parser; `scripts/i18n.sh` will add their sources once the apps exist. Rejected: `strings.xml` and `.resw` generated from `po/` (`.resw` has no plurals, and a second message format per platform).

## Windows app (planned)

What the Linux app does, the Windows way: one resident process (single instance by `AppInstance.FindOrRegisterForKey`) with a tray icon, the window hidden on close; the ntfy listener and the safety-net syncs from `todav-app`'s `Service`; reminders as toasts (`AppNotificationManager`) with "Mark as Done"; start with the session (an MSIX `StartupTask`); app passwords in the credential locker (`PasswordVault`); the system zone for `Client::set_local_zone` from `TimeZoneInfo.TryConvertWindowsIdToIanaId`; data in the app's local folder. A tray flyout with a list and the agenda would be the GNOME extension's counterpart. WinUI 3 has no tray icon of its own: a small library (H.NotifyIcon) or the Win32 calls through CsWin32, to be chosen and added to the dependency table. Distribution through the Microsoft Store (free for individual developers since September 2025; the Store signs the MSIX) and winget; check the Store's rules on AI-assisted code first, as for Flathub.

## Linux app (GTK4 + libadwaita)

A single Rust binary (`todav-linux`) that is both the GUI and a background service: it keeps the ntfy connection open, runs sync, and exposes a small D-Bus interface the GNOME extension uses. There is no separate daemon.

**Stack**: `gtk4-rs`, `libadwaita-rs`, `libsecret`; D-Bus, GNOME Online Accounts and notifications through `gio`. Static views are Blueprint files in `ui/`. No GSettings: settings live in the core (`Client::setting`, synced). App passwords in the Secret Service, under a schema named after the app ID, one entry per server and user; a GOA account's password stays in GOA (`Account::source` is its GOA id). An account without its password (keyring cleared, GOA account gone: not `Client::connected`) does not sync or listen for pushes; its lists stay usable and a banner over the tasks and the calendar names it, with Sign In (its page in Preferences). State in `$XDG_DATA_HOME/todav/core.sqlite` via the core.

**UI**

- Welcome page (no account yet) in two steps (`AdwNavigationView`, `ui/add-account.blp` and `add_account.rs`, shared with Preferences' Add Account dialog): first the kind of account (GNOME Online Accounts, a Nextcloud server, or "On This Device Only"; more can be added later in Preferences, it says), then the account to connect: the GOA account (those not added yet), or the server to sign in to with the browser (Login Flow v2; or by a copied login link) or a login name + app password, three buttons that give way to Cancel while a sign-in waits for the browser. On the welcome page only: the ntfy server and "Sync settings with this account" (on). "On This Device Only" adds the local account with a task list holding a sample task and an empty calendar (see [Local lists](#local-lists)).
- Tasks | Calendar switcher in the sidebar's header. Both pages are an `AdwOverlaySplitView` (decided 2026-10-27, for a sidebar that can be hidden, as on Android): a button at the start of the content's header and F9 show or hide the sidebar, the two pages' bound together; hidden on a wide window, the switcher stands in that header too; under 700sp it collapses into an overlay over the content, which picking a list or a day closes. Tasks page: Upcoming and Important (each only while it has tasks), the lists (colour, open count) and categories in the sidebar; tasks grouped by category (or tags, or ungrouped), subtasks in expander rows (one per top-level task, deeper levels indented inside it; a subtask's "+" gives it an "Add subtask" entry until Escape), completed ones in a "Completed" expander or at the end.
- Entry at the top of the list: type, Enter → `add_task`; `Ctrl+K` category picker. Edit dialog for dates, reminders, priority, category, event link and notes.
- Drag and drop reorders, moves between categories, makes and promotes subtasks at any depth (a task takes its own subtasks along; `Client::within` refuses a drop inside its own tree; dropped on an "Add subtask" entry, it goes where a new subtask would, which also ends a level after a deeper subtree), and moves a task to another list when dropped on it in the sidebar (also the edit dialog's list picker, `lists::picker`). On the Calendar page, an event dropped on a sidebar calendar moves there (`move_event`, the whole series), a task on a list likewise.
- Moving a task (`Client::move_task`) takes its subtasks; a subtask dropped on a list leaves its parent first. Tasks are keyed by UID, so the row goes with the task and the old server copy waits in the `moved` table for the next push to delete it (before the uploads, so the UID is free in time); undo takes it back from there if it is still waiting.
- After an edit, the app asks for a sync only when something waits for the server (`Client::pending`): edits in local lists never start one.
- Type to search; `Ctrl+Z` undo; Keyboard Shortcuts dialog. Toasts for sync errors; the sync button turns into a spinner while syncing.
- Completing a task with open subtasks follows `complete_subtasks` (synced, Preferences > Tasks; `todav_app::prefs::subtask_completion`): it completes them too (the default, as Nextcloud Tasks does, and reopening a subtask then reopens its completed parents, so a completed task has no open subtasks), leaves them open, or asks each time (`window::tick`, which every check in the app goes through; a notification's "Mark as Done" cannot ask and leaves them open). Reopening a task with completed subtasks always asks whether they reopen too (no setting). `todav_app::tasks::set_done` does any of these as one undo step. A tree stays among the open tasks, its completed tasks struck through, while any task in it is open (`split_done` in the core), so open subtasks never show as top-level tasks.
- "Edit titles in place" (synced `inline_edit`, Preferences > Tasks > Interaction, off by default): a click on a task row's title text with a mouse or touchpad (not a touchscreen) puts a `gtk::Text` in the label's place, the caret where it was clicked, with a text cursor over the title text only (`inline_title`). Enter or focus leaving saves, Escape cancels; rebuilds of the list wait until the edit ends (`Ui::renaming`). A click beside the text, or a tap, still opens the task or its subtasks.
- Updates while running (`update.rs`): a package replaces the executable, so a file monitor on it notices once the file at its path is no longer the running one (after 3 s without changes); in the Flatpak, the portal's update monitor reports a newer local commit. With no window open the app restarts at once, into the background (`--gapplication-service`); otherwise a banner on both pages offers Restart. Natively the process `exec`s the new executable after `app.run()` returns (spawning a child would die with the old one's systemd unit); the Flatpak asks the portal to `Spawn` the latest version and quits. The new instance takes over the bus name with `--gapplication-replace` (the app allows replacement), in case the old one has not released it yet.
- Calendar page: see [Calendar](#calendar-in-todav).
- Preferences: General (style, start page), Accounts (a row per account, with a drag handle when there are several; Add, the ntfy server, "Sync settings with"), Tasks, Calendar, Extension. Each account's page (`ui/account.blp`, a subpage): "Use This Account" (switched off: hidden and not synced), its name (empty gives back "user@host" or "On This Device"), for a Nextcloud account the connection (signed in in Todav: Sign In Again with the browser, a link or a password, for the same user; another server address moves the account there) and Server Status; Remove (asks; uploads waiting edits first, or asks again to lose them).
- Server status (an account's page, while connected): the server as `status.php` describes it (asked each time the page shows; unreachable or in maintenance mode says so), the last sync (`Client::last_sync`, or that account's last error) and DAV Push (`Client::push_status` plus the ntfy listener's last error). Without a push topic on any calendar the server lacks the DAV Push app: the row says what that costs and links to its page in the Nextcloud App Store; with topics but no ntfy server, it asks for one. The sync and push rows follow sync state changes while the dialog is open.

**Accounts**

- Several accounts side by side (decided 2026-10-26): Nextcloud accounts (`AccountKind::Nextcloud`, through GOA or signed in in Todav; refused when the same server and user is there) and the local account (`AccountKind::Local`, one at most), with room for other kinds of service later. The `accounts` table in the core; each list has its account, `Client::connect(id, password)` gives a Nextcloud account its connection for the run.
- Lists are keyed by their full URL (`Dav::absolute`: origin + the server's path), so two servers' lists with the same path stay apart; item hrefs stay server paths. Todav 1.2 kept paths: a synced setting holding one (`hidden_calendars`, `ext_list`) from a device still on it matches the list at that path (`todav_core::same_list`).
- Tasks are keyed by UID: a task in a calendar two added accounts share shows once, in the account that had it first (`store::task_elsewhere`); TODO(v2): key them by list and UID, as events are.
- `sync` goes through every switched-on, connected account; one failing does not stop the others (`SyncReport::failed`, by account; the sync state names them). One ntfy topic and one set of push keys for the device (`push_resource` kept from the listener's first registration); each account registers its own calendars with it, and its sync registers writable calendars that have none yet (a new account, a new calendar) instead of waiting for the listener's hourly round. `push_lock` keeps the two from registering one calendar twice.
- Order: the user's (`reorder_accounts`, `accounts.position`; dragged by their handles in Preferences → Accounts), new accounts last. `Client::accounts` returns them in it, and everything listing accounts or grouping lists by account follows it: sidebars, pickers, "Save to", "Sync settings with", the extension's lists (`GetLists`).
- Switched off (`set_account_enabled`): its lists and what is in them are hidden from every query (`shown`), not synced, its server's pushes stopped; nothing is deleted. Removed (`remove_account`): its lists and their contents leave this device (a local account's are gone for good), with its saved password.
- Settings sync through the account in the device-only `settings_account` (none: they stay here). Added on the welcome page with "Sync settings with this account" on, an account becomes it; added from Preferences, nothing changes. Each account keeps its own category file.
- Migrated from Todav 1.2 (one account in `kv`, local lists behind the `local_mode` setting): the account and, with local lists or local mode, the local account (switched off when local mode was off); `sync_settings` off became no settings account. 1.2's remembered browser sign-in (kept while GOA was used) is cleared at start, with its password unless an account uses it.

**Local lists**

- Lists and calendars kept on this device only: the local account's. Their href is `local:<uid>/` (`store::LOCAL`) in the same `lists` table, so tasks and events in them are stored like any other; `List::local` marks them.
- Never synced: the upload queue (`store::dirty`) skips them, so items in them keep `dirty` set and no href; refreshing an account's calendars and adding or removing another account leave them alone; creating, editing and deleting a local list never calls a server. Moving an event from a local calendar to a server one uploads it (it is new there), and moving one back deletes the server copy.
- Without a Nextcloud account the app runs on them alone: the background service starts for reminders and D-Bus, sync and push stay off, and the sync buttons are greyed out.
- Shown by account: both sidebars and the event dialog's calendar picker group lists by account in the accounts' order (`todav_app::lists::by_place`), with a heading per group only when there is more than one (`lists::headings`; ListBox header functions, and an `Adw.ComboRow` header factory over a `GtkFlattenListModel` of one string list per account). The new list dialog's "Save to" row offers the accounts that can take one (`lists::places`) when there is a choice.
- Not done: moving a whole list between accounts (single events move with the calendar picker, tasks by dropping them on a list).

**Process model**

- `GtkApplication` with `HOLD` flag so the process stays alive when the window closes (`--gapplication-service` on login via autostart `.desktop` with `X-GNOME-Autostart`). Closing the window hides it; quitting is explicit (menu or `Ctrl+Q`).
- Background thread: `push::listen` loop → on message `sync_topic` → emit `Changed` → UI refreshes on the main thread.
- Fallback timer: every 60 s while the window is focused, every 30 min otherwise → `sync()`. Reminders are checked every 30 s and shown as notifications.

**D-Bus interface** (session bus, name `dev.aperte.Todav`, path `/dev/aperte/Todav`)

```xml
<interface name="dev.aperte.Todav1">
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
  <property name="SyncState" type="s" access="read"/>                                 <!-- idle | syncing | error:<msg>, naming the accounts that failed -->
</interface>
```

The interface returns small tuples, not JSON, so the extension needs no parsing library. D-Bus activation (`DBusActivatable=true` + a service file) lets the extension start the app on demand.

**Packaging**: Flatpak on `org.gnome.Platform` 51 (`todav/linux/flatpak/`). Permissions: `--share=network`, `--talk-name=org.freedesktop.secrets`, `--talk-name=org.gnome.OnlineAccounts`; autostart through the Background portal. Not on Flathub yet.

- Flathub: the submission (a manifest building offline from the release tag, with a generated `cargo-sources.json`) lives in its own `flathub/dev.aperte.Todav` repository and is written and opened by hand: Flathub accepts no AI-generated manifests or AI-opened submissions, and AI-assisted code in the app must be disclosed (Claude). Expect questions on the two `--talk-name` permissions (libsecret uses the Secret portal inside Flatpak) and on the project's age. The metainfo already has what Flathub asks for: screenshots from the release tag, release notes, brand colours.
- Fedora: `todav/linux/rpm/todav.spec` builds on COPR through `.copr/Makefile` (source tarball from the checkout, `cargo vendor` next to it), with the extension as the noarch `gnome-shell-extension-todav`. The official repositories would want Fedora's crates instead of vendored ones: all are packaged at our versions except `libsecret` (not packaged) and `rusqlite` (Fedora has 0.38, which the core accepts). SQLite is the system one (no `bundled` feature), as Fedora requires.
- extensions.gnome.org reviews reject code with clear signs of AI generation; the extension must be one its author can explain line by line.

**App ID** `dev.aperte.Todav`, extension UUID `todav@aperte.dev`, on the domain of Aperte, the developer (decided 2026-10-25; a domain ID does not depend on where the code is hosted). Todav 1.2 and older were `io.github.selfref.Todav` and `todav@selfref.dev`, with no migration (no other users yet): the data directory did not depend on it, a password saved in the keyring under the old schema is not found.

## GNOME Shell extension

A panel indicator with a read-only view of a list and a month calendar; clicking anything opens the app there. It holds no state and does no networking: everything goes over D-Bus to the Linux app, which means the extension is a few hundred lines of GJS with zero dependencies.

**Behaviour**

- Panel icon (the app's symbolic icon, shipped with the extension) with the open-task count of the shown list.
- Click → a dense two-column menu (or one column): tasks on the left, calendar on the right.
  - Tasks: the list's name heads the column; clicking it swaps the tasks for the lists to pick another. Tasks are grouped by category, subtasks indented; done ones keep the same circle with a check in it. Clicking a task opens it in the app (`ShowTask`: its list, the row pulsing in the accent colour). A field adds a task (`AddTask`, no category).
  - Calendar: a month to browse (a day opens it in the app), with dots in the colours of the calendars that have something that day (`GetDays`, as the sidebar month), then an agenda of the coming week, events and dated tasks under their days (`GetAgenda`; a row opens its day, `ShowDay`), and a field that opens the app's new-event dialog with the title typed (`NewEvent`). Dots and agenda are options, on by default.
  - "Open Todav" → `ShowList(list)`.
- Its options are app settings (`ext_*`, Preferences → Extension), synced between devices like the rest; the list shown (`ext_list`) too. The extension reads them with `GetSettings` each time the menu opens and sets only `ext_list`.
- Its settings button (Extensions app, `prefs.js`) opens them in the app: the app action `show-preferences` with the page (`extension`), through GApplication's own `org.freedesktop.Application.ActivateAction` so it carries an activation token and the app's window takes the focus; then its own window closes. The token is asked for once that window is active (a headless shell never makes it so: then after half a second, and the app only says it is ready). Without the app it says what is missing, with "Get Todav"; with an app that lacks the action, where the options are.
- Subscribes to the `Changed` signal and `SyncState` property; updates the count on change; shows a small warning glyph when `SyncState` starts with `error:`.
- If the app is not running, the first D-Bus call activates it (D-Bus activation); the menu shows "Starting…" for that second. The app exports its interface in `dbus_register` (an `adw::Application` subclass in `main.rs`), before it takes the bus name, so that activating call finds it. The app's window then asks for attention (the shell does not raise a window on a D-Bus call); the extension raises it.
- If the app is not installed (D-Bus knows no such service: `ServiceUnknown`), the menu says so and offers "Get Todav", opening the latest GitHub release.

**Implementation**

- `metadata.json`: `shell-version` 48–51; no GSettings schema.
- `Gio.DBusProxy.makeProxyWrapper(interfaceXml)` from the XML in the Linux section; copy the XML verbatim into `dbus.js`.
- The task list scrolls; at most 100 tasks, then "+ n more".
- Install: `make install` copies to `~/.local/share/gnome-shell/extensions/todav@aperte.dev` (`make link` for development); no extensions.gnome.org submission in v1.

**Non-goals**: ticking or editing tasks (read-only for now), reordering, categories.

## Android app

Kotlin + Jetpack Compose (Material 3) on top of the core and `todav-app` via UniFFI (`todav-ffi`). Push arrives through UnifiedPush (ntfy or another distributor); no Google services, no foreground service of its own. Released on Google Play and F-Droid (decided 2026-10-27).

**Dependencies**: AndroidX only (Compose Material 3, Navigation 3, WorkManager, Glance, Browser for Custom Tabs) and JNA (required by UniFFI); see the Dependency policy table. UnifiedPush is spoken directly instead of through its library. Later PebbleKit (vendored AAR, see Pebble section). No Room, Retrofit, OkHttp or Hilt; the core has storage and HTTP.

**Structure**

- `android/core`: Gradle library module. Its tasks, wired in through the variant API: `cargoNdk` builds `libtodav_ffi.so` for `todav.abis` (`make run` only x86\_64), `uniffiBindings` generates the Kotlin bindings from it (package `dev.aperte.todav.core`), `catalogs` compiles `po/` into `assets/locale/<language>/todav.mo`. JNA comes as an AAR; keep rules for JNA and the bindings go to the app.
- `android/app` (`dev.aperte.todav`, minSdk 29, target 37): `TodavApp` (the `Todav` object, the service started without the ntfy listener, change and sync-state flows, accounts connected with their saved passwords, the reminder alarm and widgets set again after changes), `Passwords` (the `Keyring`: AES-GCM with a key in the Android Keystore, ciphertext in preferences; backup off), `I18n` (`tr`/`ntr`/`ptr` over `Translations`, the app's own language when set: `locales_config.xml` lists `po/LINGUAS`), `Model` (`Main`, the view model: the back stack, what the screens show), `Background` (UnifiedPush, `SyncWorker`, reminders and their notifications, boot and time-zone receivers), `NextcloudApp` (the Nextcloud app's accounts, to fill in a sign-in), `Widget` (Glance), `MainActivity` (sync on resume, `tick` every minute in front; a notification's task or day, shared text, the "New Task" shortcut), `ui/`.
- Launcher icon: the Linux icon rendered as the adaptive icon's foreground (`app/icons.sh`), the brand's light colour behind it, the symbolic icon as the monochrome layer.

**UI** (decided 2026-10-27: native Material 3, matching the Linux app's features)

- Material 3 Expressive (decided 2026-10-27): its theme and spring motion, flexible top bars that collapse on scroll (the task list's with the open count under the title, Settings' large), one add button for what is shown (a task or an event; lists and calendars are added from the sidebar), flat at the end of the navigation bar to take less room, or floating in the bottom end corner beside a rail (as Material puts it), the morphing loading indicator for pull to refresh (the sync button's own icon just turns while a sync runs), and settings rows grouped in rounded sections as Android 16's settings. It needs material3 1.5, pinned ahead of the Compose BOM in `gradle/libs.versions.toml`; drop the pin once the BOM ships 1.5.

- Tasks and Calendar are switched by buttons, one shown at a time as on Linux (decided 2026-10-27: no longer side by side on wide screens): a navigation bar on phones, a navigation rail from 600dp, Material's medium width (unfolded foldables, which can be under 700dp as Galaxy Z Folds, tablets, phones on their side). The sidebar (task lists, or the calendars with the calendar shown) has the first button of either, a menu button, used often so in reach of a thumb: on phones it is a page over the whole width (Back or a list picked closes it), on wider screens a pane beside the view, shown at first and hidden by the button (each remembered apart, so turning a phone doesn't cover it). Switched, they slide as two pages of a pager that keeps both composed (and loaded), so either shows at once; it takes no swipes, which rows and months use.
- Calendar speed: what goes on each day or hour (`on_day`, `hours`, `segments`, `columns`, all calls into the core) is worked out with the items off the main thread, once per load, not per cell or per frame; the months or weeks either side of the one shown are made ready beforehand; list colours are read once for all pages. The sidebar holds Upcoming, Important and the task lists by account (long press: edit), and with the calendar shown the calendars and lists to show or hide; New List and Settings at its end.
- Screens are Navigation 3 entries over a back stack kept in the view model (a fold or rotation keeps it): Home, a task, an event, search, settings and their pages, an account, adding an account.
- Task list: the Linux app's groups (Pinned, then by category, as tags or not at all), subtasks under a "done/all" toggle, Completed apart; a category filter in the top bar, pull to refresh (only by a pull that starts with the list at its top and the bar expanded: a scroll reaching the top opens the bar instead). Rows: check, title, note line, due date (red when overdue), bell, linked event, a small star when it has a priority, and an options button: the row's menu, its priority as a row of stars on top (pinning is there too, not a button on the row as on Linux). Swipe right completes (or reopens), left deletes, both with Undo; tap opens its details sheet, the calendar's (pulled up, the task page); long press and drag moves it (above or below a row reorders, onto a row makes it a subtask, onto a category heading sets the category, as `todav_app::tasks` decides; while held, the rows between make room so a gap shows where it lands, indented to the depth it gets: beside a row at its depth, into a row right under it one level deeper (that row highlighted), and the dropped row settles into the gap while the move is saved; a heading it would take the category of is highlighted; the landing place is worked out as the finger moves, from the rows where they are drawn; beside a row whose order only Manual sort would keep, nothing opens and a note says to set Sort to Manual, `todav_app::tasks::needs_manual` (decided 2026-10-27: no switching the sort by dragging, it is one setting for all lists and devices)), long press alone opens its menu, the options button's (priority, Edit, Add Subtask, Pin, Make Top-Level Task, Move to List, Delete). The add button opens a bottom sheet, open from the start so it rises with the keyboard in one movement: title, due date, category (the one filtered to), important; into the list shown, from Upcoming or Important the first list.
- Editors (the task and event pages, alone or in a details sheet) have no buttons in their top bar: Save is a floating button the add button's size in the accent colour at the bottom end, Cancel a small one above it, both over the keyboard while it is up (`EditorButtons`).
- Task page (full screen, Save as one undo step): the Linux edit dialog's fields (title, list, category, priority, its event, note, start, due date with time, reminders as presets or a custom span), its subtasks (ticked and added at once), Delete.
- Calendar: Month (six weeks, as many chips a day as fit, the picked day's events and tasks underneath or beside it), Week (hour grid, overlapping blocks side by side, the top row for what takes no hours, the current time), Work Week, 3 Days, Day (see the calendar plan) and Agenda (60 days); months and weeks change with a swipe (the day it moves to is not highlighted as picked, as in the Linux app); Today, a view menu and the add button. Above the hours, a day's chips are too small to hit one by one, so the day is one target: its only item opens, or more show as large rows on a card growing out of the day over a dimmed screen (a popup over the whole window, placed by screen positions as it may start below the status bar), to pick one. An item opens a details sheet as the Linux popover (title edited in place, the calendar or list as its dot and name, Edit and Delete, then an event's tasks or a task's subtasks with a field adding one). The sheet is our own (`ui/Sheet.kt`: a dialog window drawing its dim, an `AnchoredDraggableState` with three rests): Material's rests half way or at its content's height, never both, and could not grow into the editor. It rests as tall as the details (raised over the keyboard), and pulled all the way up or by Edit… the same sheet becomes the editor (the task or event page without its window insets), one fading into the other with the pull; Cancel goes back to the details, Save and Delete close it, and unsaved edits hold it in place. A tap on an empty hour adds an event there. The event page has the Linux event dialog's fields and asks which occurrences a change or delete is for.
- A hardware keyboard has the Linux app's shortcuts (Ctrl+F search, Ctrl+Z undo, Ctrl+R or F5 sync, Ctrl+N a new task or event, Ctrl+T today, Ctrl+1 to 6 the calendar's views, Ctrl+, settings), listed in Android's shortcuts helper; saving a task or an event says so with Undo.
- Settings: General (style, language through the system's per-app setting, start page, notifications), Accounts (in the order they are dragged to by their handles, `reorder_accounts`; each account's page: on/off, name, connection, server status, Remove; "Sync settings with"; push status), Tasks, Calendar, About. The extension's options have no counterpart: each widget picks its list when placed.
- Not done: moving a task to another list by dragging it onto the sidebar (Move to List in its menu does it).

**Push and sync**

1. UnifiedPush (spec AND\_3) without the connector library: `Push` registers with the installed distributor (the one saved, else the first) with a token; `PushReceiver` takes `NEW_ENDPOINT` → `push_register(endpoint, a week)`, `MESSAGE` → `push_decode` (the Web Push message as Nextcloud encrypted it, for the core's keys) → `sync_topic` for each topic, in `goAsync()` under 10 s, acknowledged with `MESSAGE_ACK`; `UNREGISTERED` → `push_unregister()`. The distributor learns the caller by shared identity (Android 14) or the `pi` pending intent; `RaiseService` is the bindable service the spec asks for.
2. Safety net: `SyncWorker` (WorkManager) every 30 minutes on a network: sync, renew the push registration (the core skips calendars whose registration has more than a quarter of its time left), set the reminder alarm. In front: a sync on resume and `tick` every minute.
3. Reminders: an exact alarm (`USE_EXACT_ALARM`, allowed for calendar apps; `SCHEDULE_EXACT_ALARM` on Android 12) for `next_reminder`; when it goes off, a notification for each of `due_reminders` (tasks with "Mark as Done", which leaves open subtasks as on Linux; events opening the calendar on their day), then the next alarm. Set again after changes and syncs, at boot and when the clock or zone changes; `due_reminders` runs at start too, for reminders missed while the process was gone. Notifications are asked for once an account exists (Android 13).

**Accounts** (decided 2026-10-27; see below why not through the Nextcloud app): the welcome page and Settings → Add Account offer, when the Nextcloud app is installed, its accounts first. Android's account picker gives the one chosen as "user@host/path", all another app may read of it; that fills in the server and login name, and the browser sign-in starts at once with the name passed along (`user`, which Nextcloud's login page fills in; a login proxy in front of it, such as Authelia, ignores it). Then a server to sign in to (Login Flow v2 in a Custom Tab, or a login name + app password), then lists on this device only. Each account gets an app password of its own, wrapped with an Android Keystore key, the blob in `SharedPreferences` (`EncryptedSharedPreferences` is deprecated). Signing in again fills in the account's login name.

**Why not sign in through the Nextcloud app** (tried and dropped 2026-10-27): the Nextcloud Android app's Single Sign-On lets another app use one of its accounts without a password of its own, every request going through the app (`InputStreamBinder` in nextcloud/android, reached over AIDL). That would be the GOA counterpart, but it does not fit a CalDAV client:

- It forwards only GET, POST, PUT, DELETE, PATCH, HEAD, MKCOL and PROPFIND. Without REPORT there is no `sync-collection` (nor `calendar-multiget`): a sync has to list every item's ETag by PROPFIND and GET each changed one. Without MKCALENDAR and PROPPATCH lists cannot be created, renamed or recoloured, and MKCOL goes out without a body, so extended MKCOL is no way around it.
- PROPFIND always goes at depth 1, whatever the request asks.
- It appends a query string to every URL, an empty one ("…/calendar/?") at least. Nextcloud then takes the calendar URL for an object's, so the DAV Push registration (`POST <calendar>`) fails with 404 "Calendar object not found": no push for such accounts, only the periodic syncs.
- A successful response comes back without its status code, a failed one as a Java exception carrying it; text bodies of POST and PATCH go out as JSON unless sent as a stream.
- The official client library brings Retrofit, OkHttp, RxJava 2 and 3, Gson and AppCompat from JitPack; a client of our own meant copying its serialized request classes byte for byte.

It worked (a test transport limited the same way synced against the Test account, and the app on the emulator signed in, synced and uploaded), but slower syncs, no push and no list management made it worse than an app password of Todav's own. It would be worth another look if the Nextcloud app forwarded any method and the URL as given.

**Battery**: the distributor keeps the only persistent connection; this app wakes only on push, on the 30-minute worker and for reminders. Ask the user to exempt the distributor (not this app) from battery optimisation.

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

## Notav integration (planned)

[Notav](https://github.com/AperteDev/notav) is a sibling app for notes and a journal, with its own repository and architecture (its `docs/ARCHITECTURE.md`). The two apps keep separate configuration and credentials, and neither reads or writes the other's. Some of Notav's code started as copies of Todav's, listed there under "Code from Todav".

What Todav provides for it (not started):

- A note links a task as `todav://task/<uid>`. Todav handles the `todav` URI scheme (`x-scheme-handler/todav` in its desktop file) and opens the task as `ShowTask` does.
- Notav shows a linked task's title and state through a new D-Bus method, `GetTask` (UID in; summary, done and list out).
- Once Notav uses them, the scheme and these methods are a public interface: change them only in compatible ways.

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
- [x] Tasks and events alike: a task on the calendar opens a details popover as an event does (tick, dates, list · category, priority, its event, reminders, notes; a task's subtasks with a field adding one, a subtask names its task; Delete, Edit…), built from the same parts as the event's (`details_box`, `checklist`, `details_popover` in `calendar.rs`). A double click on either opens its edit dialog (`on_clicks`: the popover waits out the double-click time, as its grab would take the second click; events are plain buttons now, as tasks). "Edit on double click" (synced `double_click_edit`, Preferences → Calendar, on by default) turned off shows the popover at once instead. The popover's title is a `GtkEditableLabel` (for an event, only in a calendar that may be written to): a click edits it, Enter saves, Escape keeps the old one; a repeating event asks which occurrences. The event dialog has the task dialog's two-column layout (dates as expander rows with an inline calendar and time, one column under 560sp). `ShowDay` (the extension, a reminder) pulses the day in the month, week or agenda, as `ShowTask` pulses a task
- [x] More views with hours (2026-10-27, Linux and Android): Work Week (the week's days in synced `work_days`, one to six, Monday to Friday by default: `prefs::work_days`, `dates::days_of_week`; picked in Preferences → Calendar; a gear beside it in the view menu was dropped as clutter), 3 Days (from the selected day; arrows or a swipe move a day at a time, so any three days next to each other) and Day (its hours under its week as a month-grid row; a day there shows it). Ctrl+4/5/6, Ctrl+1/2/3 kept for Month, Week, Agenda. Linux: the week's hour code shows any list of days (`Cal::days`, `days::row` takes days rather than a week start); the header's view toggles became a menu (six did not fit), with the shortcuts beside the names. Android: Week and Work Week page by weeks; 3 Days and Day by days, a pager of one-day pages three or one to the screen, the hour labels outside it, the row above the hours as tall on every day shown (each page reports how many chips it holds), the hours scrolled without `verticalScroll`'s sideways clip so a held block shows over the next days

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
- [x] Sign-in through GNOME Online Accounts and with a login name + app password; synced client settings (`.config/todav/settings.json`, device-only keys, "Sync settings with server"); undo history (`Client::undo`, `Client::together`) with Ctrl+Z; create, rename, recolour and delete lists and calendars; search; keyboard shortcuts; About dialog from the metainfo, its Details page turned into Support (donation links); new app and symbolic icons
- [x] README and CONTRIBUTING
- [x] CI and releases (GitHub Actions): every push runs fmt, clippy `-D warnings` and the unit tests on Fedora 45 (GNOME 51), and builds the packages: the Flatpak bundle (Flathub's GNOME 51 builder image), a tarball of the system-wide tree (`make -C todav/linux stage`: app, data, `/etc/xdg/autostart`, the extension under `/usr/share/gnome-shell/extensions`) built on Fedora 43 so its glibc suits newer distributions, a `.deb` for Ubuntu 26.04+ repacked from that tree (`scripts/deb.sh`, dependencies by `dpkg-shlibdeps` plus libadwaita ≥ 1.8) and the extension zip. Lowest supported GNOME: 49 (libadwaita 1.8, `Adw.ShortcutsDialog`). The manual Release workflow builds them with the version set by `scripts/release.py`, then commits `chore(release): X`, tags `vX` and publishes a GitHub release with the changelog section as notes. The AUR has the app and the extension as separate packages, each from the release tag, the release assets (`-bin`) and `master` (`-git`); they are maintained outside this repository. One version for the workspace (`[workspace.package]`), the metainfo and the extension
- [x] Translations: gettext domain `todav` for the app (Rust `i18n.rs` over glibc, Blueprint `_()`), the extension (`gettext-domain`, catalogs in its `locale/`), the desktop file and the metainfo (`msgfmt --desktop/--xml`, rules in `po/its/`, so no AppStream package is needed to build). `scripts/i18n.sh` refreshes `po/todav.pot` and merges it; build.rs compiles the catalogs for a build run (and the translated metainfo for About), `make install-data` and the extension's `stage` for installs. Languages: Polish (reviewed), German, French, Spanish, Italian, Brazilian Portuguese (drafts). Weeks start on the locale's first weekday (`i18n::first_weekday`, glibc's `_NL_TIME_WEEK_1STDAY` + `_NL_TIME_FIRST_WEEKDAY`, as GTK's calendar)
- [ ] `docs/`, Flathub (generated `cargo-sources.json`, see Packaging), extensions.gnome.org
- [x] COPR spec for Fedora (`todav/linux/rpm/`, `.copr/Makefile`); metainfo ready for Flathub (screenshots, release notes from the changelog, branding); system SQLite
- [x] `todav-app`: the Linux app's logic that is not GTK moved into a crate of its own for every front-end (settings with defaults, dates, task trees and drops, calendar items, layout and drops, accounts over a keyring trait, the background service); its unit tests moved with it
- [x] `todav-ffi`: the core and `todav-app` exported with UniFFI (a workspace of its own, bundled SQLite), Kotlin and C# bindings generated and smoke-tested against the Linux build by `scripts/bindings.sh`, in CI too
- [x] Android groundwork: Gradle project building the core with cargo-ndk, bindings and catalogs; the shared SDK and emulator (`sdk.sh`, Makefile, Zed tasks); welcome page (local lists, or Nextcloud by browser or app password), the drawer with Upcoming, Important and the lists by account, tasks by category with subtasks, add, tick, swipe to delete with Undo, sync on resume and every minute in front. Tried on the Android 15 emulator
- [x] Android, most of M6 (2026-10-27): Tasks | Calendar by a navigation bar or rail, the sidebar a page or a pane; task page, add sheet, swipes, drag and drop, stars, pins, categories, search; month, week and agenda with details sheets and the event page; settings and accounts; UnifiedPush, the 30-minute worker, exact-alarm reminders with "Mark as Done", the widget, share target and "New Task" shortcut. Tried on the Android 15 emulator against the Test account (reminders, push status, drops, foldable widths)
- [ ] M6 Android leftovers (see [Android app](#android-app)), push delivery through a real distributor → M7 Pebble, then Windows

## Roadmap

Public in the README as planned work; order not fixed.

- [x] **Multiple accounts**: several servers or users side by side, the lists on this device an account of its own; each can be switched off, settings sync through the one chosen (see [Accounts](#accounts)). Not done: moving a whole list between accounts.
- [x] **Offline mode**: use Todav without a server, or keep some lists on the device next to an account's (the local account, see [Local lists](#local-lists)).
- [ ] **More services** besides Nextcloud: other CalDAV servers first (Radicale, Baïkal, Fastmail, iCloud…), which mostly means login without Login Flow v2, settings/categories without Nextcloud Files, and polling where there is no WebDAV-Push. Each would be another `AccountKind`.
- [ ] **Android app**: M6 above.
- [ ] **Pebble app**: M7 above, fed by the Android app (see [Pebble watchapp + companion](#pebble-watchapp--companion)).
- [ ] **Windows app**: a native WinUI 3 front-end over the same Rust core, with what the Linux app does in the background: push over ntfy, reminder notifications, starting with the session (see [Windows app](#windows-app-planned)).
- [x] **Nested subtasks**: any depth, as `RELATED-TO` allows. Rows indent per level inside the top-level task's expander (no expander per level); every subtask has a "+" for adding to it and taking dropped tasks; the calendar's task popover shows a subtask's task and its own subtasks.
- [ ] **Advanced notifications**: configurable, beyond the reminder notifications the background service shows now; details to be specified.
- [x] **Starred tasks**: as Nextcloud Tasks does it, starred is high priority (1–4), no property of its own. Each task row has a star coloured by its priority (red high, yellow medium, blue low, outlined without one) whose menu sets the level as the edit dialog does (1, 5, 9 or none); the Important page lists the open high-priority tasks of every list by list (`Client::important`); "Priority, highest first" is a sort for tasks and subtasks. Pinning is Nextcloud's `X-PINNED:true` (`Task::pinned`, a `pinned` column): pinned top-level tasks open the list in a "Pinned" group, in the list's overall order and filtered by the category filter; dropping a task into it pins it, dragging one out unpins it (and sets the category of the group it lands in). Synced "Stars" (`show_stars`) and "Pins" (`show_pins`) in Preferences → Tasks, on by default, hide the stars with the Important page, and the pins with the Pinned group (X-PINNED is left as it is).
- [ ] **Advanced search, filtering and sorting**: details to be specified.

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

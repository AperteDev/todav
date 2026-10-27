# Agent notes

## Roadmap

- `docs/ROADMAP.md` holds the planned work only (features, open questions, what was ruled out);
  what is built is described by the code. Read it before working on a planned feature, and
  update it in the same change: remove what is done, add a plan or an open question, move a
  dropped plan under "Ruled out" with why.
- Notav (`AperteDev/notav`, cloned next to this repository) started from copies of some of this
  code, listed under "Code from Todav" in its `docs/ARCHITECTURE.md`. A fix to one of those
  probably applies there too: port it or say so. Notav keeps its own configuration and
  credentials; never read or write its files, keyring entries or settings.

## Shared code

- Logic a front-end needs that is not about widgets goes in `todav/app` (`todav-app`), not in a
  front-end: settings with their defaults, date rules, task and calendar rules, the background
  service. It returns no text (enums, times, ids); each front-end words and formats it.
  Android and Windows use it through `todav/ffi` (`todav-ffi`, a Cargo workspace of its own),
  so keep its API exportable: synchronous, no closures or generics in what front-ends call.
- `todav/ffi` exports the core and `todav-app` with UniFFI (Kotlin, C#). Add what a new
  front-end feature needs there too; after changing it or the types it mirrors, run
  `scripts/bindings.sh` (it generates both bindings and runs their smoke tests; CI does too),
  plus `cargo fmt` and clippy in `todav/ffi`, which the root workspace's checks don't cover.

## Releases

- Add user-visible changes to `docs/CHANGELOG.md` under `## [Unreleased]`, in the same commit as the
  change. It follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/): `Added`,
  `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`; version links at the end.
- Releases are cut by the manual Release workflow (`.github/workflows/release.yml`): with the
  version given, `scripts/release.py` sets it in `Cargo.toml`, the metainfo, the extension's
  `metadata.json` and the RPM spec and moves the `[Unreleased]` changes and link under it; the
  metainfo gets those changes as the release's notes (untranslated) and its screenshots from
  the new tag. CI builds the packages, then it commits `chore(release): X`, tags `vX` and
  publishes them. Don't bump versions by hand.
- Packaging for stores: the metainfo must pass `appstreamcli validate` without warnings (Flathub
  treats them as errors), and links in it must answer automated requests (Ko-fi's does not).
- CI (`.github/workflows/ci.yml`) runs on every push: `cargo fmt --check`, clippy with
  warnings as errors and the unit tests on Fedora (GNOME 51), the bindings
  (`scripts/bindings.sh`), the Android debug APK (`assembleDebug`, both ABIs), plus the packages: the Flatpak
  bundle, a tarball of the system-wide tree (`make -C todav/linux stage`, built on the oldest
  supported GNOME, now Fedora 43), the `.deb` made from it on Ubuntu 26.04 (`scripts/deb.sh`)
  and the extension zip. Run the checks before committing.
- The lowest supported GNOME is 49 (libadwaita 1.8, for `Adw.ShortcutsDialog`). A newer
  widget raises it: update the README, `scripts/deb.sh`'s libadwaita version and the
  tarball's build distribution together.

## Translations

- All text a user sees is translatable, in the app, its UI files and the extension; one gettext
  domain, `todav`, with catalogs in `po/` (languages in `po/LINGUAS`).
- Rust: `gettext("…")`, `ngettext("{n} task", "{n} tasks", n)` for anything counted (Polish has
  three plural forms), `pgettext("context", "…")` for a short word that could translate two
  ways; text in a `const` is marked `gettext_noop` and translated where it is shown. Fill
  placeholders by name (`{name}`) with `replace`; never build a sentence from translated pieces
  (other languages change word order and case), write each variant whole. Blueprint:
  `_("…")` / `C_("context", "…")`. Extension: `_()`, `ngettext()`, `pgettext()`.
- Android and Windows translate with `Translations` from `todav/ffi` (`gettext`, `ngettext`,
  `pgettext` over the same `.mo` catalogs), not `strings.xml` or `.resw`; the same rules apply.
- Dates and times come from the locale (LC_TIME) through GLib formats; don't translate them.
- After adding or changing text, run `scripts/i18n.sh` and commit the refreshed `po/` with the
  change. Translate new strings into Polish (reviewed by the user); add the other languages too
  when you can, as unreviewed drafts. Every catalog must compile (`msgfmt --check`, done by the
  build).

## Settings

- Client settings are synced between devices by default. Any new setting syncs unless it only
  makes sense on one device (server URLs, UI state such as the last opened list); those go in
  `DEVICE_ONLY` in `todav/core/src/settings.rs`.
- Store settings with `Client::set_setting` / `Client::setting`. `set_setting` records the change
  time; when syncing, the newer value wins implicitly per key (the server wins ties).
- Synced settings live in Nextcloud Files at `.config/todav/settings.json` of the account picked
  for them (device-only `settings_account`, `Client::set_settings_account`), merged on every sync
  of that account.
- The GNOME extension keeps no settings of its own: its options are app settings `ext_*`
  (`EXTENSION` in `todav/linux/app/src/settings.rs`, Preferences → Extension), read over D-Bus
  (`GetSettings`; it may set only those keys). Its settings button opens that page in the app.
- The user picks that account, or none, with "Sync settings with" (Preferences → Accounts); the
  welcome page sets it to the account added there unless its switch is turned off. The ntfy
  server (`ntfy_url`) syncs with them unless "Sync ntfy server" (`sync_ntfy`) is off.

## Accounts

- Lists belong to accounts (`accounts` table): Nextcloud ones and the local one, at most one.
  Anything per server (connection, push registration, category file, last sync) is per account;
  per-account `kv` keys are `a<id>_…` (`account::key`). Lists are keyed by their full URL.

## Undo

- The core records what each local change to a task or event replaced, for `Client::undo`
  (in memory, the last 100 steps). A user action made of several core calls (add, then update)
  goes inside `Client::together`, so one undo reverts all of it.

## Linux UI

- Static views are Blueprint files in `todav/linux/app/ui/`, compiled by `build.rs`; dynamic UI
  (task rows, sidebar rows) stays in Rust code.

## Android

- `todav/android` is a Gradle project: `core` (todav-ffi built by cargo-ndk, its Kotlin bindings,
  the `.mo` catalogs as assets) and `app` (Compose). Build and run it with
  `make -C todav/android <target>` (Zed tasks "android: …"): `setup`, `run`, `logs`, `build`,
  `stop`, `wipe`.
- One Android SDK for every project on the machine: `~/Android/Sdk` (or `$ANDROID_HOME`), set up
  only by `todav/android/sdk.sh setup` (Google's command-line tools, then `sdkmanager`), never
  with AUR or other system SDK packages. It installs exactly the versions in
  `todav/android/gradle.properties` (`todav.platform`, `todav.buildTools`, `todav.ndk`,
  `todav.image`), which the build scripts read too. Change a version there only, then remove the
  old package (`sdkmanager --uninstall …`) so copies don't pile up; the SDK is several GB.
- The emulator is the one for developing apps (Todav; Notav later too), on a stock image: Android
  17 with Google APIs (Chrome, `adb root`), one no project that modifies the system uses. Noogle
  patches its images (Magisk) and keeps its own AVDs and image packages; leave those alone, even
  when `todav.image` changes. Android 15's images don't fold a foldable AVD (no device states).
- System packages it needs (sdk.sh names the missing ones, it installs none): `rustup` (stable;
  sdk.sh adds the Android targets), `cargo-ndk`, `android-tools` (adb: the SDK's adb is a link to
  it, as two adb versions drop the emulator), `jdk21-openjdk` (Gradle runs on JDK 21 through
  `JAVA_HOME`), `gettext`, `unzip`; `/dev/kvm` writable.
- The emulator is the `apps` AVD on `emulator-5580`, next to Noogle's (5554 and up): a foldable,
  the Pixel 9 Pro Fold profile with the Honor Magic V5's screens, so one AVD tries both layouts.
  Open: 2172×2352 px, 827 dp wide (the navigation rail and the sidebar pane); `make fold`: its
  cover screen, 1060×2376 px, 404 dp wide (a phone: the navigation bar); `make unfold` opens it.
  It stays on and unlocked (no screen timeout, no lock screen) and the app stays open across a
  fold; `sdk.sh start` sets that up. A change whose layout differs by width (bars, panes, sheets,
  dialogs, the calendar's grids) is checked both folded and open; anything else on one is enough.
  Drive it with adb: `adb shell input tap X Y` (pixels of the screen in use), `adb shell input
  text …`, `adb shell input keyevent 66` (Enter); `make logs` for the app's log. Screenshots
  need the display: `adb exec-out screencap -d ID -p > shot.png`, ID from `adb shell dumpsys
  SurfaceFlinger --display-id` (the first open, the second folded); without `-d` a warning lands
  in the PNG. Its dialog about the computer's RAM is advisory.

## Testing the Linux app

- The app is single-instance over D-Bus (`dev.aperte.Todav`): a new launch only shows the
  running window. Quit the running copy first (`pkill -x todav-linux`).
- `make -C todav/linux sandbox` (Zed task "linux: run in sandbox") runs a separate copy next to the
  normal one (`TODAV_SANDBOX`): own data in `target/sandbox`, own keyring entries, same GNOME
  Online Accounts. Use it to test other connections; `sandbox-reset` wipes it. Combine it with
  Broadway by hand: `TODAV_SANDBOX=1 XDG_DATA_HOME=$PWD/target/sandbox` plus the variables below.
- To see and drive the app yourself, run it on GTK's Broadway backend and use a browser automation
  tool (Playwright) on `http://127.0.0.1:8085`: screenshots, clicks and typing work, with the
  user's real data and keyring. `make -C todav/linux broadway` (Zed task "linux: run in browser
  (Broadway)") quits the running copy, starts `broadwayd`, the app and a Chromium app window;
  to drive it from Playwright instead, put a no-op `chromium` first in `PATH`. Ctrl+Q quits the
  app (closing the window only hides it), and `broadwayd` stops with it. Don't restart the
  normal app afterwards; the user does that.
- Broadway limits (GTK 4.24), worked around in the make target where possible:
  - Drag and drop aborts the app (`gdk_drag_set_property` assertion). Ask the user to test DnD
    in the real session.
  - A browser that connects before the app's first frame aborts `broadwayd`
    (`append_node_ops: assertion failed`); the target opens the page 2 s after the app.
  - Animations, e.g. expanding a task row, abort it the same way; the target sets
    `gtk-enable-animations=false` through `XDG_CONFIG_HOME=target/broadway`.
  - Closing a popover (e.g. picking a ComboRow entry), the sync spinner and sometimes
    scrolling abort it the same way: wait for a sync to finish before interacting. Use it for quick looks; to change a setting, write it to the stopped app's
    database (`kv`: `setting_<key>` plus `settime_<key>` = now) instead of using Preferences.
  - After a while some icons are no longer drawn; cosmetic, Broadway only.
- For drag and drop, popovers and animations, use a headless nested GNOME Shell instead: copy
  `todav/gnome-extension/run-nested.sh` with another `display`, `gnome-shell --headless
  --unsafe-mode --virtual-monitor WxH` instead of `--devkit`, and absolute paths. Drive it through
  `org.gnome.Shell.Eval` on its bus: a Clutter virtual pointer/keyboard
  (`seat.create_virtual_device`) for input (turn hot corners off first: the pointer starts at
  0,0), `org.gnome.Shell.Screenshot` for screenshots. Give the app its own `XDG_DATA_HOME` (a copy
  of the database made with `sqlite3 .backup`) so the migration and test edits stay off the real
  one.
- In the real GNOME Wayland session, xdotool and synthetic uinput input do not reach the window,
  and screenshots of it are not available.

# Agent notes

## Architecture

- `docs/ARCHITECTURE.md` holds the architecture, the design decisions and the plan (progress,
  roadmap, open questions). Read it before working on a feature, and update it in the same
  change when the work changes what it says: a decision made, a step done, a plan dropped.
- Notav (`SelfRef/notav`, cloned next to this repository) started from copies of some of this
  code, listed in the Notav section of `docs/ARCHITECTURE.md`. A fix to one of those probably
  applies there too: port it or say so. Notav keeps its own configuration and credentials;
  never read or write its files, keyring entries or settings.

## Releases

- Add user-visible changes to `CHANGELOG.md` under `## [Unreleased]`, in the same commit as the
  change. It follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/): `Added`,
  `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`; version links at the end.
- Releases are cut by the manual Release workflow (`.github/workflows/release.yml`): with the
  version given, `scripts/release.py` sets it in `Cargo.toml`, the metainfo and the extension's
  `metadata.json` and moves the `[Unreleased]` changes and link under it; CI builds the
  packages, then it commits `chore(release): X`, tags `vX` and publishes them. Don't bump
  versions by hand.
- CI (`.github/workflows/ci.yml`) runs on every push: `cargo fmt --check`, clippy with
  warnings as errors and the unit tests on Fedora (GNOME 51), plus the packages: the Flatpak
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
- Synced settings live in Nextcloud Files at `.config/todav/settings.json`, merged on every sync.
- The GNOME extension keeps no settings of its own: its options are app settings `ext_*`
  (`EXTENSION` in `todav/linux/app/src/settings.rs`, Preferences → Extension), read over D-Bus
  (`GetSettings`; it may set only those keys).
- The user can turn syncing off with "Sync settings with server" (Preferences → Accounts),
  stored as the device-only `sync_settings` setting; on by default. The ntfy server
  (`ntfy_url`) syncs with them unless "Sync ntfy server" (`sync_ntfy`) is off.

## Undo

- The core records what each local change to a task or event replaced, for `Client::undo`
  (in memory, the last 100 steps). A user action made of several core calls (add, then update)
  goes inside `Client::together`, so one undo reverts all of it.

## Linux UI

- Static views are Blueprint files in `todav/linux/app/ui/`, compiled by `build.rs`; dynamic UI
  (task rows, sidebar rows) stays in Rust code.

## Testing the Linux app

- The app is single-instance over D-Bus (`io.github.selfref.Todav`): a new launch only shows the
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

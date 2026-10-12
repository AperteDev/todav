# Agent notes

## Settings

- Client settings are synced between devices by default. Any new setting syncs unless it only
  makes sense on one device (server URLs, UI state such as the last opened list); those go in
  `DEVICE_ONLY` in `core/src/settings.rs`.
- Store settings with `Client::set_setting` / `Client::setting`. `set_setting` records the change
  time; when syncing, the newer value wins implicitly per key (the server wins ties).
- Synced settings live in Nextcloud Files at `.config/todav/settings.json`, merged on every sync.
- The user can turn syncing off with "Sync settings with server" (Preferences → Connection),
  stored as the device-only `sync_settings` setting; on by default.

## Linux UI

- Static views are Blueprint files in `linux/app/ui/`, compiled by `build.rs`; dynamic UI
  (task rows, sidebar rows) stays in Rust code.

## Testing the Linux app

- The app is single-instance over D-Bus (`io.github.selfref.Todav`): a new launch only shows the
  running window. Quit the running copy first (`pkill -x todav-linux`).
- To see and drive the app yourself, run it on GTK's Broadway backend and use a browser automation
  tool (Playwright) on `http://127.0.0.1:8085`: screenshots, clicks and typing work, with the
  user's real data and keyring. `make -C linux broadway` (Zed task "linux: run in browser
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
  - After a while some icons are no longer drawn; cosmetic, Broadway only.
- In the real GNOME Wayland session, xdotool and synthetic uinput input do not reach the window,
  and screenshots of it are not available.

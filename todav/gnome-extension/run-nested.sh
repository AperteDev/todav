#!/bin/sh
# Runs a nested GNOME Shell on its own session bus, with the extension enabled and
# the workspace build of the app as its D-Bus backend. Expects `make link` and
# `cargo build -p todav-linux` to have been run.
set -eu

uuid=todav@aperte.dev
app=$(dirname "$0")/../../target/debug/todav-linux
display=todav-nested

if [ -z "${DBUS_RUN_SESSION:-}" ]; then
  # Own bus: the host's app instance and installed D-Bus service stay out of the way.
  HOST_BUS=$DBUS_SESSION_BUS_ADDRESS DBUS_RUN_SESSION=1 exec dbus-run-session -- "$0" "$@"
fi

# The keyring stays on the host bus; without this the app waits 25 s for it, then asks to sign in.
# Exits with the bus.
python3 "$(dirname "$0")/secrets-forward.py" "$HOST_BUS" &
gdbus wait --session --timeout 5 org.freedesktop.secrets

# Settings in memory: a fresh GNOME (default theme, no other extension enabled) each run,
# nothing written to the user's dconf. The app skips the portal, which reads the user's settings.
export GSETTINGS_BACKEND=memory GDK_DEBUG=no-portals

# The normal session minus the welcome tour, which would come back every run without settings.
modes=$(realpath "$(dirname "$0")/../..")/target/nested
mkdir -p "$modes/gnome-shell/modes"
echo '{"parentMode": "user", "showWelcomeDialog": false}' > "$modes/gnome-shell/modes/nested.json"

# A socket left by a run that was killed would pass the wait below before the shell listens.
if pgrep -f "[w]ayland-display=$display" >/dev/null; then
  echo "A nested shell on $display is already running" >&2
  exit 1
fi
rm -f "$XDG_RUNTIME_DIR/$display" "$XDG_RUNTIME_DIR/$display.lock"

MUTTER_DEBUG_DUMMY_MODE_SPECS=${MUTTER_DEBUG_DUMMY_MODE_SPECS:-1600x900} \
SHELL_DEBUG=${SHELL_DEBUG:-all} \
XDG_DATA_DIRS=$modes:${XDG_DATA_DIRS:-/usr/local/share:/usr/share} \
  gnome-shell --devkit --unsafe-mode --wayland --wayland-display=$display --mode=nested &
shell=$!

while [ ! -S "$XDG_RUNTIME_DIR/$display" ]; do
  kill -0 $shell 2>/dev/null || exit 1
  sleep 0.2
done

# TODAV_WINDOW=1 opens the app's window too; otherwise it only runs as the background service.
args=--gapplication-service
[ -n "${TODAV_WINDOW:-}" ] && args=
# No accessibility: its registry is a systemd unit, which this bus cannot start. Wayland only:
# GTK would otherwise fall back to the host's X display when the nested shell is not up.
WAYLAND_DISPLAY=$display GDK_BACKEND=wayland GTK_A11Y=none "$app" $args &
app_pid=$!

# The extension asks the app for lists right away: enable it once the app's interface is up.
# Waiting for the name first: introspecting a free name would D-Bus-activate an installed copy.
gdbus wait --session --timeout 10 dev.aperte.Todav || true
until gdbus introspect --session --dest dev.aperte.Todav \
  --object-path /dev/aperte/Todav 2>/dev/null | grep -q Todav1; do
  kill -0 $app_pid 2>/dev/null || break
  sleep 0.1
done
gnome-extensions enable $uuid || true

# The app's window maximized rather than the overview the shell starts in (Eval needs
# --unsafe-mode above; only this nested session's own bus can reach it).
if [ -n "${TODAV_WINDOW:-}" ]; then
  show='const w = global.get_window_actors().map(a => a.meta_window)
    .find(w => w.get_wm_class()?.startsWith("dev.aperte.Todav"));
  if (w) {
    Main.overview.hide();
    try { w.maximize(); } catch { w.maximize(imports.gi.Meta.MaximizeFlags.BOTH); }
  }
  !!w'
  for _ in $(seq 50); do
    gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
      --method org.gnome.Shell.Eval "$show" 2>/dev/null | grep -q "'true'" && break
    kill -0 $app_pid 2>/dev/null || break
    sleep 0.2
  done
fi

wait $shell || true
kill $app_pid 2>/dev/null || true

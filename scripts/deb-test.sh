#!/bin/sh
# Start the installed Todav (the .deb) and open its Keyboard Shortcuts, then check that it is
# still running and logged no critical error. Run it where the package's own
# dependencies are the newest libraries available: on Debian 13, whose libadwaita 1.7 has no
# Adw.ShortcutsDialog, which makes the app use GTK's shortcuts window. Needs a session bus and a
# display (`xvfb-run -a dbus-run-session -- scripts/deb-test.sh`) and gdbus.
set -eu

home=$(mktemp -d)
log=$home/log
pid=
cleanup() {
  [ -z "$pid" ] || kill "$pid" 2>/dev/null || :
  rm -rf "$home"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; sed 's/^/  | /' "$log" >&2; exit 1; }

# A home of its own: no data, settings or keyring of the user running it.
export HOME=$home XDG_DATA_HOME=$home/data XDG_CONFIG_HOME=$home/config XDG_CACHE_HOME=$home/cache
todav-linux > "$log" 2>&1 &
pid=$!

gdbus wait --session --timeout 60 dev.aperte.Todav || fail "the app did not take its D-Bus name"
sleep 2 # the window opens
activate() {
  gdbus call --session --dest dev.aperte.Todav --object-path /dev/aperte/Todav \
    --method org.gtk.Actions.Activate "$1" '[]' '{}' > /dev/null
}
activate shortcuts || fail "the shortcuts action failed"
sleep 2
kill -0 "$pid" 2>/dev/null || fail "the app stopped when the shortcuts opened"
! grep -qE 'CRITICAL|panicked' "$log" || fail "the app logged an error"
echo "ok: the shortcuts opened"

activate quit || fail "the quit action failed"
i=0
while kill -0 "$pid" 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
! kill -0 "$pid" 2>/dev/null || fail "the app did not quit"
wait "$pid" 2>/dev/null || :
pid=
echo "ok: quit"

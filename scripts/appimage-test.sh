#!/bin/sh
# Run the AppImage on this system and check that it comes up with its own libraries, not the
# system's, and with its translations. Needs a session bus and a display (`xvfb-run -a
# dbus-run-session -- scripts/appimage-test.sh <AppImage>`), gdbus and the pl_PL.UTF-8 locale.
# Run it on a system without Todav's GTK and libadwaita, such as Ubuntu 24.04 (Linux Mint 22).
#
#   scripts/appimage-test.sh <AppImage>
set -eu

appimage=$(realpath "$1")
home=$(mktemp -d)
log=$home/log
pid=
cleanup() {
  [ -z "$pid" ] || kill "$pid" 2>/dev/null || :
  rm -rf "$home"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; sed 's/^/  | /' "$log" >&2; exit 1; }

# A home of its own: no data, settings or keyring of the user running it. No FUSE needed (a
# container has none): the AppImage unpacks itself.
export HOME=$home XDG_DATA_HOME=$home/data XDG_CONFIG_HOME=$home/config XDG_CACHE_HOME=$home/cache
export APPIMAGE_EXTRACT_AND_RUN=1 LC_ALL=pl_PL.UTF-8 LANGUAGE=pl
"$appimage" > "$log" 2>&1 &
pid=$!

gdbus wait --session --timeout 60 dev.aperte.Todav || fail "the app did not take its D-Bus name"
gdbus call --session --dest dev.aperte.Todav --object-path /dev/aperte/Todav \
  --method dev.aperte.Todav1.GetSettings > /dev/null || fail "GetSettings failed"
echo "ok: the app is running and answers on D-Bus"

# The app's process: the one mapping the binary the AppImage keeps in shared/bin.
sleep 2 # the window opens and looks its text up
owner=$(grep -l '/shared/bin/todav-linux' /proc/[0-9]*/maps 2>/dev/null | head -n 1 | cut -d/ -f3)
[ -n "$owner" ] || fail "the app's process is not found"
maps=$(cat "/proc/$owner/maps")

# The libraries come from the AppImage's own directory, whatever the system has installed.
for lib in libgtk-4 libadwaita-1 libglib-2.0 libgio-2.0 libsecret-1 libsqlite3 libc.so ld-linux; do
  path=$(echo "$maps" | awk -v lib="$lib" 'index($0, lib) { print $6; exit }')
  [ -n "$path" ] || fail "$lib is not loaded"
  case $path in
    /usr/* | /lib/* | /lib64/* | /opt/*) fail "$lib comes from the system: $path" ;;
  esac
  echo "ok: $lib from $path"
done

# glibc maps a catalog when text is looked up in it: the Polish one is read from the AppImage.
echo "$maps" | grep -q '/locale/pl/LC_MESSAGES/todav\.mo' || fail "the Polish catalog is not used"
echo "ok: the Polish catalog is used"

# Quit through the app's own action, as the Makefile does.
gdbus call --session --dest dev.aperte.Todav --object-path /dev/aperte/Todav \
  --method org.gtk.Actions.Activate quit '[]' '{}' > /dev/null
i=0
while kill -0 "$pid" 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
! kill -0 "$pid" 2>/dev/null || fail "the app did not quit"
wait "$pid" 2>/dev/null || :
pid=
echo "ok: quit"

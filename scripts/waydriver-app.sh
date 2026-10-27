#!/bin/bash
# The sandbox copy of the app (as `make -C todav/linux sandbox`) for a waydriver-mcp session: give
# its path as start_session's command, with app_name "todav-waydriver". waydriver runs the app on
# the host's session bus (the keyring works) in a headless Mutter of its own; the sandbox copy is
# not unique there, so it runs beside the normal app, with its own data in target/sandbox.
#
# waydriver finds the app in the accessibility tree by name, on the host's bus: under its own
# name ("todav-waydriver", from argv[0]) a copy on the desktop is never taken for it.
# Build it first (`cargo build -p todav-linux`).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
export TODAV_SANDBOX=1 XDG_DATA_HOME="$root/target/sandbox"
exec -a todav-waydriver "$root/target/debug/todav-linux" "$@"

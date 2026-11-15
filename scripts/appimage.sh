#!/bin/sh
# Package the app installed in a staged tree (`make -C todav/linux install-data PREFIX=/usr
# DESTDIR=<tree>`) as an AppImage. Run it on Arch Linux, with the app's libraries installed there:
# quick-sharun (Anylinux-AppImages) copies the libraries the binary links to, and glibc with
# them, so the AppImage runs on older distributions than the one it was built on (Linux Mint 22,
# Ubuntu 24.04) and needs nothing from the system but the kernel. Needs patchelf and the tools
# of base-devel.
#
#   scripts/appimage.sh <staged tree> <version> <out dir>
set -eu

stage=$1 version=$2 out=$3
arch=$(uname -m)
id=dev.aperte.Todav

# quick-sharun, at a commit, so a release builds the same each time; bump it with its sha256.
quick_sharun=f7e9cb26818bdeb3d64a0fe055fdd51cbc887a81
quick_sharun_sha=0aea2f589658e5db1659a6ccb8d29546c4e03028f1e29b37c6450cece917249a
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/quick-sharun" \
  "https://raw.githubusercontent.com/pkgforge-dev/Anylinux-AppImages/$quick_sharun/useful-tools/quick-sharun.sh"
echo "$quick_sharun_sha  $work/quick-sharun" | sha256sum -c -
chmod +x "$work/quick-sharun"

# Started from the menu or a launcher by its own path, not through D-Bus: no service file comes
# with the AppImage (no DBusActivatable). It runs where the user puts it and starts nothing by
# itself; the app keeps running in the background once opened.
sed "/^DBusActivatable=/d; \$aStartupWMClass=$id" "$stage/usr/share/applications/$id.desktop" > "$work/$id.desktop"

# quick-sharun's temporary files go in work/, and its tools stay out of the build tree.
export TMPDIR=$work APPDIR=$work/AppDir OUTPATH=$out OUTNAME=todav-$version-$arch.AppImage
# The icon is the PNG: it becomes the AppImage's .DirIcon, which file managers and launchers show
# for the file, and many of them do not draw an SVG there.
export DESKTOP=$work/$id.desktop ICON=$(realpath "$(dirname "$0")/../todav/linux/data/$id.png")
# No strace pass: it runs the app to find what it loads at run time, which GTK's modules and the
# host's graphics drivers (USE_HOST_DRIVERS) do not need told.
export STRACE_MODE=0 USE_HOST_DRIVERS=1
export ARCH=$arch VERSION=$version
"$work/quick-sharun" "$stage/usr/bin/todav-linux"

# The catalogs: the app reads ../share/locale from its binary's directory, which the AppImage
# keeps in shared/ (the other data is in AppDir/share, where the libraries look for it).
mkdir -p "$APPDIR/share"
cp -a "$stage/usr/share/locale" "$APPDIR/share/"
ln -s ../share "$APPDIR/shared/share"
mkdir -p "$APPDIR/share/metainfo"
cp "$stage/usr/share/metainfo/$id.metainfo.xml" "$APPDIR/share/metainfo/"
# The icons by name, for the app itself (the About dialog, notifications).
cp -a "$stage/usr/share/icons/hicolor" "$APPDIR/share/icons/"
[ "$(head -c 4 "$APPDIR/.DirIcon" | tail -c 3)" = PNG ] || { echo ".DirIcon is not a PNG" >&2; exit 1; }

"$work/quick-sharun" --make-appimage
rm -f "$out/appinfo"

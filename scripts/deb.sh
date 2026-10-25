#!/bin/sh
# Package a staged tree (`make -C todav/linux stage`, the tarball's content) as a .deb. Run it on
# the Ubuntu release the package is for: dpkg-shlibdeps takes the dependencies from the libraries
# the binary links to, as that release names them.
#
#   scripts/deb.sh <staged tree> <version> <out dir>
set -eu

stage=$1 version=$2 out=$3
arch=$(dpkg --print-architecture)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cp -a "$stage" "$work/root"
# dpkg-shlibdeps reads a source package's control file from the current directory.
mkdir -p "$work/src/debian"
printf 'Source: todav\n\nPackage: todav\nArchitecture: any\n' > "$work/src/debian/control"
# libadwaita's version comes from the widgets the UI files use (Adw.ShortcutsDialog), which
# linking does not show: below, in place of the one found.
depends=$(cd "$work/src" && dpkg-shlibdeps -O "$work/root/usr/bin/todav-linux" |
  sed -n 's/^shlibs:Depends=//p' | sed 's/libadwaita-1-0 ([^)]*), *//; s/, *libadwaita-1-0 ([^)]*)//')

mkdir -p "$work/root/DEBIAN"
cat > "$work/root/DEBIAN/control" <<EOF
Package: todav
Version: $version
Architecture: $arch
Maintainer: SelfRef <git@selfref.dev>
Installed-Size: $(du -sk "$work/root" | cut -f1)
Depends: $depends, libadwaita-1-0 (>= 1.8)
Recommends: gnome-keyring
Suggests: gnome-shell
Section: gnome
Priority: optional
Homepage: https://github.com/AperteDev/todav
Description: Tasks and calendar for GNOME, kept in your Nextcloud
 Todav is a task list and calendar for the GNOME desktop. Tasks and events
 stay in your Nextcloud as plain CalDAV; Todav works from a local copy, syncs
 in the background and picks up changes from other devices within seconds.
 .
 Includes the Todav GNOME Shell extension.
EOF

mkdir -p "$out"
dpkg-deb --root-owner-group --build "$work/root" "$out/todav_${version}_$arch.deb"

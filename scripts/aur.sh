#!/bin/sh
# Update the AUR packages built from a release to it: pkgver, pkgrel 1, the checksums of the
# release's files and .SRCINFO, committed and pushed. The -git packages follow on their own. Run
# it on Arch (makepkg, updpkgsums from pacman-contrib) as a user allowed to push the packages:
# the Release workflow runs it after publishing, with AURomatic's key (a co-maintainer of each).
# A packaging change (dependencies, files, build steps) still goes in by hand.
#
#   scripts/aur.sh <version> [--dry-run]
#
# AUR_KEY: the SSH key to push with (else ssh's own). --dry-run commits but does not push.
# GIT_AUTHOR_NAME and the like set who the commits are by.
set -eu

version=$1 dry=${2:-}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The AUR's own host key (its fingerprints are on https://aur.archlinux.org), not whichever
# answers.
echo 'aur.archlinux.org ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEuBKrPzbawxA/k2g6NcyV5jmqwJ2s+zpgZGZ7tpLIcN' \
  > "$work/known_hosts"
GIT_SSH_COMMAND="ssh -o UserKnownHostsFile=$work/known_hosts -o StrictHostKeyChecking=yes${AUR_KEY:+ -o IdentitiesOnly=yes -i $AUR_KEY}"
export GIT_SSH_COMMAND

for pkg in todav todav-bin gnome-shell-extension-todav gnome-shell-extension-todav-bin; do
  git clone -q "ssh://aur@aur.archlinux.org/$pkg.git" "$work/$pkg"
  cd "$work/$pkg"
  sed -i -e "s/^pkgver=.*/pkgver=$version/" -e 's/^pkgrel=.*/pkgrel=1/' PKGBUILD
  updpkgsums
  makepkg --printsrcinfo > .SRCINFO
  git add PKGBUILD .SRCINFO
  # Run again after a failure, it skips what is already there.
  if git diff --cached --quiet; then
    echo "$pkg: already $version"
  else
    git commit -q -m "chore: update to $version"
    if [ "$dry" = --dry-run ]; then
      git show --stat HEAD | cat
    else
      git push -q origin HEAD:master
      echo "$pkg: $version pushed"
    fi
  fi
  cd - > /dev/null
done

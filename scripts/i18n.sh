#!/bin/sh
# Refresh the translation template (po/todav.pot) from the app, its UI files, the GNOME Shell
# extension, the Android app, the desktop file and the metainfo, then merge it into every catalog in po/LINGUAS.
# Run it after adding or changing text and commit the result with the change. Needs GNU gettext
# 0.24 or newer, for xgettext's Rust support.
set -eu
cd "$(dirname "$0")/.."
export GETTEXTDATADIRS="$PWD/po" # po/its: what of the metainfo is translated

pot=po/todav.pot
x() {
  xgettext --from-code=UTF-8 --add-comments=Translators: --sort-by-file --package-name=Todav \
    --msgid-bugs-address=https://github.com/AperteDev/todav/issues --output="$pot" "$@"
}

x -L Rust -k -kgettext -kngettext:1,2 -kpgettext:1c,2 -kgettext_noop \
  todav/linux/app/src/*.rs todav/linux/app/src/calendar/*.rs
# Blueprint's _("…") and C_("context", "…") read like C.
x --join-existing -L C -k -k_ -kC_:1c,2 todav/linux/app/ui/*.blp
x --join-existing -L JavaScript -k -k_ -kngettext:1,2 -kpgettext:1c,2 todav/gnome-extension/*.js
# Kotlin's tr("…"), ntr(…) and ptr("context", "…") read like Java (its parser warns about some
# Kotlin syntax, and finds the messages all the same).
x --join-existing -L Java -k -ktr -kntr:1,2 -kptr:1c,2 \
  $(find todav/android/app/src -name '*.kt' | sort)
x --join-existing todav/linux/data/dev.aperte.Todav.desktop \
  todav/linux/data/dev.aperte.Todav.metainfo.xml

for lang in $(grep -v '^#' po/LINGUAS); do
  if [ -f "po/$lang.po" ]; then
    msgmerge --quiet --update --backup=none --previous "po/$lang.po" "$pot"
  else
    msginit --no-translator --input="$pot" --locale="$lang.UTF-8" --output-file="po/$lang.po"
  fi
done

#!/bin/sh
# Render the launcher icon's foreground from the Linux app's icon, for each density. The icon fills
# 88 of the adaptive icon's 108 dp, its drawing (16 to 112 of 128) inside the 66 dp safe zone.
# Run after changing todav/linux/data/dev.aperte.Todav.svg; the PNGs are committed.
set -eu
cd "$(dirname "$0")"
svg=../../linux/data/dev.aperte.Todav.svg
# density: scale in tenths
for d in mdpi:10 hdpi:15 xhdpi:20 xxhdpi:30 xxxhdpi:40; do
	name=${d%:*} scale=${d#*:}
	canvas=$((108 * scale / 10))
	icon=$((88 * scale / 10))
	mkdir -p "src/main/res/mipmap-$name"
	rsvg-convert -w "$icon" -h "$icon" "$svg" \
		| magick - -background none -gravity center -extent "${canvas}x$canvas" \
			"src/main/res/mipmap-$name/ic_launcher_foreground.png"
done

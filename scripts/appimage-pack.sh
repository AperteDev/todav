#!/bin/sh
# Pack an AppDir as an AppImage with a SquashFS image. scripts/appimage.sh hands it to quick-sharun
# as its `appimagetool` (APPIMAGETOOL), which runs it with APPDIR, OUTPATH and OUTNAME set.
#
# quick-sharun's own tool makes DwarFS images, and nothing but the AppImage's runtime reads those:
# the thumbnailers of file managers (Linux Mint's xapp-thumbnailers, AppImageLauncher) read the
# icon (`.DirIcon`) with unsquashfs, so a DwarFS AppImage had a generic icon in them.
set -eu

# uruntime, the runtime quick-sharun's AppImages use (the SquashFS build, which has mksquashfs in
# it), at a version, so a release builds the same each time; bump it with its sha256.
uruntime=v0.8.3
case $(uname -m) in
  x86_64) uruntime_sha=5728431f05efb4425239aafdd48f206db5d9b1ca2e476c8c463db5fe75badb32 ;;
  aarch64) uruntime_sha=5a59547219102d353f7aca86f8eae4505965f894d74dfae768c68b23a332f5be ;;
  *) echo "no uruntime pinned for $(uname -m)" >&2; exit 1 ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
runtime=$work/uruntime
curl -fsSL -o "$runtime" \
  "https://github.com/VHSgunzo/uruntime/releases/download/$uruntime/uruntime-appimage-squashfs-$(uname -m)"
echo "$uruntime_sha  $runtime" | sha256sum -c -
chmod +x "$runtime"

mkdir -p "$OUTPATH"
"$runtime" --appimage-mksquashfs "$APPDIR" "$work/image" -root-owned -noappend -quiet \
  -comp zstd -Xcompression-level 19 -b 1M > /dev/null
cat "$runtime" "$work/image" > "$OUTPATH/$OUTNAME"
chmod +x "$OUTPATH/$OUTNAME"

#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
codec_root="$project_root/target/codecs"
archive="$codec_root/libraw-source.tar.gz"
prefix="$codec_root/libraw-macos"
expected=de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa
mkdir -p "$codec_root"
if [ ! -f "$archive" ]; then
  curl --fail --location --proto '=https' --tlsv1.2 'https://www.libraw.org/data/LibRaw-0.22.2.tar.gz' --output "$archive.part"
  actual=$(shasum -a 256 "$archive.part" | cut -d ' ' -f 1)
  [ "$actual" = "$expected" ] || { echo 'LibRaw checksum mismatch' >&2; exit 1; }
  mv "$archive.part" "$archive"
fi
actual=$(shasum -a 256 "$archive" | cut -d ' ' -f 1)
[ "$actual" = "$expected" ] || { echo 'LibRaw checksum mismatch' >&2; exit 1; }
build_root=$(mktemp -d "$codec_root/libraw-build.XXXXXX")
# The build directory is a verified child of target/codecs; no source checkout is removed.
trap 'case "$build_root" in "$codec_root"/libraw-build.*) rm -rf -- "$build_root" ;; esac' EXIT HUP INT TERM
tar -xzf "$archive" -C "$build_root"
cd "$build_root/LibRaw-0.22.2"
# The baseline has no runtime Homebrew dependency. Lossy DNG/JPEG and LCMS plugins
# require separately built static dependencies and are excluded from this baseline.
./configure --prefix="$prefix" --enable-static --disable-shared --disable-examples --disable-openmp --disable-jpeg --disable-lcms
make -j "$(sysctl -n hw.logicalcpu)"
make install
mkdir -p "$prefix/source" "$prefix/licenses"
cp "$archive" "$prefix/source/LibRaw-0.22.2.tar.gz"
cp LICENSE.LGPL COPYRIGHT "$prefix/licenses/"
printf '%s\n' 'LibRaw 0.22.2; source SHA256 de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa; no JPEG-lossy DNG or LCMS plugin' > "$prefix/provenance.txt"

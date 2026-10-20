#!/bin/bash
# Kumpulkan kara-dm.exe dengan alat bantunya untuk Windows: aria2c (resmi), yt-dlp, FFmpeg (LGPL, subfolder
# ffmpeg\) dan lisensinya. Antarmuka iced tidak butuh DLL tambahan. Jalankan di Git Bash atau shell CI setelah
# `cargo build --release`; CARGO_TARGET_DIR dihormati.
# Hasil: dist/windows/kara-dm/ (siap di-zip atau dibungkus Inno Setup).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."
target="${CARGO_TARGET_DIR:-target}"
out=dist/windows/kara-dm
rm -rf "$out"
mkdir -p "$out"

cp "$target/release/kara-dm.exe" "$out/"
# aria2: build resmi Windows (TLS bawaan Windows, statis), diunduh workflow ke dist/windows/aria2.
cp dist/windows/aria2/aria2c.exe "$out/"
mkdir -p "$out/licenses/aria2"
cp dist/windows/aria2/{COPYING,LICENSE.OpenSSL,AUTHORS} "$out/licenses/aria2/"
[[ -f dist/windows/yt-dlp.exe ]] && cp dist/windows/yt-dlp.exe "$out/"
# FFmpeg (build LGPL shared BtbN, diunduh workflow) di subfolder sendiri: yt-dlp memakainya untuk menggabung
# video + audio YouTube, Kara DM untuk DASH. ffplay tidak dibawa.
if [[ -d dist/windows/ffmpeg ]]; then
  mkdir -p "$out/ffmpeg" "$out/licenses/ffmpeg"
  cp dist/windows/ffmpeg/bin/{ffmpeg,ffprobe}.exe dist/windows/ffmpeg/bin/*.dll "$out/ffmpeg/"
  cp dist/windows/ffmpeg/LICENSE.txt "$out/licenses/ffmpeg/"
fi

cp LICENSE NOTICE THIRD-PARTY.md "$out/"
echo "bundel Windows: $out ($(du -sh "$out" | cut -f1))"

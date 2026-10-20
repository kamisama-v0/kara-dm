#!/bin/bash
# Kumpulkan kara-dm.exe + semua yang dibutuhkan untuk jalan di Windows tanpa MSYS2: DLL GTK/libadwaita, aria2c,
# loader gdk-pixbuf, schema GLib, tema ikon Adwaita. Jalankan di shell MSYS2 UCRT64 setelah `cargo build --release`.
# Hasil: dist/windows/kara-dm/ (siap di-zip atau dibungkus Inno Setup).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/../.."
prefix=/ucrt64
out=dist/windows/kara-dm
rm -rf "$out"
mkdir -p "$out"

cp target/release/kara-dm.exe "$out/"
# aria2: build resmi Windows (TLS bawaan Windows, statis) kalau sudah diunduh workflow; selain itu dari MSYS2.
if [[ -f dist/windows/aria2/aria2c.exe ]]; then
  cp dist/windows/aria2/aria2c.exe "$out/"
  mkdir -p "$out/licenses/aria2"
  cp dist/windows/aria2/{COPYING,LICENSE.OpenSSL,AUTHORS} "$out/licenses/aria2/"
else
  cp "$prefix/bin/aria2c.exe" "$out/"
fi
[[ -f dist/windows/yt-dlp.exe ]] && cp dist/windows/yt-dlp.exe "$out/"
# FFmpeg (build LGPL shared BtbN, diunduh workflow) di subfolder sendiri: yt-dlp memakainya untuk menggabung
# video + audio YouTube, Kara DM untuk DASH. ffplay tidak dibawa.
if [[ -d dist/windows/ffmpeg ]]; then
  mkdir -p "$out/ffmpeg" "$out/licenses/ffmpeg"
  cp dist/windows/ffmpeg/bin/{ffmpeg,ffprobe}.exe dist/windows/ffmpeg/bin/*.dll "$out/ffmpeg/"
  cp dist/windows/ffmpeg/LICENSE.txt "$out/licenses/ffmpeg/"
fi

# gdk-pixbuf (PNG/SVG untuk ikon)
loaders=lib/gdk-pixbuf-2.0/2.10.0
mkdir -p "$out/$loaders/loaders"
cp "$prefix/$loaders"/loaders/*.dll "$out/$loaders/loaders/"

# DLL yang dibutuhkan semua exe + loader, rekursif lewat ldd (hanya dari prefix MSYS2, bukan DLL sistem Windows).
deps() { ldd "$@" | awk -v p="$prefix/" 'index($3, p) == 1 { print $3 }'; }
mapfile -t todo < <(deps "$out"/*.exe "$out/$loaders"/loaders/*.dll | sort -u)
for dll in "${todo[@]}"; do cp -n "$dll" "$out/"; done

# loaders.cache dengan path relatif agar folder bisa dipindah.
GDK_PIXBUF_MODULEDIR="$out/$loaders/loaders" gdk-pixbuf-query-loaders \
  | sed "s|$(cygpath -m "$PWD/$out")/||; s|$PWD/$out/||" > "$out/$loaders/loaders.cache"

# Schema GLib (GtkFileDialog dll.) dan ikon
mkdir -p "$out/share/glib-2.0/schemas" "$out/share/icons"
cp "$prefix"/share/glib-2.0/schemas/org.gtk.*.xml "$out/share/glib-2.0/schemas/" 2>/dev/null || true
glib-compile-schemas "$out/share/glib-2.0/schemas"
cp -r "$prefix/share/icons/Adwaita" "$prefix/share/icons/hicolor" "$out/share/icons/"
install -Dm644 data/icons/io.github.kamisama_v0.KaraDM.svg \
  "$out/share/icons/hicolor/scalable/apps/io.github.kamisama_v0.KaraDM.svg"
gtk4-update-icon-cache -q -t -f "$out/share/icons/hicolor" || true

cp LICENSE NOTICE THIRD-PARTY.md "$out/"
echo "bundel Windows: $out ($(du -sh "$out" | cut -f1))"

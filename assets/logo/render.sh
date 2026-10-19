#!/bin/bash
# Render logo ke semua ukuran yang dipakai: ikon ekstensi, ikon aplikasi Linux (hicolor), .ico Windows.
# Butuh: rsvg-convert (librsvg), magick (ImageMagick).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
root=../..
png() { rsvg-convert -w "$2" -h "$2" "$1" -o "$3"; }

# Ekstensi browser
for s in 16 32; do png kara-dm-small.svg $s "$root/extension/src/icons/$s.png"; done
for s in 48 128; do png kara-dm.svg $s "$root/extension/src/icons/$s.png"; done
cp kara-dm.svg "$root/extension/src/icons/icon.svg"

# Aplikasi (Linux hicolor + Windows .ico)
mkdir -p "$root/data/icons" "$root/packaging/windows"
cp kara-dm.svg "$root/data/icons/io.github.kamisama_v0.KaraDM.svg"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for s in 16 24 32; do png kara-dm-small.svg $s "$tmp/$s.png"; done
for s in 48 64 128 256; do png kara-dm.svg $s "$tmp/$s.png"; done
magick "$tmp"/{16,24,32,48,64,128,256}.png "$root/packaging/windows/kara-dm.ico"
echo "ikon dirender"

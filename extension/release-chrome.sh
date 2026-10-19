#!/bin/bash
# Kemas dist/chromium jadi .crx dan daftarkan sebagai ekstensi eksternal Google Chrome + Chromium.
# Linux mengizinkan .crx lokal lewat /usr/share/<browser>/extensions/<id>.json; browser memasangnya saat dibuka
# berikutnya (sekali klik "Enable" di menu). Kunci penanda (menentukan ID ekstensi) = ~/.config/kara-dm/chrome-extension.pem
# (0600, tidak di-backup dotfiles; kalau hilang ID berubah dan ekstensi harus dipasang ulang).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
node build.mjs >/dev/null
key=$HOME/.config/kara-dm/chrome-extension.pem
out=$HOME/.local/share/kara-dm/kara-dm-chrome.crx
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp -r dist/chromium "$tmp/kara-dm"
keyarg=()
[[ -f $key ]] && keyarg=(--pack-extension-key="$key")
google-chrome-stable --no-first-run --user-data-dir="$tmp/profile" --pack-extension="$tmp/kara-dm" "${keyarg[@]}" >/dev/null 2>&1 || true
[[ -f $tmp/kara-dm.crx ]] || { echo "gagal mengemas .crx" >&2; exit 1; }
[[ -f $key ]] || install -Dm600 "$tmp/kara-dm.pem" "$key"
install -Dm644 "$tmp/kara-dm.crx" "$out"
id=$(openssl pkey -in "$key" -pubout -outform DER | sha256sum | head -c32 | tr 0-9a-f a-p)
ver=$(node -p 'require("./manifest.base.json").version')
for d in /usr/share/google-chrome/extensions /usr/share/chromium/extensions; do
  sudo install -d -m755 "$d"
  printf '{ "external_crx": "%s", "external_version": "%s" }\n' "$out" "$ver" | sudo tee "$d/$id.json" >/dev/null
done
echo "Kara DM $ver terdaftar untuk Chrome + Chromium (ID $id)."

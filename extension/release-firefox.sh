#!/bin/bash
# Build, tanda tangani (AMO unlisted) dan pasang Kara DM di Firefox. Minta API key AMO secara interaktif
# (https://addons.mozilla.org/developers/addon/api/key/); key hanya hidup di proses ini, tidak disimpan.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
ver=$(node -p 'require("./manifest.base.json").version')
node --test test/*.test.js >/dev/null
node build.mjs >/dev/null
if ls web-ext-artifacts/*-"$ver".xpi >/dev/null 2>&1; then
  # AMO menolak versi yang sama dua kali (409); pakai .xpi yang sudah ditandatangani.
  echo "Kara DM $ver sudah ditandatangani sebelumnya; tanda tangan dilewati (naikkan version untuk rilis baru)."
else
  echo "Kara DM $ver — tanda tangan Firefox (unlisted)"
  read -r -p "JWT issuer: " WEB_EXT_API_KEY
  read -r -s -p "JWT secret: " WEB_EXT_API_SECRET
  echo
  export WEB_EXT_API_KEY WEB_EXT_API_SECRET
  npx -y web-ext@8 sign -s dist/firefox --channel unlisted
  unset WEB_EXT_API_KEY WEB_EXT_API_SECRET
fi
xpi=$(ls -t web-ext-artifacts/*-"$ver".xpi | head -1)
xpi=$(readlink -f "$xpi")
echo "Bertanda tangan: $xpi"
# Buka di Firefox yang berjalan di layar laptop (juga saat dijalankan lewat SSH).
if [[ -n ${WAYLAND_DISPLAY:-}${DISPLAY:-} ]]; then
  firefox "$xpi" &
else
  HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/$(id -u)/hypr/ | head -1) hyprctl eval "hl.exec_cmd(\"firefox $xpi\")" >/dev/null
fi
echo "Klik \"Add\" di Firefox untuk memperbarui Kara DM ke $ver."

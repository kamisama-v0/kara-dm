#!/bin/bash
# Build dan pasang kara-dm dari sumber untuk user ini (Linux): binary ~/.local/bin/kara-dm, ikon, launcher,
# service systemd user (jalan saat login, menangkap unduhan dari browser).
# Butuh: Rust (cargo), GTK 4.16+, libadwaita 1.6+, aria2. Opsional: yt-dlp (HLS), ffmpeg (DASH/MKV), curl (POST).
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
id=io.github.kamisama_v0.KaraDM

for c in cargo aria2c; do
  command -v "$c" >/dev/null || { echo "Belum terpasang: $c" >&2; exit 1; }
done
for c in yt-dlp ffmpeg curl; do
  command -v "$c" >/dev/null || echo "Catatan: $c tidak ada; fitur terkait nonaktif." >&2
done

cargo build --release
install -Dm755 target/release/kara-dm "$HOME/.local/bin/kara-dm"
install -Dm644 "data/icons/$id.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/$id.svg"
install -Dm644 "data/$id.metainfo.xml" "$HOME/.local/share/metainfo/$id.metainfo.xml"
sed "s|^Exec=kara-dm|Exec=$HOME/.local/bin/kara-dm|" "data/$id.desktop" \
  | install -Dm644 /dev/stdin "$HOME/.local/share/applications/$id.desktop"
rm -f "$HOME/.local/share/applications/kara-dm.desktop" # nama lama (0.1)

mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/kara-dm.service" <<'EOF'
[Unit]
Description=Kara Download Manager (tangkap unduhan dari ekstensi browser)
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=%h/.local/bin/kara-dm --service
ExecStop=%h/.local/bin/kara-dm --quit
Restart=on-failure
RestartSec=3

[Install]
WantedBy=graphical-session.target
EOF

systemctl --user daemon-reload
systemctl --user enable kara-dm.service >/dev/null
systemctl --user restart kara-dm.service
update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "kara-dm $(~/.local/bin/kara-dm --version | cut -d' ' -f2) terpasang; layanan: $(systemctl --user is-active kara-dm.service)"

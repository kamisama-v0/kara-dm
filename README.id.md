<p align="center">
  <img src="assets/logo/kara-dm.svg" width="112" alt="Logo Kara Download Manager">
</p>

<h1 align="center">Kara Download Manager</h1>

<p align="center">
  Download manager multi-koneksi untuk <b>Linux</b> dan <b>Windows</b>, dengan ekstensi browser yang menangkap
  unduhan dan video secara otomatis.
  <br>
  <a href="https://github.com/kamisama-v0/kara-dm/releases">Unduh</a> ·
  <a href="#ekstensi-browser">Ekstensi browser</a> ·
  <a href="CONTRIBUTING.md">Berkontribusi</a> ·
  <a href="README.md">English</a>
</p>

---

Kara Download Manager (kara-dm) memecah tiap berkas menjadi hingga 16 koneksi paralel, melanjutkan unduhan
setelah koneksi putus atau komputer restart, dan mengambil alih unduhan dari browser lewat ekstensi **Kara DM**
(Firefox, Chrome, Chromium). Dibuat dengan Rust, GTK 4, dan libadwaita; mesin unduhnya
[aria2](https://aria2.github.io/). Kompatibel juga dengan ekstensi browser
[Neat Download Manager](https://www.neatdownloadmanager.com/).

## Fitur

- Hingga 16 koneksi per berkas, bar segmen langsung, batas kecepatan, batas unduhan bersamaan.
- Resume setelah crash/restart/koneksi putus; kalau server menolak banyak koneksi (HTTP 429/503) lanjut
  otomatis dengan koneksi lebih sedikit.
- Tangkap unduhan dari browser lengkap dengan cookie dan referer. Kalau kara-dm tidak jalan, browser tetap
  mengunduh sendiri (berkas tidak hilang).
- Tombol "Unduh video" di atas pemutar, stream HLS/DASH, dan "video halaman ini" (YouTube, Vimeo, X, dll) via
  yt-dlp.
- Kategori, riwayat, buka berkas/folder.
- Semua lokal: ekstensi hanya bicara ke `127.0.0.1`, tanpa telemetri.

## Pasang

**Windows 10/11 (beta)** — dari [rilis terbaru](https://github.com/kamisama-v0/kara-dm/releases/latest):
`…-windows-x64-setup.exe` (tanpa hak admin, ada opsi jalan saat login) atau `…-windows-x64-portable.zip`.
SmartScreen bisa memperingatkan karena biner belum ditandatangani: **More info → Run anyway**. Untuk
menggabungkan video + audio terpisah pasang ffmpeg (`winget install Gyan.FFmpeg`). Jangan jalankan bersamaan
dengan Neat Download Manager (sama-sama memakai port 10007).

**Linux (Flatpak)** — unduh `kara-dm-linux-x86_64.flatpak`, lalu:

```bash
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user kara-dm-linux-x86_64.flatpak
```

**Linux dari sumber** (GTK ≥ 4.16, libadwaita ≥ 1.6, mis. Arch/Fedora 41+): `./install.sh`.

## Ekstensi browser

- **Firefox**: buka `kara-dm-extension-<versi>.xpi` dari halaman rilis (sudah ditandatangani Mozilla) → **Add**.
- **Chrome/Chromium/Edge/Brave**: ekstrak `…-chromium.zip`, buka `chrome://extensions`, aktifkan **Developer
  mode** → **Load unpacked** → pilih foldernya.

Matikan ekstensi NDM resmi bila masih terpasang. Detail: [extension/](extension/).

## Lisensi

Apache License 2.0 — lihat [LICENSE](LICENSE), [NOTICE](NOTICE), dan komponen pihak ketiga di
[THIRD-PARTY.md](THIRD-PARTY.md). Tidak berafiliasi dengan Neat Download Manager.

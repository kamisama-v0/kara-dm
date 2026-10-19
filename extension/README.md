# Kara DM — ekstensi browser

Ekstensi sendiri (Firefox + Chromium, Manifest V3) pengganti ekstensi Neat Download Manager. Mengirim unduhan dan
video ke **Kara Download Manager** (`../`) lewat protokol aplikasi NDM (`ws://127.0.0.1:10007/download`,
subprotocol `neatextension.v1`), jadi juga bisa dipakai dengan NDM asli di Windows/macOS. Ditulis dari nol; tidak
memakai kode ekstensi NDM.

## Perilaku

- **Tangkap unduhan** lewat `downloads.onCreated`: unduhan yang sudah diputuskan browser diambil alih (dibatalkan
  di browser, dikirim ke aplikasi dengan cookie + referer, body POST ikut bila ada). Kalau aplikasi tidak jalan,
  unduhan **dibiarkan** ke browser (tidak hilang) dan muncul notifikasi. Jendela privat, `blob:`/`data:`, situs
  yang dikecualikan, jenis berkas yang diabaikan, dan berkas di bawah ukuran minimum dibiarkan ke browser.
- **Menu klik kanan**: unduh tautan / media / video halaman ini (yt-dlp).
- **Media per tab** dari `webRequest.onHeadersReceived` (non-blocking): berkas video/audio utuh ≥ 512 KB, playlist
  HLS `.m3u8` dan DASH `.mpd`; potongan stream (segmen `.ts`/`.m4s`, range parsial, googlevideo) diabaikan. Jumlahnya
  tampil di badge ikon.
- **Tombol kaca "Unduh video"** di atas pemutar (Shadow DOM tertutup, `backdrop-filter`), daftar media + "Video
  halaman ini (yt-dlp)" untuk YouTube/Vimeo/X/dll.
- **Unduh semua tautan** (`links.html`, dari popup atau klik kanan halaman): content script bingkai utama
  mengumpulkan `a[href]` + sumber gambar/video/audio, `prepareLinks` membuang duplikat, `#fragmen`, tautan ke
  halaman itu sendiri dan skema selain http/https/ftp, lalu mengelompokkan per jenis berkas. Yang dicentang dikirim
  sebagai unduhan biasa dengan kunci `11:1` (aplikasi tidak membuka jendela progres per berkas).
- **Kualitas video** (`videoQuality`): Terbaik, 2160p…360p, atau Audio saja, untuk item yt-dlp (video halaman,
  HLS, DASH); dikirim sebagai kunci `10`. Berkas video langsung (MP4/WebM) tidak terpengaruh. Aplikasi < 0.3
  mengabaikan kunci ini (selalu terbaik).
- **Popup**: status koneksi, tangkap on/off, matikan per situs, unduh semua tautan, kualitas video, media tab ini.
  **Opsi**: situs dikecualikan, jenis berkas, ukuran minimum, tombol video, kualitas video, notifikasi.

## Struktur

| File | Isi |
| --- | --- |
| `src/common.js` | logika murni (pesan protokol, filter, klasifikasi media) — diuji `node --test` |
| `src/background.js` | event page (Firefox) / service worker (Chromium) |
| `src/content.js` | tombol video di halaman + pengumpul tautan untuk unduh massal |
| `src/links.*` | halaman "Unduh semua tautan" |
| `src/popup.*`, `src/options.*`, `src/glass.css` | UI kaca ala macOS Tahoe |
| `manifest.base.json` + `build.mjs` | manifest bersama; build menambah bagian khusus tiap browser ke `dist/` |

## Build & tes

```bash
node --test test/*.test.js
node build.mjs                      # -> dist/firefox, dist/chromium
npx web-ext lint -s dist/firefox    # validasi aturan AMO
```

## Memasang

Rilis cepat (skrip):
- `./release-chrome.sh` — kemas `.crx` (kunci `~/.config/kara-dm/chrome-extension.pem`, ID
  `gfkkodmmgbcaacfbjegmpnedhpldeloj`) dan daftarkan sebagai ekstensi eksternal Google Chrome + Chromium
  (`/usr/share/{google-chrome,chromium}/extensions/<id>.json` → `~/.local/share/kara-dm/kara-dm-chrome.crx`).
  Browser memasang/memperbarui saat dibuka berikutnya. Chrome Web Store (publik) butuh akun developer berbayar.
- `./release-firefox.sh` — tes, build, minta API key AMO (tidak disimpan), tanda tangani unlisted, buka `.xpi` di
  Firefox laptop (juga dari SSH). Tinggal klik *Add*.
Versi baru: naikkan `version` di `manifest.base.json`, lalu jalankan kedua skrip.

**Rilis publik Firefox (AMO, listed)** ikut otomatis saat tag `v*` di-push: job `amo` di
`.github/workflows/release.yml` mengirim `dist/firefox` + `amo-metadata.json` (lisensi, catatan reviewer) ke
addons.mozilla.org untuk direview. Sekali saja, isi secret repo `AMO_JWT_ISSUER` dan `AMO_JWT_SECRET`
(GitHub → Settings → Secrets and variables → Actions). AMO menerima tiap nomor versi sekali untuk semua channel,
jadi `./release-firefox.sh` (unlisted) ikut memakai nomor itu: untuk rilis publik cukup naikkan version lalu
push tag.


**Chromium / Chrome**: `chrome://extensions` → aktifkan *Developer mode* → *Load unpacked* → pilih
`dist/chromium`. Tetap terpasang selama folder itu ada (setelah build ulang klik ↻ di kartu ekstensi).

**Firefox** (rilis wajib bertanda tangan Mozilla): tanda tangani sebagai *unlisted* (tidak tampil di AMO,
gratis, otomatis):
1. Buat API key di https://addons.mozilla.org/developers/addon/api/key/ (login akun Firefox sendiri).
2. `./release-firefox.sh` (meminta JWT issuer + secret, build, tanda tangani, buka .xpi di Firefox → *Add*);
   key tidak disimpan. Untuk rilis publik (listed) lihat job `amo` di atas.

Untuk coba tanpa tanda tangan: `about:debugging#/runtime/this-firefox` → *Load Temporary Add-on* →
`dist/firefox/manifest.json` (hilang saat Firefox ditutup).

Setelah Kara DM terpasang, matikan ekstensi NDM resmi agar unduhan tidak ditangkap dua kali.

use crate::platform;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Paths {
    pub settings: PathBuf,
    pub history: PathBuf,
    pub session: PathBuf,
    pub log: PathBuf,
    pub parts: PathBuf,
    pub runtime: PathBuf,
}

impl Paths {
    pub fn new() -> std::io::Result<Paths> {
        let (config, data, cache, runtime) =
            (platform::config_dir(), platform::data_dir(), platform::cache_dir(), platform::runtime_dir());
        for d in [&config, &data, &cache, &runtime] {
            platform::create_private_dir(d)?;
        }
        Ok(Paths {
            settings: config.join("settings.json"),
            history: data.join("history.json"),
            session: data.join("aria2.session"),
            log: data.join("aria2.log"),
            parts: cache.join("parts"),
            runtime,
        })
    }
}

/// Tulis atomik (file sementara + rename), hanya bisa dibaca pemilik.
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let mut f = platform::create_private_file(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub connections: u32,
    pub max_concurrent: u32,
    pub speed_limit_kib: u32,
    pub show_progress_window: bool,
    pub media_panel: bool,
    pub notify: bool,
    /// Permintaan autostart Flatpak (portal Background) sudah pernah dikirim.
    pub autostart_asked: bool,
    /// Cek rilis baru di GitHub saat mulai dan sehari sekali.
    pub check_updates: bool,
    /// Versi terakhir yang sudah diberitahukan lewat notifikasi (tidak diulang tiap kali aplikasi mulai).
    pub notified_version: String,
    /// Gambar jendela dengan GPU (wgpu) alih-alih renderer software (tiny-skia). GPU memungkinkan efek kaca
    /// yang lebih berat tapi memakan jauh lebih banyak RAM; berlaku setelah aplikasi dibuka ulang.
    pub hardware_acceleration: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            download_dir: String::new(),
            connections: 8,
            max_concurrent: 3,
            speed_limit_kib: 0,
            show_progress_window: true,
            media_panel: true,
            notify: true,
            autostart_asked: false,
            check_updates: true,
            notified_version: String::new(),
            hardware_acceleration: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let s: Settings = fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if !path.exists() {
            let _ = s.save(path);
        }
        s
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_private(path, &serde_json::to_vec_pretty(self).unwrap())
    }
    /// Folder unduhan; kosong = folder Downloads sistem.
    pub fn dir(&self) -> PathBuf {
        match self.download_dir.strip_prefix("~/") {
            _ if self.download_dir.is_empty() => platform::downloads_dir(),
            Some(rest) => platform::home().join(rest),
            None if self.download_dir == "~" => platform::home(),
            None => PathBuf::from(&self.download_dir),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Aria2,
    /// HLS (.m3u8) lewat yt-dlp, fragmen paralel.
    Ytdlp,
    /// Unduhan POST lewat curl (aria2 tidak mendukung POST).
    Curl,
    /// Video + audio DASH terpisah, diunduh aria2 lalu digabung ffmpeg.
    Mkv,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Waiting,
    Active,
    Paused,
    Merging,
    Complete,
    Error,
}

impl Status {
    pub fn finished(self) -> bool {
        self == Status::Complete
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub gids: Vec<String>,
    pub name: String,
    pub path: String,
    pub dir: String,
    /// Folder kerja selama unduhan berjalan (`<dir>/temp`); berkas dipindah ke `dir` begitu selesai. Kosong =
    /// unduhan dari versi lama yang langsung ditulis ke `dir`.
    pub temp_dir: String,
    pub url: String,
    pub url2: String,
    pub page: String,
    pub title: String,
    pub size: u64,
    pub done: u64,
    pub status: Status,
    pub error: String,
    pub added: u64,
    pub finished: u64,
    /// Nama berkas ditetapkan kara-dm (bukan dari Content-Disposition oleh aria2).
    pub fixed_name: bool,
    /// Batas koneksi khusus berkas ini (0 = ikut pengaturan); diturunkan otomatis saat server menolak (429/503).
    pub conn_limit: u32,
    #[serde(skip)]
    pub auto_retries: u8,
    #[serde(skip)]
    pub speed: u64,
    #[serde(skip)]
    pub conns: u32,
    #[serde(skip)]
    pub bitfield: String,
    #[serde(skip)]
    pub pieces: u32,
    #[serde(skip)]
    pub miss: u8,
    /// Header (termasuk cookie) hanya di memori, tidak ditulis ke riwayat.
    #[serde(skip)]
    pub headers: Vec<(String, String)>,
    #[serde(skip)]
    pub body: Option<String>,
    #[serde(skip)]
    pub content_type: Option<String>,
    /// Kualitas yt-dlp pilihan ekstensi (kosong = terbaik); disimpan agar "coba lagi" memakai pilihan yang sama.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub quality: String,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn new_id() -> String {
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).expect("getrandom");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Item {
    pub fn display_name(&self) -> &str {
        if !self.name.is_empty() { &self.name } else { &self.url }
    }
    /// Tempat berkas yang belum selesai ditulis.
    pub fn work_dir(&self) -> &str {
        if self.temp_dir.is_empty() { &self.dir } else { &self.temp_dir }
    }
    pub fn fraction(&self) -> f64 {
        if self.status == Status::Complete {
            1.0
        } else if self.size > 0 {
            (self.done as f64 / self.size as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    pub fn eta(&self) -> Option<u64> {
        (self.status == Status::Active && self.speed > 0 && self.size > self.done)
            .then(|| (self.size - self.done) / self.speed)
    }
    pub fn category(&self) -> &'static str {
        category(&self.name)
    }
}

/// (kunci, label) untuk sidebar: filter status lalu kategori jenis berkas (kunci = hasil `category()`).
pub const CATEGORIES: [(&str, &str); 10] = [
    ("all", "All downloads"),
    ("unfinished", "Unfinished"),
    ("finished", "Finished"),
    ("Video", "Video"),
    ("Music", "Music"),
    ("Archives", "Archives"),
    ("Documents", "Documents"),
    ("Programs", "Programs"),
    ("Images", "Images"),
    ("Other", "Other"),
];

pub fn category(name: &str) -> &'static str {
    let ext = crate::protocol::ext_of(name).unwrap_or_default();
    match ext.as_str() {
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "m4v" | "ts" | "mpg" | "mpeg" | "3gp" => "Video",
        "mp3" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "wav" | "wma" | "weba" => "Music",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "iso" | "img" | "dmg" => "Archives",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "txt" | "epub" | "csv"
        | "md" | "vtt" | "srt" => "Documents",
        "exe" | "msi" | "deb" | "rpm" | "appimage" | "apk" | "pkg" | "sh" | "jar" | "run" => "Programs",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "tif" | "tiff" | "heic" | "avif" => "Images",
        _ => "Other",
    }
}

pub fn load_history(path: &Path) -> Vec<Item> {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_history(path: &Path, items: &[Item]) -> std::io::Result<()> {
    write_private(path, &serde_json::to_vec(items).unwrap())
}

pub fn fmt_size(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", U[i]) }
}

pub fn fmt_speed(b: u64) -> String {
    if b == 0 { String::new() } else { format!("{}/s", fmt_size(b)) }
}

pub fn fmt_eta(s: Option<u64>) -> String {
    match s {
        None => String::new(),
        Some(s) if s >= 3600 => format!("{} h {} min", s / 3600, s % 3600 / 60),
        Some(s) if s >= 60 => format!("{} min {} s", s / 60, s % 60),
        Some(s) => format!("{s} s"),
    }
}

pub fn fmt_date(t: u64) -> String {
    // Tanggal lokal (zona waktu sistem).
    chrono::DateTime::from_timestamp(t as i64, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%d-%m-%Y %H:%M").to_string())
        .unwrap_or_default()
}

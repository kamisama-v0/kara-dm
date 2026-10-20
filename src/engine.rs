//! Inti Kara DM tanpa antarmuka: riwayat unduhan, aria2, yt-dlp/curl/ffmpeg, ekstensi browser (WebSocket),
//! Google Drive dan pembaruan. Tampilan (gui.rs) membaca `items`/`settings` untuk menggambar, memanggil aksi
//! (`add`, `pause`, `resume`, `remove`, ...) dan mengambil `effects`: permintaan yang hanya bisa dikerjakan
//! antarmuka (buka jendela, jendela progres, pesan, keluar).

use crate::model::{self, Item, Kind, Settings, Status};
use crate::{aria2, instance, jobs, platform, protocol, update, ws};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub enum Event {
    Extension(String),
    Status(HashMap<String, Value>),
    JobProgress {
        id: String,
        done: u64,
        total: u64,
        speed: u64,
    },
    JobDone {
        id: String,
        result: Result<Option<String>, String>,
    },
    /// Nama berkas akhir yang dipilih yt-dlp (judul + resolusi), diketahui sebelum unduhan mulai.
    JobName {
        id: String,
        name: String,
    },
    /// Port 10007 akhirnya terbuka setelah sebelumnya dipakai aplikasi lain.
    WsReady,
    /// Argumen dari peluncuran berikutnya (lihat instance.rs).
    Remote(Vec<String>),
    /// Hasil cek rilis GitHub; `manual` = dari menu "Check for updates" (hasil apa pun ditampilkan).
    UpdateChecked {
        result: Result<update::Release, String>,
        manual: bool,
    },
    /// Installer pembaruan selesai diunduh dan diverifikasi (Windows).
    UpdateDownloaded(Result<PathBuf, String>),
}

/// Permintaan untuk antarmuka.
pub enum Effect {
    OpenMain,
    ShowProgress(String),
    CloseProgress(String),
    Message(String, String),
    Quit,
}

pub struct Engine {
    paths: model::Paths,
    pub settings: Settings,
    pub items: Vec<Item>,
    aria: Option<Arc<aria2::Aria2>>,
    pub aria_error: Option<String>,
    pub ws_error: Option<String>,
    shared: Arc<ws::Shared>,
    tx: async_channel::Sender<Event>,
    rx: async_channel::Receiver<Event>,
    ua: String,
    jobs: HashMap<String, jobs::Slot>,
    dirty: bool,
    last_save: Instant,
    next_update_check: Instant,
    /// Rilis yang lebih baru dari versi ini, bila ada.
    pub update: Option<update::Release>,
    pub updating: bool,
    pub effects: Vec<Effect>,
}

const STATUS_KEYS: [&str; 9] = [
    "gid",
    "status",
    "totalLength",
    "completedLength",
    "downloadSpeed",
    "connections",
    "bitfield",
    "numPieces",
    "errorMessage",
];

fn poll(aria: Arc<aria2::Aria2>, tx: async_channel::Sender<Event>) {
    let mut keys: Vec<Value> = STATUS_KEYS.iter().map(|k| json!(k)).collect();
    keys.push(json!("files"));
    let keys = Value::Array(keys);
    loop {
        std::thread::sleep(Duration::from_millis(1000));
        let calls = [
            ("aria2.tellActive", vec![keys.clone()]),
            ("aria2.tellWaiting", vec![json!(0), json!(1000), keys.clone()]),
            ("aria2.tellStopped", vec![json!(0), json!(1000), keys.clone()]),
        ];
        let Ok(res) = aria.multicall(&calls) else { continue };
        let mut map = HashMap::new();
        for list in res.into_iter().flatten() {
            for s in list.as_array().cloned().unwrap_or_default() {
                if let Some(g) = s.get("gid").and_then(Value::as_str) {
                    map.insert(g.to_string(), s);
                }
            }
        }
        if tx.send_blocking(Event::Status(map)).is_err() {
            break;
        }
    }
}

pub fn num(s: &Value, k: &str) -> u64 {
    s.get(k).and_then(Value::as_str).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Subfolder folder unduhan untuk berkas yang belum selesai.
pub const TEMP_DIR: &str = "temp";

/// Nama yang belum dipakai di semua folder `dirs` (mis. folder unduhan dan folder temp-nya).
fn unique_name(dirs: &[&Path], stem: &str, ext: &str) -> String {
    let dot = if ext.is_empty() { "" } else { "." };
    let mut name = format!("{stem}{dot}{ext}");
    let mut i = 1;
    while dirs.iter().any(|d| d.join(&name).exists()) {
        name = format!("{stem} ({i}){dot}{ext}");
        i += 1;
    }
    name
}

/// Template nama yt-dlp: judul sebagai slug + resolusi yang benar-benar terpilih, mis. `niki-tsunami-1080.webm`
/// atau `niki-tsunami-audio.m4a`. Video halaman (YouTube dll.) memakai judul dari yt-dlp sendiri (dijadikan slug
/// lewat `--replace-in-metadata`, lihat `ytdlp_job`), karena judul tab bisa basi di situs SPA; stream HLS/DASH
/// memakai judul halaman.
fn ytdlp_template(it: &Item) -> String {
    let name = if it.url == it.page {
        "%(title).150B".to_string()
    } else {
        let t = protocol::slug(&protocol::title_stem(&it.title));
        if t.is_empty() || t == "download" { "video".into() } else { t }
    };
    let res = if it.quality == "audio" { "-audio" } else { "%(height&-{}|)s" };
    format!("{name}{res}.%(ext)s")
}

/// Pindahkan berkas yang sudah selesai dari folder temp ke folder unduhan. Folder temp ada di dalam folder
/// unduhan (satu drive), jadi ini hanya ganti nama, bukan menyalin.
fn move_to_final(path: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = match protocol::ext_of(&name) {
        Some(ext) => (name[..name.len() - ext.len() - 1].to_string(), name[name.len() - ext.len()..].to_string()),
        None => (name.clone(), String::new()),
    };
    let target = dir.join(unique_name(&[dir], &stem, &ext));
    std::fs::rename(path, &target)?;
    Ok(target)
}

impl Engine {
    pub fn new() -> Result<Engine, String> {
        let paths = model::Paths::new().map_err(|e| e.to_string())?;
        let settings = Settings::load(&paths.settings);
        let _ = std::fs::create_dir_all(settings.dir());
        let mut items = model::load_history(&paths.history);
        for it in items.iter_mut() {
            match (it.kind, it.status) {
                (_, Status::Complete) => {}
                (Kind::Ytdlp | Kind::Curl, _) => {
                    it.status = Status::Paused;
                }
                (Kind::Mkv, Status::Merging) => it.status = Status::Waiting,
                _ => {}
            }
        }
        let ua = platform::user_agent();
        let (tx, rx) = async_channel::unbounded::<Event>();
        let (aria, aria_error) = match aria2::Aria2::start(&aria2::Options {
            runtime_dir: paths.runtime.clone(),
            session: paths.session.clone(),
            log: paths.log.clone(),
            dir: &settings.dir().to_string_lossy(),
            connections: settings.connections,
            max_concurrent: settings.max_concurrent,
            speed_limit_kib: settings.speed_limit_kib,
            user_agent: &ua,
        }) {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e)),
        };
        let shared = Arc::new(ws::Shared {
            connections: AtomicUsize::new(0),
            media_panel: AtomicBool::new(settings.media_panel),
        });
        let ws_error = ws::start(tx.clone(), shared.clone()).err().map(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                "port 10007 is used by another app (e.g. Neat Download Manager); close that app and Kara DM \
                 takes over automatically"
                    .to_string()
            } else {
                e.to_string()
            }
        });
        instance::listen(tx.clone());
        if let Some(a) = &aria {
            let (a, tx) = (a.clone(), tx.clone());
            std::thread::Builder::new()
                .name("aria2-poll".into())
                .spawn(move || poll(a, tx))
                .map_err(|e| e.to_string())?;
        }
        for e in [&aria_error, &ws_error].into_iter().flatten() {
            eprintln!("kara-dm: {e}");
            platform::notify("Kara Download Manager has a problem", e, None);
        }
        let now = Instant::now();
        Ok(Engine {
            paths,
            settings,
            items,
            aria,
            aria_error,
            ws_error,
            shared,
            tx,
            rx,
            ua,
            jobs: HashMap::new(),
            dirty: false,
            last_save: now,
            // Cek pembaruan sebentar setelah mulai (jangan memperlambat login), lalu sehari sekali: aplikasi
            // biasanya berjalan terus di latar belakang.
            next_update_check: now + Duration::from_secs(30),
            update: None,
            updating: false,
            effects: vec![],
        })
    }

    /// Kerjakan semua kejadian yang menunggu (dari aria2, ekstensi, yt-dlp, ...) plus pekerjaan berkala.
    /// Dipanggil antarmuka beberapa kali per detik. `true` = ada yang berubah (perlu digambar ulang).
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(ev) = self.rx.try_recv() {
            self.handle(ev);
            changed = true;
        }
        let now = Instant::now();
        if self.dirty && now.duration_since(self.last_save) >= Duration::from_secs(2) {
            self.dirty = false;
            self.last_save = now;
            self.save_history();
        }
        if now >= self.next_update_check {
            self.next_update_check = now + Duration::from_secs(24 * 3600);
            if self.settings.check_updates {
                self.check_update(false);
            }
        }
        changed
    }

    pub fn run_args(&mut self, args: Vec<String>) {
        if args.iter().any(|a| a == "--quit") {
            self.effects.push(Effect::Quit);
            return;
        }
        if args.iter().any(|a| a == "--service") {
            return;
        }
        let urls: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
        for u in &urls {
            match protocol::Request::from_url(u) {
                Ok(r) => self.add(r, false),
                Err(e) => eprintln!("kara-dm: {e}"),
            }
        }
        if urls.is_empty() {
            self.effects.push(Effect::OpenMain);
        }
    }

    pub fn shutdown(&mut self) {
        let ids: Vec<String> = self.jobs.keys().cloned().collect();
        for id in ids {
            if let Some(slot) = self.jobs.remove(&id) {
                if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
                    it.status = Status::Paused;
                }
                jobs::kill(&slot);
            }
        }
        self.save_history();
        if let Some(a) = &self.aria {
            a.shutdown();
        }
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    fn with_item<R>(&mut self, id: &str, f: impl FnOnce(&mut Item) -> R) -> Option<R> {
        self.items.iter_mut().find(|i| i.id == id).map(f)
    }

    fn save_history(&self) {
        if let Err(e) = model::save_history(&self.paths.history, &self.items) {
            eprintln!("kara-dm: save history: {e}");
        }
    }

    pub fn extension_connections(&self) -> usize {
        self.shared.connections.load(Ordering::SeqCst)
    }

    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Extension(text) => match protocol::parse(&text) {
                Ok(req) => self.add(req, true),
                Err(e) => eprintln!("kara-dm: extension message rejected: {e}"),
            },
            Event::Status(map) => self.apply_status(map),
            Event::JobProgress { id, done, total, speed } => {
                self.with_item(&id, |it| {
                    if matches!(it.status, Status::Waiting | Status::Active) {
                        it.status = Status::Active;
                        it.done = done;
                        it.size = total.max(done);
                        it.speed = speed;
                    }
                });
            }
            Event::JobDone { id, result } => self.job_done(&id, result),
            Event::JobName { id, name } => {
                self.with_item(&id, |it| {
                    if !it.status.finished() && !name.is_empty() {
                        it.name = name;
                    }
                });
                self.dirty = true;
            }
            Event::WsReady => {
                eprintln!("kara-dm: port 10007 is open, the browser extension can connect");
                self.ws_error = None;
            }
            Event::Remote(args) => self.run_args(args),
            Event::UpdateChecked { result, manual } => self.update_checked(result, manual),
            Event::UpdateDownloaded(result) => self.update_downloaded(result),
        }
    }

    // ---------- pembaruan ----------

    pub fn check_update(&self, manual: bool) {
        let (tx, ua) = (self.tx.clone(), self.ua.clone());
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Event::UpdateChecked { result: update::fetch_latest(&ua), manual });
        });
    }

    fn update_checked(&mut self, result: Result<update::Release, String>, manual: bool) {
        let current = env!("CARGO_PKG_VERSION");
        match result {
            Ok(r) if update::is_newer(&r.version, current) => {
                if !manual && self.settings.notified_version != r.version {
                    platform::notify(
                        "Kara DM update available",
                        &format!("Version {} is ready to install.", r.version),
                        None,
                    );
                    self.settings.notified_version = r.version.clone();
                    let _ = self.settings.save(&self.paths.settings);
                }
                self.update = Some(r);
                if manual {
                    self.effects.push(Effect::OpenMain);
                }
            }
            Ok(_) if manual => self.message("Kara DM is up to date", &format!("Installed version: {current}")),
            Ok(_) => {}
            Err(e) if manual => self.message("Could not check for updates", &e),
            Err(e) => eprintln!("kara-dm: update check: {e}"),
        }
    }

    fn message(&mut self, title: &str, body: &str) {
        self.effects.push(Effect::Message(title.into(), body.into()));
    }

    /// Tombol "Update now" di spanduk. Windows: unduh installer dan pasang; lainnya: buka halaman rilis.
    pub fn start_update(&mut self) {
        let Some(r) = self.update.clone() else { return };
        let Some(asset) = r.installer.clone().filter(|_| cfg!(windows)) else {
            platform::open(&r.page);
            return;
        };
        if self.updating {
            return;
        }
        self.updating = true;
        // Folder temp unduhan: drive pilihan pengguna, bukan C: yang mungkin penuh.
        let (tx, ua, dir) = (self.tx.clone(), self.ua.clone(), self.settings.dir().join(TEMP_DIR));
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Event::UpdateDownloaded(update::download(&asset, &dir, &ua)));
        });
    }

    fn update_downloaded(&mut self, result: Result<PathBuf, String>) {
        match result.and_then(|p| update::launch_installer(&p)) {
            // Keluar seperti menu "Quit" (riwayat & sesi aria2 disimpan) agar installer bisa mengganti berkas;
            // installer membuka Kara DM lagi setelah selesai.
            Ok(()) => self.effects.push(Effect::Quit),
            Err(e) => {
                self.updating = false;
                self.message("Update failed", &e);
            }
        }
    }

    // ---------- menambah & menjalankan unduhan ----------

    pub fn add(&mut self, req: protocol::Request, from_browser: bool) {
        let dir = self.settings.dir();
        let temp = dir.join(TEMP_DIR);
        let _ = std::fs::create_dir_all(&temp);
        let dirs = [dir.as_path(), temp.as_path()];
        let title = protocol::title_stem(&req.title);
        let base = protocol::url_basename(&req.url);
        let mut it = Item {
            id: model::new_id(),
            url: req.url.clone(),
            url2: req.url2.clone().unwrap_or_default(),
            page: req.page.clone(),
            title: req.title.clone(),
            dir: dir.to_string_lossy().into_owned(),
            temp_dir: temp.to_string_lossy().into_owned(),
            size: req.size,
            added: model::now(),
            headers: req.headers.clone(),
            body: req.body.clone(),
            content_type: req.content_type.clone(),
            quality: req.quality.clone(),
            ..Default::default()
        };
        let media_stem = if title.is_empty() || title == "download" { "video".to_string() } else { title.clone() };
        if req.method == "POST" {
            it.kind = Kind::Curl;
            it.name = if base.is_empty() { "download".into() } else { base };
        } else if req.url2.is_some() {
            it.kind = Kind::Mkv;
            it.name = unique_name(&dirs, &media_stem, "mkv");
            it.fixed_name = true;
        } else if req.is_hls() {
            it.kind = Kind::Ytdlp;
            it.name = unique_name(&dirs, &media_stem, if req.quality == "audio" { "m4a" } else { "mp4" });
            it.fixed_name = true;
        } else if req.kind == "media" {
            let ext = protocol::ext_from_label(&req.label)
                .or_else(|| protocol::ext_of(&base))
                .or_else(|| protocol::ext_from_mime(&req.mime).map(String::from))
                .unwrap_or_else(|| "mp4".into());
            it.name = unique_name(&dirs, &media_stem, &ext);
            it.fixed_name = true;
        } else {
            // Nama final ditentukan aria2 (Content-Disposition / URL); ini hanya tampilan sementara.
            it.name = base;
        }
        if let Err(e) = self.start(&mut it) {
            it.status = Status::Error;
            it.error = e;
        }
        let id = it.id.clone();
        let name = it.display_name().to_string();
        self.items.push(it);
        self.dirty = true;
        if from_browser && !req.batch {
            if self.settings.show_progress_window {
                self.effects.push(Effect::ShowProgress(id));
            } else if self.settings.notify {
                platform::notify("Downloading", &name, None);
            }
        }
    }

    fn aria_add(
        &self,
        url: &str,
        dir: &str,
        out: Option<&str>,
        headers: &[(String, String)],
        conns: u32,
    ) -> Result<String, String> {
        let aria = self.aria.as_ref().ok_or_else(|| self.aria_error.clone().unwrap_or_default())?;
        let n = conns.clamp(1, 16).to_string();
        let mut opts = json!({
            "dir": dir,
            "split": n,
            "max-connection-per-server": n,
            "header": headers.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>(),
        });
        if let Some(o) = out {
            opts["out"] = json!(o);
        }
        let gid = aria.call("aria2.addUri", vec![json!([url]), opts])?;
        gid.as_str().map(String::from).ok_or_else(|| "aria2 returned no gid".into())
    }

    fn start(&mut self, it: &mut Item) -> Result<(), String> {
        it.error.clear();
        it.miss = 0;
        it.speed = 0;
        let global = self.settings.connections;
        let conns = if it.conn_limit > 0 { it.conn_limit.min(global) } else { global };
        match it.kind {
            Kind::Aria2 => {
                let out = (it.fixed_name || !it.path.is_empty()).then(|| it.name.clone());
                it.gids = vec![self.aria_add(&it.url, it.work_dir(), out.as_deref(), &it.headers, conns)?];
            }
            Kind::Mkv => {
                let parts = self.parts_dir(it);
                std::fs::create_dir_all(&parts).map_err(|e| e.to_string())?;
                let p = parts.to_string_lossy().into_owned();
                let v = self.aria_add(&it.url, &p, Some("video"), &it.headers, conns)?;
                let a = self.aria_add(&it.url2, &p, Some("audio"), &it.headers, conns)?;
                it.gids = vec![v, a];
            }
            Kind::Ytdlp => {
                let slot = jobs::spawn(self.ytdlp_job(it)?, self.tx.clone())?;
                self.jobs.insert(it.id.clone(), slot);
            }
            Kind::Curl => {
                let slot = jobs::spawn(self.curl_job(it)?, self.tx.clone())?;
                self.jobs.insert(it.id.clone(), slot);
            }
        }
        it.status = Status::Waiting;
        Ok(())
    }

    /// Potongan video + audio DASH sebelum digabung.
    fn parts_dir(&self, it: &Item) -> PathBuf {
        if it.temp_dir.is_empty() { self.paths.parts.join(&it.id) } else { Path::new(&it.temp_dir).join(&it.id) }
    }

    fn ytdlp_job(&self, it: &Item) -> Result<jobs::Job, String> {
        let mut cmd = platform::tool("yt-dlp");
        cmd.args([
            "--newline", "--progress", "--no-simulate", "--no-playlist", "--no-mtime", "--no-warnings", "-N", "8",
            "--progress-template",
            "download:KDM %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s",
            "--print", "after_move:filepath", "--print", "video:KDMNAME %(filename)s", "--windows-filenames",
            // Judul jadi slug untuk nama berkas: "niki - tsunami" -> "niki-tsunami".
            "--replace-in-metadata", "title", r"[^\w]+", "-", "--replace-in-metadata", "title", "^-+|-+$", "",
            "--user-agent",
        ]);
        cmd.arg(&self.ua);
        // FFmpeg dibundel di Windows (subfolder ffmpeg\): tanpanya yt-dlp tidak bisa menggabung video + audio
        // YouTube dan meninggalkan dua berkas .fNNN terpisah.
        if let Some(dir) = platform::bundled_tool("ffmpeg").and_then(|p| p.parent().map(Path::to_path_buf)) {
            cmd.arg("--ffmpeg-location").arg(dir);
        }
        cmd.args(protocol::ytdlp_format_args(&it.quality));
        let mut temp = vec![];
        for (k, v) in &it.headers {
            if k.eq_ignore_ascii_case("cookie") {
                let p = jobs::write_temp(&self.paths.runtime, "cookies", jobs::netscape_cookies(&it.url, v).as_bytes())
                    .map_err(|e| e.to_string())?;
                cmd.arg("--cookies").arg(&p);
                temp.push(p);
            } else {
                cmd.arg("--add-header").arg(format!("{k}:{v}"));
            }
        }
        let template = ytdlp_template(it);
        if it.temp_dir.is_empty() {
            cmd.arg("-o").arg(Path::new(&it.dir).join(template));
        } else {
            // Potongan .part di folder temp; yt-dlp sendiri memindahkan hasil akhirnya ke folder unduhan.
            cmd.arg("-P").arg(format!("home:{}", it.dir)).arg("-P").arg(format!("temp:{}", it.temp_dir));
            cmd.arg("-o").arg(template);
        }
        cmd.arg("--").arg(&it.url);
        Ok(jobs::Job { id: it.id.clone(), cmd, dir: it.dir.clone().into(), temp, output: None })
    }

    fn curl_job(&self, it: &Item) -> Result<jobs::Job, String> {
        let mut hdr = String::new();
        for (k, v) in &it.headers {
            hdr.push_str(&format!("{k}: {v}\n"));
        }
        if let Some(ct) = &it.content_type {
            hdr.push_str(&format!("Content-Type: {ct}\n"));
        }
        let rt = &self.paths.runtime;
        let hfile = jobs::write_temp(rt, "headers", hdr.as_bytes()).map_err(|e| e.to_string())?;
        let bfile =
            jobs::write_temp(rt, "body", it.body.as_deref().unwrap_or("").as_bytes()).map_err(|e| e.to_string())?;
        let mut cmd = platform::tool("curl");
        cmd.args(["-sS", "-L", "--fail", "--remove-on-error", "-X", "POST", "-A"])
            .arg(&self.ua)
            .arg("-H")
            .arg(format!("@{}", hfile.display()))
            .arg("--data-binary")
            .arg(format!("@{}", bfile.display()))
            .arg("--output-dir")
            .arg(it.work_dir())
            .args(["-O", "-J", "-w", "%{filename_effective}\n"])
            .arg(&it.url);
        Ok(jobs::Job { id: it.id.clone(), cmd, dir: it.work_dir().into(), temp: vec![hfile, bfile], output: None })
    }

    fn start_merge(&mut self, id: &str) {
        let Some(it) = self.item(id).cloned() else { return };
        let parts = self.parts_dir(&it);
        let dir = PathBuf::from(&it.dir);
        let stem = it.name.rsplit_once('.').map(|x| x.0).unwrap_or(&it.name).to_string();
        let name = unique_name(&[&dir], &stem, "mkv");
        let out = dir.join(&name);
        let mut cmd = platform::tool("ffmpeg");
        cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-i"])
            .arg(parts.join("video"))
            .arg("-i")
            .arg(parts.join("audio"))
            .args(["-map", "0:v:0", "-map", "1:a:0", "-c", "copy"])
            .arg(&out);
        let job = jobs::Job { id: id.to_string(), cmd, dir: dir.clone(), temp: vec![], output: Some(out) };
        let res = jobs::spawn(job, self.tx.clone());
        self.with_item(id, |it| match &res {
            Ok(_) => {
                it.status = Status::Merging;
                it.name = name.clone();
                it.speed = 0;
            }
            Err(e) => {
                it.status = Status::Error;
                it.error = e.clone();
            }
        });
        if let Ok(slot) = res {
            self.jobs.insert(id.to_string(), slot);
        }
    }

    fn job_done(&mut self, id: &str, result: Result<Option<String>, String>) {
        self.jobs.remove(id);
        let Some(it) = self.item(id).cloned() else { return };
        if it.status == Status::Paused {
            return; // dihentikan oleh tombol Pause
        }
        match result {
            Ok(path) => {
                if it.kind == Kind::Mkv {
                    let _ = std::fs::remove_dir_all(self.parts_dir(&it));
                }
                self.with_item(id, |it| {
                    if let Some(p) = path.filter(|p| Path::new(p).exists()) {
                        it.name =
                            Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        it.path = p;
                    } else if it.path.is_empty() {
                        it.path = Path::new(&it.dir).join(&it.name).to_string_lossy().into_owned();
                    }
                    if let Ok(m) = std::fs::metadata(&it.path) {
                        it.size = m.len();
                        it.done = m.len();
                    }
                    it.status = Status::Complete;
                });
                self.completed(id);
            }
            Err(e) => {
                self.with_item(id, |it| {
                    it.status = Status::Error;
                    it.error = e;
                    it.speed = 0;
                });
            }
        }
        self.dirty = true;
    }

    /// Unduhan Google Drive yang ternyata halaman "can't scan this file for viruses" (token sekali pakai di URL
    /// browser sudah terpakai): hapus halaman itu dan unduh ulang lewat formulirnya, yang membawa token baru.
    /// Halaman lain dari Drive (mis. "Quota exceeded") bukan berkasnya: unduhan ditandai gagal dengan pesan Google,
    /// bukan "selesai" dengan berkas HTML 2 KB. `true` = sudah ditangani di sini.
    fn retry_drive_warning(&mut self, id: &str) -> bool {
        let Some(it) = self.item(id).cloned() else { return false };
        if it.kind != Kind::Aria2 || protocol::url_host(&it.url) != "drive.usercontent.google.com" {
            return false;
        }
        let small = std::fs::metadata(&it.path).is_ok_and(|m| m.len() < 64 * 1024);
        let Some(html) =
            small.then(|| std::fs::read_to_string(&it.path).ok()).flatten().filter(|t| protocol::is_html(t))
        else {
            return false;
        };
        let _ = std::fs::remove_file(&it.path);
        match protocol::drive_confirm_url(&html).filter(|_| it.auto_retries < 2) {
            Some(url) => {
                self.with_item(id, |it| {
                    it.url = url;
                    it.path.clear();
                    it.name.clear();
                    it.finished = 0;
                    it.auto_retries += 1;
                    it.status = Status::Error;
                });
                self.resume(&[id.to_string()]);
            }
            None => {
                self.with_item(id, |it| {
                    it.status = Status::Error;
                    it.error = protocol::drive_page_error(&html);
                    it.path.clear();
                    it.speed = 0;
                });
            }
        }
        true
    }

    fn completed(&mut self, id: &str) {
        let Some(it) = self.with_item(id, |it| {
            it.finished = model::now();
            it.speed = 0;
            if !it.temp_dir.is_empty() && !it.path.is_empty() && Path::new(&it.path).starts_with(&it.temp_dir) {
                match move_to_final(Path::new(&it.path), Path::new(&it.dir)) {
                    Ok(p) => {
                        it.name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        it.path = p.to_string_lossy().into_owned();
                    }
                    // Berkas tetap utuh di folder temp; "Open folder" tetap menunjuk ke sana.
                    Err(e) => eprintln!("kara-dm: could not move {} out of the temp folder: {e}", it.path),
                }
            }
            it.clone()
        }) else {
            return;
        };
        if self.settings.notify {
            platform::notify("Download complete", &it.name, Some(&it.dir));
        }
    }

    // ---------- status dari aria2 ----------

    fn apply_status(&mut self, map: HashMap<String, Value>) {
        let mut done = vec![];
        let mut merge = vec![];
        let mut retry = vec![];
        let global_conns = self.settings.connections;
        let mut status_changed = false;
        for it in self.items.iter_mut() {
            if it.gids.is_empty() || matches!(it.kind, Kind::Ytdlp | Kind::Curl) {
                continue;
            }
            if matches!(it.status, Status::Complete | Status::Merging) {
                continue;
            }
            let old = it.status;
            let parts: Vec<Option<&Value>> = it.gids.iter().map(|g| map.get(g)).collect();
            if parts.iter().any(Option::is_none) {
                if matches!(it.status, Status::Active | Status::Waiting | Status::Paused) {
                    it.miss += 1;
                    if it.miss >= 3 {
                        it.status = Status::Error;
                        it.error = "No longer in the aria2 queue; click Resume to try again.".into();
                        it.speed = 0;
                    }
                }
            } else {
                let parts: Vec<&Value> = parts.into_iter().flatten().collect();
                it.miss = 0;
                it.size = parts.iter().map(|s| num(s, "totalLength")).sum();
                it.done = parts.iter().map(|s| num(s, "completedLength")).sum();
                it.speed = parts.iter().map(|s| num(s, "downloadSpeed")).sum();
                it.conns = parts.iter().map(|s| num(s, "connections") as u32).sum();
                it.bitfield = parts[0].get("bitfield").and_then(Value::as_str).unwrap_or("").to_string();
                it.pieces = num(parts[0], "numPieces") as u32;
                let st: Vec<&str> =
                    parts.iter().map(|s| s.get("status").and_then(Value::as_str).unwrap_or("")).collect();
                if it.kind == Kind::Aria2 {
                    let p = parts[0]
                        .get("files")
                        .and_then(|f| f.get(0))
                        .and_then(|f| f.get("path"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !p.is_empty() {
                        it.path = p.to_string();
                        it.name =
                            Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    }
                }
                let new = if st.contains(&"error") {
                    it.error = parts
                        .iter()
                        .filter_map(|s| s.get("errorMessage").and_then(Value::as_str))
                        .find(|m| !m.is_empty())
                        .unwrap_or("error")
                        .to_string();
                    Status::Error
                } else if st.iter().all(|s| *s == "complete") {
                    Status::Complete
                } else if st.contains(&"active") {
                    Status::Active
                } else if st.contains(&"removed") {
                    old
                } else if st.iter().all(|s| matches!(*s, "paused" | "complete")) {
                    Status::Paused
                } else {
                    Status::Waiting
                };
                if new == Status::Complete && it.kind == Kind::Mkv {
                    merge.push(it.id.clone());
                } else {
                    if new == Status::Complete {
                        done.push(it.id.clone());
                    }
                    it.status = new;
                }
                if new != Status::Active {
                    it.speed = 0;
                }
            }
            if it.status == Status::Error
                && old != Status::Error
                && it.auto_retries < 4
                && (it.error.contains("status=429") || it.error.contains("status=503"))
            {
                // Server membatasi jumlah koneksi: lanjutkan dengan separuh koneksi (segmentasi adaptif).
                let cur = if it.conn_limit > 0 { it.conn_limit } else { global_conns };
                it.conn_limit = (cur / 2).max(1);
                it.auto_retries += 1;
                it.error = format!("The server limits connections; continuing with {} connections", it.conn_limit);
                retry.push(it.id.clone());
            }
            if it.status != old {
                status_changed = true;
            }
        }
        self.dirty |= status_changed;
        let known: HashSet<String> =
            self.items.iter().filter(|i| !i.status.finished()).flat_map(|i| i.gids.iter().cloned()).collect();
        // Hasil aria2 yang sudah dicatat di riwayat (atau dihapus) dibersihkan dari daftar aria2.
        if let Some(aria) = &self.aria {
            let stale: Vec<String> = map
                .iter()
                .filter(|(g, s)| {
                    !known.contains(*g)
                        && matches!(s.get("status").and_then(Value::as_str), Some("complete" | "error" | "removed"))
                })
                .map(|(g, _)| g.clone())
                .collect();
            if !stale.is_empty() {
                let aria = aria.clone();
                std::thread::spawn(move || {
                    let calls: Vec<(&str, Vec<Value>)> =
                        stale.iter().map(|g| ("aria2.removeDownloadResult", vec![json!(g)])).collect();
                    let _ = aria.multicall(&calls);
                });
            }
        }
        for id in &done {
            if !self.retry_drive_warning(id) {
                self.completed(id);
            }
        }
        for id in &merge {
            self.start_merge(id);
        }
        if !retry.is_empty() {
            self.resume(&retry);
        }
        if !done.is_empty() || !merge.is_empty() {
            self.dirty = true;
        }
    }

    // ---------- aksi pengguna ----------

    pub fn pause(&mut self, ids: &[String]) {
        for id in ids {
            let Some(it) = self.item(id).cloned() else { continue };
            if !matches!(it.status, Status::Active | Status::Waiting) {
                continue;
            }
            match it.kind {
                Kind::Aria2 | Kind::Mkv => {
                    if let Some(a) = &self.aria {
                        for g in &it.gids {
                            let _ = a.call("aria2.pause", vec![json!(g)]);
                        }
                    }
                }
                Kind::Ytdlp | Kind::Curl => {
                    self.with_item(id, |it| {
                        it.status = Status::Paused;
                        it.speed = 0;
                    });
                    if let Some(slot) = self.jobs.remove(id) {
                        jobs::kill(&slot);
                    }
                }
            }
        }
        self.dirty = true;
    }

    pub fn resume(&mut self, ids: &[String]) {
        for id in ids {
            let Some(mut it) = self.item(id).cloned() else { continue };
            if matches!(it.status, Status::Complete | Status::Active | Status::Waiting | Status::Merging) {
                continue;
            }
            let aria = self.aria.clone();
            let paused_in_aria = it.status == Status::Paused && matches!(it.kind, Kind::Aria2 | Kind::Mkv);
            let res = if paused_in_aria {
                match aria.as_ref() {
                    None => Err("aria2 is not running".to_string()),
                    Some(a) => it.gids.iter().try_for_each(|g| match a.call("aria2.unpause", vec![json!(g)]) {
                        Ok(_) => Ok(()),
                        Err(e) if e.contains("cannot be unpaused") || e.contains("is not found") => Ok(()),
                        Err(e) => Err(e),
                    }),
                }
            } else {
                if let Some(a) = &aria {
                    for g in &it.gids {
                        let _ = a.call("aria2.removeDownloadResult", vec![json!(g)]);
                    }
                }
                if it.kind == Kind::Mkv && it.status == Status::Error {
                    let parts = self.parts_dir(&it);
                    if parts.join("video").exists()
                        && parts.join("audio").exists()
                        && !parts.join("video.aria2").exists()
                        && !parts.join("audio.aria2").exists()
                    {
                        self.start_merge(id);
                        continue;
                    }
                }
                self.start(&mut it)
            };
            match res {
                Ok(()) => {
                    if !paused_in_aria {
                        self.with_item(id, |x| *x = it);
                    }
                }
                Err(e) => {
                    self.with_item(id, |x| {
                        x.status = Status::Error;
                        x.error = e;
                    });
                }
            }
        }
        self.dirty = true;
    }

    pub fn remove(&mut self, ids: &[String], delete_files: bool) {
        for id in ids {
            let Some(it) = self.item(id).cloned() else { continue };
            if let Some(slot) = self.jobs.remove(id) {
                jobs::kill(&slot);
            }
            if let Some(a) = &self.aria
                && !it.status.finished()
            {
                for g in &it.gids {
                    let _ = a.call("aria2.forceRemove", vec![json!(g)]);
                }
            }
            if delete_files {
                if !it.path.is_empty() {
                    let _ = std::fs::remove_file(&it.path);
                    let _ = std::fs::remove_file(format!("{}.aria2", it.path));
                }
                if it.kind == Kind::Mkv {
                    let _ = std::fs::remove_dir_all(self.parts_dir(&it));
                }
                if it.kind == Kind::Ytdlp && !it.status.finished() {
                    let stem = it.name.rsplit_once('.').map(|x| x.0).unwrap_or(&it.name).to_string();
                    if let Ok(rd) = std::fs::read_dir(it.work_dir()) {
                        for e in rd.flatten() {
                            let n = e.file_name().to_string_lossy().into_owned();
                            if n.starts_with(&format!("{stem}.")) && (n.contains(".part") || n.ends_with(".ytdl")) {
                                let _ = std::fs::remove_file(e.path());
                            }
                        }
                    }
                }
            }
            self.items.retain(|i| &i.id != id);
            self.effects.push(Effect::CloseProgress(id.clone()));
        }
        self.dirty = true;
    }

    pub fn clear_finished(&mut self) {
        let ids: Vec<String> = self.items.iter().filter(|i| i.status.finished()).map(|i| i.id.clone()).collect();
        self.remove(&ids, false);
    }

    pub fn open_file(&self, id: &str) {
        if let Some(it) = self.item(id).filter(|i| i.status.finished() && !i.path.is_empty()) {
            platform::open(&it.path);
        }
    }

    pub fn open_folder(&self, id: &str) {
        let Some(it) = self.item(id) else { return };
        if !it.path.is_empty() && Path::new(&it.path).exists() {
            platform::reveal(Path::new(&it.path));
        } else {
            platform::open(&it.dir);
        }
    }

    pub fn open_download_dir(&self) {
        platform::open(self.settings.dir());
    }

    pub fn apply_settings(&mut self, s: Settings) {
        let _ = std::fs::create_dir_all(s.dir());
        if let Err(e) = s.save(&self.paths.settings) {
            eprintln!("kara-dm: save settings: {e}");
        }
        self.shared.media_panel.store(s.media_panel, Ordering::SeqCst);
        if let Some(a) = &self.aria {
            let n = s.connections.clamp(1, 16).to_string();
            let _ = a.call(
                "aria2.changeGlobalOption",
                vec![json!({
                    "max-concurrent-downloads": s.max_concurrent.max(1).to_string(),
                    "max-overall-download-limit": format!("{}K", s.speed_limit_kib),
                    "split": n,
                    "max-connection-per-server": n,
                    "dir": s.dir().to_string_lossy(),
                })],
            );
        }
        self.settings = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_names_carry_title_and_resolution() {
        let page = "https://www.youtube.com/watch?v=x";
        let mut it = Item { url: page.into(), page: page.into(), title: "Basi - YouTube".into(), ..Default::default() };
        assert_eq!(ytdlp_template(&it), "%(title).150B%(height&-{}|)s.%(ext)s");
        it.quality = "audio".into();
        assert_eq!(ytdlp_template(&it), "%(title).150B-audio.%(ext)s");
        let hls = Item {
            url: "https://cdn.x/master.m3u8".into(),
            page: "https://x/watch".into(),
            title: "niki - Tsunami (100% live) - YouTube".into(),
            ..Default::default()
        };
        assert_eq!(ytdlp_template(&hls), "niki-Tsunami-100-live%(height&-{}|)s.%(ext)s");
        let untitled = Item { url: "https://cdn.x/a.m3u8".into(), ..Default::default() };
        assert_eq!(ytdlp_template(&untitled), "video%(height&-{}|)s.%(ext)s");
    }

    #[test]
    fn finished_file_moves_out_of_temp_with_unique_name() {
        let dir = std::env::temp_dir().join(format!("kara-dm-test-{}", model::new_id()));
        let temp = dir.join(TEMP_DIR);
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(dir.join("a.zip"), "lama").unwrap();
        std::fs::write(temp.join("a.zip"), "baru").unwrap();
        std::fs::write(temp.join("README"), "x").unwrap();
        assert_eq!(unique_name(&[&dir, &temp], "a", "zip"), "a (1).zip");
        let p = move_to_final(&temp.join("a.zip"), &dir).unwrap();
        assert_eq!(p, dir.join("a (1).zip"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "baru");
        assert!(!temp.join("a.zip").exists());
        assert_eq!(move_to_final(&temp.join("README"), &dir).unwrap(), dir.join("README"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

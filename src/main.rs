#![cfg_attr(windows, windows_subsystem = "windows")]
//! kara-dm: download manager multi-koneksi untuk Linux dan Windows (implementasi clean-room, kompatibel dengan
//! ekstensi browser Neat Download Manager).
//!
//!   kara-dm             buka jendela (layanan ikut berjalan bila belum)
//!   kara-dm --service   jalan di latar tanpa jendela (autostart)
//!   kara-dm URL...      tambah unduhan
//!   kara-dm --quit      hentikan layanan
//!   kara-dm --version   cetak versi
//!   kara-dm --selftest  nyalakan aria2 + server WebSocket tanpa jendela, lalu keluar (untuk CI/diagnosis)
//!
//! Menerima tautan dari ekstensi browser resmi NDM lewat ws://127.0.0.1:10007 (lihat protocol.rs), mengunduh
//! dengan aria2 (multi-koneksi, resume), yt-dlp (HLS), ffmpeg (gabung DASH) dan curl (POST).

mod aria2;
mod instance;
mod jobs;
mod model;
mod platform;
mod protocol;
mod ui;
mod update;
mod ws;

use adw::prelude::*;
use gtk::{gio, glib};
use model::{Item, Kind, Settings, Status};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

pub const APP_ID: &str = "io.github.kamisama_v0.KaraDM";

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
    /// Argumen dari peluncuran kedua (Windows, lihat instance.rs).
    Remote(Vec<String>),
    /// Hasil cek rilis GitHub; `manual` = dari menu "Periksa pembaruan" (hasil apa pun ditampilkan).
    UpdateChecked {
        result: Result<update::Release, String>,
        manual: bool,
    },
    /// Installer pembaruan selesai diunduh dan diverifikasi (Windows).
    UpdateDownloaded(Result<PathBuf, String>),
}

pub struct App {
    pub gapp: adw::Application,
    paths: model::Paths,
    pub settings: RefCell<Settings>,
    pub items: RefCell<Vec<Item>>,
    aria: Option<Arc<aria2::Aria2>>,
    aria_error: Option<String>,
    ws_error: RefCell<Option<String>>,
    shared: Arc<ws::Shared>,
    tx: async_channel::Sender<Event>,
    ua: String,
    ui: RefCell<Option<Rc<ui::MainWindow>>>,
    pub progress: RefCell<HashMap<String, Rc<ui::ProgressWindow>>>,
    jobs: RefCell<HashMap<String, jobs::Slot>>,
    dirty: Cell<bool>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    /// Rilis yang lebih baru dari versi ini, bila ada.
    pub update: RefCell<Option<update::Release>>,
    updating: Cell<bool>,
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

fn num(s: &Value, k: &str) -> u64 {
    s.get(k).and_then(Value::as_str).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Subfolder folder unduhan untuk berkas yang belum selesai.
const TEMP_DIR: &str = "temp";

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

/// Template nama yt-dlp: judul + resolusi yang benar-benar terpilih, mis. `Judul [1080p].webm` atau
/// `Judul [audio].m4a`. Video halaman (YouTube dll.) memakai judul dari yt-dlp sendiri, karena judul tab bisa
/// basi di situs SPA; stream HLS/DASH memakai judul halaman.
fn ytdlp_template(it: &Item) -> String {
    let name = if it.url == it.page {
        "%(title).150B".to_string()
    } else {
        let t = protocol::title_stem(&it.title);
        if t.is_empty() || t == "unduhan" { "video".into() } else { t.replace('%', "%%") }
    };
    let res = if it.quality == "audio" { " [audio]" } else { "%(height& [{}p]|)s" };
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

impl App {
    fn new(gapp: &adw::Application) -> Result<Rc<App>, String> {
        let paths = model::Paths::new().map_err(|e| e.to_string())?;
        let mut settings = Settings::load(&paths.settings);
        if !settings.autostart_asked && platform::in_flatpak() {
            platform::request_flatpak_autostart();
            settings.autostart_asked = true;
            let _ = settings.save(&paths.settings);
        }
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
                "port 10007 sudah dipakai aplikasi lain (mis. Neat Download Manager); tutup aplikasi itu, Kara DM \
                 mengambil alih otomatis"
                    .to_string()
            } else {
                e.to_string()
            }
        });
        #[cfg(windows)]
        instance::listen(tx.clone());
        if let Some(a) = &aria {
            let (a, tx) = (a.clone(), tx.clone());
            std::thread::Builder::new()
                .name("aria2-poll".into())
                .spawn(move || poll(a, tx))
                .map_err(|e| e.to_string())?;
        }
        let app = Rc::new(App {
            gapp: gapp.clone(),
            paths,
            settings: RefCell::new(settings),
            items: RefCell::new(items),
            aria,
            aria_error,
            ws_error: RefCell::new(ws_error),
            shared,
            tx,
            ua,
            ui: RefCell::default(),
            progress: RefCell::default(),
            jobs: RefCell::default(),
            dirty: Cell::new(false),
            hold: RefCell::new(Some(gapp.hold())),
            update: RefCell::default(),
            updating: Cell::new(false),
        });
        for e in [app.aria_error.clone(), app.ws_error.borrow().clone()].iter().flatten() {
            eprintln!("kara-dm: {e}");
            app.notify("Kara Download Manager bermasalah", e, None);
        }
        {
            let app = app.clone();
            glib::spawn_future_local(async move {
                while let Ok(ev) = rx.recv().await {
                    app.handle(ev);
                }
            });
        }
        {
            let app = app.clone();
            glib::timeout_add_seconds_local(2, move || {
                if app.dirty.replace(false) {
                    app.save_history();
                }
                glib::ControlFlow::Continue
            });
        }
        // Cek pembaruan sebentar setelah mulai (jangan memperlambat login), lalu sehari sekali: aplikasi biasanya
        // berjalan terus di latar belakang.
        {
            let app = app.clone();
            glib::timeout_add_seconds_local_once(30, move || {
                app.auto_check_update();
                glib::timeout_add_seconds_local(24 * 3600, move || {
                    app.auto_check_update();
                    glib::ControlFlow::Continue
                });
            });
        }
        let quit = gio::SimpleAction::new("quit", None);
        {
            let g = gapp.clone();
            quit.connect_activate(move |_, _| g.quit());
        }
        gapp.add_action(&quit);
        // Klik notifikasi "Unduhan selesai" membuka foldernya.
        let open_path = gio::SimpleAction::new("open-path", Some(glib::VariantTy::STRING));
        open_path.connect_activate(|_, p| {
            if let Some(path) = p.and_then(|v| v.get::<String>()) {
                let _ = gio::AppInfo::launch_default_for_uri(
                    &gio::File::for_path(path).uri(),
                    None::<&gio::AppLaunchContext>,
                );
            }
        });
        gapp.add_action(&open_path);
        gtk::Window::set_default_icon_name(APP_ID);
        Ok(app)
    }

    fn command_line(self: &Rc<Self>, cmd: &gio::ApplicationCommandLine) {
        self.run_args(cmd.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect());
    }

    fn run_args(self: &Rc<Self>, args: Vec<String>) {
        if args.iter().any(|a| a == "--quit") {
            self.gapp.quit();
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
            self.window().win.present();
        }
    }

    fn shutdown(&self) {
        for (id, slot) in self.jobs.borrow_mut().drain() {
            if let Some(it) = self.items.borrow_mut().iter_mut().find(|i| i.id == id) {
                it.status = Status::Paused;
            }
            jobs::kill(&slot);
        }
        self.save_history();
        if let Some(a) = &self.aria {
            a.shutdown();
        }
        self.hold.borrow_mut().take();
    }

    fn window(self: &Rc<Self>) -> Rc<ui::MainWindow> {
        if let Some(w) = self.ui.borrow().clone() {
            return w;
        }
        let w = ui::MainWindow::build(self);
        *self.ui.borrow_mut() = Some(w.clone());
        self.refresh_summary();
        w.show_update(self);
        w
    }

    pub fn item(&self, id: &str) -> Option<Item> {
        self.items.borrow().iter().find(|i| i.id == id).cloned()
    }

    fn with_item<R>(&self, id: &str, f: impl FnOnce(&mut Item) -> R) -> Option<R> {
        self.items.borrow_mut().iter_mut().find(|i| i.id == id).map(f)
    }

    fn save_history(&self) {
        if let Err(e) = model::save_history(&self.paths.history, &self.items.borrow()) {
            eprintln!("kara-dm: simpan riwayat: {e}");
        }
    }

    fn notify(&self, title: &str, body: &str, open_dir: Option<&str>) {
        platform::notify(self.gapp.upcast_ref(), title, body, open_dir);
    }

    fn handle(self: &Rc<Self>, ev: Event) {
        match ev {
            Event::Extension(text) => match protocol::parse(&text) {
                Ok(req) => self.add(req, true),
                Err(e) => eprintln!("kara-dm: pesan ekstensi ditolak: {e}"),
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
                self.refresh_item(&id);
            }
            Event::JobDone { id, result } => self.job_done(&id, result),
            Event::JobName { id, name } => {
                self.with_item(&id, |it| {
                    if !it.status.finished() && !name.is_empty() {
                        it.name = name;
                    }
                });
                self.dirty.set(true);
                self.refresh_item(&id);
            }
            Event::WsReady => {
                eprintln!("kara-dm: port 10007 terbuka, ekstensi browser bisa tersambung");
                self.ws_error.replace(None);
                self.refresh_summary();
            }
            Event::Remote(args) => self.run_args(args),
            Event::UpdateChecked { result, manual } => self.update_checked(result, manual),
            Event::UpdateDownloaded(result) => self.update_downloaded(result),
        }
    }

    // ---------- pembaruan ----------

    fn auto_check_update(self: &Rc<Self>) {
        if self.settings.borrow().check_updates {
            self.check_update(false);
        }
    }

    pub fn check_update(self: &Rc<Self>, manual: bool) {
        let (tx, ua) = (self.tx.clone(), self.ua.clone());
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Event::UpdateChecked { result: update::fetch_latest(&ua), manual });
        });
    }

    fn update_checked(self: &Rc<Self>, result: Result<update::Release, String>, manual: bool) {
        let current = env!("CARGO_PKG_VERSION");
        match result {
            Ok(r) if update::is_newer(&r.version, current) => {
                let first_notice = self.settings.borrow().notified_version != r.version;
                if !manual && first_notice {
                    self.notify("Pembaruan Kara DM tersedia", &format!("Versi {} siap dipasang.", r.version), None);
                    let mut s = self.settings.borrow().clone();
                    s.notified_version = r.version.clone();
                    let _ = s.save(&self.paths.settings);
                    *self.settings.borrow_mut() = s;
                }
                *self.update.borrow_mut() = Some(r);
                if let Some(w) = self.ui.borrow().clone() {
                    w.show_update(self);
                }
                if manual {
                    self.window().win.present();
                }
            }
            Ok(_) if manual => self.message("Kara DM sudah versi terbaru", &format!("Versi terpasang: {current}")),
            Ok(_) => {}
            Err(e) if manual => self.message("Gagal memeriksa pembaruan", &e),
            Err(e) => eprintln!("kara-dm: cek pembaruan: {e}"),
        }
    }

    fn message(self: &Rc<Self>, title: &str, body: &str) {
        let w = self.window();
        w.win.present();
        let d = adw::AlertDialog::new(Some(title), Some(body));
        d.add_response("ok", "OK");
        d.present(Some(&w.win));
    }

    /// Tombol "Perbarui" di spanduk. Windows: unduh installer dan pasang; lainnya: buka halaman rilis.
    pub fn start_update(self: &Rc<Self>) {
        let Some(r) = self.update.borrow().clone() else { return };
        let installer = r.installer.clone().filter(|_| cfg!(windows));
        let Some(asset) = installer else {
            let _ = gio::AppInfo::launch_default_for_uri(&r.page, None::<&gio::AppLaunchContext>);
            return;
        };
        if self.updating.replace(true) {
            return;
        }
        if let Some(w) = self.ui.borrow().clone() {
            w.show_update(self);
        }
        // Folder temp unduhan: drive pilihan pengguna, bukan C: yang mungkin penuh.
        let (tx, ua, dir) = (self.tx.clone(), self.ua.clone(), self.settings.borrow().dir().join(TEMP_DIR));
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Event::UpdateDownloaded(update::download(&asset, &dir, &ua)));
        });
    }

    fn update_downloaded(self: &Rc<Self>, result: Result<PathBuf, String>) {
        let res = result.and_then(|p| update::launch_installer(&p));
        match res {
            // Keluar seperti menu "Keluar" (riwayat & sesi aria2 disimpan) agar installer bisa mengganti berkas;
            // installer membuka Kara DM lagi setelah selesai.
            Ok(()) => self.gapp.quit(),
            Err(e) => {
                self.updating.set(false);
                if let Some(w) = self.ui.borrow().clone() {
                    w.show_update(self);
                }
                self.message("Pembaruan gagal", &e);
            }
        }
    }

    pub fn is_updating(&self) -> bool {
        self.updating.get()
    }

    // ---------- menambah & menjalankan unduhan ----------

    fn add(self: &Rc<Self>, req: protocol::Request, from_browser: bool) {
        let dir = self.settings.borrow().dir();
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
        let media_stem = if title.is_empty() || title == "unduhan" { "video".to_string() } else { title.clone() };
        if req.method == "POST" {
            it.kind = Kind::Curl;
            it.name = if base.is_empty() { "unduhan".into() } else { base };
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
        self.items.borrow_mut().push(it.clone());
        if let Some(w) = self.ui.borrow().clone() {
            w.add_row(&it);
        }
        self.dirty.set(true);
        self.refresh_summary();
        if from_browser && !req.batch {
            if self.settings.borrow().show_progress_window {
                self.show_progress(&id);
            } else if self.settings.borrow().notify {
                self.notify("Mengunduh", &name, None);
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
        gid.as_str().map(String::from).ok_or_else(|| "aria2 tidak mengembalikan gid".into())
    }

    fn start(&self, it: &mut Item) -> Result<(), String> {
        it.error.clear();
        it.miss = 0;
        it.speed = 0;
        let global = self.settings.borrow().connections;
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
                self.jobs.borrow_mut().insert(it.id.clone(), slot);
            }
            Kind::Curl => {
                let slot = jobs::spawn(self.curl_job(it)?, self.tx.clone())?;
                self.jobs.borrow_mut().insert(it.id.clone(), slot);
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

    fn start_merge(&self, id: &str) {
        let Some(it) = self.item(id) else { return };
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
            self.jobs.borrow_mut().insert(id.to_string(), slot);
        }
    }

    fn job_done(self: &Rc<Self>, id: &str, result: Result<Option<String>, String>) {
        self.jobs.borrow_mut().remove(id);
        let Some(it) = self.item(id) else { return };
        if it.status == Status::Paused {
            return; // dihentikan oleh tombol Jeda
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
        self.dirty.set(true);
        self.refresh_item(id);
        self.refresh_summary();
        if let Some(w) = self.ui.borrow().clone() {
            w.refilter();
        }
    }

    fn completed(&self, id: &str) {
        let Some(it) = self.with_item(id, |it| {
            it.finished = model::now();
            it.speed = 0;
            if !it.temp_dir.is_empty() && !it.path.is_empty() && Path::new(&it.path).starts_with(&it.temp_dir) {
                match move_to_final(Path::new(&it.path), Path::new(&it.dir)) {
                    Ok(p) => {
                        it.name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        it.path = p.to_string_lossy().into_owned();
                    }
                    // Berkas tetap utuh di folder temp; "Buka folder" tetap menunjuk ke sana.
                    Err(e) => eprintln!("kara-dm: gagal memindahkan {} dari folder temp: {e}", it.path),
                }
            }
            it.clone()
        }) else {
            return;
        };
        if self.settings.borrow().notify {
            self.notify("Unduhan selesai", &it.name, Some(&it.dir));
        }
    }

    // ---------- status dari aria2 ----------

    fn apply_status(self: &Rc<Self>, map: HashMap<String, Value>) {
        let mut changed = vec![];
        let mut done = vec![];
        let mut merge = vec![];
        let mut retry = vec![];
        let mut refilter = false;
        let global_conns = self.settings.borrow().connections;
        let known: HashSet<String>;
        {
            let mut items = self.items.borrow_mut();
            for it in items.iter_mut() {
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
                            it.error = "Tidak ada lagi di antrean aria2; klik Lanjutkan untuk mencoba lagi.".into();
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
                            .unwrap_or("galat")
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
                    it.error = format!("Server membatasi koneksi, dilanjutkan dengan {} koneksi", it.conn_limit);
                    retry.push(it.id.clone());
                }
                if it.status != old {
                    refilter = true;
                }
                changed.push(it.id.clone());
            }
            known = items.iter().filter(|i| !i.status.finished()).flat_map(|i| i.gids.iter().cloned()).collect();
        }
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
            self.completed(id);
        }
        for id in &merge {
            self.start_merge(id);
        }
        if !retry.is_empty() {
            self.resume(&retry);
        }
        if !done.is_empty() || !merge.is_empty() || refilter {
            self.dirty.set(true);
        }
        for id in &changed {
            self.refresh_item(id);
        }
        self.refresh_summary();
        if (refilter || !merge.is_empty())
            && let Some(w) = self.ui.borrow().clone()
        {
            w.refilter();
        }
    }

    fn refresh_item(&self, id: &str) {
        let Some(it) = self.item(id) else { return };
        if let Some(w) = self.ui.borrow().as_ref() {
            w.update_row(&it);
        }
        if let Some(p) = self.progress.borrow().get(id) {
            p.update(&it);
        }
    }

    fn refresh_summary(&self) {
        if let Some(w) = self.ui.borrow().as_ref() {
            w.refresh_summary(
                &self.items.borrow(),
                self.shared.connections.load(Ordering::SeqCst),
                self.ws_error.borrow().as_deref(),
                self.aria_error.as_deref(),
            );
        }
    }

    // ---------- aksi pengguna ----------

    pub fn pause(&self, ids: &[String]) {
        for id in ids {
            let Some(it) = self.item(id) else { continue };
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
                    if let Some(slot) = self.jobs.borrow_mut().remove(id) {
                        jobs::kill(&slot);
                    }
                    self.refresh_item(id);
                }
            }
        }
        self.dirty.set(true);
    }

    pub fn resume(&self, ids: &[String]) {
        for id in ids {
            let Some(mut it) = self.item(id) else { continue };
            if matches!(it.status, Status::Complete | Status::Active | Status::Waiting | Status::Merging) {
                continue;
            }
            let aria = self.aria.clone();
            let paused_in_aria = it.status == Status::Paused && matches!(it.kind, Kind::Aria2 | Kind::Mkv);
            let res = if paused_in_aria {
                match aria.as_ref() {
                    None => Err("aria2 tidak jalan".to_string()),
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
                        let started = it.clone();
                        self.with_item(id, |x| *x = started);
                    }
                }
                Err(e) => {
                    self.with_item(id, |x| {
                        x.status = Status::Error;
                        x.error = e;
                    });
                }
            }
            self.refresh_item(id);
        }
        self.dirty.set(true);
        if let Some(w) = self.ui.borrow().clone() {
            w.refilter();
        }
    }

    pub fn confirm_remove(self: &Rc<Self>, ids: Vec<String>, parent: &impl IsA<gtk::Widget>) {
        if ids.is_empty() {
            return;
        }
        let body = if ids.len() == 1 {
            self.item(&ids[0]).map(|i| i.display_name().to_string()).unwrap_or_default()
        } else {
            format!("{} unduhan dipilih", ids.len())
        };
        let d = adw::AlertDialog::new(Some("Hapus unduhan?"), Some(&body));
        d.add_responses(&[("cancel", "Batal"), ("list", "Hapus dari daftar"), ("files", "Hapus beserta berkas")]);
        d.set_response_appearance("files", adw::ResponseAppearance::Destructive);
        d.set_close_response("cancel");
        d.set_default_response(Some("list"));
        let app = self.clone();
        d.connect_response(None, move |_, r| match r {
            "list" => app.remove(&ids, false),
            "files" => app.remove(&ids, true),
            _ => {}
        });
        d.present(Some(parent));
    }

    fn remove(&self, ids: &[String], delete_files: bool) {
        for id in ids {
            let Some(it) = self.item(id) else { continue };
            if let Some(slot) = self.jobs.borrow_mut().remove(id) {
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
            self.items.borrow_mut().retain(|i| &i.id != id);
            if let Some(w) = self.ui.borrow().as_ref() {
                w.remove_row(id);
            }
            if let Some(p) = self.progress.borrow_mut().remove(id) {
                p.win.close();
            }
        }
        self.dirty.set(true);
        self.refresh_summary();
    }

    fn clear_finished(&self) {
        let ids: Vec<String> =
            self.items.borrow().iter().filter(|i| i.status.finished()).map(|i| i.id.clone()).collect();
        self.remove(&ids, false);
    }

    pub fn open_file(&self, id: &str) {
        if let Some(it) = self.item(id).filter(|i| i.status.finished() && !i.path.is_empty()) {
            let uri = gio::File::for_path(&it.path).uri();
            let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
        }
    }

    pub fn open_folder(self: &Rc<Self>, id: &str) {
        let Some(it) = self.item(id) else { return };
        let win = self.window().win.clone();
        if !it.path.is_empty() && Path::new(&it.path).exists() {
            gtk::FileLauncher::new(Some(&gio::File::for_path(&it.path))).open_containing_folder(
                Some(&win),
                gio::Cancellable::NONE,
                |_| {},
            );
        } else {
            let uri = gio::File::for_path(&it.dir).uri();
            let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
        }
    }

    fn open_download_dir(&self) {
        let uri = gio::File::for_path(self.settings.borrow().dir()).uri();
        let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
    }

    pub fn show_progress(self: &Rc<Self>, id: &str) {
        let Some(it) = self.item(id) else { return };
        if let Some(p) = self.progress.borrow().get(id) {
            p.win.present();
            return;
        }
        let p = ui::ProgressWindow::new(self, &it);
        p.win.present();
        self.progress.borrow_mut().insert(id.to_string(), p);
    }

    fn show_add_dialog(self: &Rc<Self>) {
        let win = self.window().win.clone();
        let entry = gtk::Entry::builder().placeholder_text("https://…").activates_default(true).hexpand(true).build();
        {
            let entry = entry.clone();
            win.clipboard().read_text_async(gio::Cancellable::NONE, move |res| {
                if let Ok(Some(t)) = res
                    && protocol::valid_url(t.trim())
                    && entry.text().is_empty()
                {
                    entry.set_text(t.trim());
                }
            });
        }
        let d = adw::AlertDialog::new(
            Some("Tambah unduhan"),
            Some("Tempel satu URL per baris tidak didukung; satu URL saja."),
        );
        d.set_body("");
        d.add_responses(&[("cancel", "Batal"), ("add", "Unduh")]);
        d.set_response_appearance("add", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("add"));
        d.set_close_response("cancel");
        d.set_extra_child(Some(&entry));
        let app = self.clone();
        d.connect_response(None, move |_, r| {
            if r == "add" {
                match protocol::Request::from_url(&entry.text()) {
                    Ok(req) => app.add(req, false),
                    Err(e) => app.notify("URL tidak valid", &e, None),
                }
            }
        });
        d.present(Some(&win));
    }

    fn show_settings(self: &Rc<Self>) {
        let win = self.window().win.clone();
        ui::settings_dialog(self, &win);
    }

    fn apply_settings(&self, s: Settings) {
        let _ = std::fs::create_dir_all(s.dir());
        if let Err(e) = s.save(&self.paths.settings) {
            eprintln!("kara-dm: simpan pengaturan: {e}");
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
        *self.settings.borrow_mut() = s;
    }
}

fn selftest_download(a: &aria2::Aria2, url: &str, dir: &Path) -> bool {
    let opts = json!({"dir": dir.to_string_lossy(), "split": "4", "max-connection-per-server": "4"});
    let gid = match a.call("aria2.addUri", vec![json!([url]), opts]) {
        Ok(Value::String(g)) => g,
        other => {
            println!("unduhan uji: GAGAL menambah ({other:?})");
            return false;
        }
    };
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(500));
        let Ok(s) = a.call("aria2.tellStatus", vec![json!(gid)]) else { continue };
        match s.get("status").and_then(Value::as_str) {
            Some("complete") => {
                println!("unduhan uji: OK {} byte", num(&s, "completedLength"));
                return true;
            }
            Some("error") => {
                println!("unduhan uji: GAGAL {}", s.get("errorMessage").and_then(Value::as_str).unwrap_or("?"));
                return false;
            }
            _ => {}
        }
    }
    println!("unduhan uji: GAGAL waktu habis");
    false
}

/// Nyalakan aria2 dan server WebSocket tanpa GTK, laporkan, lalu keluar. Dipakai CI dan untuk diagnosis.
fn selftest() -> glib::ExitCode {
    let dir = std::env::temp_dir().join(format!("kara-dm-selftest-{}", std::process::id()));
    if let Err(e) = platform::create_private_dir(&dir) {
        println!("folder uji: GAGAL {e}");
        return glib::ExitCode::FAILURE;
    }
    let ua = platform::user_agent();
    let aria = aria2::Aria2::start(&aria2::Options {
        runtime_dir: dir.clone(),
        session: dir.join("aria2.session"),
        log: dir.join("aria2.log"),
        dir: &dir.to_string_lossy(),
        connections: 4,
        max_concurrent: 1,
        speed_limit_kib: 0,
        user_agent: &ua,
    });
    let mut ok = match &aria {
        Ok(a) => match a.call("aria2.getVersion", vec![]) {
            Ok(v) => {
                println!("aria2: OK {}", v.get("version").and_then(Value::as_str).unwrap_or("?"));
                true
            }
            Err(e) => {
                println!("aria2: GAGAL {e}");
                false
            }
        },
        Err(e) => {
            println!("aria2: GAGAL {e}");
            false
        }
    };
    // Uji unduhan sungguhan (HTTPS/sertifikat) bila KARA_SELFTEST_URL diset, mis. di CI.
    if let (Ok(a), Ok(url)) = (&aria, std::env::var("KARA_SELFTEST_URL")) {
        ok &= selftest_download(a, &url, &dir);
    }
    let (tx, _rx) = async_channel::unbounded();
    let shared = Arc::new(ws::Shared { connections: AtomicUsize::new(0), media_panel: AtomicBool::new(true) });
    match ws::start(tx, shared) {
        Ok(()) => println!("websocket: OK 127.0.0.1:{}", protocol::PORT),
        Err(e) => println!("websocket: tidak bisa membuka port {} ({e})", protocol::PORT),
    }
    for t in ["yt-dlp", "ffmpeg", "curl"] {
        println!("{t}: {}", if platform::has_tool(t) { "ada" } else { "tidak ada (fitur terkait nonaktif)" });
    }
    println!("folder unduhan: {}", Settings::default().dir().display());
    if let Ok(a) = aria {
        a.shutdown();
    }
    let _ = std::fs::remove_dir_all(&dir);
    if ok { glib::ExitCode::SUCCESS } else { glib::ExitCode::FAILURE }
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("kara-dm {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--selftest") {
        return selftest();
    }
    #[cfg(windows)]
    if instance::forward(&args) {
        return glib::ExitCode::SUCCESS;
    }
    let mut flags = gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    if cfg!(windows) {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let gapp = adw::Application::builder().application_id(APP_ID).flags(flags).build();
    let state: Rc<RefCell<Option<Rc<App>>>> = Rc::default();
    {
        let state = state.clone();
        gapp.connect_startup(move |g| match App::new(g) {
            Ok(a) => *state.borrow_mut() = Some(a),
            Err(e) => {
                eprintln!("kara-dm: gagal mulai: {e}");
                g.quit();
            }
        });
    }
    {
        let state = state.clone();
        gapp.connect_command_line(move |_, cmd| {
            if let Some(a) = state.borrow().clone() {
                a.command_line(cmd);
            }
            glib::ExitCode::SUCCESS
        });
    }
    {
        let state = state.clone();
        gapp.connect_shutdown(move |_| {
            if let Some(a) = state.borrow_mut().take() {
                a.shutdown();
            }
        });
    }
    gapp.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_names_carry_title_and_resolution() {
        let page = "https://www.youtube.com/watch?v=x";
        let mut it = Item { url: page.into(), page: page.into(), title: "Basi - YouTube".into(), ..Default::default() };
        assert_eq!(ytdlp_template(&it), "%(title).150B%(height& [{}p]|)s.%(ext)s");
        it.quality = "audio".into();
        assert_eq!(ytdlp_template(&it), "%(title).150B [audio].%(ext)s");
        let hls = Item {
            url: "https://cdn.x/master.m3u8".into(),
            page: "https://x/tonton".into(),
            title: "Diskon 100% - YouTube".into(),
            ..Default::default()
        };
        assert_eq!(ytdlp_template(&hls), "Diskon 100%%%(height& [{}p]|)s.%(ext)s");
        let untitled = Item { url: "https://cdn.x/a.m3u8".into(), ..Default::default() };
        assert_eq!(ytdlp_template(&untitled), "video%(height& [{}p]|)s.%(ext)s");
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

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
//! Menerima tautan dari ekstensi browser lewat ws://127.0.0.1:10007 (lihat protocol.rs), mengunduh dengan aria2
//! (multi-koneksi, resume), yt-dlp (HLS/video halaman), ffmpeg (gabung DASH) dan curl (POST). Inti ada di
//! engine.rs, antarmuka iced di gui.rs.

mod aria2;
mod engine;
mod gui;
mod instance;
mod jobs;
mod model;
mod platform;
mod protocol;
mod update;
mod ws;

pub use engine::Event;
use model::Settings;
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

pub const APP_ID: &str = "io.github.kamisama_v0.KaraDM";

fn selftest_download(a: &aria2::Aria2, url: &str, dir: &Path) -> bool {
    let opts = json!({"dir": dir.to_string_lossy(), "split": "4", "max-connection-per-server": "4"});
    let gid = match a.call("aria2.addUri", vec![json!([url]), opts]) {
        Ok(Value::String(g)) => g,
        other => {
            println!("test download: FAILED to add ({other:?})");
            return false;
        }
    };
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(500));
        let Ok(s) = a.call("aria2.tellStatus", vec![json!(gid)]) else { continue };
        match s.get("status").and_then(Value::as_str) {
            Some("complete") => {
                println!("test download: OK {} bytes", engine::num(&s, "completedLength"));
                return true;
            }
            Some("error") => {
                println!("test download: FAILED {}", s.get("errorMessage").and_then(Value::as_str).unwrap_or("?"));
                return false;
            }
            _ => {}
        }
    }
    println!("test download: FAILED, timed out");
    false
}

/// Nyalakan aria2 dan server WebSocket tanpa jendela, laporkan, lalu keluar. Dipakai CI dan untuk diagnosis.
fn selftest() -> ExitCode {
    let dir = std::env::temp_dir().join(format!("kara-dm-selftest-{}", std::process::id()));
    if let Err(e) = platform::create_private_dir(&dir) {
        println!("test folder: FAILED {e}");
        return ExitCode::FAILURE;
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
                println!("aria2: FAILED {e}");
                false
            }
        },
        Err(e) => {
            println!("aria2: FAILED {e}");
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
        Err(e) => println!("websocket: could not open port {} ({e})", protocol::PORT),
    }
    for t in ["yt-dlp", "ffmpeg", "curl"] {
        println!("{t}: {}", if platform::has_tool(t) { "found" } else { "missing (related features off)" });
    }
    println!("download folder: {}", Settings::default().dir().display());
    if let Ok(a) = aria {
        a.shutdown();
    }
    let _ = std::fs::remove_dir_all(&dir);
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("kara-dm {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--selftest") {
        return selftest();
    }
    // Dibuka ulang dari Settings: tunggu instance lama selesai menutup diri sebelum mengambil alih.
    if args.iter().any(|a| a == "--after-restart") {
        for _ in 0..50 {
            if !instance::running() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    if instance::forward(&args) {
        return ExitCode::SUCCESS;
    }
    // Renderer dipilih sebelum thread apa pun jalan: software (tiny-skia, ±10 MB) kecuali pengguna menyalakan
    // "Use hardware acceleration" (GPU lewat wgpu).
    let settings = Settings::load(&platform::config_dir().join("settings.json"));
    if std::env::var_os("ICED_BACKEND").is_none() {
        let backend = if settings.hardware_acceleration { "wgpu,tiny-skia" } else { "tiny-skia" };
        // SAFETY: masih satu thread; belum ada thread lain yang membaca lingkungan proses.
        unsafe { std::env::set_var("ICED_BACKEND", backend) };
    }
    let engine = match engine::Engine::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("kara-dm: could not start: {e}");
            platform::notify("Kara Download Manager could not start", &e, None);
            return ExitCode::FAILURE;
        }
    };
    let args = args.into_iter().filter(|a| a != "--after-restart").collect();
    match gui::run(engine, args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kara-dm: {e}");
            ExitCode::FAILURE
        }
    }
}

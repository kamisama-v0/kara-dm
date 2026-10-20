//! Perbedaan antar-OS dikumpulkan di sini: folder aplikasi, berkas privat, alat bantu eksternal, notifikasi.
//! Folder diambil dari GLib (XDG di Linux/Flatpak, %LOCALAPPDATA% di Windows).

use gtk::prelude::*;
use gtk::{gio, glib};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn home() -> PathBuf {
    glib::home_dir()
}

/// Folder Downloads milik sistem (mengikuti nama lokal, mis. ~/Unduhan), jatuh ke ~/Downloads.
pub fn downloads_dir() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(|| home().join("Downloads"))
}

pub fn config_dir() -> PathBuf {
    glib::user_config_dir().join("kara-dm")
}

pub fn data_dir() -> PathBuf {
    glib::user_data_dir().join("kara-dm")
}

pub fn cache_dir() -> PathBuf {
    glib::user_cache_dir().join("kara-dm")
}

/// Tempat berkas sesaat (conf aria2 berisi token, cookie untuk yt-dlp). Linux: $XDG_RUNTIME_DIR (tmpfs, 0700).
pub fn runtime_dir() -> PathBuf {
    glib::user_runtime_dir().join("kara-dm")
}

pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Di Flatpak, minta izin jalan di latar + autostart saat login lewat portal Background (pengguna yang memutuskan
/// di dialog sistem). Di luar Flatpak autostart diatur installer (systemd user / Startup Windows).
pub fn request_flatpak_autostart() {
    if !in_flatpak() {
        return;
    }
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
    let opts = glib::VariantDict::new(None);
    opts.insert_value("reason", &"Catching browser downloads since sign-in".to_variant());
    opts.insert_value("autostart", &true.to_variant());
    opts.insert_value("commandline", &vec!["kara-dm".to_string(), "--service".to_string()].to_variant());
    let params = glib::Variant::tuple_from_iter(["".to_variant(), opts.end()]);
    bus.call(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        |r| {
            if let Err(e) = r {
                eprintln!("kara-dm: portal Background: {e}");
            }
        },
    );
}

/// Folder yang hanya bisa dibaca pemiliknya (0700 di Unix; di Windows profil pengguna sudah ber-ACL per user).
pub fn create_private_dir(p: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(p)
}

/// Berkas baru (gagal kalau sudah ada) yang hanya bisa dibaca pemiliknya.
pub fn create_private_file(p: &Path) -> io::Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(p)
}

/// Salinan alat bantu yang dibundel di samping kara-dm.exe, atau di subfolder bernama sama (FFmpeg: `ffmpeg\`
/// berisi ffmpeg.exe, ffprobe.exe dan DLL-nya, terpisah dari DLL GTK).
pub fn bundled_tool(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let sub = if name == "ffprobe" { "ffmpeg" } else { name };
    [dir.join(&file), dir.join(sub).join(&file)].into_iter().find(|p| p.is_file())
}

/// Perintah untuk alat bantu (aria2c, yt-dlp, ffmpeg, curl). Di Windows dipakai salinan yang dibundel bila ada,
/// dan tidak memunculkan jendela konsol.
pub fn tool(name: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = match bundled_tool(name) {
            Some(p) => Command::new(p),
            None => Command::new(name),
        };
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
    #[cfg(not(windows))]
    Command::new(name)
}

pub fn has_tool(name: &str) -> bool {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    bundled_tool(name).is_some() || glib::find_program_in_path(&file).is_some()
}

fn firefox_version() -> Option<String> {
    if in_flatpak() || cfg!(windows) {
        return None;
    }
    let out = Command::new("firefox").arg("--version").output().ok()?;
    let v = String::from_utf8(out.stdout).ok()?.split_whitespace().last()?.to_string();
    v.chars().next().filter(char::is_ascii_digit)?;
    Some(v.split('.').next()?.to_string())
}

/// User-Agent mirip Firefox di OS ini, supaya server (mis. Cloudflare) tidak menolak cookie sesi browser.
pub fn user_agent() -> String {
    let v = firefox_version().unwrap_or_else(|| "140".into());
    let os = if cfg!(windows) { "Windows NT 10.0; Win64; x64" } else { "X11; Linux x86_64" };
    format!("Mozilla/5.0 ({os}; rv:{v}.0) Gecko/20100101 Firefox/{v}.0")
}

#[cfg(unix)]
fn spawn_quiet(mut cmd: Command) -> bool {
    match cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(mut c) => {
            std::thread::spawn(move || c.wait());
            true
        }
        Err(_) => false,
    }
}

/// Notifikasi desktop. Omarchy punya pembantu sendiri (bisa membuka folder saat diklik); selain itu GNotification
/// (portal di Flatpak, Shell di GNOME/KDE, Windows).
pub fn notify(app: &gio::Application, title: &str, body: &str, open_dir: Option<&str>) {
    #[cfg(unix)]
    if !in_flatpak() && has_tool("omarchy-notification-send") {
        let mut cmd = Command::new("omarchy-notification-send");
        cmd.args(["-g", "󰇚", title, body]);
        if let Some(d) = open_dir {
            cmd.args(["--exec", "xdg-open", d]);
        }
        if spawn_quiet(cmd) {
            return;
        }
    }
    let n = gio::Notification::new(title);
    n.set_body(Some(body));
    if let Some(d) = open_dir {
        n.set_default_action_and_target_value("app.open-path", Some(&d.to_variant()));
    }
    app.send_notification(None, &n);
}

//! Perbedaan antar-OS dikumpulkan di sini: folder aplikasi, berkas privat, alat bantu eksternal, notifikasi,
//! membuka berkas/folder. Folder mengikuti aturan GLib versi sebelumnya (variabel XDG_* bila diset, selain itu
//! XDG di Linux dan %LOCALAPPDATA% di Windows), jadi pengaturan dan riwayat lama tetap terbaca.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

/// Folder Downloads milik sistem (mengikuti nama lokal, mis. ~/Unduhan), jatuh ke ~/Downloads.
pub fn downloads_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| home().join("Downloads"))
}

fn xdg(var: &str, fallback: Option<PathBuf>) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).or(fallback).unwrap_or_else(home)
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", if cfg!(windows) { dirs::data_local_dir() } else { dirs::config_dir() }).join("kara-dm")
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", dirs::data_local_dir()).join("kara-dm")
}

pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", dirs::cache_dir()).join("kara-dm")
}

/// Tempat berkas sesaat (conf aria2 berisi token, cookie untuk yt-dlp). Linux: $XDG_RUNTIME_DIR (tmpfs, 0700).
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::runtime_dir)
        .map(|d| d.join("kara-dm"))
        .unwrap_or_else(cache_dir)
}

pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Di Flatpak, minta izin jalan di latar + autostart saat login lewat portal Background (pengguna yang memutuskan
/// di dialog sistem). Di luar Flatpak autostart diatur installer (systemd user / Startup Windows). Lewat `gdbus`
/// dari runtime GNOME, tanpa menunggu jawabannya.
pub fn request_flatpak_autostart() {
    if !in_flatpak() {
        return;
    }
    let opts = "{'reason': <'Catching browser downloads since sign-in'>, 'autostart': <true>, \
                'commandline': <['kara-dm', '--service']>}";
    let spawned = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Background.RequestBackground",
            "",
            opts,
        ])
        .stdout(std::process::Stdio::null())
        .spawn();
    if let Err(e) = spawned {
        eprintln!("kara-dm: portal Background: {e}");
    }
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
/// berisi ffmpeg.exe, ffprobe.exe dan DLL-nya).
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

fn in_path(file: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(file).is_file()))
}

pub fn has_tool(name: &str) -> bool {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    bundled_tool(name).is_some() || in_path(&file)
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

/// Notifikasi desktop. Omarchy punya pembantu sendiri (bisa membuka folder saat diklik); selain itu notifikasi
/// sistem (D-Bus di Linux, toast di Windows). Dikirim dari thread terpisah agar tidak menahan antarmuka.
pub fn notify(title: &str, body: &str, open_dir: Option<&str>) {
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
    let _ = open_dir;
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().appname("Kara Download Manager").summary(&title).body(&body).show();
    });
}

/// Buka berkas atau alamat dengan program bawaan sistem.
pub fn open(target: impl AsRef<std::ffi::OsStr>) {
    if let Err(e) = opener::open(target) {
        eprintln!("kara-dm: open: {e}");
    }
}

/// Buka folder berisi berkas itu dengan berkasnya terpilih.
pub fn reveal(path: &Path) {
    if opener::reveal(path).is_err() {
        open(path.parent().unwrap_or(path));
    }
}

/// Jendela tanpa bingkai Windows (title bar digambar sendiri): pasang ikon aplikasi dari resource exe (taskbar,
/// Alt-Tab) dan minta DWM membulatkan sudutnya seperti jendela Windows 11 biasa.
#[cfg(windows)]
pub fn style_window(hwnd: isize) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn LoadImageW(instance: isize, name: *const u16, kind: u32, cx: i32, cy: i32, flags: u32) -> isize;
        fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn GetSystemMetrics(index: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const std::ffi::c_void, size: u32) -> i32;
    }
    const IMAGE_ICON: u32 = 1;
    const LR_SHARED: u32 = 0x8000;
    const WM_SETICON: u32 = 0x0080;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;
    // SAFETY: hwnd milik jendela yang masih hidup (dipanggil dari window::run); resource ikon 1 disematkan build.rs
    // dan dimuat bersama (LR_SHARED, tidak perlu dibebaskan); nilai atribut DWM berukuran 4 byte seperti yang diminta.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        // (ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)
        for (kind, metric) in [(0usize, 49), (1, 11)] {
            let size = GetSystemMetrics(metric);
            let icon = LoadImageW(instance, std::ptr::without_provenance::<u16>(1), IMAGE_ICON, size, size, LR_SHARED);
            if icon != 0 {
                SendMessageW(hwnd, WM_SETICON, kind, icon);
            }
        }
        let round = DWMWCP_ROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, (&raw const round).cast(), 4);
    }
}

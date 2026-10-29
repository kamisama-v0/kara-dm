//! Ikon di area notifikasi (system tray). Klik kiri membuka jendela; klik kanan memberi menu Open / Settings /
//! Quit. Hanya di Windows untuk sekarang: di Linux tray-icon butuh GTK + libappindicator.

pub enum TrayAction {
    Open,
    Settings,
    Quit,
}

#[cfg(windows)]
pub struct Tray {
    _icon: tray_icon::TrayIcon,
    open: tray_icon::menu::MenuId,
    settings: tray_icon::menu::MenuId,
    quit: tray_icon::menu::MenuId,
}

#[cfg(windows)]
impl Tray {
    pub fn new() -> Option<Tray> {
        use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
        follow_system_menu_theme();
        let open = MenuItem::new("Open Kara DM", true, None);
        let settings = MenuItem::new("Settings", true, None);
        let quit = MenuItem::new("Quit (stop catching downloads)", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &settings, &PredefinedMenuItem::separator(), &quit]).ok()?;
        // Ikon aplikasi yang disematkan build.rs (resource ikon pertama kara-dm.exe).
        let icon = tray_icon::Icon::from_resource(1, Some((32, 32))).ok();
        let mut builder = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("Kara Download Manager");
        if let Some(icon) = icon {
            builder = builder.with_icon(icon);
        }
        let icon = builder.build().map_err(|e| eprintln!("kara-dm: tray: {e}")).ok()?;
        Some(Tray { _icon: icon, open: open.id().clone(), settings: settings.id().clone(), quit: quit.id().clone() })
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent, menu::MenuEvent};
        let mut out = vec![];
        while let Ok(e) = TrayIconEvent::receiver().try_recv() {
            match e {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => out.push(TrayAction::Open),
                _ => {}
            }
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if e.id == self.open {
                out.push(TrayAction::Open);
            } else if e.id == self.settings {
                out.push(TrayAction::Settings);
            } else if e.id == self.quit {
                out.push(TrayAction::Quit);
            }
        }
        out
    }
}

/// Menu pop-up Win32 (menu tray) selalu terang kecuali aplikasi memintanya ikut tema: `SetPreferredAppMode(AllowDark)`
/// membuatnya gelap saat Windows memakai mode gelap dan terang saat mode terang, termasuk bila tema diganti saat
/// aplikasi berjalan. Fungsi uxtheme ini tidak punya nama ekspor, hanya nomor urut (135, 136; Windows 10 1903+);
/// bila tidak ada, menu tetap terang.
#[cfg(windows)]
fn follow_system_menu_theme() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> isize;
        fn GetProcAddress(module: isize, name: *const u8) -> Option<unsafe extern "system" fn()>;
    }
    const ALLOW_DARK: i32 = 1;
    let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
    // SAFETY: nama berakhiran NUL; nomor urut dikirim sebagai pointer (MAKEINTRESOURCE) seperti yang diminta
    // GetProcAddress, dan tanda tangan fungsi sesuai yang dipakai Windows sejak 1903.
    unsafe {
        let module = LoadLibraryW(name.as_ptr());
        if module == 0 {
            return;
        }
        if let Some(f) = GetProcAddress(module, 135 as *const u8) {
            let set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set_preferred_app_mode(ALLOW_DARK);
        }
        if let Some(f) = GetProcAddress(module, 136 as *const u8) {
            let flush_menu_themes = f;
            flush_menu_themes();
        }
    }
}

#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn new() -> Option<Tray> {
        None
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        vec![]
    }
}

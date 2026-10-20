//! Antarmuka iced. Berjalan sebagai daemon: tanpa jendela saat di latar belakang (hanya inti dan ikon tray yang
//! hidup); jendela utama dan jendela progres dibuka saat diperlukan.
//!
//! Gaya: gaya aplikasi Windows 11 di atas material Mica dari OS (kacanya datang dari sistem, bukan dari efek
//! gambar sendiri). Permukaan datar, satu warna aksen untuk aksi utama, progres dan pilihan; ikon Phosphor;
//! bayangan hanya untuk yang benar-benar melayang (menu, dialog).

use crate::engine::{Effect, Engine};
use crate::model::{self, CATEGORIES, Item, Settings, Status};
use crate::tray::{Tray, TrayAction};
use crate::{platform, protocol};
use iced::widget::{
    button, center, column, container, mouse_area, opaque, progress_bar, responsive, row, scrollable, slider, space,
    stack, text, text_input, toggler, tooltip,
};
use iced::{
    Alignment, Background, Border, Color, Element, Event, Font, Length, Padding, Point, Shadow, Subscription, Task,
    Theme, Vector, event, font, keyboard, window,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const EXTENSION_URL: &str = "https://github.com/kamisama-v0/kara-dm#browser-extension";

pub fn run(engine: Engine, args: Vec<String>) -> iced::Result {
    // iced memanggil boot sekali, tapi tipenya Fn: inti diserahkan lewat Mutex<Option<_>>.
    let start = std::sync::Mutex::new(Some((engine, args)));
    let boot = move || {
        let (engine, args) = start.lock().unwrap().take().expect("boot dipanggil sekali");
        Gui::boot(engine, args)
    };
    iced::daemon(boot, Gui::update, Gui::view)
        .title(Gui::title)
        .subscription(Gui::subscription)
        .theme(|_: &Gui, _| theme())
        .style(|_: &Gui, _: &Theme| iced::theme::Style { background_color: Color::TRANSPARENT, text_color: TEXT })
        .font(include_bytes!("../assets/fonts/Phosphor.ttf").as_slice())
        .default_font(UI)
        .run()
}

// ---------- token desain ----------

const UI: Font = if cfg!(windows) { Font::with_name("Segoe UI") } else { Font::DEFAULT };
const STRONG: Font = Font { weight: font::Weight::Semibold, ..UI };
const ICONS: Font = Font::with_name("Phosphor");

const TEXT: Color = Color::from_rgb(0.953, 0.953, 0.953);
const SUB: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.66);
const FAINT: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.38);
const ACCENT: Color = Color::from_rgb(0.298, 0.761, 1.0);
const ON_ACCENT: Color = Color::from_rgb(0.0, 0.075, 0.12);
const DANGER: Color = Color::from_rgb(1.0, 0.60, 0.643);
const OK: Color = Color::from_rgb(0.424, 0.796, 0.373);

fn white(a: f32) -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, a)
}

/// Tema gelap dengan warna aksen aplikasi (slider, toggle, fokus kolom isian).
fn theme() -> Theme {
    Theme::custom(
        "Kara",
        iced::theme::Palette { primary: ACCENT, success: OK, danger: DANGER, ..iced::theme::Palette::DARK },
    )
}

// Ikon Phosphor (regular), kode karakter dari @phosphor-icons/web 2.1.2.
mod ic {
    pub const ADD: char = '\u{e3d4}';
    pub const PLAY: char = '\u{e3d0}';
    pub const PAUSE: char = '\u{e39e}';
    pub const TRASH: char = '\u{e4a6}';
    pub const FOLDER: char = '\u{e256}';
    pub const LINK: char = '\u{e2e2}';
    pub const GEAR: char = '\u{e272}';
    pub const MORE: char = '\u{e1fe}';
    pub const OPEN: char = '\u{e5de}';
    pub const CHECK: char = '\u{e182}';
    pub const DOWNLOAD: char = '\u{e20c}';
    pub const FILE: char = '\u{e230}';
    pub const VIDEO: char = '\u{e792}';
    pub const MUSIC: char = '\u{e33c}';
    pub const ZIP: char = '\u{e958}';
    pub const DOC: char = '\u{e23a}';
    pub const APP: char = '\u{e5da}';
    pub const IMAGE: char = '\u{e2ca}';
    pub const RETRY: char = '\u{e036}';
    pub const WARN: char = '\u{e4e2}';
    pub const ALL: char = '\u{e466}';
    pub const DONE: char = '\u{e184}';
    pub const ACTIVE: char = '\u{e03e}';
    pub const POWER: char = '\u{e3da}';
    pub const GLOBE: char = '\u{e288}';
}

fn icon<'a>(c: char, size: u16) -> text::Text<'a> {
    text(c.to_string()).font(ICONS).size(f32::from(size)).line_height(1.0)
}

fn sub<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(12).color(SUB)
}

fn category_icon(key: &str) -> char {
    match key {
        "all" => ic::ALL,
        "unfinished" => ic::ACTIVE,
        "finished" => ic::DONE,
        "Video" => ic::VIDEO,
        "Music" => ic::MUSIC,
        "Archives" => ic::ZIP,
        "Documents" => ic::DOC,
        "Programs" => ic::APP,
        "Images" => ic::IMAGE,
        _ => ic::FILE,
    }
}

/// Lapisan isi di atas Mica: sedikit lebih terang dengan tepi tipis, tanpa bayangan.
fn layer() -> container::Style {
    container::Style {
        background: Some(Background::Color(white(0.045))),
        border: Border { color: white(0.07), width: 1.0, radius: 8.0.into() },
        text_color: Some(TEXT),
        ..Default::default()
    }
}

/// Permukaan yang melayang (menu, dialog, tooltip): pekat, dengan bayangan.
fn flyout(radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.17, 0.17, 0.18, 0.985))),
        border: Border { color: white(0.10), width: 1.0, radius: radius.into() },
        text_color: Some(TEXT),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.40),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        snap: true,
    }
}

fn btn_style(bg: Option<Color>, fg: Color, stroke: Option<Color>) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: Border {
            color: stroke.unwrap_or(Color::TRANSPARENT),
            width: stroke.map_or(0.0, |_| 1.0),
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn label_with_icon<'a>(c: char, label: &'a str) -> Element<'a, Msg> {
    row![icon(c, 16), text(label).size(14)].spacing(8).align_y(Alignment::Center).into()
}

/// Aksi utama: satu-satunya tombol berwarna aksen di layar.
fn primary<'a>(content: impl Into<Element<'a, Msg>>, msg: Msg) -> button::Button<'a, Msg> {
    button(content).padding([7, 14]).on_press(msg).style(|_, s| {
        let bg = match s {
            button::Status::Hovered => Color::from_rgb(0.40, 0.80, 1.0),
            button::Status::Pressed => Color::from_rgb(0.26, 0.66, 0.88),
            _ => ACCENT,
        };
        btn_style(Some(bg), ON_ACCENT, None)
    })
}

/// Aksi biasa: tanpa latar sampai disentuh kursor.
fn subtle<'a>(content: impl Into<Element<'a, Msg>>, msg: Option<Msg>) -> button::Button<'a, Msg> {
    button(content).padding([7, 12]).on_press_maybe(msg).style(|_, s| match s {
        button::Status::Hovered => btn_style(Some(white(0.07)), TEXT, None),
        button::Status::Pressed => btn_style(Some(white(0.04)), SUB, None),
        button::Status::Disabled => btn_style(None, FAINT, None),
        button::Status::Active => btn_style(None, TEXT, None),
    })
}

/// Tombol sekunder di dialog: berbingkai tipis.
fn secondary<'a>(label: &'a str, msg: Msg) -> button::Button<'a, Msg> {
    button(text(label).size(14)).padding([7, 14]).on_press(msg).style(|_, s| {
        let bg = if matches!(s, button::Status::Hovered) { white(0.09) } else { white(0.055) };
        btn_style(Some(bg), TEXT, Some(white(0.09)))
    })
}

fn icon_button<'a>(c: char, tip: &'a str, msg: Msg) -> Element<'a, Msg> {
    tooltip(
        subtle(container(icon(c, 16)).center(18), Some(msg)).padding(6),
        container(text(tip).size(12)).padding([4, 8]).style(|_| flyout(6.0)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(500))
    .into()
}

fn menu_item<'a>(c: char, label: &'a str, msg: Msg, danger: bool) -> Element<'a, Msg> {
    let color = if danger { DANGER } else { TEXT };
    button(row![icon(c, 16).color(color), text(label).size(14).color(color)].spacing(10).align_y(Alignment::Center))
        .padding([7, 10])
        .width(Length::Fill)
        .on_press(msg)
        .style(|_, s| btn_style(matches!(s, button::Status::Hovered).then(|| white(0.07)), TEXT, None))
        .into()
}

fn separator<'a>() -> Element<'a, Msg> {
    container(space())
        .height(1)
        .width(Length::Fill)
        .style(|_| container::Style { background: Some(Background::Color(white(0.08))), ..Default::default() })
        .into()
}

fn matches(cat: &str, it: &Item) -> bool {
    match cat {
        "all" => true,
        "unfinished" => !it.status.finished(),
        "finished" => it.status.finished(),
        c => it.category() == c,
    }
}

/// Baris kedua sebuah unduhan: apa yang sedang terjadi, dalam satu kalimat pendek.
fn meta(it: &Item) -> String {
    let of = |done: u64, size: u64| {
        if size > 0 { format!("{} of {}", model::fmt_size(done), model::fmt_size(size)) } else { model::fmt_size(done) }
    };
    match it.status {
        Status::Active => {
            let mut parts = vec![of(it.done, it.size)];
            if it.speed > 0 {
                parts.push(model::fmt_speed(it.speed));
            }
            let eta = model::fmt_eta(it.eta());
            if !eta.is_empty() {
                parts.push(format!("{eta} left"));
            }
            parts.join("  ·  ")
        }
        Status::Waiting => format!("Queued  ·  {}", of(it.done, it.size)),
        Status::Paused => format!("Paused  ·  {}", of(it.done, it.size)),
        Status::Merging => "Merging video and audio…".into(),
        Status::Complete => {
            let host = protocol::url_host(&it.url).trim_start_matches("www.").to_string();
            format!("{}  ·  {}  ·  {}", model::fmt_size(it.size), model::fmt_date(it.finished.max(it.added)), host)
        }
        Status::Error => it.error.clone(),
    }
}

// ---------- state ----------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Win {
    Main,
    Progress,
}

enum Modal {
    Add(String),
    Remove(Vec<String>),
    Settings(Box<Settings>, String),
    About,
    Message(String, String),
}

enum Popup {
    /// Menu klik kanan untuk unduhan terpilih, di posisi kursor.
    Context(Point),
    /// Menu ⋯ di command bar.
    More,
}

pub struct Gui {
    engine: Engine,
    tray: Option<Tray>,
    windows: HashMap<window::Id, Win>,
    main: Option<window::Id>,
    progress: HashMap<window::Id, String>,
    selected: HashSet<String>,
    hovered: Option<String>,
    category: &'static str,
    modal: Option<Modal>,
    popup: Option<Popup>,
    cursor: Point,
    ctrl: bool,
    /// Renderer saat aplikasi mulai: mengubah "Use hardware acceleration" baru berlaku setelah dibuka ulang.
    hw_at_start: bool,
    /// Konfirmasi tombol Cancel di jendela progres (klik kedua menghapus beserta berkasnya).
    cancel_armed: HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Tick,
    Opened(window::Id),
    Closed(window::Id),
    Key(keyboard::Key, keyboard::Modifiers),
    Modifiers(keyboard::Modifiers),
    Cursor(Point),
    Category(&'static str),
    Hover(Option<String>),
    Click(String),
    Activate(String),
    ContextMenu(String),
    ClosePopup,
    // command bar & menu
    ShowAdd,
    Resume,
    Pause,
    AskRemove,
    OpenSelected,
    FolderSelected,
    CopyUrl,
    ShowMore,
    OpenDownloadDir,
    ClearFinished,
    ShowSettings,
    CheckUpdate,
    StartUpdate,
    ShowAbout,
    Quit,
    // dialog
    CloseModal,
    AddText(String),
    AddPasted(Option<String>),
    AddConfirm,
    RemoveConfirm(bool),
    SetConnections(u32),
    SetConcurrent(u32),
    SetSpeed(String),
    SetToggle(&'static str, bool),
    PickFolder,
    FolderPicked(Option<std::path::PathBuf>),
    SaveSettings,
    Restart,
    OpenUrl(&'static str),
    // tombol di baris dan jendela progres
    PauseItem(String),
    ResumeItem(String),
    ProgressCancel(String),
    ProgressHide(window::Id),
    OpenFile(String),
    OpenFolder(String),
    Noop,
}

impl Gui {
    fn boot(mut engine: Engine, args: Vec<String>) -> (Gui, Task<Msg>) {
        engine.run_args(args);
        let hw_at_start = engine.settings.hardware_acceleration;
        let mut gui = Gui {
            engine,
            tray: Tray::new(),
            windows: HashMap::new(),
            main: None,
            progress: HashMap::new(),
            selected: HashSet::new(),
            hovered: None,
            category: "all",
            modal: None,
            popup: None,
            cursor: Point::ORIGIN,
            ctrl: false,
            hw_at_start,
            cancel_armed: HashSet::new(),
        };
        let task = gui.apply_effects();
        (gui, task)
    }

    fn title(&self, id: window::Id) -> String {
        match self.windows.get(&id) {
            Some(Win::Progress) => {
                let name =
                    self.progress.get(&id).and_then(|i| self.engine.item(i)).map(|i| i.display_name().to_string());
                format!("Download: {}", name.unwrap_or_default())
            }
            _ => "Kara Download Manager".into(),
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        // Tanpa jendela cukup 4x per detik (ikon tray tetap responsif); dengan jendela 8x agar progres hidup.
        let every = if self.windows.is_empty() { Duration::from_millis(250) } else { Duration::from_millis(125) };
        Subscription::batch([
            iced::time::every(every).map(|_| Msg::Tick),
            window::close_events().map(Msg::Closed),
            event::listen_with(|ev, status, _| match ev {
                Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => Some(Msg::Modifiers(m)),
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                    if status == event::Status::Ignored =>
                {
                    Some(Msg::Key(key, modifiers))
                }
                _ => None,
            }),
        ])
    }

    // ---------- jendela ----------

    fn open_window(&mut self, kind: Win) -> (window::Id, Task<Msg>) {
        let settings = match kind {
            Win::Main => window::Settings {
                size: iced::Size::new(1040.0, 640.0),
                min_size: Some(iced::Size::new(720.0, 420.0)),
                transparent: true,
                blur: true,
                ..Default::default()
            },
            Win::Progress => window::Settings {
                size: iced::Size::new(560.0, 340.0),
                resizable: false,
                transparent: true,
                blur: true,
                ..Default::default()
            },
        };
        let (id, task) = window::open(settings);
        self.windows.insert(id, kind);
        (id, task.map(Msg::Opened))
    }

    fn open_main(&mut self) -> Task<Msg> {
        match self.main {
            Some(id) => window::gain_focus(id),
            None => {
                let (id, task) = self.open_window(Win::Main);
                self.main = Some(id);
                task
            }
        }
    }

    fn show_progress(&mut self, item: String) -> Task<Msg> {
        if let Some((id, _)) = self.progress.iter().find(|(_, i)| **i == item) {
            return window::gain_focus(*id);
        }
        let (id, task) = self.open_window(Win::Progress);
        self.progress.insert(id, item);
        task
    }

    fn apply_effects(&mut self) -> Task<Msg> {
        let mut tasks = vec![];
        for e in std::mem::take(&mut self.engine.effects) {
            tasks.push(match e {
                Effect::OpenMain => self.open_main(),
                Effect::ShowProgress(id) => self.show_progress(id),
                Effect::CloseProgress(item) => {
                    self.selected.remove(&item);
                    match self.progress.iter().find(|(_, i)| **i == item).map(|(w, _)| *w) {
                        Some(w) => window::close(w),
                        None => Task::none(),
                    }
                }
                Effect::Message(t, b) => {
                    self.modal = Some(Modal::Message(t, b));
                    self.open_main()
                }
                Effect::Quit => return self.quit(),
            });
        }
        Task::batch(tasks)
    }

    fn quit(&mut self) -> Task<Msg> {
        self.engine.shutdown();
        self.tray = None;
        iced::exit()
    }

    fn selection(&self) -> Vec<String> {
        self.selected.iter().filter(|id| self.engine.item(id).is_some()).cloned().collect()
    }

    fn open_settings(&mut self) {
        self.popup = None;
        let s = self.engine.settings.clone();
        let speed = s.speed_limit_kib.to_string();
        self.modal = Some(Modal::Settings(Box::new(s), speed));
    }

    // ---------- update ----------

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let task = match msg {
            Msg::Tick => {
                self.engine.tick();
                let actions = self.tray.as_ref().map(Tray::poll).unwrap_or_default();
                let mut tasks = vec![];
                for a in actions {
                    tasks.push(match a {
                        TrayAction::Open => self.open_main(),
                        TrayAction::Settings => {
                            self.open_settings();
                            self.open_main()
                        }
                        TrayAction::Quit => return self.quit(),
                    });
                }
                Task::batch(tasks)
            }
            Msg::Opened(id) => window::run(id, |w| apply_vibrancy(w)).map(|_| Msg::Noop),
            Msg::Closed(id) => {
                if self.main == Some(id) {
                    self.main = None;
                    self.popup = None;
                    self.hovered = None;
                }
                if let Some(item) = self.progress.remove(&id) {
                    self.cancel_armed.remove(&item);
                }
                self.windows.remove(&id);
                Task::none()
            }
            Msg::Modifiers(m) => {
                self.ctrl = m.command();
                Task::none()
            }
            Msg::Cursor(p) => {
                self.cursor = p;
                Task::none()
            }
            Msg::Key(key, m) => match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    if self.popup.take().is_none() {
                        self.modal = None;
                    }
                    Task::none()
                }
                keyboard::Key::Named(keyboard::key::Named::Delete) if self.modal.is_none() => self.ask_remove(),
                keyboard::Key::Named(keyboard::key::Named::Enter) if self.modal.is_none() => {
                    match self.selection().first().cloned() {
                        Some(id) => self.update(Msg::Activate(id)),
                        None => Task::none(),
                    }
                }
                keyboard::Key::Character("n") if m.command() => self.show_add(),
                keyboard::Key::Character("o") if m.command() => {
                    self.engine.open_download_dir();
                    Task::none()
                }
                keyboard::Key::Character("a") if m.command() && self.modal.is_none() => {
                    self.selected =
                        self.engine.items.iter().filter(|i| matches(self.category, i)).map(|i| i.id.clone()).collect();
                    Task::none()
                }
                _ => Task::none(),
            },
            Msg::Category(c) => {
                self.category = c;
                Task::none()
            }
            Msg::Hover(id) => {
                self.hovered = id;
                Task::none()
            }
            Msg::Click(id) => {
                self.popup = None;
                if self.ctrl {
                    if !self.selected.remove(&id) {
                        self.selected.insert(id);
                    }
                } else {
                    self.selected = HashSet::from([id]);
                }
                Task::none()
            }
            Msg::Activate(id) => {
                self.selected = HashSet::from([id.clone()]);
                match self.engine.item(&id) {
                    Some(it) if it.status.finished() => {
                        self.engine.open_file(&id);
                        Task::none()
                    }
                    Some(_) => self.show_progress(id),
                    None => Task::none(),
                }
            }
            Msg::ContextMenu(id) => {
                if !self.selected.contains(&id) {
                    self.selected = HashSet::from([id]);
                }
                self.popup = Some(Popup::Context(self.cursor));
                Task::none()
            }
            Msg::ClosePopup => {
                self.popup = None;
                Task::none()
            }
            Msg::ShowAdd => self.show_add(),
            Msg::Resume => {
                self.popup = None;
                let ids = self.selection();
                self.engine.resume(&ids);
                Task::none()
            }
            Msg::Pause => {
                self.popup = None;
                let ids = self.selection();
                self.engine.pause(&ids);
                Task::none()
            }
            Msg::AskRemove => self.ask_remove(),
            Msg::OpenSelected => {
                self.popup = None;
                for id in self.selection() {
                    self.engine.open_file(&id);
                }
                Task::none()
            }
            Msg::FolderSelected => {
                self.popup = None;
                if let Some(id) = self.selection().first() {
                    self.engine.open_folder(id);
                }
                Task::none()
            }
            Msg::CopyUrl => {
                self.popup = None;
                let urls: Vec<String> =
                    self.selection().iter().filter_map(|id| self.engine.item(id).map(|i| i.url.clone())).collect();
                iced::clipboard::write(urls.join("\n"))
            }
            Msg::ShowMore => {
                self.popup = match self.popup {
                    Some(Popup::More) => None,
                    _ => Some(Popup::More),
                };
                Task::none()
            }
            Msg::OpenDownloadDir => {
                self.popup = None;
                self.engine.open_download_dir();
                Task::none()
            }
            Msg::ClearFinished => {
                self.popup = None;
                self.engine.clear_finished();
                Task::none()
            }
            Msg::ShowSettings => {
                self.open_settings();
                Task::none()
            }
            Msg::CheckUpdate => {
                self.popup = None;
                self.engine.check_update(true);
                Task::none()
            }
            Msg::StartUpdate => {
                self.engine.start_update();
                Task::none()
            }
            Msg::ShowAbout => {
                self.popup = None;
                self.modal = Some(Modal::About);
                Task::none()
            }
            Msg::Quit => self.quit(),
            Msg::CloseModal => {
                self.modal = None;
                Task::none()
            }
            Msg::AddText(t) => {
                if let Some(Modal::Add(s)) = &mut self.modal {
                    *s = t;
                }
                Task::none()
            }
            Msg::AddPasted(t) => {
                if let (Some(Modal::Add(s)), Some(t)) = (&mut self.modal, t)
                    && s.is_empty()
                    && protocol::valid_url(t.trim())
                {
                    *s = t.trim().to_string();
                }
                Task::none()
            }
            Msg::AddConfirm => {
                if let Some(Modal::Add(url)) = &self.modal {
                    match protocol::Request::from_url(url) {
                        Ok(req) => {
                            self.engine.add(req, false);
                            self.modal = None;
                        }
                        Err(e) => self.modal = Some(Modal::Message("Invalid URL".into(), e)),
                    }
                }
                Task::none()
            }
            Msg::RemoveConfirm(files) => {
                if let Some(Modal::Remove(ids)) = self.modal.take() {
                    self.engine.remove(&ids, files);
                    for id in &ids {
                        self.selected.remove(id);
                    }
                }
                Task::none()
            }
            Msg::SetConnections(n) => self.edit_settings(|s, _| s.connections = n),
            Msg::SetConcurrent(n) => self.edit_settings(|s, _| s.max_concurrent = n),
            Msg::SetSpeed(t) => {
                self.edit_settings(|_, speed| *speed = t.chars().filter(char::is_ascii_digit).collect())
            }
            Msg::SetToggle(key, v) => self.edit_settings(|s, _| match key {
                "progress" => s.show_progress_window = v,
                "panel" => s.media_panel = v,
                "notify" => s.notify = v,
                "updates" => s.check_updates = v,
                "hw" => s.hardware_acceleration = v,
                _ => {}
            }),
            Msg::PickFolder => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose download folder")
                        .pick_folder()
                        .await
                        .map(|h| h.path().to_path_buf())
                },
                Msg::FolderPicked,
            ),
            Msg::FolderPicked(p) => match p {
                Some(p) => self.edit_settings(|s, _| s.download_dir = p.to_string_lossy().into_owned()),
                None => Task::none(),
            },
            Msg::SaveSettings => {
                self.save_settings();
                Task::none()
            }
            Msg::Restart => {
                self.save_settings();
                self.engine.shutdown();
                self.tray = None;
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--after-restart").spawn();
                }
                iced::exit()
            }
            Msg::OpenUrl(u) => {
                platform::open(u);
                Task::none()
            }
            Msg::PauseItem(id) => {
                self.engine.pause(&[id]);
                Task::none()
            }
            Msg::ResumeItem(id) => {
                self.engine.resume(&[id]);
                Task::none()
            }
            Msg::ProgressCancel(id) => {
                if self.cancel_armed.remove(&id) {
                    self.engine.remove(&[id], true);
                } else {
                    self.cancel_armed.insert(id);
                }
                Task::none()
            }
            Msg::ProgressHide(w) => window::close(w),
            Msg::OpenFile(id) => {
                self.engine.open_file(&id);
                Task::none()
            }
            Msg::OpenFolder(id) => {
                self.engine.open_folder(&id);
                Task::none()
            }
            Msg::Noop => Task::none(),
        };
        Task::batch([task, self.apply_effects()])
    }

    fn save_settings(&mut self) {
        if let Some(Modal::Settings(s, speed)) = self.modal.take() {
            let mut s = *s;
            s.speed_limit_kib = speed.parse().unwrap_or(0);
            self.engine.apply_settings(s);
        }
    }

    fn show_add(&mut self) -> Task<Msg> {
        self.popup = None;
        self.modal = Some(Modal::Add(String::new()));
        Task::batch([
            iced::clipboard::read().map(Msg::AddPasted),
            iced::widget::operation::focus(iced::widget::Id::new("add-url")),
        ])
    }

    fn ask_remove(&mut self) -> Task<Msg> {
        self.popup = None;
        let ids = self.selection();
        if !ids.is_empty() {
            self.modal = Some(Modal::Remove(ids));
        }
        Task::none()
    }

    fn edit_settings(&mut self, f: impl FnOnce(&mut Settings, &mut String)) -> Task<Msg> {
        if let Some(Modal::Settings(s, speed)) = &mut self.modal {
            f(s, speed);
        }
        Task::none()
    }

    // ---------- view: jendela utama ----------

    fn view(&self, id: window::Id) -> Element<'_, Msg> {
        match self.windows.get(&id) {
            Some(Win::Progress) => self.progress_view(id),
            _ => self.main_view(),
        }
    }

    fn main_view(&self) -> Element<'_, Msg> {
        let mut right = column![self.command_bar()].spacing(10);
        if let Some(banner) = self.update_banner() {
            right = right.push(banner);
        }
        right = right.push(self.list());
        right = right.push(self.status_bar());
        let base = row![self.nav(), right.width(Length::Fill).height(Length::Fill)].spacing(12).padding(Padding {
            top: 10.0,
            right: 14.0,
            bottom: 10.0,
            left: 10.0,
        });

        let mut layers = stack![base];
        if let Some(p) = &self.popup {
            layers = layers.push(self.popup_view(p));
        }
        if let Some(m) = &self.modal {
            layers = layers.push(self.modal_view(m));
        }
        mouse_area(layers).on_move(Msg::Cursor).into()
    }

    fn nav(&self) -> Element<'_, Msg> {
        let item = |key: &'static str, label: &'static str| -> Element<'_, Msg> {
            let n = self.engine.items.iter().filter(|it| matches(key, it)).count();
            let active = self.category == key;
            let indicator = container(space()).width(3).height(16).style(move |_| container::Style {
                background: active.then_some(Background::Color(ACCENT)),
                border: Border { radius: 2.0.into(), ..Border::default() },
                ..Default::default()
            });
            button(
                row![
                    indicator,
                    icon(category_icon(key), 16).color(if active { TEXT } else { SUB }),
                    text(label).size(14).width(Length::Fill),
                    sub(if n > 0 { n.to_string() } else { String::new() }),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 8.0, right: 10.0, bottom: 8.0, left: 4.0 })
            .width(Length::Fill)
            .on_press(Msg::Category(key))
            .style(move |_, s| {
                let bg = if active {
                    Some(white(0.07))
                } else if matches!(s, button::Status::Hovered) {
                    Some(white(0.045))
                } else {
                    None
                };
                btn_style(bg, TEXT, None)
            })
            .into()
        };
        let (status, kinds) = CATEGORIES.split_at(3);
        let mut col = column![
            row![icon(ic::DOWNLOAD, 20).color(ACCENT), text("Kara DM").size(16).font(STRONG)]
                .spacing(10)
                .align_y(Alignment::Center)
                .padding(Padding { top: 6.0, right: 0.0, bottom: 14.0, left: 10.0 }),
        ]
        .spacing(2);
        for (key, label) in status {
            col = col.push(item(key, label));
        }
        col =
            col.push(container(sub("File types")).padding(Padding { top: 16.0, right: 0.0, bottom: 6.0, left: 12.0 }));
        for (key, label) in kinds {
            col = col.push(item(key, label));
        }
        col = col.push(space().height(Length::Fill));
        col = col.push(
            button(label_with_icon(ic::GEAR, "Settings"))
                .padding([8, 12])
                .width(Length::Fill)
                .on_press(Msg::ShowSettings)
                .style(|_, s| btn_style(matches!(s, button::Status::Hovered).then(|| white(0.045)), TEXT, None)),
        );
        container(col).width(220).height(Length::Fill).into()
    }

    fn command_bar(&self) -> Element<'_, Msg> {
        let sel = self.selection();
        let any = !sel.is_empty();
        let can = |want: fn(Status) -> bool| sel.iter().any(|id| self.engine.item(id).is_some_and(|i| want(i.status)));
        let resumable = can(|s| matches!(s, Status::Paused | Status::Error));
        let pausable = can(|s| matches!(s, Status::Active | Status::Waiting));
        row![
            primary(
                row![icon(ic::ADD, 16).color(ON_ACCENT), text("Add download").size(14).font(STRONG)]
                    .spacing(8)
                    .align_y(Alignment::Center),
                Msg::ShowAdd
            ),
            space().width(6),
            subtle(label_with_icon(ic::PLAY, "Resume"), resumable.then_some(Msg::Resume)),
            subtle(label_with_icon(ic::PAUSE, "Pause"), pausable.then_some(Msg::Pause)),
            subtle(label_with_icon(ic::TRASH, "Remove"), any.then_some(Msg::AskRemove)),
            space().width(Length::Fill),
            icon_button(ic::FOLDER, "Open download folder", Msg::OpenDownloadDir),
            icon_button(ic::MORE, "More", Msg::ShowMore),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .into()
    }

    fn update_banner(&self) -> Option<Element<'_, Msg>> {
        let r = self.engine.update.as_ref()?;
        let installs = cfg!(windows) && r.installer.is_some();
        let (title, action): (String, Option<(&str, Msg)>) = if self.engine.updating {
            (format!("Downloading Kara DM {}. The app reopens after installing.", r.version), None)
        } else if installs {
            (format!("Kara DM {} is available.", r.version), Some(("Update now", Msg::StartUpdate)))
        } else {
            (format!("Kara DM {} is available.", r.version), Some(("View release", Msg::StartUpdate)))
        };
        let mut bar = row![icon(ic::DOWNLOAD, 16).color(ACCENT), text(title).size(14).width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center);
        if let Some((label, msg)) = action {
            bar = bar.push(primary(text(label).size(14), msg));
        }
        Some(
            container(bar)
                .padding([8, 14])
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.298, 0.761, 1.0, 0.12))),
                    border: Border { color: Color::from_rgba(0.298, 0.761, 1.0, 0.30), width: 1.0, radius: 8.0.into() },
                    text_color: Some(TEXT),
                    ..Default::default()
                })
                .into(),
        )
    }

    fn list(&self) -> Element<'_, Msg> {
        let shown: Vec<&Item> = self.engine.items.iter().rev().filter(|it| matches(self.category, it)).collect();
        let inner: Element<'_, Msg> = if shown.is_empty() {
            let (title, body) = if self.engine.items.is_empty() {
                (
                    "No downloads yet",
                    "Click a download link in your browser with the Kara DM extension, or add a link here.",
                )
            } else {
                ("Nothing here", "No downloads in this category.")
            };
            let mut col = column![icon(ic::DOWNLOAD, 40).color(FAINT), text(title).size(18).font(STRONG), sub(body)]
                .spacing(8)
                .align_x(Alignment::Center);
            if self.engine.items.is_empty() {
                col = col.push(space().height(8)).push(primary(
                    row![icon(ic::ADD, 16).color(ON_ACCENT), text("Add download").size(14).font(STRONG)]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    Msg::ShowAdd,
                ));
            }
            center(col).into()
        } else {
            scrollable(column(shown.into_iter().map(|it| self.item_row(it))).spacing(2).padding(4))
                .height(Length::Fill)
                .into()
        };
        container(inner).width(Length::Fill).height(Length::Fill).style(|_| layer()).into()
    }

    fn item_row<'a>(&'a self, it: &'a Item) -> Element<'a, Msg> {
        let selected = self.selected.contains(&it.id);
        let hovered = self.hovered.as_deref() == Some(it.id.as_str());
        let f = it.fraction() as f32;

        let tile = container(icon(category_icon(it.category()), 18).color(if it.status == Status::Error {
            DANGER
        } else {
            SUB
        }))
        .center(36)
        .style(|_| container::Style {
            background: Some(Background::Color(white(0.05))),
            border: Border { radius: 6.0.into(), ..Border::default() },
            ..Default::default()
        });

        let meta_line = if it.status == Status::Error {
            row![icon(ic::WARN, 13).color(DANGER), text(meta(it)).size(12).color(DANGER).wrapping(text::Wrapping::None)]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![text(meta(it)).size(12).color(SUB).wrapping(text::Wrapping::None)]
        };
        let mut info = column![
            container(text(it.display_name()).size(14).wrapping(text::Wrapping::None)).clip(true),
            container(meta_line).clip(true),
        ]
        .spacing(3);
        if !it.status.finished() && (it.size > 0 || it.status == Status::Active) {
            let bar = match it.status {
                Status::Active | Status::Merging => ACCENT,
                Status::Error => DANGER,
                _ => white(0.38),
            };
            info = info.push(
                container(progress_bar(0.0..=1.0, f).girth(3).style(move |_| progress_bar::Style {
                    background: Background::Color(white(0.10)),
                    bar: Background::Color(bar),
                    border: Border { radius: 2.0.into(), ..Border::default() },
                }))
                .padding(Padding { top: 4.0, right: 0.0, bottom: 0.0, left: 0.0 }),
            );
        }

        let id = it.id.clone();
        let trailing: Element<'_, Msg> = if hovered || selected {
            let mut r = row![].spacing(2).align_y(Alignment::Center);
            match it.status {
                Status::Active | Status::Waiting => {
                    r = r.push(icon_button(ic::PAUSE, "Pause", Msg::PauseItem(id.clone())))
                }
                Status::Paused => r = r.push(icon_button(ic::PLAY, "Resume", Msg::ResumeItem(id.clone()))),
                Status::Error => r = r.push(icon_button(ic::RETRY, "Try again", Msg::ResumeItem(id.clone()))),
                Status::Complete => r = r.push(icon_button(ic::OPEN, "Open", Msg::OpenFile(id.clone()))),
                Status::Merging => {}
            }
            r = r.push(icon_button(ic::FOLDER, "Show in folder", Msg::OpenFolder(id.clone())));
            r.into()
        } else {
            match it.status {
                Status::Active | Status::Paused | Status::Waiting if it.size > 0 => {
                    text(format!("{:.0}%", f * 100.0)).size(13).color(SUB).into()
                }
                Status::Complete => icon(ic::CHECK, 16).color(OK).into(),
                _ => space().into(),
            }
        };

        let row_box = container(
            row![
                tile,
                info.width(Length::Fill),
                container(trailing).width(84).align_right(84).align_y(Alignment::Center)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |_| {
            let bg = if selected {
                Some(Background::Color(Color::from_rgba(0.298, 0.761, 1.0, 0.13)))
            } else if hovered {
                Some(Background::Color(white(0.04)))
            } else {
                None
            };
            container::Style {
                background: bg,
                border: Border { radius: 6.0.into(), ..Border::default() },
                text_color: Some(TEXT),
                ..Default::default()
            }
        });
        let id = it.id.clone();
        mouse_area(row_box)
            .on_press(Msg::Click(id.clone()))
            .on_double_click(Msg::Activate(id.clone()))
            .on_right_press(Msg::ContextMenu(id.clone()))
            .on_enter(Msg::Hover(Some(id)))
            .on_exit(Msg::Hover(None))
            .into()
    }

    fn status_bar(&self) -> Element<'_, Msg> {
        let items = &self.engine.items;
        let active = items.iter().filter(|i| i.status == Status::Active).count();
        let speed: u64 = items.iter().map(|i| i.speed).sum();
        let left = match &self.engine.aria_error {
            Some(e) => format!("Download engine is not running: {e}"),
            None if active > 0 => format!("{} active  ·  {}", active, model::fmt_speed(speed)),
            None => format!("{} downloads", items.len()),
        };
        let (dot, right) = match (&self.engine.ws_error, self.engine.extension_connections()) {
            (Some(_), _) => (DANGER, "Browser catching is off: port 10007 is in use".to_string()),
            (None, 0) => (FAINT, "Browser extension waiting".to_string()),
            (None, _) => (OK, "Browser extension connected".to_string()),
        };
        let right = tooltip(
            row![text("●").size(10).color(dot), sub(right)].spacing(6).align_y(Alignment::Center),
            container(
                text(self.engine.ws_error.clone().unwrap_or_else(|| {
                    "Kara DM receives downloads from the browser extension on 127.0.0.1:10007.".into()
                }))
                .size(12),
            )
            .max_width(360)
            .padding([6, 10])
            .style(|_| flyout(6.0)),
            tooltip::Position::Top,
        );
        row![sub(left).width(Length::Fill), right].padding([2, 4]).align_y(Alignment::Center).into()
    }

    fn popup_view(&self, p: &Popup) -> Element<'_, Msg> {
        let at = match p {
            Popup::Context(pt) => Some(*pt),
            Popup::More => None,
        };
        let placed = responsive(move |size| {
            let Some(p) = &self.popup else { return space().into() };
            let (menu, h) = self.popup_menu(p);
            let w = 250.0;
            let (x, y) = match at {
                Some(pt) => (pt.x.min(size.width - w - 8.0).max(8.0), pt.y.min(size.height - h - 8.0).max(8.0)),
                None => (size.width - w - 14.0, 50.0),
            };
            container(container(menu).padding(5).width(w).style(|_| flyout(8.0)))
                .padding(Padding { top: y, right: 0.0, bottom: 0.0, left: x })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        });
        // Klik di luar menu menutupnya.
        stack![
            mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
                .on_press(Msg::ClosePopup)
                .on_right_press(Msg::ClosePopup),
            placed
        ]
        .into()
    }

    /// Isi menu dan perkiraan tingginya (untuk menjaga menu tetap di dalam jendela).
    fn popup_menu(&self, p: &Popup) -> (Element<'_, Msg>, f32) {
        let (mut n, seps) = match p {
            Popup::More => (6.0, 2.0),
            Popup::Context(_) => (2.0, 1.0),
        };
        let menu: Element<'_, Msg> = match p {
            Popup::More => column![
                menu_item(ic::FOLDER, "Open download folder", Msg::OpenDownloadDir, false),
                menu_item(ic::CHECK, "Clear finished from list", Msg::ClearFinished, false),
                separator(),
                menu_item(ic::GEAR, "Settings", Msg::ShowSettings, false),
                menu_item(ic::DOWNLOAD, "Check for updates", Msg::CheckUpdate, false),
                menu_item(ic::GLOBE, "About Kara DM", Msg::ShowAbout, false),
                separator(),
                menu_item(ic::POWER, "Quit (stop catching downloads)", Msg::Quit, true),
            ]
            .spacing(2)
            .into(),
            Popup::Context(_) => {
                let sel = self.selection();
                let one = sel.len() == 1;
                let has = |want: fn(Status) -> bool| {
                    sel.iter().any(|id| self.engine.item(id).is_some_and(|i| want(i.status)))
                };
                let mut col = column![].spacing(2);
                if has(|s| s == Status::Complete) {
                    n += 1.0;
                    col = col.push(menu_item(ic::OPEN, "Open", Msg::OpenSelected, false));
                }
                if one {
                    n += 1.0;
                    col = col.push(menu_item(ic::FOLDER, "Show in folder", Msg::FolderSelected, false));
                }
                if has(|s| matches!(s, Status::Active | Status::Waiting)) {
                    n += 1.0;
                    col = col.push(menu_item(ic::PAUSE, "Pause", Msg::Pause, false));
                }
                if has(|s| matches!(s, Status::Paused | Status::Error)) {
                    n += 1.0;
                    col = col.push(menu_item(ic::PLAY, "Resume", Msg::Resume, false));
                }
                col = col.push(menu_item(ic::LINK, if one { "Copy link" } else { "Copy links" }, Msg::CopyUrl, false));
                col = col.push(separator());
                col = col.push(menu_item(ic::TRASH, "Remove…", Msg::AskRemove, true));
                col.into()
            }
        };
        (menu, n * 36.0 + seps * 3.0 + 12.0)
    }

    fn modal_view<'a>(&'a self, m: &'a Modal) -> Element<'a, Msg> {
        let title = |t: &'a str| text(t).size(20).font(STRONG);
        let actions = |items: Vec<Element<'a, Msg>>| -> Element<'a, Msg> {
            let mut r = row![space().width(Length::Fill)].spacing(8);
            for i in items {
                r = r.push(i);
            }
            r.into()
        };
        let card: Element<'a, Msg> = match m {
            Modal::Add(url) => column![
                title("Add download"),
                sub("Paste a link to a file, a video page or an .m3u8 stream."),
                text_input("https://", url)
                    .id(iced::widget::Id::new("add-url"))
                    .on_input(Msg::AddText)
                    .on_submit(Msg::AddConfirm)
                    .padding(10)
                    .size(14),
                actions(vec![
                    secondary("Cancel", Msg::CloseModal).into(),
                    primary(text("Download").size(14), Msg::AddConfirm).into()
                ]),
            ]
            .spacing(14)
            .width(480)
            .into(),
            Modal::Remove(ids) => {
                let body = if ids.len() == 1 {
                    self.engine.item(&ids[0]).map(|i| i.display_name().to_string()).unwrap_or_default()
                } else {
                    format!("{} downloads", ids.len())
                };
                column![
                    title("Remove download?"),
                    text(body).size(14),
                    sub("Removing from the list keeps the file. Removing with files also deletes it from disk."),
                    actions(vec![
                        secondary("Cancel", Msg::CloseModal).into(),
                        button(text("Delete files").size(14))
                            .padding([7, 14])
                            .on_press(Msg::RemoveConfirm(true))
                            .style(|_, s| {
                                let a = if matches!(s, button::Status::Hovered) { 0.22 } else { 0.14 };
                                btn_style(Some(Color::from_rgba(1.0, 0.36, 0.40, a)), DANGER, None)
                            })
                            .into(),
                        primary(text("Remove from list").size(14), Msg::RemoveConfirm(false)).into(),
                    ]),
                ]
                .spacing(12)
                .width(500)
                .into()
            }
            Modal::Settings(s, speed) => self.settings_view(s, speed),
            Modal::About => column![
                row![icon(ic::DOWNLOAD, 28).color(ACCENT), title("Kara Download Manager")]
                    .spacing(12)
                    .align_y(Alignment::Center),
                sub(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                text(
                    "A multi-connection download manager (aria2) that takes downloads from the Kara DM browser \
                      extension or the Neat Download Manager extension. Clean-room implementation; not affiliated \
                      with Neat Download Manager."
                )
                .size(14),
                sub("Apache License 2.0  ·  kamisama-v0  ·  Icons: Phosphor (MIT)"),
                actions(vec![
                    secondary("Website", Msg::OpenUrl("https://github.com/kamisama-v0/kara-dm")).into(),
                    secondary("Report a problem", Msg::OpenUrl("https://github.com/kamisama-v0/kara-dm/issues")).into(),
                    primary(text("Close").size(14), Msg::CloseModal).into(),
                ]),
            ]
            .spacing(12)
            .width(500)
            .into(),
            Modal::Message(t, b) => column![
                title(t.as_str()),
                text(b.as_str()).size(14),
                actions(vec![primary(text("OK").size(14), Msg::CloseModal).into()]),
            ]
            .spacing(12)
            .width(460)
            .into(),
        };
        let card = container(card).padding(24).max_height(620).style(|_| flyout(10.0));
        opaque(
            mouse_area(center(opaque(card)).style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.50))),
                ..Default::default()
            }))
            .on_press(Msg::CloseModal),
        )
    }

    fn settings_view<'a>(&'a self, s: &'a Settings, speed: &'a str) -> Element<'a, Msg> {
        let line = |title: &'a str, note: &'a str, control: Element<'a, Msg>| -> Element<'a, Msg> {
            let mut label = column![text(title).size(14)].spacing(2).width(Length::Fill);
            if !note.is_empty() {
                label = label.push(sub(note));
            }
            container(row![label, control].spacing(16).align_y(Alignment::Center))
                .padding([10, 14])
                .width(Length::Fill)
                .into()
        };
        let group = |name: &'a str, lines: Vec<Element<'a, Msg>>| -> Element<'a, Msg> {
            let mut col = column![].spacing(0);
            for (i, l) in lines.into_iter().enumerate() {
                if i > 0 {
                    col = col.push(separator());
                }
                col = col.push(l);
            }
            column![text(name).size(14).font(STRONG), container(col).style(|_| layer())].spacing(8).into()
        };
        let switch = |key: &'static str, on: bool| -> Element<'a, Msg> {
            toggler(on).on_toggle(move |v| Msg::SetToggle(key, v)).size(20).into()
        };
        let hw_changed = s.hardware_acceleration != self.hw_at_start;
        let content = column![
            text("Settings").size(20).font(STRONG),
            group(
                "Downloads",
                vec![
                    line(
                        "Download folder",
                        "Unfinished files wait in its \"temp\" subfolder.",
                        row![
                            container(sub(s.dir().to_string_lossy().into_owned()).wrapping(text::Wrapping::None))
                                .max_width(220)
                                .clip(true),
                            secondary("Change…", Msg::PickFolder)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    ),
                    line(
                        "Connections per file",
                        "Parallel segments, up to 16",
                        row![
                            slider(1..=16, s.connections, Msg::SetConnections).width(150),
                            text(s.connections.to_string()).size(14).width(22)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    ),
                    line(
                        "Downloads at the same time",
                        "",
                        row![
                            slider(1..=10, s.max_concurrent, Msg::SetConcurrent).width(150),
                            text(s.max_concurrent.to_string()).size(14).width(22)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    ),
                    line(
                        "Speed limit",
                        "KB/s for all downloads; 0 means no limit",
                        text_input("0", speed).on_input(Msg::SetSpeed).width(100).padding(7).size(14).into(),
                    ),
                ],
            ),
            group(
                "Browser",
                vec![
                    line("Show a progress window for new downloads", "", switch("progress", s.show_progress_window)),
                    line(
                        "Download button on videos",
                        "Shown by the extension on video players",
                        switch("panel", s.media_panel)
                    ),
                    line("Notify when a download finishes", "", switch("notify", s.notify)),
                    line(
                        "Browser extension",
                        "Kara DM for Firefox and Chrome; the Neat Download Manager extension also works",
                        secondary("Get it", Msg::OpenUrl(EXTENSION_URL)).into(),
                    ),
                ],
            ),
            group(
                "Appearance",
                vec![line(
                    "Use hardware acceleration",
                    "Draws with the GPU. Uses about 200 MB of memory instead of about 10 MB. Applies after a restart.",
                    switch("hw", s.hardware_acceleration),
                )],
            ),
            group(
                "Updates",
                vec![line(
                    "Check for updates automatically",
                    if cfg!(windows) { "Once a day; installing still waits for you" } else { "Once a day" },
                    switch("updates", s.check_updates),
                )],
            ),
        ]
        .spacing(18)
        .width(640)
        .padding(Padding { top: 0.0, right: 16.0, bottom: 0.0, left: 0.0 });
        let mut buttons: Vec<Element<'_, Msg>> = vec![secondary("Cancel", Msg::CloseModal).into()];
        if hw_changed {
            buttons.push(secondary("Save and restart", Msg::Restart).into());
        }
        buttons.push(primary(text("Save").size(14), Msg::SaveSettings).into());
        let mut actions = row![space().width(Length::Fill)].spacing(8);
        for b in buttons {
            actions = actions.push(b);
        }
        column![scrollable(content).height(Length::Fill), actions].spacing(16).width(656).height(560).into()
    }

    // ---------- view: jendela progres ----------

    fn progress_view(&self, win: window::Id) -> Element<'_, Msg> {
        let Some(it) = self.progress.get(&win).and_then(|i| self.engine.item(i)) else {
            return center(sub("This download was removed.")).into();
        };
        let f = it.fraction() as f32;
        let id = it.id.clone();
        let stat = |label: &'static str, v: String| -> Element<'_, Msg> {
            column![sub(label), text(v).size(14)].spacing(2).width(Length::FillPortion(1)).into()
        };
        let segs = segments(it, f);
        let color = match it.status {
            Status::Error => DANGER,
            Status::Paused | Status::Waiting => white(0.45),
            _ => ACCENT,
        };
        // Kotak bersebelahan yang sama digabung: lebar pecahan per kotak meninggalkan garis tipis di renderer.
        let mut runs: Vec<(bool, u16)> = vec![];
        for on in segs {
            match runs.last_mut() {
                Some((last, n)) if *last == on => *n += 1,
                _ => runs.push((on, 1)),
            }
        }
        let seg_bar = container(row(runs.into_iter().map(move |(on, n)| {
            container(space())
                .width(Length::FillPortion(n))
                .height(10)
                .style(move |_| container::Style {
                    background: Some(Background::Color(if on { color } else { white(0.08) })),
                    ..Default::default()
                })
                .into()
        })))
        .clip(true)
        .style(|_| container::Style {
            border: Border { radius: 3.0.into(), ..Border::default() },
            ..Default::default()
        });

        let headline = match it.status {
            Status::Complete => "Download complete".to_string(),
            Status::Error => "Download failed".to_string(),
            Status::Merging => "Merging video and audio".to_string(),
            Status::Paused => format!("Paused at {:.0}%", f * 100.0),
            Status::Waiting => "Waiting to start".to_string(),
            Status::Active if it.size > 0 => format!("{:.0}%", f * 100.0),
            Status::Active => model::fmt_size(it.done),
        };
        let buttons = if it.status.finished() {
            row![
                space().width(Length::Fill),
                secondary("Close", Msg::ProgressHide(win)),
                secondary("Show in folder", Msg::OpenFolder(id.clone())),
                primary(text("Open").size(14), Msg::OpenFile(id)),
            ]
        } else {
            let armed = self.cancel_armed.contains(&id);
            let toggle = if matches!(it.status, Status::Active | Status::Waiting) {
                secondary("Pause", Msg::PauseItem(id.clone()))
            } else {
                secondary("Resume", Msg::ResumeItem(id.clone()))
            };
            row![
                button(text(if armed { "Click again to delete" } else { "Cancel" }).size(14))
                    .padding([7, 14])
                    .on_press(Msg::ProgressCancel(id))
                    .style(move |_, s| {
                        let a = if matches!(s, button::Status::Hovered) || armed { 0.22 } else { 0.0 };
                        btn_style(Some(Color::from_rgba(1.0, 0.36, 0.40, a)), DANGER, None)
                    }),
                space().width(Length::Fill),
                toggle,
                primary(text("Hide").size(14), Msg::ProgressHide(win)),
            ]
        }
        .spacing(8)
        .align_y(Alignment::Center);

        let mut content = column![
            row![
                container(icon(category_icon(it.category()), 22).color(SUB)).center(44).style(|_| container::Style {
                    background: Some(Background::Color(white(0.05))),
                    border: Border { radius: 8.0.into(), ..Border::default() },
                    ..Default::default()
                }),
                column![
                    container(text(it.display_name()).size(15).font(STRONG).wrapping(text::Wrapping::None)).clip(true),
                    container(sub(it.url.clone()).wrapping(text::Wrapping::None)).clip(true),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            row![text(headline).size(22).font(STRONG).color(if it.status == Status::Error { DANGER } else { TEXT })],
            seg_bar,
            row![
                stat(
                    "Downloaded",
                    if it.size > 0 {
                        format!("{} of {}", model::fmt_size(it.done), model::fmt_size(it.size))
                    } else {
                        model::fmt_size(it.done)
                    }
                ),
                stat("Speed", {
                    let v = model::fmt_speed(it.speed);
                    if v.is_empty() { "–".into() } else { v }
                }),
                stat("Time left", {
                    let v = model::fmt_eta(it.eta());
                    if v.is_empty() { "–".into() } else { v }
                }),
                stat("Connections", if it.conns > 0 { it.conns.to_string() } else { "–".into() }),
            ],
        ]
        .spacing(14);
        if it.status == Status::Error {
            content = content.push(text(it.error.as_str()).size(13).color(DANGER));
        }
        content = content.push(space().height(Length::Fill)).push(buttons);
        container(content).padding(20).width(Length::Fill).height(Length::Fill).into()
    }
}

/// Bar segmen ala NDM: 120 kotak; kotak menyala bila ada potongan (piece) aria2 yang sudah terunduh di sana.
fn segments(it: &Item, frac: f32) -> Vec<bool> {
    const N: usize = 120;
    let bytes: Vec<u8> = (0..it.bitfield.len() / 2)
        .filter_map(|i| u8::from_str_radix(&it.bitfield[2 * i..2 * i + 2], 16).ok())
        .collect();
    let pieces = it.pieces as usize;
    if pieces == 0 || bytes.is_empty() || frac >= 1.0 {
        return (0..N).map(|i| (i as f32 + 0.5) / N as f32 <= frac).collect();
    }
    let has = |i: usize| bytes.get(i / 8).is_some_and(|b| b & (0x80 >> (i % 8)) != 0);
    (0..N)
        .map(|b| {
            let (s, e) = (b * pieces / N, ((b + 1) * pieces).div_ceil(N).max(b * pieces / N + 1));
            (s..e.min(pieces)).any(has)
        })
        .collect()
}

/// Blur dari OS di belakang jendela transparan: Mica di Windows 11 (Acrylic di Windows 10), vibrancy di macOS.
/// Di Linux blur diurus compositor (Hyprland/KDE) lewat `window::Settings::blur`.
fn apply_vibrancy(w: &dyn window::Window) {
    #[cfg(windows)]
    {
        if window_vibrancy::apply_mica(w, Some(true)).is_err() {
            let _ = window_vibrancy::apply_acrylic(w, Some((18, 20, 26, 140)));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = window_vibrancy::apply_vibrancy(w, window_vibrancy::NSVisualEffectMaterial::HudWindow, None, None);
    }
    let _ = w;
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::mouse;
    use iced_test::simulator::Simulator;

    fn item(id: &str, name: &str, status: Status, size: u64, done: u64, speed: u64) -> Item {
        Item {
            id: id.into(),
            name: name.into(),
            url: format!("https://example.org/{name}"),
            status,
            size,
            done,
            speed,
            added: 1_790_000_000,
            ..Default::default()
        }
    }

    fn gui() -> Gui {
        let items = vec![
            item("a", "ubuntu-26.04-desktop-amd64.iso", Status::Paused, 6_400_000_000, 2_100_000_000, 0),
            item("b", "niki-Tsunami-1080.webm", Status::Complete, 48_200_000, 48_200_000, 0),
            item("c", "Quarterly report 2026 (final).pdf", Status::Active, 12_000_000, 4_300_000, 2_100_000),
        ];
        let (gui, _) = Gui::boot(Engine::offline(items), vec![]);
        gui
    }

    fn simulate(g: &Gui) -> Simulator<'_, Msg> {
        let settings = iced::Settings {
            fonts: vec![include_bytes!("../assets/fonts/Phosphor.ttf").as_slice().into()],
            default_font: UI,
            ..Default::default()
        };
        Simulator::with_size(settings, (1040.0, 600.0), g.main_view())
    }

    /// Simpan potret bila KARA_UI_SHOTS menunjuk ke sebuah folder (untuk memeriksa tampilan dengan mata).
    fn shot(sim: &mut Simulator<'_, Msg>, name: &str) {
        if let Some(dir) = std::env::var_os("KARA_UI_SHOTS") {
            let path = std::path::Path::new(&dir).join(format!("{name}.png"));
            for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if f.file_name().to_string_lossy().starts_with(&format!("{name}-")) {
                    let _ = std::fs::remove_file(f.path());
                }
            }
            let snap = sim.snapshot(&theme()).expect("snapshot");
            snap.matches_image(path).expect("save snapshot");
        }
    }

    /// Jalankan pesan yang dihasilkan simulator di aplikasi.
    fn apply(g: &mut Gui, msgs: Vec<Msg>) {
        for m in msgs {
            let _ = g.update(m);
        }
    }

    #[test]
    fn right_click_on_a_download_opens_its_context_menu() {
        let mut g = gui();
        let mut sim = simulate(&g);
        shot(&mut sim, "main");
        let row = sim.find("niki-Tsunami-1080.webm").expect("row");
        sim.point_at(row.visible_bounds().expect("visible").center());
        sim.simulate([
            Event::Mouse(mouse::Event::CursorMoved { position: row.visible_bounds().unwrap().center() }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ]);
        let msgs: Vec<Msg> = sim.into_messages().collect();
        assert!(msgs.iter().any(|m| matches!(m, Msg::ContextMenu(id) if id == "b")), "{msgs:?}");
        apply(&mut g, msgs);
        assert!(g.selected.contains("b"));

        let mut sim = simulate(&g);
        shot(&mut sim, "context-menu");
        for entry in ["Open", "Show in folder", "Copy link", "Remove…"] {
            sim.find(entry).unwrap_or_else(|e| panic!("{entry}: {e:?}"));
        }

        // Memilih menu menjalankan aksinya dan menutup menu.
        let _ = sim.click("Show in folder").expect("click");
        let msgs: Vec<Msg> = sim.into_messages().collect();
        assert!(msgs.iter().any(|m| matches!(m, Msg::FolderSelected)), "{msgs:?}");
    }

    #[test]
    fn more_menu_settings_and_add_dialog_render() {
        let mut g = gui();
        for (msgs, name, expect) in [
            (vec![Msg::ShowMore], "more-menu", "Quit (stop catching downloads)"),
            (vec![Msg::ShowSettings], "settings", "Use hardware acceleration"),
            (vec![Msg::CloseModal, Msg::ShowAdd], "add", "Paste a link to a file, a video page or an .m3u8 stream."),
        ] {
            apply(&mut g, msgs);
            let mut sim = simulate(&g);
            shot(&mut sim, name);
            sim.find(expect).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }
}

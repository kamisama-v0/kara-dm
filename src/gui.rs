//! Antarmuka iced. Berjalan sebagai daemon: tanpa jendela saat di latar belakang (hanya inti yang hidup), jendela
//! utama dan jendela progres dibuka saat diperlukan. Gaya kaca: panel tembus pandang dengan kilau tepi di atas
//! blur dari OS (Mica di Windows 11, blur compositor di Linux).

use crate::engine::{Effect, Engine};
use crate::model::{self, CATEGORIES, Item, Settings, Status};
use crate::{platform, protocol};
use iced::widget::{
    button, center, column, container, mouse_area, opaque, progress_bar, row, scrollable, slider, space, stack, text,
    text_input, toggler, tooltip,
};
use iced::{
    Alignment, Background, Border, Color, Element, Event, Font, Gradient, Length, Shadow, Subscription, Task, Theme,
    Vector, event, keyboard, window,
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
    let mut daemon = iced::daemon(boot, Gui::update, Gui::view)
        .title(Gui::title)
        .subscription(Gui::subscription)
        .theme(|_: &Gui, _| Theme::Dark)
        .style(|_: &Gui, _: &Theme| iced::theme::Style { background_color: Color::TRANSPARENT, text_color: TEXT });
    if cfg!(windows) {
        daemon = daemon.default_font(Font::with_name("Segoe UI"));
    }
    daemon.run()
}

// ---------- warna kaca ----------

const TEXT: Color = Color::from_rgb(0.95, 0.96, 0.96);
const DIM: Color = Color::from_rgba(0.95, 0.96, 0.96, 0.62);
const ACCENT: Color = Color::from_rgb(0.40, 0.62, 1.0);
const DANGER: Color = Color::from_rgb(1.0, 0.45, 0.42);
const OK: Color = Color::from_rgb(0.33, 0.80, 0.52);

fn white(a: f32) -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, a)
}

/// Panel kaca: isi tembus pandang, kilau di tepi atas (gradien), tepi tipis terang, bayangan lembut.
fn glass_style(radius: f32, strength: f32) -> container::Style {
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0.0, white(0.15 * strength))
                .add_stop(0.10, white(0.07 * strength))
                .add_stop(1.0, white(0.045 * strength)),
        ))),
        text_color: Some(TEXT),
        border: Border { color: white(0.20), width: 1.0, radius: radius.into() },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.22),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 20.0,
        },
        snap: true,
    }
}

/// Panel yang melayang di atas isi lain (menu, dialog, tooltip). Renderer software tidak bisa mengaburkan isi
/// aplikasi di belakangnya, jadi kacanya hampir pekat (gelap berwarna dingin) dengan kilau tepi yang sama.
fn float_style(radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0.0, Color::from_rgba(0.24, 0.25, 0.30, 0.97))
                .add_stop(0.08, Color::from_rgba(0.16, 0.17, 0.21, 0.97))
                .add_stop(1.0, Color::from_rgba(0.12, 0.13, 0.16, 0.97)),
        ))),
        text_color: Some(TEXT),
        border: Border { color: white(0.18), width: 1.0, radius: radius.into() },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        snap: true,
    }
}

fn glass<'a>(content: impl Into<Element<'a, Msg>>) -> container::Container<'a, Msg> {
    container(content).padding(8).style(|_| glass_style(18.0, 1.0))
}

fn pill(label: &str, msg: Option<Msg>) -> button::Button<'_, Msg> {
    button(text(label).size(13))
        .padding([6, 14])
        .style(|_, status| {
            let a = match status {
                button::Status::Hovered => 0.24,
                button::Status::Pressed => 0.30,
                button::Status::Disabled => 0.04,
                button::Status::Active => 0.12,
            };
            button::Style {
                background: Some(Background::Color(white(a))),
                text_color: if matches!(status, button::Status::Disabled) { DIM } else { TEXT },
                border: Border { color: white(0.30), width: 1.0, radius: 999.0.into() },
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .on_press_maybe(msg)
}

fn primary(label: &str, msg: Msg) -> button::Button<'_, Msg> {
    button(text(label).size(13)).padding([6, 16]).on_press(msg).style(|_, status| button::Style {
        background: Some(Background::Color(if matches!(status, button::Status::Hovered) {
            Color::from_rgb(0.48, 0.68, 1.0)
        } else {
            ACCENT
        })),
        text_color: Color::WHITE,
        border: Border { radius: 999.0.into(), ..Border::default() },
        shadow: Shadow::default(),
        snap: true,
    })
}

fn flat(label: &str, msg: Msg) -> button::Button<'_, Msg> {
    button(text(label).size(13)).padding([7, 12]).width(Length::Fill).on_press(msg).style(|_, status| button::Style {
        background: matches!(status, button::Status::Hovered).then(|| Background::Color(white(0.12))),
        text_color: TEXT,
        border: Border { radius: 10.0.into(), ..Border::default() },
        shadow: Shadow::default(),
        snap: true,
    })
}

fn dim<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(12).color(DIM)
}

fn category_glyph(key: &str) -> &'static str {
    match key {
        "all" => "⇩",
        "unfinished" => "►",
        "finished" => "✓",
        "Video" => "▣",
        "Music" => "♪",
        "Archives" => "▤",
        "Documents" => "≡",
        "Programs" => "⚙",
        "Images" => "◩",
        _ => "•",
    }
}

fn matches(cat: &str, it: &Item) -> bool {
    match cat {
        "all" => true,
        "unfinished" => !it.status.finished(),
        "finished" => it.status.finished(),
        c => it.category() == c,
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

pub struct Gui {
    engine: Engine,
    windows: HashMap<window::Id, Win>,
    main: Option<window::Id>,
    progress: HashMap<window::Id, String>,
    selected: HashSet<String>,
    category: &'static str,
    modal: Option<Modal>,
    menu: bool,
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
    Category(&'static str),
    Click(String),
    Activate(String),
    // toolbar & menu
    ShowAdd,
    Resume,
    Pause,
    AskRemove,
    OpenSelected,
    FolderSelected,
    CopyUrl,
    ToggleMenu,
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
    // jendela progres
    ProgressPause(String),
    ProgressResume(String),
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
            windows: HashMap::new(),
            main: None,
            progress: HashMap::new(),
            selected: HashSet::new(),
            category: "all",
            modal: None,
            menu: false,
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
        // Tanpa jendela cukup sedetik sekali; dengan jendela 4x per detik agar progres terasa hidup.
        let every = if self.windows.is_empty() { Duration::from_millis(1000) } else { Duration::from_millis(250) };
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
                size: iced::Size::new(1080.0, 640.0),
                min_size: Some(iced::Size::new(760.0, 420.0)),
                transparent: true,
                blur: true,
                ..Default::default()
            },
            Win::Progress => window::Settings {
                size: iced::Size::new(580.0, 330.0),
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
                Effect::Quit => {
                    self.engine.shutdown();
                    return iced::exit();
                }
            });
        }
        Task::batch(tasks)
    }

    fn selection(&self) -> Vec<String> {
        self.selected.iter().filter(|id| self.engine.item(id).is_some()).cloned().collect()
    }

    // ---------- update ----------

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let task = match msg {
            Msg::Tick => {
                self.engine.tick();
                Task::none()
            }
            Msg::Opened(id) => window::run(id, |w| apply_vibrancy(w)).map(|_| Msg::Noop),
            Msg::Closed(id) => {
                if self.main == Some(id) {
                    self.main = None;
                    self.menu = false;
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
            Msg::Key(key, m) => match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    self.modal = None;
                    self.menu = false;
                    Task::none()
                }
                keyboard::Key::Named(keyboard::key::Named::Delete) if self.modal.is_none() => self.ask_remove(),
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
            Msg::Click(id) => {
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
            Msg::ShowAdd => self.show_add(),
            Msg::Resume => {
                let ids = self.selection();
                self.engine.resume(&ids);
                Task::none()
            }
            Msg::Pause => {
                let ids = self.selection();
                self.engine.pause(&ids);
                Task::none()
            }
            Msg::AskRemove => self.ask_remove(),
            Msg::OpenSelected => {
                for id in self.selection() {
                    self.engine.open_file(&id);
                }
                Task::none()
            }
            Msg::FolderSelected => {
                if let Some(id) = self.selection().first() {
                    self.engine.open_folder(id);
                }
                Task::none()
            }
            Msg::CopyUrl => {
                let urls: Vec<String> =
                    self.selection().iter().filter_map(|id| self.engine.item(id).map(|i| i.url.clone())).collect();
                iced::clipboard::write(urls.join("\n"))
            }
            Msg::ToggleMenu => {
                self.menu = !self.menu;
                Task::none()
            }
            Msg::OpenDownloadDir => {
                self.menu = false;
                self.engine.open_download_dir();
                Task::none()
            }
            Msg::ClearFinished => {
                self.menu = false;
                self.engine.clear_finished();
                Task::none()
            }
            Msg::ShowSettings => {
                self.menu = false;
                let s = self.engine.settings.clone();
                let speed = s.speed_limit_kib.to_string();
                self.modal = Some(Modal::Settings(Box::new(s), speed));
                Task::none()
            }
            Msg::CheckUpdate => {
                self.menu = false;
                self.engine.check_update(true);
                Task::none()
            }
            Msg::StartUpdate => {
                self.engine.start_update();
                Task::none()
            }
            Msg::ShowAbout => {
                self.menu = false;
                self.modal = Some(Modal::About);
                Task::none()
            }
            Msg::Quit => {
                self.engine.shutdown();
                iced::exit()
            }
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
                if let Some(Modal::Settings(s, speed)) = self.modal.take() {
                    let mut s = *s;
                    s.speed_limit_kib = speed.parse().unwrap_or(0);
                    self.engine.apply_settings(s);
                }
                Task::none()
            }
            Msg::Restart => {
                if let Some(Modal::Settings(s, speed)) = self.modal.take() {
                    let mut s = *s;
                    s.speed_limit_kib = speed.parse().unwrap_or(0);
                    self.engine.apply_settings(s);
                }
                self.engine.shutdown();
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--after-restart").spawn();
                }
                iced::exit()
            }
            Msg::OpenUrl(u) => {
                platform::open(u);
                Task::none()
            }
            Msg::ProgressPause(id) => {
                self.engine.pause(&[id]);
                Task::none()
            }
            Msg::ProgressResume(id) => {
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

    fn show_add(&mut self) -> Task<Msg> {
        self.menu = false;
        self.modal = Some(Modal::Add(String::new()));
        Task::batch([
            iced::clipboard::read().map(Msg::AddPasted),
            iced::widget::operation::focus(iced::widget::Id::new("add-url")),
        ])
    }

    fn ask_remove(&mut self) -> Task<Msg> {
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

    // ---------- view ----------

    fn view(&self, id: window::Id) -> Element<'_, Msg> {
        match self.windows.get(&id) {
            Some(Win::Progress) => self.progress_view(id),
            _ => self.main_view(),
        }
    }

    fn main_view(&self) -> Element<'_, Msg> {
        let sel = !self.selection().is_empty();
        let toolbar = glass(
            row![
                pill("＋  Add", Some(Msg::ShowAdd)),
                pill("►  Resume", sel.then_some(Msg::Resume)),
                pill("❚❚  Pause", sel.then_some(Msg::Pause)),
                pill("✕  Remove", sel.then_some(Msg::AskRemove)),
                space().width(8),
                pill("Open", sel.then_some(Msg::OpenSelected)),
                pill("Folder", sel.then_some(Msg::FolderSelected)),
                pill("Copy URL", sel.then_some(Msg::CopyUrl)),
                space().width(Length::Fill),
                pill("⋯", Some(Msg::ToggleMenu)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([8, 10])
        .width(Length::Fill);

        let mut body = column![toolbar].spacing(10);
        if let Some(r) = &self.engine.update {
            let installs = cfg!(windows) && r.installer.is_some();
            let (title, action): (String, Option<(&str, Msg)>) = if self.engine.updating {
                (format!("Downloading Kara DM {}… The app reopens after installing.", r.version), None)
            } else if installs {
                (format!("Kara DM {} is available.", r.version), Some(("Update now", Msg::StartUpdate)))
            } else {
                (format!("Kara DM {} is available.", r.version), Some(("View release", Msg::StartUpdate)))
            };
            let mut banner = row![text(title).size(13).width(Length::Fill)].align_y(Alignment::Center);
            if let Some((label, msg)) = action {
                banner = banner.push(primary(label, msg));
            }
            body = body.push(container(banner).padding([8, 14]).width(Length::Fill).style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.40, 0.62, 1.0, 0.22))),
                border: Border { color: Color::from_rgba(0.40, 0.62, 1.0, 0.5), width: 1.0, radius: 14.0.into() },
                text_color: Some(TEXT),
                ..Default::default()
            }));
        }

        let sidebar = glass(
            column(CATEGORIES.iter().enumerate().map(|(i, (key, label))| {
                let n = self.engine.items.iter().filter(|it| matches(key, it)).count();
                let active = self.category == *key;
                let entry = button(
                    row![
                        text(category_glyph(key)).size(14).width(22),
                        text(*label).size(13).width(Length::Fill),
                        dim(if n > 0 { n.to_string() } else { String::new() }),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([7, 10])
                .width(Length::Fill)
                .on_press(Msg::Category(key))
                .style(move |_, status| button::Style {
                    background: (active || matches!(status, button::Status::Hovered))
                        .then(|| Background::Color(white(if active { 0.16 } else { 0.08 }))),
                    text_color: TEXT,
                    border: Border { radius: 10.0.into(), ..Border::default() },
                    shadow: Shadow::default(),
                    snap: true,
                });
                if i == 3 {
                    column![
                        container(space()).height(1).width(Length::Fill).style(|_| container::Style {
                            background: Some(Background::Color(white(0.12))),
                            ..Default::default()
                        }),
                        entry
                    ]
                    .spacing(6)
                    .into()
                } else {
                    entry.into()
                }
            }))
            .spacing(2),
        )
        .width(210)
        .height(Length::Fill);

        let header = row![
            space().width(22),
            dim("Name").width(Length::Fill),
            dim("Size").width(80).align_x(Alignment::End),
            dim("Progress").width(140).align_x(Alignment::Center),
            dim("Speed").width(85).align_x(Alignment::End),
            dim("Time left").width(75).align_x(Alignment::End),
            dim("Status").width(95),
        ]
        .spacing(10)
        .padding([4, 12]);
        let shown: Vec<&Item> = self.engine.items.iter().rev().filter(|it| matches(self.category, it)).collect();
        let list: Element<'_, Msg> = if shown.is_empty() {
            center(
                column![
                    text("No downloads yet").size(16),
                    dim("Click a download link in the browser (Kara DM extension), or press Add."),
                ]
                .spacing(6)
                .align_x(Alignment::Center),
            )
            .into()
        } else {
            scrollable(column(shown.into_iter().map(|it| self.item_row(it))).spacing(4)).height(Length::Fill).into()
        };
        let content = glass(column![header, list].spacing(4)).width(Length::Fill).height(Length::Fill);

        body = body.push(row![sidebar, content].spacing(10).height(Length::Fill));
        body = body.push(self.status_bar());

        let base = container(body).padding(12).width(Length::Fill).height(Length::Fill);
        let mut layers = stack![base];
        if self.menu {
            layers = layers.push(self.menu_view());
        }
        if let Some(m) = &self.modal {
            layers = layers.push(self.modal_view(m));
        }
        layers.into()
    }

    fn item_row<'a>(&'a self, it: &'a Item) -> Element<'a, Msg> {
        let selected = self.selected.contains(&it.id);
        let f = it.fraction() as f32;
        let progress: Element<'_, Msg> = if it.status == Status::Active && it.size == 0 {
            dim(model::fmt_size(it.done)).into()
        } else {
            row![
                progress_bar(0.0..=1.0, f).length(92).girth(6).style(|_| progress_bar::Style {
                    background: Background::Color(white(0.12)),
                    bar: Background::Color(ACCENT),
                    border: Border { radius: 3.0.into(), ..Border::default() },
                }),
                dim(format!("{:.0}%", f * 100.0)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };
        let status: Element<'_, Msg> = if it.status == Status::Error {
            tooltip(
                text(it.status.label()).size(13).color(DANGER),
                container(text(it.error.as_str()).size(12)).padding(8).max_width(420).style(|_| float_style(10.0)),
                tooltip::Position::Left,
            )
            .into()
        } else {
            text(it.status.label()).size(13).color(if it.status == Status::Complete { OK } else { TEXT }).into()
        };
        let cells = row![
            text(category_glyph(it.category())).size(14).width(22),
            container(text(it.display_name()).size(13).wrapping(text::Wrapping::None)).width(Length::Fill).clip(true),
            dim(if it.size > 0 { model::fmt_size(it.size) } else { String::new() }).width(80).align_x(Alignment::End),
            container(progress).width(140).center_x(140),
            dim(model::fmt_speed(it.speed)).width(85).align_x(Alignment::End),
            dim(model::fmt_eta(it.eta())).width(75).align_x(Alignment::End),
            container(status).width(95),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let when = if it.finished > 0 {
            format!("Finished {}", model::fmt_date(it.finished))
        } else {
            format!("Added {}", model::fmt_date(it.added))
        };
        let row_box = container(cells).padding([8, 12]).width(Length::Fill).style(move |_| container::Style {
            background: Some(Background::Color(if selected {
                Color::from_rgba(0.40, 0.62, 1.0, 0.24)
            } else {
                white(0.05)
            })),
            border: Border {
                color: if selected { Color::from_rgba(0.40, 0.62, 1.0, 0.55) } else { white(0.07) },
                width: 1.0,
                radius: 12.0.into(),
            },
            text_color: Some(TEXT),
            ..Default::default()
        });
        tooltip(
            mouse_area(row_box).on_press(Msg::Click(it.id.clone())).on_double_click(Msg::Activate(it.id.clone())),
            container(column![text(it.display_name()).size(12), dim(it.url.clone()), dim(when)].spacing(2))
                .padding(8)
                .max_width(520)
                .style(|_| float_style(10.0)),
            tooltip::Position::Bottom,
        )
        .delay(Duration::from_millis(700))
        .into()
    }

    fn status_bar(&self) -> Element<'_, Msg> {
        let items = &self.engine.items;
        let active = items.iter().filter(|i| i.status == Status::Active).count();
        let speed: u64 = items.iter().map(|i| i.speed).sum();
        let left = match &self.engine.aria_error {
            Some(e) => format!("aria2 engine is not running: {e}"),
            None if active > 0 => format!("↓ {}   ·   {active} active", model::fmt_speed(speed)),
            None => "Ready".into(),
        };
        let right = match (&self.engine.ws_error, self.engine.extension_connections()) {
            (Some(e), _) => format!("Browser catching is off: {e}"),
            (None, 0) => "Browser extension: not connected yet (connects on the first download)".into(),
            (None, n) => format!("Browser extension: connected ({n})"),
        };
        row![dim(left).width(Length::Fill), dim(right)].padding([0, 6]).into()
    }

    fn menu_view(&self) -> Element<'_, Msg> {
        let menu = container(
            column![
                flat("Open download folder", Msg::OpenDownloadDir),
                flat("Clear finished from list", Msg::ClearFinished),
                flat("Settings", Msg::ShowSettings),
                flat("Check for updates", Msg::CheckUpdate),
                flat("About", Msg::ShowAbout),
                flat("Quit (stop catching downloads)", Msg::Quit),
            ]
            .spacing(2)
            .width(250),
        )
        .padding(6)
        .style(|_| float_style(14.0));
        mouse_area(
            container(menu).width(Length::Fill).height(Length::Fill).align_right(Length::Fill).padding(iced::Padding {
                top: 62.0,
                right: 22.0,
                bottom: 0.0,
                left: 0.0,
            }),
        )
        .on_press(Msg::ToggleMenu)
        .into()
    }

    fn modal_view<'a>(&'a self, m: &'a Modal) -> Element<'a, Msg> {
        let card: Element<'a, Msg> =
            match m {
                Modal::Add(url) => column![
                    text("Add download").size(18),
                    dim("Paste one URL."),
                    text_input("https://…", url)
                        .id(iced::widget::Id::new("add-url"))
                        .on_input(Msg::AddText)
                        .on_submit(Msg::AddConfirm)
                        .padding(10),
                    row![
                        space().width(Length::Fill),
                        pill("Cancel", Some(Msg::CloseModal)),
                        primary("Download", Msg::AddConfirm)
                    ]
                    .spacing(8),
                ]
                .spacing(12)
                .width(460)
                .into(),
                Modal::Remove(ids) => {
                    let body = if ids.len() == 1 {
                        self.engine.item(&ids[0]).map(|i| i.display_name().to_string()).unwrap_or_default()
                    } else {
                        format!("{} downloads selected", ids.len())
                    };
                    column![
                        text("Remove download?").size(18),
                        dim(body),
                        row![
                            space().width(Length::Fill),
                            pill("Cancel", Some(Msg::CloseModal)),
                            pill("Remove with files", Some(Msg::RemoveConfirm(true))),
                            primary("Remove from list", Msg::RemoveConfirm(false)),
                        ]
                        .spacing(8),
                    ]
                    .spacing(12)
                    .width(480)
                    .into()
                }
                Modal::Settings(s, speed) => self.settings_view(s, speed),
                Modal::About => column![
                text("Kara Download Manager").size(20),
                dim(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                text("Multi-connection download manager (aria2) that takes downloads from the Kara DM or Neat Download \
                      Manager browser extension. Clean-room implementation; not affiliated with Neat Download Manager.")
                .size(13),
                dim("Apache License 2.0 · kamisama-v0"),
                row![
                    pill("Website", Some(Msg::OpenUrl("https://github.com/kamisama-v0/kara-dm"))),
                    pill("Report an issue", Some(Msg::OpenUrl("https://github.com/kamisama-v0/kara-dm/issues"))),
                    space().width(Length::Fill),
                    primary("Close", Msg::CloseModal),
                ]
                .spacing(8),
            ]
                .spacing(10)
                .width(480)
                .into(),
                Modal::Message(t, b) => column![
                    text(t.as_str()).size(18),
                    text(b.as_str()).size(13),
                    row![space().width(Length::Fill), primary("OK", Msg::CloseModal)],
                ]
                .spacing(12)
                .width(440)
                .into(),
            };
        let card = container(card).padding(22).max_height(600).style(|_| float_style(22.0));
        opaque(
            mouse_area(center(opaque(card)).style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55))),
                ..Default::default()
            }))
            .on_press(Msg::CloseModal),
        )
    }

    fn settings_view<'a>(&'a self, s: &'a Settings, speed: &'a str) -> Element<'a, Msg> {
        let line = |title: &'a str, sub: &'a str, control: Element<'a, Msg>| -> Element<'a, Msg> {
            row![column![text(title).size(13), dim(sub)].spacing(2).width(Length::Fill), control]
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
        };
        let switch = |key: &'static str, on: bool| -> Element<'a, Msg> {
            toggler(on).on_toggle(move |v| Msg::SetToggle(key, v)).size(20).into()
        };
        let hw_changed = s.hardware_acceleration != self.hw_at_start;
        let section = |t: &'a str| dim(t).size(12);
        let content = column![
            text("Settings").size(20),
            section("DOWNLOADS"),
            line(
                "Download folder",
                "While downloading, files are kept in its \"temp\" subfolder and moved in once complete.",
                row![dim(s.dir().to_string_lossy().into_owned()), pill("Change…", Some(Msg::PickFolder))]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            ),
            line(
                "Connections per file",
                "Parallel segments (max 16)",
                row![slider(1..=16, s.connections, Msg::SetConnections).width(160), text(s.connections.to_string()).width(24)]
                    .spacing(8)
                    .into(),
            ),
            line(
                "Simultaneous downloads",
                "Downloads running at the same time",
                row![slider(1..=10, s.max_concurrent, Msg::SetConcurrent).width(160), text(s.max_concurrent.to_string()).width(24)]
                    .spacing(8)
                    .into(),
            ),
            line(
                "Total speed limit (KB/s)",
                "0 = unlimited; applies immediately",
                text_input("0", speed).on_input(Msg::SetSpeed).width(110).padding(6).into(),
            ),
            section("BROWSER"),
            line("Show a progress window for browser downloads", "", switch("progress", s.show_progress_window)),
            line("Video catch panel on pages", "Applies when the extension reconnects", switch("panel", s.media_panel)),
            line("Notify when a download completes", "", switch("notify", s.notify)),
            line(
                "Browser extension",
                "Kara DM for Firefox, Chrome, Chromium; the Neat Download Manager extension also works",
                pill("Open page", Some(Msg::OpenUrl(EXTENSION_URL))).into(),
            ),
            section("APPEARANCE"),
            line(
                "Use hardware acceleration",
                "Draws the window with the GPU. Uses much more memory (±200 MB vs ±10 MB); takes effect after a restart.",
                switch("hw", s.hardware_acceleration),
            ),
            section("UPDATES"),
            line(
                "Check for updates automatically",
                if cfg!(windows) {
                    "Checks GitHub for a new release once a day; installing still waits for you"
                } else {
                    "Checks GitHub for a new release once a day and tells you"
                },
                switch("updates", s.check_updates),
            ),
            {
                let mut actions = row![space().width(Length::Fill), pill("Cancel", Some(Msg::CloseModal))].spacing(8);
                if hw_changed {
                    actions = actions.push(pill("Save and restart", Some(Msg::Restart)));
                }
                actions.push(primary("Save", Msg::SaveSettings))
            },
        ]
        .spacing(12);
        scrollable(content.width(640).padding(iced::Padding { top: 0.0, right: 18.0, bottom: 0.0, left: 4.0 })).into()
    }

    // ---------- jendela progres ----------

    fn progress_view(&self, win: window::Id) -> Element<'_, Msg> {
        let Some(it) = self.progress.get(&win).and_then(|i| self.engine.item(i)) else {
            return container(center(dim("This download was removed."))).style(|_| glass_style(0.0, 1.0)).into();
        };
        let f = it.fraction() as f32;
        let value = |label: &'static str, v: String| -> Element<'_, Msg> {
            column![dim(label), text(v).size(13)].spacing(2).width(Length::FillPortion(1)).into()
        };
        let segs = segments(it, f);
        let seg_bar = row(segs.into_iter().map(|on| {
            container(space())
                .width(Length::FillPortion(1))
                .height(14)
                .style(move |_| container::Style {
                    background: Some(Background::Color(if on { ACCENT } else { white(0.10) })),
                    ..Default::default()
                })
                .into()
        }));
        let seg_bar = container(seg_bar).clip(true).style(|_| container::Style {
            border: Border { radius: 5.0.into(), ..Border::default() },
            ..Default::default()
        });
        let id = it.id.clone();
        let buttons = if it.status.finished() {
            row![
                space().width(Length::Fill),
                pill("Close", Some(Msg::ProgressHide(win))),
                pill("Open folder", Some(Msg::OpenFolder(id.clone()))),
                primary("Open", Msg::OpenFile(id)),
            ]
        } else {
            let armed = self.cancel_armed.contains(&id);
            let toggle = if matches!(it.status, Status::Active | Status::Waiting) {
                pill("Pause", Some(Msg::ProgressPause(id.clone())))
            } else {
                pill("Resume", Some(Msg::ProgressResume(id.clone())))
            };
            row![
                pill(if armed { "Click again to delete" } else { "Cancel…" }, Some(Msg::ProgressCancel(id))),
                space().width(Length::Fill),
                toggle,
                primary("Hide", Msg::ProgressHide(win)),
            ]
        }
        .spacing(8);
        let mut content = column![
            container(text(it.display_name()).size(15).wrapping(text::Wrapping::None)).clip(true),
            container(dim(it.url.clone()).wrapping(text::Wrapping::None)).clip(true),
            row![
                value("Status", it.status.label().into()),
                value("Size", if it.size > 0 { model::fmt_size(it.size) } else { "unknown".into() }),
                value("Downloaded", format!("{} ({:.1}%)", model::fmt_size(it.done), f * 100.0)),
            ],
            row![
                value("Speed", model::fmt_speed(it.speed)),
                value("Time left", model::fmt_eta(it.eta())),
                value("Connections", if it.conns > 0 { it.conns.to_string() } else { String::new() }),
            ],
            seg_bar,
            progress_bar(0.0..=1.0, f).girth(6).style(|_| progress_bar::Style {
                background: Background::Color(white(0.12)),
                bar: Background::Color(ACCENT),
                border: Border { radius: 3.0.into(), ..Border::default() },
            }),
        ]
        .spacing(10);
        if it.status == Status::Error {
            content = content.push(text(it.error.as_str()).size(12).color(DANGER));
        }
        content = content.push(buttons);
        container(container(content).padding(16).style(|_| glass_style(18.0, 1.2)))
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
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

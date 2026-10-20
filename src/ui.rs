use crate::App;
use crate::model::{self, CATEGORIES, Item, Status};
use adw::prelude::*;
use gtk::{gdk, gio, glib, pango};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const EXTENSION_URL: &str = "https://github.com/kamisama-v0/kara-dm#browser-extension";

const W_SIZE: i32 = 80;
const W_PROGRESS: i32 = 120;
const W_SPEED: i32 = 85;
const W_ETA: i32 = 60;
const W_STATUS: i32 = 85;

fn label(text: &str, width: i32, xalign: f32) -> gtk::Label {
    let l = gtk::Label::builder().label(text).xalign(xalign).width_request(width).build();
    l.set_ellipsize(pango::EllipsizeMode::End);
    l
}

struct Row {
    row: gtk::ListBoxRow,
    icon: gtk::Image,
    name: gtk::Label,
    size: gtk::Label,
    bar: gtk::ProgressBar,
    speed: gtk::Label,
    eta: gtk::Label,
    status: gtk::Label,
}

impl Row {
    fn new(it: &Item) -> Row {
        let b = gtk::Box::builder().spacing(10).margin_top(6).margin_bottom(6).margin_start(10).margin_end(10).build();
        let icon = gtk::Image::from_icon_name(model::category_icon(it.category()));
        let name = gtk::Label::builder().xalign(0.0).hexpand(true).width_chars(18).build();
        name.set_ellipsize(pango::EllipsizeMode::Middle);
        let size = label("", W_SIZE, 1.0);
        let bar =
            gtk::ProgressBar::builder().show_text(true).valign(gtk::Align::Center).width_request(W_PROGRESS).build();
        let speed = label("", W_SPEED, 1.0);
        let eta = label("", W_ETA, 1.0);
        let status = label("", W_STATUS, 0.0);
        for l in [&size, &speed, &eta] {
            l.add_css_class("numeric");
            l.add_css_class("dim-label");
        }
        b.append(&icon);
        b.append(&name);
        b.append(&size);
        b.append(&bar);
        b.append(&speed);
        b.append(&eta);
        b.append(&status);
        let row = gtk::ListBoxRow::builder().child(&b).build();
        row.set_widget_name(&it.id);
        let r = Row { row, icon, name, size, bar, speed, eta, status };
        r.update(it);
        r
    }

    fn update(&self, it: &Item) {
        self.icon.set_icon_name(Some(model::category_icon(it.category())));
        self.name.set_label(it.display_name());
        let when = if it.finished > 0 {
            format!("Finished {}", model::fmt_date(it.finished))
        } else {
            format!("Added {}", model::fmt_date(it.added))
        };
        self.row.set_tooltip_text(Some(&format!("{}\n{}\n{when}", it.display_name(), it.url)));
        self.size.set_label(&if it.size > 0 { model::fmt_size(it.size) } else { String::new() });
        let f = it.fraction();
        if it.status == Status::Active && it.size == 0 {
            self.bar.pulse();
            self.bar.set_text(Some(&model::fmt_size(it.done)));
        } else {
            self.bar.set_fraction(f);
            self.bar.set_text(Some(&format!("{:.0}%", f * 100.0)));
        }
        self.speed.set_label(&model::fmt_speed(it.speed));
        self.eta.set_label(&model::fmt_eta(it.eta()));
        self.status.set_label(it.status.label());
        if it.status == Status::Error {
            self.status.add_css_class("error");
            self.status.set_tooltip_text(Some(&it.error));
        } else {
            self.status.remove_css_class("error");
            self.status.set_tooltip_text(None);
        }
    }
}

pub struct MainWindow {
    pub win: adw::ApplicationWindow,
    list: gtk::ListBox,
    rows: RefCell<HashMap<String, Row>>,
    counts: HashMap<&'static str, gtk::Label>,
    speed: gtk::Label,
    ext: gtk::Label,
    category: Rc<RefCell<&'static str>>,
    update: adw::Banner,
}

fn matches(cat: &str, it: &Item) -> bool {
    match cat {
        "all" => true,
        "unfinished" => !it.status.finished(),
        "finished" => it.status.finished(),
        c => it.category() == c,
    }
}

fn add_action(win: &adw::ApplicationWindow, name: &str, f: impl Fn() + 'static) {
    let a = gio::SimpleAction::new(name, None);
    a.connect_activate(move |_, _| f());
    win.add_action(&a);
}

impl MainWindow {
    pub fn build(app: &Rc<App>) -> Rc<MainWindow> {
        let win = adw::ApplicationWindow::builder()
            .application(&app.gapp)
            .title("Kara Download Manager")
            .default_width(1080)
            .default_height(620)
            .build();
        win.set_hide_on_close(true);

        // Toolbar ala NDM: tombol berlabel.
        let hb = adw::HeaderBar::new();
        for (icon, text, action, tip) in [
            ("list-add-symbolic", "Add", "win.add", "Add URL (Ctrl+N)"),
            ("media-playback-start-symbolic", "Resume", "win.resume", "Resume selected"),
            ("media-playback-pause-symbolic", "Pause", "win.pause", "Pause selected"),
            ("user-trash-symbolic", "Remove", "win.remove", "Remove selected (Delete)"),
        ] {
            let content = adw::ButtonContent::builder().icon_name(icon).label(text).build();
            let b = gtk::Button::builder().child(&content).action_name(action).tooltip_text(tip).build();
            hb.pack_start(&b);
        }
        let menu = gio::Menu::new();
        menu.append(Some("Open download folder"), Some("win.open-dir"));
        menu.append(Some("Clear finished from list"), Some("win.clear-finished"));
        menu.append(Some("Settings"), Some("win.settings"));
        menu.append(Some("Check for updates"), Some("win.check-update"));
        menu.append(Some("About"), Some("win.about"));
        menu.append(Some("Quit (stop catching downloads)"), Some("app.quit"));
        hb.pack_end(&gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).build());

        // Sidebar kategori.
        let side = gtk::ListBox::new();
        side.add_css_class("navigation-sidebar");
        let mut counts = HashMap::new();
        for (key, title, icon) in CATEGORIES.iter() {
            let b = gtk::Box::builder().spacing(10).build();
            b.append(&gtk::Image::from_icon_name(icon));
            b.append(&gtk::Label::builder().label(*title).xalign(0.0).hexpand(true).build());
            let c = gtk::Label::new(None);
            c.add_css_class("dim-label");
            c.add_css_class("numeric");
            b.append(&c);
            counts.insert(*key, c);
            let row = gtk::ListBoxRow::builder().child(&b).build();
            row.set_widget_name(key);
            side.append(&row);
        }
        // Pemisah antara filter status dan kategori jenis berkas.
        side.set_header_func(|row, _| {
            if row.index() == 3 {
                let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                sep.set_margin_top(6);
                sep.set_margin_bottom(6);
                row.set_header(Some(&sep));
            } else {
                row.set_header(None::<&gtk::Widget>);
            }
        });
        let side_scroll = gtk::ScrolledWindow::builder()
            .child(&side)
            .width_request(200)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        // Daftar unduhan + header kolom.
        let header =
            gtk::Box::builder().spacing(10).margin_top(6).margin_bottom(6).margin_start(10).margin_end(10).build();
        header.append(&gtk::Box::builder().width_request(16).build());
        let hname = gtk::Label::builder().label("Name").xalign(0.0).hexpand(true).build();
        header.append(&hname);
        for (t, w, x) in [
            ("Size", W_SIZE, 1.0),
            ("Progress", W_PROGRESS, 0.5),
            ("Speed", W_SPEED, 1.0),
            ("Time left", W_ETA, 1.0),
            ("Status", W_STATUS, 0.0),
        ] {
            header.append(&label(t, w, x));
        }
        header.add_css_class("dim-label");
        header.add_css_class("caption-heading");
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Multiple);
        list.set_activate_on_single_click(false);
        list.set_placeholder(Some(
            &adw::StatusPage::builder()
                .icon_name("folder-download-symbolic")
                .title("No downloads yet")
                .description("Click a download link in the browser (Kara DM extension), or press Add.")
                .build(),
        ));
        let scroll = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();
        let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main.append(&header);
        main.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        main.append(&scroll);

        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&side_scroll)
            .end_child(&main)
            .position(210)
            .shrink_start_child(false)
            .resize_start_child(false)
            .build();

        let bottom =
            gtk::Box::builder().spacing(12).margin_start(12).margin_end(12).margin_top(4).margin_bottom(4).build();
        let speed = gtk::Label::builder().xalign(0.0).hexpand(true).build();
        let ext = gtk::Label::builder().xalign(1.0).build();
        for l in [&speed, &ext] {
            l.add_css_class("caption");
            l.add_css_class("dim-label");
            bottom.append(l);
        }

        let update = adw::Banner::new("");
        {
            let a = app.clone();
            update.connect_button_clicked(move |_| a.start_update());
        }
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&hb);
        tv.add_top_bar(&update);
        tv.add_bottom_bar(&bottom);
        tv.set_content(Some(&paned));
        win.set_content(Some(&tv));

        let category: Rc<RefCell<&'static str>> = Rc::new(RefCell::new("all"));
        {
            let (app, category) = (app.clone(), category.clone());
            list.set_filter_func(move |row| {
                let id = row.widget_name();
                let cat = *category.borrow();
                app.items.borrow().iter().find(|i| i.id == id.as_str()).map(|it| matches(cat, it)).unwrap_or(true)
            });
        }
        {
            let (list, category) = (list.clone(), category.clone());
            side.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    let name = row.widget_name();
                    if let Some(c) = CATEGORIES.iter().find(|c| c.0 == name.as_str()) {
                        *category.borrow_mut() = c.0;
                        list.invalidate_filter();
                    }
                }
            });
        }
        side.select_row(side.row_at_index(0).as_ref());

        let mw = Rc::new(MainWindow {
            win: win.clone(),
            list: list.clone(),
            rows: RefCell::default(),
            counts,
            speed,
            ext,
            category,
            update,
        });

        // Aksi.
        let a = app.clone();
        add_action(&win, "add", move || a.show_add_dialog());
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "resume", move || {
            if let Some(m) = m.upgrade() {
                a.resume(&m.selected())
            }
        });
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "pause", move || {
            if let Some(m) = m.upgrade() {
                a.pause(&m.selected())
            }
        });
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "remove", move || {
            if let Some(m) = m.upgrade() {
                a.confirm_remove(m.selected(), &m.win)
            }
        });
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "open", move || {
            if let Some(m) = m.upgrade()
                && let Some(id) = m.selected().first()
            {
                a.open_file(id)
            }
        });
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "open-folder", move || {
            if let Some(m) = m.upgrade()
                && let Some(id) = m.selected().first()
            {
                a.open_folder(id)
            }
        });
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "progress", move || {
            if let Some(m) = m.upgrade()
                && let Some(id) = m.selected().first()
            {
                a.show_progress(id)
            }
        });
        let (a, m) = (app.clone(), Rc::downgrade(&mw));
        add_action(&win, "copy-url", move || {
            if let Some(m) = m.upgrade() {
                let urls: Vec<String> = m.selected().iter().filter_map(|id| a.item(id).map(|i| i.url)).collect();
                m.win.clipboard().set_text(&urls.join("\n"));
            }
        });
        let a = app.clone();
        add_action(&win, "open-dir", move || a.open_download_dir());
        let a = app.clone();
        add_action(&win, "clear-finished", move || a.clear_finished());
        let a = app.clone();
        add_action(&win, "settings", move || a.show_settings());
        let a = app.clone();
        add_action(&win, "check-update", move || a.check_update(true));
        let w = win.clone();
        add_action(&win, "about", move || {
            adw::AboutDialog::builder()
                .application_name("Kara Download Manager")
                .application_icon(crate::APP_ID)
                .version(env!("CARGO_PKG_VERSION"))
                .developer_name("kamisama-v0")
                .comments("Multi-connection download manager (aria2) that takes downloads from the Kara DM or Neat Download Manager browser extension. Clean-room implementation; not affiliated with Neat Download Manager.")
                .website("https://github.com/kamisama-v0/kara-dm")
                .issue_url("https://github.com/kamisama-v0/kara-dm/issues")
                .license_type(gtk::License::Apache20)
                .build()
                .present(Some(&w));
        });
        app.gapp.set_accels_for_action("win.add", &["<Control>n"]);
        app.gapp.set_accels_for_action("win.remove", &["Delete"]);
        app.gapp.set_accels_for_action("win.open-dir", &["<Control>o"]);

        // Klik ganda: berkas selesai dibuka, selain itu jendela progres.
        {
            let a = app.clone();
            list.connect_row_activated(move |_, row| {
                let id = row.widget_name().to_string();
                match a.item(&id) {
                    Some(it) if it.status.finished() => a.open_file(&id),
                    Some(_) => a.show_progress(&id),
                    None => {}
                }
            });
        }
        // Menu klik kanan.
        let ctx = gio::Menu::new();
        ctx.append(Some("Open"), Some("win.open"));
        ctx.append(Some("Open folder"), Some("win.open-folder"));
        ctx.append(Some("Progress window"), Some("win.progress"));
        ctx.append(Some("Resume"), Some("win.resume"));
        ctx.append(Some("Pause"), Some("win.pause"));
        ctx.append(Some("Copy URL"), Some("win.copy-url"));
        ctx.append(Some("Remove…"), Some("win.remove"));
        let pop = gtk::PopoverMenu::from_model(Some(&ctx));
        pop.set_parent(&list);
        pop.set_has_arrow(false);
        let click = gtk::GestureClick::builder().button(3).build();
        {
            let list = list.clone();
            click.connect_pressed(move |_, _, x, y| {
                if let Some(row) = list.row_at_y(y as i32) {
                    if !row.is_selected() {
                        list.unselect_all();
                        list.select_row(Some(&row));
                    }
                    pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                    pop.popup();
                }
            });
        }
        list.add_controller(click);

        for it in app.items.borrow().iter() {
            mw.add_row(it);
        }
        mw
    }

    pub fn selected(&self) -> Vec<String> {
        self.list.selected_rows().iter().map(|r| r.widget_name().to_string()).collect()
    }

    pub fn add_row(&self, it: &Item) {
        let r = Row::new(it);
        self.list.prepend(&r.row);
        self.rows.borrow_mut().insert(it.id.clone(), r);
    }

    pub fn update_row(&self, it: &Item) {
        if let Some(r) = self.rows.borrow().get(&it.id) {
            r.update(it);
        }
    }

    pub fn remove_row(&self, id: &str) {
        if let Some(r) = self.rows.borrow_mut().remove(id) {
            self.list.remove(&r.row);
        }
    }

    pub fn refilter(&self) {
        let _ = &self.category;
        self.list.invalidate_filter();
    }

    /// Spanduk "pembaruan tersedia" di bawah toolbar, mengikuti `app.update`.
    pub fn show_update(&self, app: &App) {
        let Some(r) = app.update.borrow().clone() else {
            self.update.set_revealed(false);
            return;
        };
        let installs = cfg!(windows) && r.installer.is_some();
        let (title, button) = if app.is_updating() {
            (format!("Downloading Kara DM {}… The app reopens after installing.", r.version), "")
        } else if installs {
            (format!("Kara DM {} is available.", r.version), "Update now")
        } else {
            (format!("Kara DM {} is available.", r.version), "View release")
        };
        self.update.set_title(&title);
        self.update.set_button_label(Some(button).filter(|b| !b.is_empty()));
        self.update.set_revealed(true);
    }

    pub fn refresh_summary(
        &self,
        items: &[Item],
        connections: usize,
        ext_error: Option<&str>,
        aria_error: Option<&str>,
    ) {
        for (key, l) in &self.counts {
            let n = items.iter().filter(|it| matches(key, it)).count();
            l.set_label(&if n > 0 { n.to_string() } else { String::new() });
        }
        let active = items.iter().filter(|i| i.status == Status::Active).count();
        let speed: u64 = items.iter().map(|i| i.speed).sum();
        self.speed.set_label(&match aria_error {
            Some(e) => format!("aria2 engine is not running: {e}"),
            None if active > 0 => format!("↓ {}   ·   {active} active", model::fmt_speed(speed)),
            None => "Ready".into(),
        });
        self.ext.set_label(&match (ext_error, connections) {
            (Some(e), _) => format!("Browser catching is off: {e}"),
            (None, 0) => "Browser extension: not connected yet (connects on the first download)".into(),
            (None, n) => format!("Browser extension: connected ({n})"),
        });
    }
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// Bar segmen ala NDM: tiap potongan (piece) aria2 yang sudah ada digambar dari bitfield.
fn segment_bar(data: Rc<RefCell<(String, u32, f64)>>) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder().content_height(16).hexpand(true).build();
    area.set_draw_func(move |area, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        let (bits, pieces, frac) = data.borrow().clone();
        let fg = area.color();
        rounded_rect(cr, 0.0, 0.0, w, h, 4.0);
        cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.12);
        let _ = cr.fill_preserve();
        cr.clip();
        let acc = adw::StyleManager::default().accent_color_rgba();
        cr.set_source_rgba(acc.red() as f64, acc.green() as f64, acc.blue() as f64, 1.0);
        let bytes: Vec<u8> =
            (0..bits.len() / 2).filter_map(|i| u8::from_str_radix(&bits[2 * i..2 * i + 2], 16).ok()).collect();
        if pieces > 0 && !bytes.is_empty() && frac < 1.0 {
            let n = pieces as usize;
            let has = |i: usize| bytes.get(i / 8).is_some_and(|b| b & (0x80 >> (i % 8)) != 0);
            let mut i = 0;
            while i < n {
                if has(i) {
                    let s = i;
                    while i < n && has(i) {
                        i += 1;
                    }
                    let x0 = (s as f64 * w / n as f64).floor();
                    let x1 = (i as f64 * w / n as f64).ceil();
                    cr.rectangle(x0, 0.0, x1 - x0, h);
                } else {
                    i += 1;
                }
            }
        } else {
            cr.rectangle(0.0, 0.0, w * frac, h);
        }
        let _ = cr.fill();
    });
    area
}

pub struct ProgressWindow {
    pub win: adw::Window,
    name: gtk::Label,
    values: [gtk::Label; 6],
    seg_data: Rc<RefCell<(String, u32, f64)>>,
    seg: gtk::DrawingArea,
    bar: gtk::ProgressBar,
    pause: gtk::Button,
    running: gtk::Box,
    done: gtk::Box,
    error: gtk::Label,
}

impl ProgressWindow {
    pub fn new(app: &Rc<App>, it: &Item) -> Rc<ProgressWindow> {
        let win = adw::Window::builder()
            .application(&app.gapp)
            .title(format!("Download: {}", it.display_name()))
            .default_width(560)
            .resizable(false)
            .build();
        let b = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();
        let name = gtk::Label::builder().xalign(0.0).build();
        name.set_ellipsize(pango::EllipsizeMode::Middle);
        name.add_css_class("title-4");
        let url = gtk::Label::builder().label(&it.url).xalign(0.0).selectable(true).build();
        url.set_ellipsize(pango::EllipsizeMode::Middle);
        url.add_css_class("dim-label");
        url.add_css_class("caption");
        b.append(&name);
        b.append(&url);
        let grid = gtk::Grid::builder().column_spacing(16).row_spacing(4).build();
        let values: [gtk::Label; 6] = std::array::from_fn(|_| {
            let l = gtk::Label::builder().xalign(0.0).hexpand(true).build();
            l.add_css_class("numeric");
            l
        });
        for (i, t) in ["Status", "Size", "Downloaded", "Speed", "Time left", "Connections"].iter().enumerate() {
            let k = gtk::Label::builder().label(*t).xalign(0.0).build();
            k.add_css_class("dim-label");
            let (col, row) = ((i % 2) as i32 * 2, (i / 2) as i32);
            grid.attach(&k, col, row, 1, 1);
            grid.attach(&values[i], col + 1, row, 1, 1);
        }
        b.append(&grid);
        let seg_data = Rc::new(RefCell::new((String::new(), 0, 0.0)));
        let seg = segment_bar(seg_data.clone());
        b.append(&seg);
        let bar = gtk::ProgressBar::builder().show_text(true).build();
        b.append(&bar);
        let error = gtk::Label::builder().xalign(0.0).wrap(true).visible(false).build();
        error.add_css_class("error");
        b.append(&error);

        let running = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        let pause = gtk::Button::with_label("Pause");
        let cancel = gtk::Button::with_label("Cancel…");
        cancel.add_css_class("destructive-action");
        let hide = gtk::Button::with_label("Hide");
        running.append(&cancel);
        running.append(&pause);
        running.append(&hide);
        let done = gtk::Box::builder().spacing(8).halign(gtk::Align::End).visible(false).build();
        let open = gtk::Button::with_label("Open");
        open.add_css_class("suggested-action");
        let folder = gtk::Button::with_label("Open folder");
        let close = gtk::Button::with_label("Close");
        done.append(&close);
        done.append(&folder);
        done.append(&open);
        b.append(&running);
        b.append(&done);
        win.set_content(Some(&b));

        let id = it.id.clone();
        {
            let (a, id) = (app.clone(), id.clone());
            pause.connect_clicked(move |_| {
                if let Some(it) = a.item(&id) {
                    if matches!(it.status, Status::Active | Status::Waiting) {
                        a.pause(std::slice::from_ref(&id))
                    } else {
                        a.resume(std::slice::from_ref(&id))
                    }
                }
            });
        }
        {
            let (a, id, w) = (app.clone(), id.clone(), win.clone());
            cancel.connect_clicked(move |_| a.confirm_remove(vec![id.clone()], &w));
        }
        for btn in [&hide, &close] {
            let w = win.clone();
            btn.connect_clicked(move |_| w.close());
        }
        {
            let (a, id, w) = (app.clone(), id.clone(), win.clone());
            open.connect_clicked(move |_| {
                a.open_file(&id);
                w.close();
            });
        }
        {
            let (a, id, w) = (app.clone(), id.clone(), win.clone());
            folder.connect_clicked(move |_| {
                a.open_folder(&id);
                w.close();
            });
        }
        {
            let (a, id) = (app.clone(), id.clone());
            win.connect_close_request(move |_| {
                a.progress.borrow_mut().remove(&id);
                glib::Propagation::Proceed
            });
        }
        let pw = Rc::new(ProgressWindow { win, name, values, seg_data, seg, bar, pause, running, done, error });
        pw.update(it);
        pw
    }

    pub fn update(&self, it: &Item) {
        self.name.set_label(it.display_name());
        let f = it.fraction();
        let v = [
            it.status.label().to_string(),
            if it.size > 0 { model::fmt_size(it.size) } else { "unknown".into() },
            format!("{} ({:.1}%)", model::fmt_size(it.done), f * 100.0),
            model::fmt_speed(it.speed),
            model::fmt_eta(it.eta()),
            if it.conns > 0 { it.conns.to_string() } else { String::new() },
        ];
        for (l, t) in self.values.iter().zip(v) {
            l.set_label(&t);
        }
        *self.seg_data.borrow_mut() = (it.bitfield.clone(), it.pieces, f);
        self.seg.queue_draw();
        if it.status == Status::Active && it.size == 0 {
            self.bar.pulse();
        } else {
            self.bar.set_fraction(f);
        }
        self.bar.set_text(Some(&format!("{:.0}%", f * 100.0)));
        self.pause.set_label(if matches!(it.status, Status::Active | Status::Waiting) { "Pause" } else { "Resume" });
        self.pause.set_sensitive(it.status != Status::Merging);
        self.error.set_visible(it.status == Status::Error);
        self.error.set_label(&it.error);
        let finished = it.status.finished();
        self.running.set_visible(!finished);
        self.done.set_visible(finished);
    }
}

/// Dialog pengaturan; nilai diterapkan saat dialog ditutup.
pub fn settings_dialog(app: &Rc<App>, parent: &impl IsA<gtk::Widget>) {
    let s = app.settings.borrow().clone();
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title("Settings");
    let page = adw::PreferencesPage::new();

    let g = adw::PreferencesGroup::builder()
        .title("Downloads")
        .description(
            "While downloading, files are kept in this folder's \"temp\" subfolder and moved here once complete.",
        )
        .build();
    let dir = Rc::new(RefCell::new(s.download_dir.clone()));
    let dir_row = adw::ActionRow::builder().title("Download folder").subtitle(s.dir().to_string_lossy()).build();
    let pick = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text("Choose folder")
        .build();
    pick.add_css_class("flat");
    {
        let (dir, dir_row, d) = (dir.clone(), dir_row.clone(), dialog.clone());
        pick.connect_clicked(move |_| {
            let fd = gtk::FileDialog::builder().title("Choose download folder").build();
            let (dir, dir_row) = (dir.clone(), dir_row.clone());
            let root = d.root().and_then(|r| r.downcast::<gtk::Window>().ok());
            fd.select_folder(root.as_ref(), gio::Cancellable::NONE, move |res| {
                if let Some(p) = res.ok().and_then(|f| f.path()) {
                    let text = p.to_string_lossy().into_owned();
                    dir_row.set_subtitle(&text);
                    *dir.borrow_mut() = text;
                }
            });
        });
    }
    dir_row.add_suffix(&pick);
    g.add(&dir_row);
    let conns = adw::SpinRow::with_range(1.0, 16.0, 1.0);
    conns.set_title("Connections per file");
    conns.set_subtitle("Parallel segments (max 16)");
    conns.set_value(s.connections as f64);
    g.add(&conns);
    let conc = adw::SpinRow::with_range(1.0, 10.0, 1.0);
    conc.set_title("Simultaneous downloads");
    conc.set_value(s.max_concurrent as f64);
    g.add(&conc);
    let limit = adw::SpinRow::with_range(0.0, 1_000_000.0, 256.0);
    limit.set_title("Total speed limit (KB/s)");
    limit.set_subtitle("0 = unlimited; applies immediately");
    limit.set_value(s.speed_limit_kib as f64);
    g.add(&limit);
    page.add(&g);

    let g2 = adw::PreferencesGroup::builder().title("Browser").build();
    let prog = adw::SwitchRow::builder()
        .title("Show a progress window for browser downloads")
        .active(s.show_progress_window)
        .build();
    let panel = adw::SwitchRow::builder()
        .title("Video catch panel on pages")
        .subtitle("Applies when the extension reconnects")
        .active(s.media_panel)
        .build();
    let notify = adw::SwitchRow::builder().title("Notify when a download completes").active(s.notify).build();
    g2.add(&prog);
    g2.add(&panel);
    g2.add(&notify);
    let ext_row = adw::ActionRow::builder()
        .title("Browser extension")
        .subtitle(
            "Install the Kara DM extension (Firefox, Chrome, Chromium); the Neat Download Manager extension also works",
        )
        .build();
    let open = gtk::Button::builder()
        .icon_name("external-link-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text("Open the extension page")
        .build();
    open.add_css_class("flat");
    open.connect_clicked(|b| {
        let root = b.root().and_then(|r| r.downcast::<gtk::Window>().ok());
        gtk::UriLauncher::new(EXTENSION_URL).launch(root.as_ref(), gio::Cancellable::NONE, |_| {});
    });
    ext_row.add_suffix(&open);
    g2.add(&ext_row);
    page.add(&g2);

    let g3 = adw::PreferencesGroup::builder().title("Updates").build();
    let updates = adw::SwitchRow::builder()
        .title("Check for updates automatically")
        .subtitle(if cfg!(windows) {
            "Checks GitHub for a new release once a day; installing still waits for you"
        } else {
            "Checks GitHub for a new release once a day and tells you"
        })
        .active(s.check_updates)
        .build();
    g3.add(&updates);
    page.add(&g3);
    dialog.add(&page);

    let a = app.clone();
    dialog.connect_closed(move |_| {
        let mut n = a.settings.borrow().clone();
        n.download_dir = dir.borrow().clone();
        n.connections = conns.value() as u32;
        n.max_concurrent = conc.value() as u32;
        n.speed_limit_kib = limit.value() as u32;
        n.show_progress_window = prog.is_active();
        n.media_panel = panel.is_active();
        n.notify = notify.is_active();
        n.check_updates = updates.is_active();
        a.apply_settings(n);
    });
    dialog.present(Some(parent));
}

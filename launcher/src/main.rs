//! Omnivores Rust: the launcher for the Rust rewrites of the games. It
//! finds the games and mods in the library folders, shows each with the
//! title and icon it carries in its own files, and starts it with the
//! engine it was built for. One program, for Windows and Linux alike.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod importer;
mod install;
mod library;
mod winicon;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use eframe::egui;
use egui::text::{LayoutJob, TextWrapping};
use egui::{Align, Color32, FontId, Key, Layout, RichText, Sense, TextureHandle, TextureOptions};

use config::{default_library, Config};
use install::Place;
use library::{Engine, Game, ENGINES};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = command_line(&args) {
        std::process::exit(code);
    }
    // Folders and archives given to it - dropped on its icon, say - are
    // offered for adding.
    let sources: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .collect();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Omnivores Rust")
        .with_app_id("omnivores-rust")
        .with_inner_size([720.0, 560.0])
        .with_min_inner_size([420.0, 300.0]);
    if let Some(img) = winicon::read_ico(include_bytes!("../icon.ico")) {
        viewport = viewport.with_icon(egui::IconData {
            rgba: img.rgba,
            width: img.width as u32,
            height: img.height as u32,
        });
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Omnivores Rust",
        options,
        Box::new(|_cc| Ok(Box::new(Launcher::new(sources)))),
    )
}

// ------------------------------------------------------------------------
// The command line

fn usage() -> String {
    format!(
        "Usage: omnivores-rust [OPTION]... [FOLDER|ARCHIVE]...\n\
         With no option, opens the launcher; folders and archives given to it\n\
         are offered for adding to the games folder.\n\n\
         \x20 --install     copy the programs to {}, and add Omnivores Rust\n\
         \x20               to the {} and the desktop\n\
         \x20 --no-copy     with --install: leave the programs where they are\n\
         \x20 --no-menu     with --install: no menu entry\n\
         \x20 --no-desktop  with --install: no desktop shortcut\n\
         \x20 --uninstall   take all of that away again; the games folder and the\n\
         \x20               settings stay\n\
         \x20 --version     print the version\n",
        install::programs_dir().display(),
        Place::Menu.label()
    )
}

/// The options that do their work without a window, and the exit code
/// they end with; None to open the window.
fn command_line(args: &[String]) -> Option<i32> {
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--help") || has("-h") {
        print!("{}", usage());
        return Some(0);
    }
    if has("--version") {
        println!("omnivores-rust {}", env!("CARGO_PKG_VERSION"));
        return Some(0);
    }
    if !has("--install") && !has("--uninstall") {
        return None;
    }
    let title = "Omnivores Rust";
    let mut config = Config::load();
    let mut must_exit = false;
    let outcome = if has("--install") {
        let opts = install::Options {
            copy: !has("--no-copy"),
            menu: !has("--no-menu"),
            desktop: !has("--no-desktop"),
        };
        let done = install::install(&mut config, opts);
        if done.is_ok() {
            config.offer_install = false;
        }
        done
    } else {
        install::uninstall(&mut config).map(|r| {
            must_exit = r.must_exit;
            let library = if config.library_paths.is_empty() {
                default_library().display().to_string()
            } else {
                config.library_paths.join(", ")
            };
            let mut lines = r.lines;
            lines.push(format!(
                "Your games and settings were kept, in {library} and {}.",
                config::config_dir().display()
            ));
            lines
        })
    };
    Some(match outcome {
        Ok(lines) => {
            if let Err(e) = config.save() {
                eprintln!("omnivores-rust: could not save the settings: {e}");
            }
            install::tell(title, &lines.join("\n"));
            if must_exit {
                install::remove_after_exit();
            }
            0
        }
        Err(e) => {
            install::tell(title, &e);
            1
        }
    })
}

// ------------------------------------------------------------------------
// The system's file dialogs

/// What a file dialog offers to open.
#[derive(Clone, Copy, PartialEq)]
enum Files {
    Images,
    Programs,
    Archives,
}

impl Files {
    fn filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Files::Images => (
                "Images",
                &["png", "jpg", "jpeg", "bmp", "webp", "ico", "exe"],
            ),
            Files::Programs => ("Programs", &["exe"]),
            Files::Archives => ("Archives", &["zip", "7z", "rar"]),
        }
    }
}

#[cfg(windows)]
fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_directory(start)
        .pick_folder()
}

#[cfg(windows)]
fn pick_file(title: &str, start: &Path, files: Files) -> Option<PathBuf> {
    let (name, exts) = files.filter();
    rfd::FileDialog::new()
        .set_title(title)
        .set_directory(start)
        .add_filter(name, exts)
        .add_filter("All files", &["*"])
        .pick_file()
}

/// Elsewhere the desktop's own dialog asks: zenity, or KDE's kdialog when
/// zenity is not installed. `None` when neither could be started.
#[cfg(not(windows))]
fn ask(zenity: &[&str], kdialog: &[&str]) -> Option<Option<PathBuf>> {
    let out = Command::new("zenity")
        .args(zenity)
        .output()
        .or_else(|_| Command::new("kdialog").args(kdialog).output())
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some((out.status.success() && !s.is_empty()).then(|| PathBuf::from(s)))
}

#[cfg(not(windows))]
fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    let start = format!("{}/", start.display());
    ask(
        &[
            "--file-selection",
            "--directory",
            "--title",
            title,
            "--filename",
            &start,
        ],
        &["--title", title, "--getexistingdirectory", &start],
    )
    .unwrap_or_else(no_dialog)
}

/// Programs here have no extension to filter on, and pictures come in too
/// many kinds to be worth it: only archives are filtered.
#[cfg(not(windows))]
fn pick_file(title: &str, start: &Path, files: Files) -> Option<PathBuf> {
    let start = format!("{}/", start.display());
    let mut zenity = vec!["--file-selection", "--title", title, "--filename", &start];
    let mut kdialog = vec!["--title", title, "--getopenfilename", &start];
    let (zenity_filter, kdialog_filter);
    if files == Files::Archives {
        let (name, exts) = files.filter();
        let globs: Vec<String> = exts
            .iter()
            .flat_map(|e| [format!("*.{e}"), format!("*.{}", e.to_uppercase())])
            .collect();
        zenity_filter = format!("--file-filter={name} | {}", globs.join(" "));
        kdialog_filter = format!("{}|{name}", globs.join(" "));
        zenity.extend([zenity_filter.as_str(), "--file-filter=All files | *"]);
        kdialog.push(&kdialog_filter);
    }
    ask(&zenity, &kdialog).unwrap_or_else(no_dialog)
}

#[cfg(not(windows))]
fn no_dialog() -> Option<PathBuf> {
    eprintln!("omnivores-rust: install zenity or kdialog to choose files and folders");
    None
}

fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

// ------------------------------------------------------------------------
// Starting things

/// Starts a game: the engine runs in the install's folder, which is where
/// it takes its data from and writes its profiles back to, and it is left
/// to run on its own.
fn spawn_detached(program: &Path, args: &[&Path], cwd: Option<&Path>) -> std::io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_| ())
}

fn open_folder(path: &Path) -> std::io::Result<()> {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    spawn_detached(Path::new(opener), &[path], None)
}

// ------------------------------------------------------------------------
// The window

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Library,
    Engines,
    Customised,
    Appearance,
    Install,
}

/// The settings window's working copy; OK keeps it, Cancel drops it.
struct Settings {
    tab: Tab,
    paths: Vec<String>,
    path_sel: BTreeSet<usize>,
    engines: HashMap<Engine, String>,
    overrides: std::collections::BTreeMap<String, config::Override>,
    custom_sel: BTreeSet<String>,
    icon_size: u32,
    confirm_clear: bool,
    install: InstallTab,
}

/// The Install tab. Unlike the rest of the settings its buttons act at
/// once.
struct InstallTab {
    status: install::Status,
    copy: bool,
    menu: bool,
    desktop: bool,
    /// What the last install or uninstall did.
    report: Vec<String>,
    confirm_uninstall: bool,
}

impl InstallTab {
    fn new() -> InstallTab {
        let status = install::status();
        // Everything ticked the first time; afterwards, what is there.
        let fresh = !status.installed && !status.menu && status.desktop != Some(true);
        InstallTab {
            copy: !status.running_installed,
            menu: status.menu || fresh,
            desktop: status.desktop == Some(true) || (fresh && status.desktop.is_some()),
            status,
            report: Vec::new(),
            confirm_uninstall: false,
        }
    }
}

enum InstallAct {
    Install(install::Options),
    Uninstall,
}

/// One thing found to add, as the player has set it.
struct Pick {
    found: importer::Found,
    include: bool,
    name: String,
    is_mod: bool,
    engine: Engine,
    /// For something that is only part of a game: the game to lay it over
    /// (its key), if any.
    base: Option<String>,
}

enum Work {
    Look(importer::Job<importer::Analysis>),
    Search(importer::Job<Vec<importer::Found>>),
    /// With where each chosen pick is going, its name and its engine.
    Copy(
        importer::Job<importer::Installed>,
        Vec<(usize, PathBuf, String, Engine)>,
    ),
}

impl Work {
    fn ctl(&self) -> &importer::Ctl {
        match self {
            Work::Look(j) => &j.ctl,
            Work::Search(j) => &j.ctl,
            Work::Copy(j, _) => &j.ctl,
        }
    }
}

/// The Add games and mods window.
#[derive(Default)]
struct Adding {
    picks: Vec<Pick>,
    /// Folders archives were unpacked into, deleted when the window closes.
    unpacked: Vec<PathBuf>,
    notes: Vec<String>,
    work: Option<Work>,
    /// Dropped on the window while it was busy, to look at next.
    queued: Vec<PathBuf>,
    /// Which library folder things go into.
    target: usize,
}

impl Drop for Adding {
    fn drop(&mut self) {
        if let Some(w) = &self.work {
            w.ctl().cancel();
        }
        for d in &self.unpacked {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

enum Dialog {
    /// The games folder does not exist yet.
    FirstRun,
    Rename {
        game: usize,
        text: String,
        focused: bool,
    },
    Message {
        title: String,
        text: String,
    },
    Settings(Box<Settings>),
    Add(Box<Adding>),
}

enum Action {
    Play(usize),
    Rename(usize),
    ChangeIcon(usize),
    SetEngine(usize, Engine),
    Reset(usize),
    OpenFolder(usize),
    Shortcut(usize, Place, bool),
}

struct Launcher {
    config: Config,
    games: Vec<Game>,
    selected: Option<usize>,
    search: String,
    focus_search: bool,
    status: String,
    icons: HashMap<PathBuf, Option<(TextureHandle, [usize; 2])>>,
    dialog: Option<Dialog>,
    scroll_to_selected: bool,
    /// Offering to install the launcher, which has no menu entry yet.
    offer: bool,
    /// Given on the command line, to add once the window is free.
    startup: Vec<PathBuf>,
}

/// Sizes the way people read them.
fn size_text(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    match bytes {
        b if b >= 10 * MB => format!("{} MB", b / MB),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b => format!("{} KB", b.div_ceil(1024)),
    }
}

fn new_ctl(ctx: &egui::Context) -> importer::Ctl {
    let ctx = ctx.clone();
    importer::Ctl::new(move || ctx.request_repaint())
}

impl Launcher {
    fn new(startup: Vec<PathBuf>) -> Launcher {
        let mut l = Launcher {
            config: Config::load(),
            games: Vec::new(),
            selected: None,
            search: String::new(),
            focus_search: false,
            status: String::new(),
            icons: HashMap::new(),
            dialog: None,
            scroll_to_selected: false,
            offer: false,
            startup,
        };
        l.offer = l.config.offer_install && !install::launcher_has(Place::Menu);
        l.first_run();
        importer::sweep(&l.library_roots());
        l.reload();
        l
    }

    /// The library folders; the default one while none is set, which is
    /// where anything added goes.
    fn library_roots(&self) -> Vec<PathBuf> {
        if self.config.library_paths.is_empty() {
            vec![default_library()]
        } else {
            self.config
                .library_paths
                .iter()
                .map(PathBuf::from)
                .collect()
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.config.save() {
            self.dialog = Some(Dialog::Message {
                title: "Could not save the settings".into(),
                text: e.to_string(),
            });
        }
    }

    /// The first time through: take the games folder if it is there, or ask.
    fn first_run(&mut self) {
        if !self.config.library_paths.is_empty() {
            return;
        }
        let lib = default_library();
        if lib.is_dir() {
            self.config.library_paths = vec![lib.to_string_lossy().into_owned()];
            self.save();
        } else {
            self.dialog = Some(Dialog::FirstRun);
        }
    }

    fn reload(&mut self) {
        let key = self.selected.and_then(|i| self.games.get(i)).map(Game::key);
        let mut games = library::scan(&self.config.library_paths);
        library::sort(&mut games);
        self.config.apply_overrides(&mut games);
        self.games = games;
        self.icons.clear();
        self.selected = key
            .and_then(|k| self.games.iter().position(|g| g.key() == k))
            .or(if self.games.is_empty() { None } else { Some(0) });
        self.fix_selection();
        self.update_status();
    }

    fn update_status(&mut self) {
        self.status = if self.config.library_paths.is_empty() {
            "No library folder set — open Settings to add one.".into()
        } else if self.games.is_empty() {
            format!("Nothing found in {}", self.config.library_paths.join(", "))
        } else {
            let mods = self.games.iter().filter(|g| g.is_mod).count();
            let base = self.games.len() - mods;
            format!(
                "{base} game{}, {mods} mod{}",
                if base != 1 { "s" } else { "" },
                if mods != 1 { "s" } else { "" }
            )
        };
    }

    fn visible(&self, g: &Game) -> bool {
        let t = self.search.trim().to_lowercase();
        t.is_empty()
            || g.title().to_lowercase().contains(&t)
            || g.subtitle().to_lowercase().contains(&t)
    }

    /// The selection moves to the first entry the search leaves showing.
    fn fix_selection(&mut self) {
        let ok = self
            .selected
            .and_then(|i| self.games.get(i))
            .map(|g| self.visible(g))
            .unwrap_or(false);
        if !ok {
            self.selected = (0..self.games.len()).find(|&i| self.visible(&self.games[i]));
        }
    }

    fn icon(&mut self, ctx: &egui::Context, g: &Game) -> Option<(TextureHandle, [usize; 2])> {
        let source = g.custom_icon.clone().or(g.icon_source.clone())?;
        if let Some(t) = self.icons.get(&source) {
            return t.clone();
        }
        let size = self.config.icon_size as usize;
        // The player's own image if they set one, else whatever the install
        // carries.
        let loaded = source
            .is_file()
            .then(|| winicon::picture(&source))
            .flatten()
            .map(|img| {
                let (dim, rgba) = ([img.width, img.height], img.rgba);
                // Retail artwork is 32x32 pixel art: scaled by a whole number it
                // stays crisp, anything else is better off smoothed.
                let sharp = size > dim[0] && dim[0] > 0 && size.is_multiple_of(dim[0]);
                let options = if sharp {
                    TextureOptions::NEAREST
                } else {
                    TextureOptions::LINEAR
                };
                let img = egui::ColorImage::from_rgba_unmultiplied(dim, &rgba);
                (
                    ctx.load_texture(source.to_string_lossy(), img, options),
                    dim,
                )
            });
        self.icons.insert(source, loaded.clone());
        loaded
    }

    // -------------------------------------------------------------- actions

    fn launch(&mut self, i: usize) {
        let Some(g) = self.games.get(i) else { return };
        let engine = g.active_engine();
        let bin = self.config.engine_path(engine).trim().to_string();
        if bin.is_empty() || !Path::new(&bin).is_file() {
            self.dialog = Some(Dialog::Message {
                title: "Engine not found".into(),
                text: format!(
                    "{} needs the {} program ({}), which is not where the settings say it is.\n\n\
                     Set its location under Settings → Engines.",
                    g.title(),
                    engine.label(),
                    engine.binary()
                ),
            });
            return;
        }
        let title = g.title().to_string();
        match spawn_detached(Path::new(&bin), &[], Some(&g.path)) {
            Ok(()) => self.status = format!("Launched {title}"),
            Err(e) => {
                self.dialog = Some(Dialog::Message {
                    title: "Could not start".into(),
                    text: e.to_string(),
                })
            }
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Play(i) => self.launch(i),
            Action::Rename(i) => {
                if let Some(g) = self.games.get(i) {
                    self.dialog = Some(Dialog::Rename {
                        game: i,
                        text: g.title().to_string(),
                        focused: false,
                    });
                }
            }
            Action::ChangeIcon(i) => {
                let Some(g) = self.games.get(i).cloned() else {
                    return;
                };
                let Some(path) = pick_file("Choose an icon", &home(), Files::Images) else {
                    return;
                };
                match self.config.store_custom_icon(&g, &path) {
                    Ok(stored) => {
                        self.config.set_override(&g, |o| {
                            o.icon = Some(stored.to_string_lossy().into_owned())
                        });
                        self.save();
                        self.reload();
                    }
                    Err(e) => {
                        self.dialog = Some(Dialog::Message {
                            title: "Could not use that image".into(),
                            text: e.to_string(),
                        })
                    }
                }
            }
            Action::SetEngine(i, e) => {
                let Some(g) = self.games.get(i).cloned() else {
                    return;
                };
                self.config
                    .set_override(&g, |o| o.engine = Some(e.id().into()));
                self.save();
                self.reload();
            }
            Action::Reset(i) => {
                let Some(g) = self.games.get(i).cloned() else {
                    return;
                };
                self.config.overrides.remove(&g.key());
                self.save();
                self.reload();
            }
            Action::OpenFolder(i) => {
                if let Some(g) = self.games.get(i) {
                    if let Err(e) = open_folder(&g.path) {
                        self.dialog = Some(Dialog::Message {
                            title: "Could not open folder".into(),
                            text: e.to_string(),
                        });
                    }
                }
            }
            Action::Shortcut(i, place, on) => {
                let Some(g) = self.games.get(i).cloned() else {
                    return;
                };
                let result = install::set_game_shortcut(&mut self.config, &g, place, on);
                self.save();
                match result {
                    Ok(()) if on => {
                        self.status = format!("Added {} to the {}", g.title(), place.label())
                    }
                    Ok(()) => self.status = format!("Took {} off the {}", g.title(), place.label()),
                    Err(e) => {
                        self.dialog = Some(Dialog::Message {
                            title: "Could not make the shortcut".into(),
                            text: e,
                        })
                    }
                }
            }
        }
    }

    fn open_settings(&mut self) {
        self.open_settings_at(Tab::Library);
    }

    fn open_settings_at(&mut self, tab: Tab) {
        let c = &self.config;
        self.dialog = Some(Dialog::Settings(Box::new(Settings {
            tab,
            install: InstallTab::new(),
            paths: c.library_paths.clone(),
            path_sel: BTreeSet::new(),
            engines: ENGINES.into_iter().map(|e| (e, c.engine_path(e))).collect(),
            overrides: c.overrides.clone(),
            custom_sel: BTreeSet::new(),
            icon_size: c.icon_size,
            confirm_clear: false,
        })));
    }

    // ------------------------------------------------------------- drawing

    fn rows(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let size = self.config.icon_size as f32;
        let row_h = size + 18.0;
        let mut action = None;
        let ctx = ui.ctx().clone();
        let order: Vec<usize> = (0..self.games.len())
            .filter(|&i| self.visible(&self.games[i]))
            .collect();
        for (n, &i) in order.iter().enumerate() {
            let g = self.games[i].clone();
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), Sense::click());
            let selected = self.selected == Some(i);
            let v = ui.visuals().clone();
            // Selection and hover in the theme's colours, alternate rows faint.
            let bg = if selected {
                Some(v.selection.bg_fill)
            } else if resp.hovered() {
                Some(v.widgets.hovered.weak_bg_fill)
            } else if n % 2 == 1 {
                Some(v.faint_bg_color)
            } else {
                None
            };
            if let Some(c) = bg {
                ui.painter().rect_filled(rect, 3.0, c);
            }

            let pad = 9.0;
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + pad, rect.center().y - size / 2.0),
                egui::vec2(size, size),
            );
            match self.icon(&ctx, &g) {
                Some((tex, dim)) => {
                    // Kept to its shape inside the square.
                    let k = (size / dim[0] as f32).min(size / dim[1] as f32);
                    let fit = egui::Rect::from_center_size(
                        icon_rect.center(),
                        egui::vec2(dim[0] as f32 * k, dim[1] as f32 * k),
                    );
                    egui::Image::new((tex.id(), fit.size())).paint_at(ui, fit);
                }
                None => {
                    // A neutral tile for an install whose artwork cannot be read.
                    let r = icon_rect.shrink(2.0);
                    ui.painter().rect_filled(
                        r,
                        6.0,
                        Color32::from_rgba_unmultiplied(120, 120, 120, 60),
                    );
                    ui.painter().rect_stroke(
                        r,
                        6.0,
                        egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 120, 140)),
                        egui::StrokeKind::Inside,
                    );
                }
            }

            let x = icon_rect.right() + pad;
            let w = (rect.right() - pad - x).max(10.0);
            let (text_col, sub_col) = if selected {
                (
                    v.selection.stroke.color,
                    v.selection.stroke.color.gamma_multiply(0.8),
                )
            } else {
                (v.strong_text_color(), v.weak_text_color())
            };
            let line = |text: &str, font: FontId, color: Color32| {
                let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
                job.wrap = TextWrapping {
                    max_width: w,
                    max_rows: 1,
                    break_anywhere: true,
                    overflow_character: Some('…'),
                };
                ui.fonts(|f| f.layout_job(job))
            };
            let title = line(g.title(), FontId::proportional(15.0), text_col);
            let sub = line(&g.subtitle(), FontId::proportional(12.5), sub_col);
            let top = rect.center().y - (title.size().y + sub.size().y) / 2.0;
            ui.painter()
                .galley(egui::pos2(x, top), title.clone(), text_col);
            ui.painter()
                .galley(egui::pos2(x, top + title.size().y), sub, sub_col);

            if selected && self.scroll_to_selected {
                ui.scroll_to_rect(rect, None);
                self.scroll_to_selected = false;
            }

            let guess = if g.engine_guessed && g.custom_engine.is_none() {
                "  (best guess)"
            } else {
                ""
            };
            let resp = resp.on_hover_text(format!(
                "{}\n{}\nEngine: {}{guess}",
                g.title(),
                g.path.display(),
                g.active_engine().label()
            ));
            if resp.clicked() || resp.secondary_clicked() {
                self.selected = Some(i);
            }
            if resp.double_clicked() {
                action = Some(Action::Play(i));
            }
            resp.context_menu(|ui| {
                if ui.button("Play").clicked() {
                    action = Some(Action::Play(i));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Rename…").clicked() {
                    action = Some(Action::Rename(i));
                    ui.close_menu();
                }
                if ui.button("Change icon…").clicked() {
                    action = Some(Action::ChangeIcon(i));
                    ui.close_menu();
                }
                ui.menu_button("Runs on", |ui| {
                    for e in ENGINES {
                        if ui.radio(g.active_engine() == e, e.label()).clicked() {
                            action = Some(Action::SetEngine(i, e));
                            ui.close_menu();
                        }
                    }
                });
                let custom =
                    g.custom_name.is_some() || g.custom_icon.is_some() || g.custom_engine.is_some();
                if ui
                    .add_enabled(custom, egui::Button::new("Reset to defaults"))
                    .clicked()
                {
                    action = Some(Action::Reset(i));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Open folder").clicked() {
                    action = Some(Action::OpenFolder(i));
                    ui.close_menu();
                }
                // Its own shortcuts, which start the game without the
                // launcher.
                ui.separator();
                for (place, label) in [
                    (Place::Menu, format!("In the {}", Place::Menu.label())),
                    (Place::Desktop, "On the desktop".to_string()),
                ] {
                    if place.dir().is_none() {
                        continue;
                    }
                    let mut on = install::game_has(&self.config, &g, place);
                    if ui.checkbox(&mut on, label).clicked() {
                        action = Some(Action::Shortcut(i, place, on));
                        ui.close_menu();
                    }
                }
            });
        }
        action
    }

    fn keys(&mut self, ctx: &egui::Context) -> Option<Action> {
        if self.dialog.is_some() {
            return None;
        }
        let (enter, up, down, refresh, settings, find) = ctx.input(|i| {
            (
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::F5),
                i.modifiers.command && i.key_pressed(Key::Comma),
                i.modifiers.command && i.key_pressed(Key::F),
            )
        });
        if refresh {
            self.reload();
        }
        if settings {
            self.open_settings();
        }
        if find {
            self.focus_search = true;
        }
        if up || down {
            let order: Vec<usize> = (0..self.games.len())
                .filter(|&i| self.visible(&self.games[i]))
                .collect();
            if let Some(pos) = self
                .selected
                .and_then(|s| order.iter().position(|&i| i == s))
            {
                let next = if up {
                    pos.checked_sub(1)
                } else {
                    Some(pos + 1)
                };
                if let Some(&i) = next.and_then(|p| order.get(p)) {
                    self.selected = Some(i);
                    self.scroll_to_selected = true;
                }
            } else {
                self.selected = order.first().copied();
            }
        }
        if enter {
            return self.selected.map(Action::Play);
        }
        None
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let mut keep = true;
        match dialog {
            Dialog::FirstRun => {
                let lib = default_library();
                let mut choice = None;
                egui::Modal::new("first-run".into()).show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.heading("Create games folder");
                    ui.label(format!(
                        "Omnivores Rust looks for your games in:\n\n{}\n\n\
                         That folder does not exist yet. Create it now?\n\n\
                         Choose No to pick a different folder instead.",
                        lib.display()
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            choice = Some(0);
                        }
                        if ui.button("No").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(2);
                        }
                    });
                });
                match choice {
                    Some(0) => {
                        keep = false;
                        match std::fs::create_dir_all(&lib) {
                            Ok(()) => {
                                self.config.library_paths =
                                    vec![lib.to_string_lossy().into_owned()];
                                self.save();
                                self.reload();
                            }
                            Err(e) => {
                                self.dialog = Some(Dialog::Message {
                                    title: "Could not create folder".into(),
                                    text: e.to_string(),
                                });
                                return;
                            }
                        }
                    }
                    Some(1) => {
                        keep = false;
                        if let Some(d) = pick_folder("Choose your games folder", &home()) {
                            self.config.library_paths = vec![d.to_string_lossy().into_owned()];
                            self.save();
                            self.reload();
                        }
                    }
                    Some(_) => keep = false,
                    None => {}
                }
                if keep {
                    self.dialog = Some(Dialog::FirstRun);
                }
            }
            Dialog::Message { title, text } => {
                egui::Modal::new("message".into()).show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.heading(&title);
                    ui.label(&text);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked()
                        || ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape))
                    {
                        keep = false;
                    }
                });
                if keep {
                    self.dialog = Some(Dialog::Message { title, text });
                }
            }
            Dialog::Rename {
                game,
                mut text,
                mut focused,
            } => {
                let mut done = None;
                egui::Modal::new("rename".into()).show(ctx, |ui| {
                    ui.set_min_width(340.0);
                    ui.heading("Rename");
                    ui.label("Show this game as:");
                    let r =
                        ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
                    if !focused {
                        r.request_focus();
                        focused = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                            done = Some(true);
                        }
                        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape))
                        {
                            done = Some(false);
                        }
                    });
                });
                match done {
                    Some(true) => {
                        if let Some(g) = self.games.get(game).cloned() {
                            let name = text.trim().to_string();
                            self.config
                                .set_override(&g, |o| o.name = (!name.is_empty()).then_some(name));
                            self.save();
                            self.reload();
                        }
                    }
                    Some(false) => {}
                    None => {
                        self.dialog = Some(Dialog::Rename {
                            game,
                            text,
                            focused,
                        })
                    }
                }
            }
            Dialog::Add(mut a) => {
                if self.adding(ctx, &mut a) && self.dialog.is_none() {
                    self.dialog = Some(Dialog::Add(a));
                }
            }
            Dialog::Settings(mut s) => {
                let mut done = None;
                let mut act = None;
                egui::Modal::new("settings".into()).show(ctx, |ui| {
                    ui.set_width(600.0);
                    ui.heading("Omnivores Rust Settings");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut s.tab, Tab::Library, "Library");
                        ui.selectable_value(&mut s.tab, Tab::Engines, "Engines");
                        ui.selectable_value(&mut s.tab, Tab::Customised, "Customised");
                        ui.selectable_value(&mut s.tab, Tab::Appearance, "Appearance");
                        ui.selectable_value(&mut s.tab, Tab::Install, "Install");
                    });
                    ui.separator();
                    ui.allocate_ui(egui::vec2(600.0, 300.0), |ui| {
                        ui.set_min_height(300.0);
                        act = settings_tab(ui, &mut s);
                    });
                    ui.separator();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Cancel").clicked() {
                            done = Some(false);
                        }
                        if ui.button("OK").clicked() {
                            done = Some(true);
                        }
                    });
                });
                if let Some(act) = act {
                    self.install_act(ctx, &mut s, act);
                }
                if ui_escape(ctx) && !s.confirm_clear && !s.install.confirm_uninstall {
                    done = Some(false);
                }
                match done {
                    Some(true) => {
                        self.config.library_paths = s.paths.clone();
                        for (e, p) in &s.engines {
                            self.config
                                .engines
                                .insert(e.id().into(), p.trim().to_string());
                        }
                        self.config.overrides = s.overrides.clone();
                        self.config.icon_size = s.icon_size;
                        self.save();
                        if self.dialog.is_none() {
                            self.reload();
                        }
                    }
                    Some(false) => {}
                    None => self.dialog = Some(Dialog::Settings(s)),
                }
            }
        }
    }
}

// ------------------------------------------------------------------------
// Installing the launcher, and adding games and mods

impl Launcher {
    /// The Install tab's buttons.
    fn install_act(&mut self, ctx: &egui::Context, s: &mut Settings, act: InstallAct) {
        let t = &mut s.install;
        t.confirm_uninstall = false;
        match act {
            InstallAct::Install(opts) => match install::install(&mut self.config, opts) {
                Ok(lines) => {
                    t.report = lines;
                    self.offer = false;
                    self.config.offer_install = false;
                    self.save();
                }
                Err(e) => t.report = vec![e],
            },
            InstallAct::Uninstall => match install::uninstall(&mut self.config) {
                Ok(r) => {
                    t.report = r.lines;
                    t.report.push(format!(
                        "Your games and settings were kept, in {} and {}.",
                        self.library_roots()
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        config::config_dir().display()
                    ));
                    self.save();
                    if r.must_exit {
                        // This copy is the one being deleted, which Windows
                        // does only once it has ended.
                        install::tell("Omnivores Rust", &t.report.join("\n"));
                        install::remove_after_exit();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Err(e) => t.report = vec![e],
            },
        }
        // The settings' working copy follows what the install changed.
        s.engines = ENGINES
            .into_iter()
            .map(|e| (e, self.config.engine_path(e)))
            .collect();
        let report = std::mem::take(&mut s.install.report);
        s.install = InstallTab::new();
        s.install.report = report;
    }

    /// Opens the Add window, or brings more to it: folders and archives to
    /// look through, or a search of the whole computer.
    fn add(&mut self, ctx: &egui::Context, sources: Vec<PathBuf>, search: bool) {
        let mut a = match self.dialog.take() {
            Some(Dialog::Add(a)) => a,
            None => Box::default(),
            // Busy with something else: leave it be.
            other => {
                self.dialog = other;
                return;
            }
        };
        if !sources.is_empty() || search {
            a.notes.clear();
        }
        a.queued.extend(sources);
        if search && a.work.is_none() {
            let library = self.library_roots();
            let ctl = new_ctl(ctx);
            a.work = Some(Work::Search(importer::Job::start(ctl, move |c| {
                importer::search(&library, c)
            })));
        }
        self.next_look(ctx, &mut a);
        self.dialog = Some(Dialog::Add(a));
    }

    /// Starts looking at whatever is queued, unless busy.
    fn next_look(&mut self, ctx: &egui::Context, a: &mut Adding) {
        if a.work.is_some() || a.queued.is_empty() {
            return;
        }
        let sources = std::mem::take(&mut a.queued);
        let library = self.library_roots();
        // Archives are unpacked beside the library, so that what is in them
        // can be moved into it rather than copied.
        let work = library[a.target.min(library.len() - 1)].clone();
        if let Err(e) = std::fs::create_dir_all(&work) {
            a.notes
                .push(format!("Could not create {}: {e}", work.display()));
            return;
        }
        let ctl = new_ctl(ctx);
        a.work = Some(Work::Look(importer::Job::start(ctl, move |c| {
            importer::analyse(&sources, &work, &library, c)
        })));
    }

    /// Takes in what was found, each set up the way it will most likely be
    /// wanted.
    fn take_found(&self, a: &mut Adding, found: Vec<importer::Found>) {
        for f in found {
            if a.picks.iter().any(|p| p.found.game.path == f.game.path) {
                continue;
            }
            let engine = f.game.engine;
            let mut p = Pick {
                include: !f.in_library,
                name: f.game.name.clone(),
                is_mod: f.game.is_mod,
                engine,
                base: None,
                found: f,
            };
            p.base = self.default_base(&p);
            a.picks.push(p);
        }
    }

    /// The library's games a pick for `engine` can be laid over.
    fn bases(&self, engine: Engine) -> Vec<&Game> {
        self.games
            .iter()
            .filter(|g| !g.is_mod && g.active_engine() == engine)
            .collect()
    }

    /// The game a part-mod goes over: the one named after its engine if the
    /// library has it, else the first.
    fn default_base(&self, p: &Pick) -> Option<String> {
        if p.found.complete(p.engine) {
            return None;
        }
        let bases = self.bases(p.engine);
        let named = bases.iter().find(|g| {
            let t = g.title().to_lowercase();
            t == p.engine.label().to_lowercase() || t == p.engine.folder().to_lowercase()
        });
        named.or(bases.first()).map(|g| g.key())
    }

    /// Copies the ticked picks into the library.
    fn start_copy(&mut self, ctx: &egui::Context, a: &mut Adding) {
        let roots = self.library_roots();
        let root = roots[a.target.min(roots.len() - 1)].clone();
        let mut taken = Vec::new();
        let mut plans = Vec::new();
        let mut meta = Vec::new();
        for (i, p) in a.picks.iter().enumerate().filter(|(_, p)| p.include) {
            let dest = importer::destination(&root, &p.name, p.is_mod, p.engine, &taken);
            taken.push(dest.clone());
            let base = p
                .base
                .as_ref()
                .filter(|_| !p.found.complete(p.engine))
                .and_then(|k| self.games.iter().find(|g| &g.key() == k))
                .map(|g| g.path.clone());
            plans.push(importer::Plan {
                source: p.found.game.path.clone(),
                bare: p.found.bare,
                base,
                dest: dest.clone(),
                movable: p.found.unpacked,
            });
            meta.push((i, dest, p.name.trim().to_string(), p.engine));
        }
        if plans.is_empty() {
            return;
        }
        // The first thing added makes the default games folder the library.
        if self.config.library_paths.is_empty() {
            self.config.library_paths = vec![root.to_string_lossy().into_owned()];
            self.save();
        }
        let ctl = new_ctl(ctx);
        a.notes.clear();
        a.work = Some(Work::Copy(
            importer::Job::start(ctl, move |c| importer::install(&plans, c)),
            meta,
        ));
    }

    /// The copying is over: the library takes in what was added, each
    /// named and running on what the player chose.
    fn copied(
        &mut self,
        a: &mut Adding,
        result: importer::Installed,
        meta: Vec<(usize, PathBuf, String, Engine)>,
    ) {
        self.reload();
        let mut added = Vec::new();
        let mut gone = BTreeSet::new();
        for (i, dest, name, engine) in meta {
            if !result.done.contains(&dest) {
                continue;
            }
            gone.insert(i);
            let key = dest.to_string_lossy().into_owned();
            if let Some(g) = self.games.iter().find(|g| g.key() == key).cloned() {
                let folder = g.name.clone();
                self.config.set_override(&g, |o| {
                    o.name = (!name.is_empty() && name != folder).then(|| name.clone());
                    o.engine = (g.engine != engine).then(|| engine.id().into());
                });
            }
            added.push(if name.is_empty() { key } else { name });
        }
        self.save();
        self.reload();
        let mut i = 0;
        a.picks.retain(|_| {
            i += 1;
            !gone.contains(&(i - 1))
        });
        if let Some(first) = result.done.first() {
            let key = first.to_string_lossy().into_owned();
            self.selected = self.games.iter().position(|g| g.key() == key);
            self.scroll_to_selected = true;
        }
        if !added.is_empty() {
            self.status = format!("Added {}", added.join(", "));
        }
        match result.error {
            Some(e) if e == importer::CANCELLED => a
                .notes
                .push("Stopped. Nothing was left half-copied.".into()),
            Some(e) => a.notes.push(format!("Could not add {e}")),
            None => {}
        }
        // Other picks' bases may have just arrived.
        for p in a.picks.iter_mut().filter(|p| p.base.is_none()) {
            p.base = self.default_base(p);
        }
    }

    /// The Add games and mods window; false once it is closed.
    fn adding(&mut self, ctx: &egui::Context, a: &mut Adding) -> bool {
        // What the work in the background has come to.
        match a.work.take() {
            Some(Work::Look(j)) => match j.poll() {
                None => a.work = Some(Work::Look(j)),
                Some(Ok(found)) => {
                    a.unpacked.extend(found.unpacked);
                    a.notes.extend(found.problems);
                    self.take_found(a, found.found);
                }
                Some(Err(e)) => a.notes.push(e),
            },
            Some(Work::Search(j)) => match j.poll() {
                None => a.work = Some(Work::Search(j)),
                Some(Ok(found)) => {
                    if found.is_empty() && !j.ctl.cancelled() {
                        a.notes.push(
                            "Nothing new turned up in the usual places. Choose the folder \
                             your games are in, or the archives they came in, instead."
                                .into(),
                        );
                    }
                    self.take_found(a, found);
                }
                Some(Err(e)) => a.notes.push(e),
            },
            Some(Work::Copy(j, meta)) => match j.poll() {
                None => a.work = Some(Work::Copy(j, meta)),
                Some(Ok(result)) => {
                    self.copied(a, result, meta);
                    if a.picks.is_empty() && a.notes.is_empty() {
                        return false;
                    }
                }
                Some(Err(e)) => {
                    a.notes.push(e);
                    self.reload();
                }
            },
            None => {}
        }
        self.next_look(ctx, a);

        let busy = a.work.is_some();
        let roots = self.library_roots();
        a.target = a.target.min(roots.len() - 1);
        // Where each ticked pick would go, none taking another's name.
        let mut taken = Vec::new();
        let dests: Vec<Option<PathBuf>> = a
            .picks
            .iter()
            .map(|p| {
                p.include.then(|| {
                    let d = importer::destination(
                        &roots[a.target],
                        &p.name,
                        p.is_mod,
                        p.engine,
                        &taken,
                    );
                    taken.push(d.clone());
                    d
                })
            })
            .collect();

        let (mut close, mut folder, mut archive, mut search, mut copy) =
            (false, false, false, false, false);
        egui::Modal::new("add".into()).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.heading("Add games and mods");
            ui.label(format!(
                "Copies a game or a mod into your games folder, {}, ready to play. \
                 Nothing is changed where it came from.",
                roots[a.target].display()
            ));
            ui.add_space(4.0);
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    folder = ui
                        .button("Folder…")
                        .on_hover_text("A game or mod folder: an old install, a CD, a Wine prefix…")
                        .clicked();
                    archive = ui
                        .button("Archive…")
                        .on_hover_text("A .zip, .7z or .rar, the way mods are shared")
                        .clicked();
                    search = ui
                        .button("Find on this computer")
                        .on_hover_text(
                            "Looks in the usual places: program folders, Wine prefixes, \
                             Steam, Downloads, CDs and USB sticks",
                        )
                        .clicked();
                });
            });
            ui.label(
                RichText::new("Folders and archives can be dropped on this window too.")
                    .weak()
                    .small(),
            );

            if !a.picks.is_empty() {
                ui.separator();
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for (i, p) in a.picks.iter_mut().enumerate() {
                                ui.push_id(i, |ui| self.pick_row(ui, p, dests[i].as_deref()));
                            }
                        });
                });
            }
            for n in &a.notes {
                ui.label(RichText::new(n).color(ui.visuals().warn_fg_color));
            }
            if let Some(w) = &a.work {
                let p = w.ctl().progress();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Stop").clicked() {
                        w.ctl().cancel();
                    }
                    if p.total > 0 {
                        ui.add(
                            egui::ProgressBar::new((p.done as f32 / p.total as f32).min(1.0))
                                .text(&p.text),
                        );
                    } else {
                        ui.spinner();
                        ui.label(&p.text);
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if roots.len() > 1 {
                    ui.label("Into:");
                    egui::ComboBox::from_id_salt("target")
                        .selected_text(roots[a.target].display().to_string())
                        .show_ui(ui, |ui| {
                            for (i, r) in roots.iter().enumerate() {
                                ui.selectable_value(&mut a.target, i, r.display().to_string());
                            }
                        });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close = ui.add_enabled(!busy, egui::Button::new("Close")).clicked();
                    let n = a.picks.iter().filter(|p| p.include).count();
                    let label = if n == 0 {
                        "Add".to_string()
                    } else {
                        format!("Add {n}")
                    };
                    copy = ui
                        .add_enabled(!busy && n > 0, egui::Button::new(label))
                        .clicked();
                });
            });
        });

        if !busy && ui_escape(ctx) {
            close = true;
        }
        if folder || archive || search {
            a.notes.clear();
        }
        if folder {
            if let Some(d) = pick_folder("Choose a game or mod folder", &home()) {
                a.queued.push(d);
            }
        }
        if archive {
            let downloads = home().join("Downloads");
            let start = if downloads.is_dir() {
                downloads
            } else {
                home()
            };
            if let Some(f) = pick_file("Choose an archive", &start, Files::Archives) {
                a.queued.push(f);
            }
        }
        if search {
            let library = self.library_roots();
            a.work = Some(Work::Search(importer::Job::start(new_ctl(ctx), move |c| {
                importer::search(&library, c)
            })));
        }
        if copy {
            self.start_copy(ctx, a);
        }
        self.next_look(ctx, a);
        if a.work.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        !close
    }

    /// One thing found, with what the player can change about it.
    fn pick_row(&mut self, ui: &mut egui::Ui, p: &mut Pick, dest: Option<&Path>) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.checkbox(&mut p.include, "");
            let side = 32.0;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
            if let Some((tex, dim)) = self.icon(&ctx, &p.found.game) {
                let k = (side / dim[0] as f32).min(side / dim[1] as f32);
                let fit = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(dim[0] as f32 * k, dim[1] as f32 * k),
                );
                egui::Image::new((tex.id(), fit.size())).paint_at(ui, fit);
            }
            ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(210.0))
                .on_hover_text("What the launcher calls it, and its folder's name");
            ui.selectable_value(&mut p.is_mod, false, "Game");
            ui.selectable_value(&mut p.is_mod, true, "Mod");
            let before = p.engine;
            egui::ComboBox::from_id_salt("engine")
                .selected_text(p.engine.label())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for e in ENGINES {
                        ui.selectable_value(&mut p.engine, e, e.label());
                    }
                });
            if p.engine != before {
                p.base = self.default_base(p);
            }
        });
        ui.indent("details", |ui| {
            let mut from = format!(
                "From {} · {}",
                p.found.origin.display(),
                size_text(p.found.bytes)
            );
            if p.found.game.engine_guessed && p.engine == p.found.game.engine {
                from += " · the game it is for is a best guess";
            }
            ui.label(RichText::new(from).weak().small());
            if p.found.in_library {
                ui.label(RichText::new("Already in your library.").weak().small());
            }
            if !p.found.complete(p.engine) {
                let bases: Vec<(String, String)> = self
                    .bases(p.engine)
                    .iter()
                    .map(|g| (g.key(), g.title().to_string()))
                    .collect();
                if bases.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "Only part of a game: it needs {} in your library to be laid \
                             over, and will not start without it.",
                            p.engine.label()
                        ))
                        .color(ui.visuals().warn_fg_color),
                    );
                } else {
                    ui.horizontal(|ui| {
                        let mut over = p.base.is_some();
                        if ui
                            .checkbox(&mut over, "Only part of a game: lay it over a copy of")
                            .on_hover_text(
                                "The game is copied, then the mod's files replace its own - \
                                 as the mod's readme would have you do by hand",
                            )
                            .changed()
                        {
                            p.base = over.then(|| bases[0].0.clone());
                        }
                        let current = bases
                            .iter()
                            .find(|(k, _)| Some(k) == p.base.as_ref())
                            .map(|(_, t)| t.clone())
                            .unwrap_or_else(|| bases[0].1.clone());
                        ui.add_enabled_ui(p.base.is_some(), |ui| {
                            egui::ComboBox::from_id_salt("base")
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    for (k, t) in &bases {
                                        ui.selectable_value(&mut p.base, Some(k.clone()), t);
                                    }
                                });
                        });
                    });
                }
            }
            if let Some(d) = dest {
                ui.label(
                    RichText::new(format!("Goes to {}", d.display()))
                        .weak()
                        .small(),
                );
            }
        });
        ui.separator();
    }
}

fn ui_escape(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(Key::Escape))
}

fn settings_tab(ui: &mut egui::Ui, s: &mut Settings) -> Option<InstallAct> {
    match s.tab {
        Tab::Library => {
            ui.label(
                "Folders searched for Carnivores games and mods. Any folder \
                 containing a HUNTDAT is treated as an install.",
            );
            egui::ScrollArea::vertical()
                .max_height(210.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (i, p) in s.paths.iter().enumerate() {
                        let on = s.path_sel.contains(&i);
                        if ui.selectable_label(on, p).clicked() {
                            if !ui.input(|i| i.modifiers.command) {
                                s.path_sel.clear();
                            }
                            if on {
                                s.path_sel.remove(&i);
                            } else {
                                s.path_sel.insert(i);
                            }
                        }
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Add…").clicked() {
                    let start = s.paths.first().map(PathBuf::from).unwrap_or_else(home);
                    if let Some(d) = pick_folder("Add library folder", &start) {
                        let d = d.to_string_lossy().into_owned();
                        if !s.paths.contains(&d) {
                            s.paths.push(d);
                        }
                    }
                }
                let one = s.path_sel.iter().next().copied();
                if ui
                    .add_enabled(one.is_some(), egui::Button::new("Change…"))
                    .clicked()
                {
                    if let Some(i) = one {
                        if let Some(d) =
                            pick_folder("Change library folder", Path::new(&s.paths[i]))
                        {
                            s.paths[i] = d.to_string_lossy().into_owned();
                        }
                    }
                }
                if ui
                    .add_enabled(!s.path_sel.is_empty(), egui::Button::new("Remove"))
                    .clicked()
                {
                    for &i in s.path_sel.iter().rev() {
                        s.paths.remove(i);
                    }
                    s.path_sel.clear();
                }
            });
        }
        Tab::Engines => {
            ui.label(
                "The Rust rewrites of the games. Each install is launched with \
                 the engine it was built for.",
            );
            ui.add_space(6.0);
            // Not a Grid: a grid cell only offers last frame's column width,
            // so a text field in one never grows past its first size.
            for e in ENGINES {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(140.0, ui.spacing().interact_size.y),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(140.0);
                            ui.label(format!("{}:", e.label()));
                        },
                    );
                    let path = s.engines.entry(e).or_default();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Browse…").clicked() {
                            let cur = PathBuf::from(path.as_str());
                            let start = cur
                                .parent()
                                .filter(|d| d.is_dir())
                                .map(Path::to_path_buf)
                                .unwrap_or_else(home);
                            if let Some(f) = pick_file(
                                &format!("Select the {} program", e.label()),
                                &start,
                                Files::Programs,
                            ) {
                                *path = f.to_string_lossy().into_owned();
                            }
                        }
                        ui.add(
                            egui::TextEdit::singleline(path)
                                .hint_text(format!("{} (not found)", e.binary()))
                                .desired_width(ui.available_width()),
                        )
                        .on_hover_text(path.as_str());
                    });
                });
                ui.add_space(4.0);
            }
        }
        Tab::Customised => {
            ui.label(
                "Entries you have renamed, re-pictured or reassigned. Removing one \
                 restores what the install says about itself; the install itself \
                 is never modified.",
            );
            egui::ScrollArea::vertical()
                .max_height(210.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (key, o) in &s.overrides {
                        let mut bits = Vec::new();
                        if let Some(n) = &o.name {
                            bits.push(format!("named \"{n}\""));
                        }
                        if o.icon.is_some() {
                            bits.push("custom icon".to_string());
                        }
                        if let Some(e) = &o.engine {
                            let label = Engine::from_id(e).map(Engine::label).unwrap_or(e.as_str());
                            bits.push(format!("engine {label}"));
                        }
                        let base = Path::new(key)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| key.clone());
                        let on = s.custom_sel.contains(key);
                        if ui
                            .selectable_label(on, format!("{base}  —  {}", bits.join(", ")))
                            .on_hover_text(key)
                            .clicked()
                        {
                            if !ui.input(|i| i.modifiers.command) {
                                s.custom_sel.clear();
                            }
                            if on {
                                s.custom_sel.remove(key);
                            } else {
                                s.custom_sel.insert(key.clone());
                            }
                        }
                    }
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !s.custom_sel.is_empty(),
                        egui::Button::new("Remove selected"),
                    )
                    .clicked()
                {
                    for k in std::mem::take(&mut s.custom_sel) {
                        s.overrides.remove(&k);
                    }
                }
                if ui
                    .add_enabled(!s.overrides.is_empty(), egui::Button::new("Remove all"))
                    .clicked()
                {
                    s.confirm_clear = true;
                }
            });
            if s.confirm_clear {
                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.label("Remove every rename, custom icon and engine override?");
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            s.overrides.clear();
                            s.custom_sel.clear();
                            s.confirm_clear = false;
                        }
                        if ui.button("No").clicked() {
                            s.confirm_clear = false;
                        }
                    });
                });
            }
        }
        Tab::Appearance => {
            ui.horizontal(|ui| {
                ui.label("Icon size:");
                ui.add(
                    egui::Slider::new(&mut s.icon_size, 32..=128)
                        .step_by(8.0)
                        .suffix(" px"),
                );
            });
        }
        Tab::Install => return install_tab(ui, &mut s.install),
    }
    None
}

fn install_tab(ui: &mut egui::Ui, t: &mut InstallTab) -> Option<InstallAct> {
    let mut act = None;
    let st = &t.status;
    let dir = st.dir.display().to_string();
    let here = install::this_program()
        .parent()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    ui.label(
        "Omnivores Rust can copy itself and the three games to a folder of their own \
         and add shortcuts, so that it starts like any other program and the \
         downloaded copy can go. It needs no administrator rights.",
    );
    ui.add_space(6.0);
    let state = if st.running_installed {
        format!("Installed in {dir}; this is the installed copy.")
    } else if st.installed {
        format!("Installed in {dir}. This copy runs from {here}.")
    } else {
        format!("Not installed. This copy runs from {here}.")
    };
    ui.label(RichText::new(state).strong());
    ui.add_space(6.0);
    if st.running_installed {
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut true, format!("The programs are in {dir}")),
        );
    } else {
        let label = if st.installed {
            format!("Copy this version's programs to {dir}, over the installed ones")
        } else {
            format!("Copy the programs to {dir}")
        };
        ui.checkbox(&mut t.copy, label)
            .on_hover_text("Otherwise the shortcuts start the programs where they are now");
    }
    ui.checkbox(
        &mut t.menu,
        format!("Add it to the {}", Place::Menu.label()),
    );
    ui.add_enabled_ui(st.desktop.is_some(), |ui| {
        ui.checkbox(&mut t.desktop, "Put a shortcut on the desktop")
            .on_disabled_hover_text("This system has no desktop folder");
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let install = if st.installed || st.menu || st.desktop == Some(true) {
            "Apply"
        } else {
            "Install"
        };
        if ui.button(install).clicked() {
            act = Some(InstallAct::Install(install::Options {
                copy: t.copy && !st.running_installed,
                menu: t.menu,
                desktop: t.desktop && st.desktop.is_some(),
            }));
        }
        let anything = st.installed || st.menu || st.desktop == Some(true);
        if ui
            .add_enabled(anything, egui::Button::new("Uninstall…"))
            .clicked()
        {
            t.confirm_uninstall = true;
        }
    });
    if t.confirm_uninstall {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(
                "Remove the installed programs and every shortcut Omnivores Rust made? \
                 Your games folder and settings stay.",
            );
            ui.horizontal(|ui| {
                if ui.button("Uninstall").clicked() {
                    act = Some(InstallAct::Uninstall);
                }
                if ui.button("Keep it").clicked() {
                    t.confirm_uninstall = false;
                }
            });
        });
    }
    if !t.report.is_empty() {
        ui.add_space(6.0);
        for line in &t.report {
            ui.label(line);
        }
    }
    act
}

impl eframe::App for Launcher {
    /// Whatever no panel covers takes the panels' own colour, not eframe's
    /// near-black default.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut action = self.keys(ctx);
        let (mut add, mut find) = (false, false);

        // Folders and archives dropped on the window are added.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add(ctx, dropped, false);
        }
        if !self.startup.is_empty() && self.dialog.is_none() {
            let sources = std::mem::take(&mut self.startup);
            self.add(ctx, sources, false);
        }

        // Each bar is one row: a right-to-left layout straight in a panel
        // would take all the height it is offered, and the panel keep it.
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Settings…").clicked() {
                        self.open_settings();
                    }
                    if ui.button("Refresh").clicked() {
                        self.reload();
                    }
                    add = ui
                        .button("Add games…")
                        .on_hover_text("Add games and mods from folders, archives or this computer")
                        .clicked();
                    if !self.search.is_empty()
                        && ui.small_button("✕").on_hover_text("Clear").clicked()
                    {
                        self.search.clear();
                        self.fix_selection();
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search games and mods…")
                            .desired_width(f32::INFINITY),
                    );
                    if self.focus_search {
                        r.request_focus();
                        self.focus_search = false;
                    }
                    if r.changed() {
                        self.fix_selection();
                    }
                });
            });
            ui.add_space(6.0);
        });

        // Until it has a menu entry, the launcher offers to install itself.
        if self.offer {
            egui::TopBottomPanel::top("offer").show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Add Omnivores Rust to your {} and desktop?",
                        Place::Menu.label()
                    ));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button("No thanks")
                            .on_hover_text("Settings → Install does it any time")
                            .clicked()
                        {
                            self.offer = false;
                            self.config.offer_install = false;
                            self.save();
                        }
                        if ui.button("Install…").clicked() {
                            self.open_settings_at(Tab::Install);
                        }
                    });
                });
                ui.add_space(4.0);
            });
        }

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let play = ui.add_enabled(
                        self.selected.is_some(),
                        egui::Button::new(RichText::new("Play").size(15.0))
                            .min_size(egui::vec2(120.0, 28.0)),
                    );
                    if play.clicked() {
                        action = self.selected.map(Action::Play);
                    }
                    // The status has what room the button leaves, cut short
                    // if it needs more.
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(&self.status).truncate());
                    });
                });
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.games.is_empty() {
                // Nothing yet: how to get something.
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() / 2.0 - 110.0).max(12.0));
                    ui.heading("No games yet");
                    ui.add_space(6.0);
                    ui.label(
                        "Bring in your Carnivores games and mods: Omnivores Rust copies \
                         them into your games folder.",
                    );
                    ui.add_space(12.0);
                    find = ui
                        .add(
                            egui::Button::new("Find games on this computer")
                                .min_size(egui::vec2(260.0, 28.0)),
                        )
                        .clicked();
                    add |= ui
                        .add(
                            egui::Button::new("Add a folder or an archive…")
                                .min_size(egui::vec2(260.0, 28.0)),
                        )
                        .clicked();
                    ui.add_space(12.0);
                    let lib = self
                        .library_roots()
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.label(
                        RichText::new(format!(
                            "Folders and archives can be dropped here too, or copied into \
                             {lib} by hand (then press F5)."
                        ))
                        .weak(),
                    );
                });
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if let Some(a) = self.rows(ui) {
                        action = Some(a);
                    }
                });
        });

        // Something is being dragged over the window: say what dropping it
        // will do.
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.screen_rect();
            let painter =
                ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, "drop".into()));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                "Drop to add to your games",
                FontId::proportional(22.0),
                Color32::WHITE,
            );
        }

        if let Some(a) = action {
            self.apply(a);
        }
        if add || find {
            self.add(ctx, Vec::new(), find);
        }
        self.dialogs(ctx);
    }
}

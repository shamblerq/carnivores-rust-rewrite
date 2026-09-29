//! Putting Omnivores Rust where the desktop finds it, and taking it away
//! again: the four programs copied to a folder of their own, an entry in the
//! applications menu (the Start menu on Windows) and a shortcut on the
//! desktop - for the launcher, or for any one game in the library.
//!
//! On Linux the programs go to `~/.local/bin`, a menu entry is a .desktop
//! file in `~/.local/share/applications` and a desktop shortcut the same
//! file on the desktop. On Windows the programs go to
//! `%LOCALAPPDATA%\Programs\Omnivores Rust`, the shortcuts are .lnk files
//! the shell writes itself, and the install is listed under the system's
//! installed apps, so it can be removed from there too. Nothing needs
//! administrator rights, and nothing here touches the games folder or the
//! settings.

use std::io;
use std::path::{Path, PathBuf};

use crate::config::{config_dir, detect_engine, home, Config, Shortcuts};
use crate::library::{Engine, Game, ENGINES};
use crate::winicon::{self, IconImage};

pub const LAUNCHER: &str = if cfg!(windows) {
    "omnivores-rust.exe"
} else {
    "omnivores-rust"
};

const NAME: &str = "Omnivores Rust";
const COMMENT: &str = "Play Carnivores, Carnivores 2 and Carnivores: Ice Age, and their mods";

// ------------------------------------------------------------------------
// Where things go

/// The folder the programs are installed to.
pub fn programs_dir() -> PathBuf {
    if cfg!(windows) {
        env_dir("LOCALAPPDATA")
            .unwrap_or_else(|| home().join("AppData").join("Local"))
            .join("Programs")
            .join(NAME)
    } else {
        home().join(".local").join("bin")
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// `$XDG_DATA_HOME`, `~/.local/share` when unset.
#[cfg(not(windows))]
fn data_home() -> PathBuf {
    env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local").join("share"))
}

/// Where menu entries go.
pub fn menu_dir() -> PathBuf {
    #[cfg(windows)]
    {
        env_dir("APPDATA")
            .unwrap_or_else(|| home().join("AppData").join("Roaming"))
            .join(r"Microsoft\Windows\Start Menu\Programs")
    }
    #[cfg(not(windows))]
    {
        data_home().join("applications")
    }
}

/// The desktop folder, if the system has one.
pub fn desktop_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win::desktop().or_else(|| Some(home().join("Desktop")))
    }
    #[cfg(not(windows))]
    {
        let dir = env_dir("XDG_DESKTOP_DIR")
            .or_else(|| {
                let dirs = config_home().join("user-dirs.dirs");
                user_dir(
                    &std::fs::read_to_string(dirs).ok()?,
                    "XDG_DESKTOP_DIR",
                    &home(),
                )
            })
            .unwrap_or_else(|| home().join("Desktop"));
        // A desktop set to the home folder is the user-dirs way of saying
        // there is none.
        (dir.is_dir() && dir != home()).then_some(dir)
    }
}

#[cfg(not(windows))]
fn config_home() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

/// One folder out of `user-dirs.dirs`: `XDG_DESKTOP_DIR="$HOME/Desktop"`.
#[cfg_attr(windows, allow(dead_code))]
fn user_dir(file: &str, key: &str, home: &Path) -> Option<PathBuf> {
    file.lines().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim().trim_matches('"');
        match v.strip_prefix("$HOME") {
            Some(rest) => Some(home.join(rest.trim_start_matches('/'))),
            None => Some(PathBuf::from(v)).filter(|p| p.is_absolute()),
        }
    })
}

/// Pictures made for shortcuts: the formats the desktop reads, from the
/// artwork the games and mods carry.
fn icon_store() -> PathBuf {
    config_dir().join("shortcut-icons")
}

/// The launcher's own icon for its menu entry.
#[cfg(not(windows))]
fn launcher_icon_path() -> PathBuf {
    data_home().join("icons/hicolor/256x256/apps/omnivores-rust.png")
}

// ------------------------------------------------------------------------
// Shortcuts

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Place {
    Menu,
    Desktop,
}

impl Place {
    pub fn dir(self) -> Option<PathBuf> {
        match self {
            Place::Menu => Some(menu_dir()),
            Place::Desktop => desktop_dir(),
        }
    }

    /// What the player calls it on this system.
    pub fn label(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Place::Menu, true) => "Start menu",
            (Place::Menu, false) => "applications menu",
            (Place::Desktop, _) => "desktop",
        }
    }
}

/// A menu entry or desktop shortcut: what it is called and what it starts.
pub struct Shortcut {
    /// The file's name without its extension. On Linux this is also the
    /// desktop's id for the program.
    pub file: String,
    pub name: String,
    pub comment: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub workdir: Option<PathBuf>,
    /// A PNG on Linux; an .ico or a program carrying an icon on Windows.
    pub icon: Option<PathBuf>,
    /// The window class of what it starts, so a dock can tell the running
    /// program belongs to the shortcut.
    pub wm_class: Option<String>,
}

impl Shortcut {
    pub fn path(&self, place: Place) -> Option<PathBuf> {
        let ext = if cfg!(windows) { "lnk" } else { "desktop" };
        Some(place.dir()?.join(format!("{}.{ext}", self.file)))
    }

    pub fn write(&self, place: Place) -> io::Result<PathBuf> {
        let path = self
            .path(place)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "there is no desktop folder"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        #[cfg(windows)]
        win::write_link(&path, self)?;
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&path, desktop_entry(self))?;
            // A launcher on the desktop has to be executable before the
            // desktop will start it, and GNOME's wants it marked trusted.
            if place == Place::Desktop {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
                let _ = std::process::Command::new("gio")
                    .args(["set", "-t", "string"])
                    .arg(&path)
                    .args(["metadata::trusted", "true"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
        Ok(path)
    }
}

/// A .desktop file's text.
#[cfg_attr(windows, allow(dead_code))]
fn desktop_entry(s: &Shortcut) -> String {
    let exec: Vec<String> = std::iter::once(s.program.to_string_lossy().into_owned())
        .chain(s.args.iter().cloned())
        .map(|a| exec_arg(&a))
        .collect();
    let mut out = format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}\n",
        entry_value(&s.name),
        entry_value(&s.comment),
        exec.join(" ")
    );
    if let Some(d) = &s.workdir {
        out += &format!("Path={}\n", entry_value(&d.to_string_lossy()));
    }
    if let Some(i) = &s.icon {
        out += &format!("Icon={}\n", entry_value(&i.to_string_lossy()));
    }
    out += "Terminal=false\nCategories=Game;\nKeywords=Carnivores;dinosaur;hunting;\n";
    if let Some(c) = &s.wm_class {
        out += &format!("StartupWMClass={}\n", entry_value(c));
    }
    out
}

/// A string value in a desktop entry: its own escapes for what would end
/// the line.
fn entry_value(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' => out += "\\r",
            '\t' => out += "\\t",
            _ => out.push(c),
        }
    }
    out
}

/// One argument of a desktop entry's Exec line. An argument with any of
/// the spec's reserved characters is quoted, with ", `, $ and \ escaped
/// inside the quotes; the value's own escaping then doubles every
/// backslash, so a literal backslash takes four.
fn exec_arg(a: &str) -> String {
    let a = a.replace('%', "%%");
    let reserved = |c: char| " \t\n\"'\\><~|&;$*?#()`".contains(c);
    if !a.is_empty() && !a.chars().any(reserved) {
        return a;
    }
    let mut q = String::from("\"");
    for c in a.chars() {
        match c {
            '"' | '`' | '$' => {
                q += "\\\\";
                q.push(c);
            }
            '\\' => q += "\\\\\\\\",
            '\n' => q += "\\n",
            '\t' => q += "\\t",
            _ => q.push(c),
        }
    }
    q.push('"');
    q
}

/// One argument of a Windows command line, quoted the way programs split
/// them again: backslashes are only special before a quote.
#[cfg_attr(not(windows), allow(dead_code))]
fn win_arg(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"']) {
        return a.to_string();
    }
    let mut q = String::from("\"");
    let mut slashes = 0;
    for c in a.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                q += &"\\".repeat(slashes * 2 + 1);
                slashes = 0;
            }
            _ => {
                q += &"\\".repeat(slashes);
                slashes = 0;
            }
        }
        if c != '\\' {
            q.push(c);
        }
    }
    q += &"\\".repeat(slashes * 2);
    q.push('"');
    q
}

/// The launcher's own shortcut, starting `program`.
fn launcher_shortcut(program: &Path) -> Shortcut {
    Shortcut {
        file: if cfg!(windows) {
            NAME
        } else {
            "omnivores-rust"
        }
        .into(),
        name: NAME.into(),
        comment: COMMENT.into(),
        program: program.to_path_buf(),
        args: Vec::new(),
        workdir: program.parent().map(Path::to_path_buf),
        #[cfg(windows)]
        icon: Some(program.to_path_buf()),
        #[cfg(not(windows))]
        icon: Some(launcher_icon_path()),
        wm_class: Some("omnivores-rust".into()),
    }
}

/// Whether the launcher has a shortcut there: its own name, whichever copy
/// of the launcher made it, or one written by hand.
pub fn launcher_has(place: Place) -> bool {
    launcher_shortcut(Path::new(LAUNCHER))
        .path(place)
        .map(|p| p.is_file())
        .unwrap_or(false)
}

// ------------------------------------------------------------------------
// Pictures

/// An icon as a 256-pixel square: pixel art scaled by a whole number stays
/// crisp, anything else is smoothed, and a picture that is not square is
/// centred.
fn square(img: &IconImage) -> Option<image::RgbaImage> {
    use image::imageops::{self, FilterType};
    let src = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.rgba.clone())?;
    const SIDE: u32 = 256;
    let (w, h) = src.dimensions();
    let big = w.max(h);
    if big == 0 {
        return None;
    }
    let filter = if big <= SIDE && SIDE.is_multiple_of(big) {
        FilterType::Nearest
    } else {
        FilterType::Lanczos3
    };
    let (nw, nh) = ((w * SIDE / big).max(1), (h * SIDE / big).max(1));
    let scaled = imageops::resize(&src, nw, nh, filter);
    let mut out = image::RgbaImage::new(SIDE, SIDE);
    imageops::overlay(
        &mut out,
        &scaled,
        ((SIDE - nw) / 2).into(),
        ((SIDE - nh) / 2).into(),
    );
    Some(out)
}

fn png_bytes(img: &image::RgbaImage) -> io::Result<Vec<u8>> {
    let mut out = io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    Ok(out.into_inner())
}

/// An .ico holding one PNG, which every Windows since Vista reads.
#[cfg_attr(not(windows), allow(dead_code))]
fn ico_bytes(png: &[u8], side: u32) -> Vec<u8> {
    let dim = if side >= 256 { 0 } else { side as u8 };
    let mut v = vec![0, 0, 1, 0, 1, 0, dim, dim, 0, 0];
    v.extend(1u16.to_le_bytes());
    v.extend(32u16.to_le_bytes());
    v.extend((png.len() as u32).to_le_bytes());
    v.extend(22u32.to_le_bytes());
    v.extend(png);
    v
}

/// Writes an icon where a shortcut can use it: a PNG on Linux, an .ico on
/// Windows.
fn store_icon(img: &IconImage, path: &Path) -> io::Result<()> {
    let sq = square(img).ok_or_else(|| io::Error::other("the picture is empty"))?;
    let png = png_bytes(&sq)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if cfg!(windows) {
        std::fs::write(path, ico_bytes(&png, 256))
    } else {
        std::fs::write(path, png)
    }
}

/// The icon each game's program carries, for an entry with no artwork of
/// its own.
fn engine_icon(e: Engine) -> Option<IconImage> {
    winicon::read_ico(match e {
        Engine::C1 => include_bytes!("../../games/carnivores1/icon.ico"),
        Engine::C2 => include_bytes!("../../games/carnivores2/icon.ico"),
        Engine::Ice => include_bytes!("../../games/carnivores-iceage/icon.ico"),
    })
}

// ------------------------------------------------------------------------
// A game's own shortcut

/// A short fixed name for a game's files, from where it lives: FNV-1a.
fn slug(key: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in key.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

/// A name any file system takes: what Windows forbids is dropped, and so is
/// what it will not end a name with.
pub fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| !c.is_control() && !r#"<>:"/\|?*"#.contains(*c))
        .collect();
    s.trim().trim_end_matches(['.', ' ']).to_string()
}

/// The shortcut for one game: its engine started in its folder.
fn game_shortcut(g: &Game, program: &Path, icon: Option<PathBuf>) -> Shortcut {
    let name = format!("{} (Rust)", g.title());
    let engine = g.active_engine();
    let file = if cfg!(windows) {
        let f = file_safe(&name);
        if f.is_empty() {
            format!("Omnivores {}", slug(&g.key()))
        } else {
            f
        }
    } else {
        format!("omnivores-rust-{}", slug(&g.key()))
    };
    let comment = if g.is_mod {
        format!("A {} mod, played with Omnivores Rust", engine.label())
    } else {
        format!("{}, played with Omnivores Rust", engine.label())
    };
    Shortcut {
        file,
        name,
        comment,
        program: program.to_path_buf(),
        args: vec![format!("data={}", g.path.display())],
        workdir: Some(g.path.clone()),
        icon,
        wm_class: Some(
            match engine {
                Engine::C1 => "carnivores1-rs",
                Engine::C2 => "carnivores2-rs",
                Engine::Ice => "carnivores-iceage-rs",
            }
            .into(),
        ),
    }
}

/// The icon file for a game's shortcut. Windows takes an .ico or a program
/// as it is; anything else, and everything on Linux, is converted.
fn game_icon(g: &Game, program: &Path) -> Option<PathBuf> {
    let source = g.custom_icon.clone().or(g.icon_source.clone());
    if cfg!(windows) {
        let direct = source.as_ref().filter(|s| {
            s.extension()
                .map(|e| e.eq_ignore_ascii_case("ico") || e.eq_ignore_ascii_case("exe"))
                .unwrap_or(false)
        });
        if let Some(s) = direct {
            return Some(s.clone());
        }
    }
    let img = source
        .as_deref()
        .and_then(winicon::picture)
        .or_else(|| engine_icon(g.active_engine()));
    let Some(img) = img else {
        // Windows can always show the engine's own.
        return cfg!(windows).then(|| program.to_path_buf());
    };
    let ext = if cfg!(windows) { "ico" } else { "png" };
    let path = icon_store().join(format!("{}.{ext}", slug(&g.key())));
    store_icon(&img, &path).ok().map(|()| path)
}

/// Whether the launcher has made a shortcut for this game there.
pub fn game_has(config: &Config, g: &Game, place: Place) -> bool {
    config
        .shortcuts
        .get(&g.key())
        .and_then(|s| match place {
            Place::Menu => s.menu.as_ref(),
            Place::Desktop => s.desktop.as_ref(),
        })
        .map(|p| Path::new(p).is_file())
        .unwrap_or(false)
}

/// Makes or takes away a game's shortcut in one place.
pub fn set_game_shortcut(
    config: &mut Config,
    g: &Game,
    place: Place,
    on: bool,
) -> Result<(), String> {
    let key = g.key();
    let program = PathBuf::from(config.engine_path(g.active_engine()).trim());
    if on && !program.is_file() {
        return Err(format!(
            "The {} program ({}) was not found. Set where it is under Settings → Engines.",
            g.active_engine().label(),
            g.active_engine().binary()
        ));
    }
    let made = config.shortcuts.entry(key.clone()).or_default();
    let slot = match place {
        Place::Menu => &mut made.menu,
        Place::Desktop => &mut made.desktop,
    };
    if let Some(old) = slot.take() {
        remove_file(Path::new(&old)).map_err(|e| e.to_string())?;
    }
    if on {
        let icon = game_icon(g, &program);
        let path = game_shortcut(g, &program, icon)
            .write(place)
            .map_err(|e| e.to_string())?;
        *slot = Some(path.to_string_lossy().into_owned());
    }
    if made == &Shortcuts::default() {
        config.shortcuts.remove(&key);
        let _ = remove_file(&icon_store().join(format!("{}.png", slug(&key))));
        let _ = remove_file(&icon_store().join(format!("{}.ico", slug(&key))));
    }
    Ok(())
}

// ------------------------------------------------------------------------
// Installing the launcher

/// How the launcher stands on this system.
pub struct Status {
    /// Where the programs go.
    pub dir: PathBuf,
    /// This copy is the installed one.
    pub running_installed: bool,
    /// A launcher is in the programs folder.
    pub installed: bool,
    pub menu: bool,
    /// None when there is no desktop folder.
    pub desktop: Option<bool>,
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// This program's file. Linux names one that has been replaced while it
/// runs - installed over - "<path> (deleted)"; the file at <path> is then
/// its successor, and stands for it.
pub fn this_program() -> PathBuf {
    let p = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(LAUNCHER));
    match p.to_str().and_then(|s| s.strip_suffix(" (deleted)")) {
        Some(s) => PathBuf::from(s),
        None => p,
    }
}

pub fn status() -> Status {
    let dir = programs_dir();
    let installed = dir.join(LAUNCHER);
    Status {
        running_installed: same_file(&this_program(), &installed),
        installed: installed.is_file(),
        menu: launcher_has(Place::Menu),
        desktop: desktop_dir().map(|_| launcher_has(Place::Desktop)),
        dir,
    }
}

#[derive(Clone, Copy)]
pub struct Options {
    /// Copy the programs to the programs folder.
    pub copy: bool,
    pub menu: bool,
    pub desktop: bool,
}

/// Puts `src` at `dst`, even over a program that is running: the new file
/// is written beside it and renamed over it, and on Windows, which will
/// not replace a running program, the old one is first moved aside.
fn replace_file(src: &Path, dst: &Path) -> io::Result<()> {
    let tmp = dst.with_extension("new");
    std::fs::copy(src, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(windows)]
    if dst.exists() && std::fs::remove_file(dst).is_err() {
        let old = dst.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(dst, &old)?;
    }
    std::fs::rename(&tmp, dst).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn remove_file(p: &Path) -> io::Result<()> {
    match std::fs::remove_file(p) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Where each game's program is now: what the settings say, else beside
/// this launcher.
fn engine_source(config: &Config, e: Engine) -> Option<PathBuf> {
    let set = PathBuf::from(config.engine_path(e).trim());
    if set.is_file() {
        return Some(set);
    }
    let beside = this_program().parent()?.join(e.binary());
    beside.is_file().then_some(beside)
}

/// Installs the launcher as `opts` asks, and says what it did. A shortcut
/// left unticked is taken away if it was there.
pub fn install(config: &mut Config, opts: Options) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    let st = status();
    let mut program = this_program();

    if opts.copy && !st.running_installed {
        let dir = &st.dir;
        let err =
            |what: &str, e: io::Error| format!("Could not copy {what} to {}: {e}", dir.display());
        std::fs::create_dir_all(dir).map_err(|e| err("the programs", e))?;
        let target = dir.join(LAUNCHER);
        replace_file(&program, &target).map_err(|e| err(LAUNCHER, e))?;
        let mut missing = Vec::new();
        for e in ENGINES {
            let Some(src) = engine_source(config, e) else {
                missing.push(e.binary());
                continue;
            };
            let dst = dir.join(e.binary());
            if !same_file(&src, &dst) {
                replace_file(&src, &dst).map_err(|x| err(e.binary(), x))?;
            }
            config
                .engines
                .insert(e.id().into(), dst.to_string_lossy().into_owned());
        }
        program = target;
        done.push(format!("Copied the programs to {}.", dir.display()));
        if !missing.is_empty() {
            done.push(format!(
                "{} could not be found, so {} not copied.",
                missing.join(", "),
                if missing.len() == 1 {
                    "it was"
                } else {
                    "they were"
                }
            ));
        }
        #[cfg(windows)]
        win::register(&program, dir)
            .map_err(|e| format!("Could not list the install under installed apps: {e}"))?;
    }
    // Otherwise the shortcuts start this copy, wherever it is.

    let shortcut = launcher_shortcut(&program);
    #[cfg(not(windows))]
    if opts.menu || opts.desktop {
        let img = winicon::read_ico(include_bytes!("../icon.ico"));
        if let Some(img) = img {
            store_icon(&img, &launcher_icon_path())
                .map_err(|e| format!("Could not save the icon: {e}"))?;
        }
    }
    for (place, on, was) in [
        (Place::Menu, opts.menu, st.menu),
        (Place::Desktop, opts.desktop, st.desktop.unwrap_or(false)),
    ] {
        if on {
            if place == Place::Desktop && st.desktop.is_none() {
                done.push("There is no desktop folder, so no desktop shortcut was made.".into());
                continue;
            }
            shortcut
                .write(place)
                .map_err(|e| format!("Could not add the {} shortcut: {e}", place.label()))?;
            done.push(match place {
                Place::Menu => format!("Added {NAME} to the {}.", place.label()),
                Place::Desktop => "Put a shortcut on the desktop.".into(),
            });
        } else if was {
            if let Some(p) = shortcut.path(place) {
                remove_file(&p).map_err(|e| e.to_string())?;
            }
            done.push(format!("Took {NAME} off the {}.", place.label()));
        }
    }
    if done.is_empty() {
        done.push("Nothing to do.".into());
    }
    Ok(done)
}

/// What uninstalling did, and whether this program has to end so that it
/// can be deleted: see [`remove_after_exit`].
pub struct Removed {
    pub lines: Vec<String>,
    pub must_exit: bool,
}

/// Takes away the installed programs and every shortcut the launcher made.
/// The games folder and the settings stay.
pub fn uninstall(config: &mut Config) -> Result<Removed, String> {
    let mut lines = Vec::new();
    let mut failed = Vec::new();
    let mut must_exit = false;

    let own = launcher_shortcut(Path::new(LAUNCHER));
    for place in [Place::Menu, Place::Desktop] {
        if let Some(p) = own.path(place).filter(|p| p.is_file()) {
            match remove_file(&p) {
                Ok(()) => lines.push(format!("Took {NAME} off the {}.", place.label())),
                Err(e) => failed.push(format!("{}: {e}", p.display())),
            }
        }
    }
    #[cfg(not(windows))]
    let _ = remove_file(&launcher_icon_path());

    let games = std::mem::take(&mut config.shortcuts);
    let mut n = 0;
    for p in games
        .values()
        .flat_map(|s| s.menu.iter().chain(s.desktop.iter()))
    {
        match remove_file(Path::new(p)) {
            Ok(()) => n += 1,
            Err(e) => failed.push(format!("{p}: {e}")),
        }
    }
    let _ = std::fs::remove_dir_all(icon_store());
    if n > 0 {
        lines.push(format!(
            "Removed {n} game shortcut{}.",
            if n == 1 { "" } else { "s" }
        ));
    }

    let dir = programs_dir();
    let me = this_program();
    let mut removed_any = false;
    for name in std::iter::once(LAUNCHER).chain(ENGINES.map(Engine::binary)) {
        let p = dir.join(name);
        if !p.is_file() {
            continue;
        }
        if cfg!(windows) && same_file(&p, &me) {
            // A running program cannot be deleted on Windows: it is
            // deleted once this one has ended.
            must_exit = true;
            removed_any = true;
            continue;
        }
        match remove_file(&p) {
            Ok(()) => removed_any = true,
            Err(e) => failed.push(format!("{}: {e}", p.display())),
        }
        let _ = remove_file(&p.with_extension("old"));
    }
    // Settings pointing at programs that are gone find them again.
    for e in ENGINES {
        let p = config.engine_path(e);
        if Path::new(&p).starts_with(&dir) && !Path::new(&p).is_file() {
            config.engines.insert(e.id().into(), detect_engine(e));
        }
    }
    #[cfg(windows)]
    {
        let _ = win::unregister();
        if !must_exit {
            let _ = std::fs::remove_dir(&dir);
        }
    }
    if removed_any {
        lines.push(format!("Removed the programs from {}.", dir.display()));
    }
    if !failed.is_empty() {
        return Err(format!(
            "Some files could not be removed:\n{}",
            failed.join("\n")
        ));
    }
    if lines.is_empty() {
        lines.push(format!("{NAME} was not installed."));
    }
    Ok(Removed { lines, must_exit })
}

/// For an uninstall that has to wait for this program to end, which is
/// how a program removes itself on Windows: the programs folder is deleted
/// as soon as it can be. To be called just before the program ends.
pub fn remove_after_exit() {
    #[cfg(windows)]
    win::delete_after_exit(&programs_dir());
}

// ------------------------------------------------------------------------
// Windows

#[cfg(windows)]
mod win {
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};

    use windows::core::{Interface, HSTRING};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        REG_DWORD, REG_SZ,
    };
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    use super::{win_arg, Shortcut, NAME};

    const UNINSTALL_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\omnivores-rust";

    fn err(e: windows::core::Error) -> io::Error {
        io::Error::other(e.message())
    }

    /// The shell writes the .lnk itself.
    pub fn write_link(path: &Path, s: &Shortcut) -> io::Result<()> {
        let args: Vec<String> = s.args.iter().map(|a| win_arg(a)).collect();
        // SAFETY: plain COM calls on interfaces the shell hands back; every
        // string is an HSTRING that outlives the call it is passed to.
        unsafe {
            // Already set up on the window's thread; either way it is now.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(err)?;
            link.SetPath(&HSTRING::from(s.program.as_path()))
                .map_err(err)?;
            link.SetArguments(&HSTRING::from(args.join(" ")))
                .map_err(err)?;
            if let Some(d) = &s.workdir {
                link.SetWorkingDirectory(&HSTRING::from(d.as_path()))
                    .map_err(err)?;
            }
            link.SetDescription(&HSTRING::from(s.comment.as_str()))
                .map_err(err)?;
            if let Some(i) = &s.icon {
                link.SetIconLocation(&HSTRING::from(i.as_path()), 0)
                    .map_err(err)?;
            }
            let file: IPersistFile = link.cast().map_err(err)?;
            file.Save(&HSTRING::from(path), true).map_err(err)?;
        }
        Ok(())
    }

    /// The desktop, wherever it has been moved to (OneDrive, say).
    pub fn desktop() -> Option<PathBuf> {
        // SAFETY: the returned string is the shell's, freed once copied.
        unsafe {
            let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
            let s = p.to_hstring().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s.map(|s| PathBuf::from(s.to_os_string()))
        }
    }

    fn set(key: HKEY, name: &str, value: &str) -> io::Result<()> {
        let data: Vec<u8> = value
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        // SAFETY: the key is open and the data is a NUL-terminated UTF-16
        // string, as REG_SZ asks.
        let r = unsafe { RegSetValueExW(key, &HSTRING::from(name), 0, REG_SZ, Some(&data)) };
        (r == ERROR_SUCCESS)
            .then_some(())
            .ok_or_else(|| io::Error::from_raw_os_error(r.0 as i32))
    }

    fn set_dword(key: HKEY, name: &str, value: u32) -> io::Result<()> {
        // SAFETY: the key is open and the data is four bytes, as REG_DWORD asks.
        let r = unsafe {
            RegSetValueExW(
                key,
                &HSTRING::from(name),
                0,
                REG_DWORD,
                Some(&value.to_le_bytes()),
            )
        };
        (r == ERROR_SUCCESS)
            .then_some(())
            .ok_or_else(|| io::Error::from_raw_os_error(r.0 as i32))
    }

    /// Lists the install under the system's installed apps, which start
    /// `omnivores-rust.exe --uninstall` to remove it.
    pub fn register(program: &Path, dir: &Path) -> io::Result<()> {
        let size: u64 = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0);
        let mut key = HKEY::default();
        // SAFETY: creates or opens a key under the user's own hive.
        let r =
            unsafe { RegCreateKeyW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY), &mut key) };
        if r != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(r.0 as i32));
        }
        let exe = program.display().to_string();
        let result = (|| {
            set(key, "DisplayName", NAME)?;
            set(key, "DisplayVersion", env!("CARGO_PKG_VERSION"))?;
            set(key, "DisplayIcon", &format!("{exe},0"))?;
            set(key, "InstallLocation", &dir.display().to_string())?;
            set(key, "UninstallString", &format!("\"{exe}\" --uninstall"))?;
            set(key, "URLInfoAbout", env!("CARGO_PKG_REPOSITORY"))?;
            set_dword(key, "EstimatedSize", (size / 1024) as u32)?;
            set_dword(key, "NoModify", 1)?;
            set_dword(key, "NoRepair", 1)
        })();
        // SAFETY: the key was opened above.
        unsafe {
            let _ = RegCloseKey(key);
        }
        result
    }

    pub fn unregister() -> io::Result<()> {
        // SAFETY: deletes the one key this program made.
        let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY)) };
        (r == ERROR_SUCCESS)
            .then_some(())
            .ok_or_else(|| io::Error::from_raw_os_error(r.0 as i32))
    }

    /// Deletes the programs folder once this program has ended: a hidden
    /// command prompt tries every second, for a minute, until it is gone.
    pub fn delete_after_exit(dir: &Path) {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let dir = dir.display();
        let _ = std::process::Command::new("cmd")
            .raw_arg(format!(
                "/C for /L %i in (1,1,60) do @(ping -n 2 127.0.0.1 >nul & \
                 rmdir /S /Q \"{dir}\" 2>nul & if not exist \"{dir}\" exit)"
            ))
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    /// A message for when there is no window to show it in.
    pub fn tell(title: &str, text: &str) {
        // SAFETY: both strings outlive the call.
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                &HSTRING::from(title),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}

/// Tells the player how `--install` or `--uninstall` went: on the terminal,
/// or in a message box on Windows, where the program has none.
pub fn tell(title: &str, text: &str) {
    println!("{text}");
    #[cfg(windows)]
    win::tell(title, text);
    #[cfg(not(windows))]
    let _ = title;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_quoting() {
        assert_eq!(
            exec_arg("/usr/bin/omnivores-rust"),
            "/usr/bin/omnivores-rust"
        );
        assert_eq!(
            exec_arg("data=/home/me/.omnivores/Carnivores 2"),
            "\"data=/home/me/.omnivores/Carnivores 2\""
        );
        assert_eq!(exec_arg("100%"), "100%%");
        assert_eq!(exec_arg("a$b"), "\"a\\\\$b\"");
        assert_eq!(exec_arg("a\\b"), "\"a\\\\\\\\b\"");
        assert_eq!(exec_arg(""), "\"\"");
    }

    #[test]
    fn windows_quoting() {
        assert_eq!(win_arg("data=C:\\Games\\C2"), "data=C:\\Games\\C2");
        assert_eq!(
            win_arg("data=C:\\Games\\Carnivores 2"),
            "\"data=C:\\Games\\Carnivores 2\""
        );
        assert_eq!(win_arg("a \\\"b"), "\"a \\\\\\\"b\"");
        assert_eq!(win_arg("dir with space\\"), "\"dir with space\\\\\"");
    }

    #[test]
    fn user_dirs() {
        let file = "# comment\nXDG_DOCUMENTS_DIR=\"$HOME/Documents\"\nXDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\n";
        let home = Path::new("/home/me");
        assert_eq!(
            user_dir(file, "XDG_DESKTOP_DIR", home),
            Some(PathBuf::from("/home/me/Schreibtisch"))
        );
        assert_eq!(
            user_dir("XDG_DESKTOP_DIR=\"/data/desk\"", "XDG_DESKTOP_DIR", home),
            Some(PathBuf::from("/data/desk"))
        );
        assert_eq!(user_dir(file, "XDG_MUSIC_DIR", home), None);
    }

    #[test]
    fn desktop_file() {
        let s = Shortcut {
            file: "omnivores-rust-1".into(),
            name: "Some Mod (Rust)".into(),
            comment: "A Carnivores 2 mod".into(),
            program: "/home/me/.local/bin/carnivores2-rs".into(),
            args: vec!["data=/home/me/.omnivores/Carnivores 2 mods/Some Mod".into()],
            workdir: Some("/home/me/.omnivores/Carnivores 2 mods/Some Mod".into()),
            icon: Some("/home/me/icon.png".into()),
            wm_class: Some("carnivores2-rs".into()),
        };
        let text = desktop_entry(&s);
        assert!(text.starts_with("[Desktop Entry]\nType=Application\nName=Some Mod (Rust)\n"));
        assert!(text.contains(
            "\nExec=/home/me/.local/bin/carnivores2-rs \"data=/home/me/.omnivores/Carnivores 2 mods/Some Mod\"\n"
        ));
        assert!(text.contains("\nPath=/home/me/.omnivores/Carnivores 2 mods/Some Mod\n"));
    }

    #[test]
    fn icons() {
        // 32x32 pixel art is scaled by eight, a whole number: every source
        // pixel becomes an 8x8 block.
        let mut rgba = vec![0u8; 32 * 32 * 4];
        rgba[..4].copy_from_slice(&[255, 0, 0, 255]);
        let img = IconImage {
            width: 32,
            height: 32,
            rgba,
        };
        let sq = square(&img).unwrap();
        assert_eq!(sq.dimensions(), (256, 256));
        assert_eq!(sq.get_pixel(7, 7).0, [255, 0, 0, 255]);
        assert_eq!(sq.get_pixel(8, 8).0, [0, 0, 0, 0]);
        let png = png_bytes(&sq).unwrap();
        let ico = ico_bytes(&png, 256);
        let back = winicon::read_ico(&ico).unwrap();
        assert_eq!((back.width, back.height), (256, 256));
    }

    #[test]
    fn names() {
        assert_eq!(file_safe("Carnivores: Ice Age"), "Carnivores Ice Age");
        assert_eq!(file_safe(" What? Mod... "), "What Mod");
        assert_eq!(slug("a"), slug("a"));
        assert_ne!(slug("a"), slug("b"));
    }
}

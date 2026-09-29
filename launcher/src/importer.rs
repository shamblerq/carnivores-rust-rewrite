//! Bringing games and mods into the library: from a folder (an old install,
//! a CD, a Wine prefix), from the archive a mod came in, or from wherever on
//! this computer they turn out to be.
//!
//! What is found is copied into the library the way the scan expects it: a
//! game in a folder of its own at the top, a mod in "<game> mods". A mod
//! that carries only the files it changes is laid over a copy of the game
//! it was made for, as its readme would have had the player do by hand,
//! with names matched regardless of case, the way the games look them up.
//! Nothing is ever changed where it was found.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::config::home;
use crate::install::file_safe;
use crate::library::{self, engine_from_name, Engine, Game};

pub const CANCELLED: &str = "Cancelled.";

// ------------------------------------------------------------------------
// Work in the background

/// What a job is doing, for the window to show.
#[derive(Clone, Default)]
pub struct Progress {
    pub text: String,
    pub done: u64,
    /// 0 while there is no telling how far there is to go.
    pub total: u64,
}

/// The line between a job and the window: progress one way, a request to
/// stop the other.
#[derive(Clone)]
pub struct Ctl {
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Ctl {
    /// `wake` is called when there is something new to show.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Ctl {
        Ctl {
            progress: Arc::default(),
            cancel: Arc::default(),
            wake: Arc::new(wake),
        }
    }

    pub fn step(&self, text: impl Into<String>, total: u64) {
        if let Ok(mut p) = self.progress.lock() {
            *p = Progress {
                text: text.into(),
                done: 0,
                total,
            };
        }
        (self.wake)();
    }

    /// New words for the same step.
    fn say(&self, text: impl Into<String>) {
        if let Ok(mut p) = self.progress.lock() {
            p.text = text.into();
        }
        (self.wake)();
    }

    fn advance(&self, n: u64) {
        if let Ok(mut p) = self.progress.lock() {
            p.done += n;
        }
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn check(&self) -> io::Result<()> {
        if self.cancelled() {
            Err(io::Error::new(io::ErrorKind::Interrupted, CANCELLED))
        } else {
            Ok(())
        }
    }
}

/// Work on a thread of its own, so the window stays responsive.
pub struct Job<T> {
    pub ctl: Ctl,
    rx: mpsc::Receiver<T>,
}

impl<T: Send + 'static> Job<T> {
    pub fn start(ctl: Ctl, work: impl FnOnce(&Ctl) -> T + Send + 'static) -> Job<T> {
        let (tx, rx) = mpsc::channel();
        let c = ctl.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work(&c));
            (c.wake)();
        });
        Job { ctl, rx }
    }

    /// The result, once there is one.
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(Ok(r)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("It stopped unexpectedly.".into())),
        }
    }
}

// ------------------------------------------------------------------------
// Files, matched the games' way

/// The entry in `dir` named `name` regardless of case, the exact spelling
/// first.
fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Symbolic links are followed, this many deep, so a link that leads back
/// up cannot go on for ever.
const MAX_DEPTH: usize = 32;

/// The bytes in every file under `dir`.
pub fn tree_size(dir: &Path) -> u64 {
    fn walk(dir: &Path, depth: usize) -> u64 {
        if depth > MAX_DEPTH {
            return 0;
        }
        let Ok(rd) = fs::read_dir(dir) else { return 0 };
        rd.flatten()
            .map(|e| {
                let p = e.path();
                match fs::metadata(&p) {
                    Ok(m) if m.is_dir() => walk(&p, depth + 1),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                }
            })
            .sum()
    }
    walk(dir, 0)
}

/// Files copied from a CD, or unpacked read-only, would stop the game
/// saving its hunters and the next mod replacing them.
fn make_writable(p: &Path) {
    let Ok(m) = fs::metadata(p) else { return };
    let mut perm = m.permissions();
    if !perm.readonly() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perm.set_mode(perm.mode() | 0o200);
    }
    #[cfg(not(unix))]
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    let _ = fs::set_permissions(p, perm);
}

/// Copies everything in `src` into `dst`, over what is there: a name that
/// matches one already there regardless of case replaces it under the
/// spelling it had, so the game never sees two of a file.
fn merge(src: &Path, dst: &Path, ctl: &Ctl) -> io::Result<()> {
    fn walk(src: &Path, dst: &Path, depth: usize, ctl: &Ctl) -> io::Result<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        fs::create_dir_all(dst)?;
        let existing: Vec<(String, PathBuf)> = fs::read_dir(dst)?
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();
        for entry in fs::read_dir(src)? {
            ctl.check()?;
            let entry = entry?;
            let name = entry.file_name();
            let lossy = name.to_string_lossy();
            let target = existing
                .iter()
                .find(|(n, _)| *n == lossy)
                .or_else(|| {
                    existing
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(&lossy))
                })
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| dst.join(&name));
            let path = entry.path();
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                // A link to nothing is passed by.
                Err(_) if entry.file_type().is_ok_and(|t| t.is_symlink()) => continue,
                Err(e) => return Err(e),
            };
            if meta.is_dir() {
                if target.is_file() {
                    fs::remove_file(&target)?;
                }
                walk(&path, &target, depth + 1, ctl)?;
            } else {
                if target.is_dir() {
                    fs::remove_dir_all(&target)?;
                } else if target.exists() {
                    make_writable(&target);
                }
                fs::copy(&path, &target)?;
                make_writable(&target);
                ctl.advance(meta.len());
            }
        }
        Ok(())
    }
    walk(src, dst, 0, ctl)
}

// ------------------------------------------------------------------------
// What a folder holds

/// What a HUNTDAT holds of what the games need to start.
#[derive(Clone, Copy, Default)]
pub struct Contents {
    /// Menus, sounds and a map: what every game needs.
    base: bool,
    /// The resource list Carnivores 2 and Ice Age read their creatures and
    /// weapons from; the first game kept them in its code.
    res: bool,
}

impl Contents {
    pub fn of(huntdat: &Path) -> Contents {
        let file = |n: &str| find_ci(huntdat, n).is_some_and(|p| p.is_file());
        let dir = |n: &str| find_ci(huntdat, n).filter(|p| p.is_dir());
        let map = dir("AREAS")
            .and_then(|a| fs::read_dir(a).ok())
            .map(|rd| {
                rd.flatten().any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".map")
                })
            })
            .unwrap_or(false);
        Contents {
            base: dir("MENU").is_some() && dir("SOUNDFX").is_some() && map,
            res: file("_RES.TXT"),
        }
    }

    /// Whether a game on `engine` can start from it by itself.
    pub fn complete(self, engine: Engine) -> bool {
        self.base && (self.res || engine == Engine::C1)
    }
}

/// Whether a folder looks like the inside of a HUNTDAT: what a mod made to
/// be "copied into HUNTDAT" ships as.
fn huntdat_like(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_ascii_lowercase();
        if e.path().is_dir() {
            matches!(
                n.as_str(),
                "areas" | "menu" | "soundfx" | "multiplr" | "weapons"
            )
        } else {
            n == "_res.txt" || n.ends_with(".car") || n.ends_with(".3df")
        }
    })
}

/// The shallowest folder under `root` that looks like the inside of a
/// HUNTDAT.
fn find_bare(root: &Path) -> Option<PathBuf> {
    let mut level = vec![root.to_path_buf()];
    for _ in 0..4 {
        if let Some(d) = level.iter().find(|d| huntdat_like(d)) {
            return Some(d.clone());
        }
        level = level.iter().flat_map(|d| subdirs(d)).collect();
    }
    None
}

/// Something found that can go into the library.
#[derive(Clone)]
pub struct Found {
    /// As the library will see it: name, engine, mod or not, icon.
    pub game: Game,
    /// `game.path` is the inside of a HUNTDAT rather than a folder holding
    /// one.
    pub bare: bool,
    /// What of a game its HUNTDAT holds.
    pub contents: Contents,
    pub bytes: u64,
    /// What the player chose that it was found in.
    pub origin: PathBuf,
    /// It is in the library already.
    pub in_library: bool,
    /// It lives in an unpacked archive, and may be moved rather than copied.
    pub unpacked: bool,
}

impl Found {
    /// It has all a game on `engine` needs to start by itself.
    pub fn complete(&self, engine: Engine) -> bool {
        self.contents.complete(engine)
    }
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// An archive's name without its extensions: "Some Mod v2" for
/// "Some Mod v2.tar.gz".
fn archive_stem(p: &Path) -> String {
    let name = name_of(p);
    let low = name.to_ascii_lowercase();
    for ext in [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst"] {
        if low.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(name)
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn in_library(p: &Path, library: &[PathBuf]) -> bool {
    let p = canonical(p);
    library.iter().any(|l| p.starts_with(canonical(l)))
}

/// Everything installable under `root`, which the player knows as `name`.
fn look(root: &Path, name: &str, origin: &Path, library: &[PathBuf], unpacked: bool) -> Vec<Found> {
    let mut found: Vec<(Game, bool)> = library::scan_path(root)
        .into_iter()
        .map(|g| (g, false))
        .collect();
    if found.is_empty() {
        if let Some(inside) = find_bare(root) {
            let engine = engine_from_name(name);
            found.push((
                Game {
                    path: inside,
                    name: name.to_string(),
                    engine: engine.unwrap_or(Engine::C2),
                    icon_source: None,
                    is_mod: true,
                    engine_guessed: engine.is_none(),
                    custom_name: None,
                    custom_icon: None,
                    custom_engine: None,
                },
                true,
            ));
        }
    }
    found
        .into_iter()
        .map(|(mut g, bare)| {
            // The folder itself goes by what the player chose: the archive,
            // not the folder it was unpacked into.
            if g.path == root {
                g.name = name.to_string();
            }
            let huntdat = if bare {
                Some(g.path.clone())
            } else {
                find_ci(&g.path, "HUNTDAT")
            };
            let contents = huntdat.as_deref().map(Contents::of).unwrap_or_default();
            if !contents.complete(g.engine) {
                // Only part of a game is a mod, and most mods are for
                // Carnivores 2: better than the scan's guess from the
                // missing renderers.
                g.is_mod = true;
                if g.engine_guessed {
                    g.engine = engine_from_name(&g.name)
                        .or_else(|| engine_from_name(name))
                        .unwrap_or(Engine::C2);
                }
            }
            Found {
                bytes: tree_size(&g.path),
                in_library: !unpacked && in_library(&g.path, library),
                origin: origin.to_path_buf(),
                game: g,
                bare,
                contents,
                unpacked,
            }
        })
        .collect()
}

/// What was made of what the player chose.
#[derive(Default)]
pub struct Analysis {
    pub found: Vec<Found>,
    /// Folders archives were unpacked into, to delete once done with.
    pub unpacked: Vec<PathBuf>,
    /// What could not be used, a line each.
    pub problems: Vec<String>,
}

static SERIAL: AtomicUsize = AtomicUsize::new(0);

/// A name for a working folder no other is using. It starts with a dot, so
/// the library's scan passes it by.
fn work_name(what: &str) -> String {
    format!(
        ".omnivores-{what}-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}

/// Looks through folders and archives for games and mods. Archives are
/// unpacked into `work`, which should be on the same drive as the library
/// so that they can be moved there rather than copied.
pub fn analyse(sources: &[PathBuf], work: &Path, library: &[PathBuf], ctl: &Ctl) -> Analysis {
    let mut a = Analysis::default();
    for src in sources {
        if ctl.cancelled() {
            break;
        }
        // A game's HUNTDAT chosen for the game itself.
        let src = match src.parent() {
            Some(game) if name_of(src).eq_ignore_ascii_case("huntdat") && src.is_dir() => game,
            _ => src.as_path(),
        };
        if src.is_dir() {
            ctl.step(format!("Looking in {}…", src.display()), 0);
            let found = look(src, &name_of(src), src, library, false);
            if found.is_empty() {
                a.problems.push(format!(
                    "No Carnivores game or mod was found in {}.",
                    src.display()
                ));
            }
            a.found.extend(found);
        } else if src.is_file() {
            let dest = work.join(work_name("unpack"));
            ctl.step(format!("Unpacking {}…", name_of(src)), 0);
            match unpack(src, &dest, ctl) {
                Ok(()) => {
                    let found = look(&dest, &archive_stem(src), src, library, true);
                    if found.is_empty() {
                        a.problems.push(format!(
                            "No Carnivores game or mod was found in {}.",
                            name_of(src)
                        ));
                    }
                    a.found.extend(found);
                    a.unpacked.push(dest);
                }
                Err(e) => {
                    let _ = fs::remove_dir_all(&dest);
                    if e != CANCELLED {
                        a.problems.push(format!("{}: {e}", name_of(src)));
                    }
                }
            }
        } else {
            a.problems
                .push(format!("{} could not be found.", src.display()));
        }
    }
    a
}

// ------------------------------------------------------------------------
// Archives

fn is_zip(p: &Path) -> bool {
    let mut magic = [0u8; 4];
    fs::File::open(p)
        .and_then(|mut f| io::Read::read_exact(&mut f, &mut magic))
        .map(|()| magic == *b"PK\x03\x04" || magic == *b"PK\x05\x06")
        .unwrap_or(false)
}

/// A name inside an archive as a path under the folder it is unpacked
/// into, or None for one that would land outside it.
fn safe_path(name: &str) -> Option<PathBuf> {
    let mut p = PathBuf::new();
    for c in name.split(['/', '\\']) {
        match c {
            "" | "." => {}
            ".." => return None,
            c if c.contains(':') => return None,
            c => p.push(c),
        }
    }
    (!p.as_os_str().is_empty()).then_some(p)
}

/// Unpacks an archive into `into`: a zip here, anything else with 7-Zip,
/// unrar or bsdtar if the system has one of them.
pub fn unpack(archive: &Path, into: &Path, ctl: &Ctl) -> Result<(), String> {
    fs::create_dir_all(into).map_err(|e| e.to_string())?;
    let ext = archive
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if is_zip(archive) {
        return unzip(archive, into, ctl);
    }
    if ext == "exe" {
        return Err(
            "This is a program, not an archive. If it installs a mod, run it \
                    (with Wine on Linux) and then add the folder it installed to."
                .into(),
        );
    }
    let known = [
        "7z", "rar", "tar", "gz", "tgz", "xz", "txz", "bz2", "tbz2", "zst", "cab", "lzh", "zip",
    ];
    if !known.contains(&ext.as_str()) {
        return Err("This is not an archive Omnivores Rust can open (.zip, .7z or .rar).".into());
    }
    unpack_with_tool(archive, into, &ext, ctl)
}

fn unzip(archive: &Path, into: &Path, ctl: &Ctl) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| format!("The zip archive could not be read ({e})."))?;
    let total: u64 = (0..zip.len())
        .filter_map(|i| zip.by_index_raw(i).ok().map(|f| f.size()))
        .sum();
    ctl.step(format!("Unpacking {}…", name_of(archive)), total);
    for i in 0..zip.len() {
        if ctl.cancelled() {
            return Err(CANCELLED.into());
        }
        let mut entry = zip.by_index(i).map_err(|e| match e {
            zip::result::ZipError::UnsupportedArchive(m) if m.contains("assword") => {
                "The archive is protected by a password.".to_string()
            }
            e => format!("The zip archive could not be read ({e})."),
        })?;
        let name = entry.name().to_string();
        let Some(rel) = safe_path(&name) else {
            continue;
        };
        let out = into.join(rel);
        let res = if entry.is_dir() || name.ends_with('\\') {
            fs::create_dir_all(&out)
        } else {
            out.parent()
                .map(fs::create_dir_all)
                .unwrap_or(Ok(()))
                .and_then(|()| fs::File::create(&out))
                .and_then(|mut w| io::copy(&mut entry, &mut w))
                .map(|n| ctl.advance(n))
        };
        res.map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}

/// The programs that can unpack an archive, most capable first, each with
/// the arguments it takes.
fn extractors(archive: &Path, into: &Path, ext: &str) -> Vec<(PathBuf, Vec<OsString>)> {
    let mut v: Vec<(PathBuf, Vec<OsString>)> = Vec::new();
    let a = archive.as_os_str().to_os_string();
    let dir = into.as_os_str().to_os_string();
    let mut seven: Vec<PathBuf> = Vec::new();
    let mut unrar: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let pf: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect();
        seven.push("7z.exe".into());
        seven.extend(pf.iter().map(|p| p.join(r"7-Zip\7z.exe")));
        unrar.push("unrar.exe".into());
        unrar.extend(pf.iter().map(|p| p.join(r"WinRAR\UnRAR.exe")));
    } else {
        seven.extend(["7zz", "7z", "7za"].map(PathBuf::from));
        unrar.push("unrar".into());
    }
    for p in seven {
        let mut out = OsString::from("-o");
        out.push(&dir);
        v.push((
            p,
            vec!["x".into(), "-y".into(), "-bd".into(), out, a.clone()],
        ));
    }
    if ext == "rar" {
        let mut d = dir.clone();
        d.push(std::path::MAIN_SEPARATOR_STR);
        for p in unrar {
            v.push((
                p,
                vec!["x".into(), "-y".into(), "-idq".into(), a.clone(), d.clone()],
            ));
        }
        if !cfg!(windows) {
            v.push((
                "unar".into(),
                vec![
                    "-q".into(),
                    "-f".into(),
                    "-D".into(),
                    "-o".into(),
                    dir.clone(),
                    a.clone(),
                ],
            ));
        }
    }
    // bsdtar reads zip, 7z, rar and tar; Windows 10 and later ship it as
    // tar.exe. GNU tar, Linux's usual one, reads only tar.
    let tar_args = vec!["-xf".into(), a.clone(), "-C".into(), dir.clone()];
    v.push(("bsdtar".into(), tar_args.clone()));
    let tar_family = matches!(
        ext,
        "tar" | "gz" | "tgz" | "xz" | "txz" | "bz2" | "tbz2" | "zst"
    );
    if cfg!(windows) {
        let system = std::env::var_os("SystemRoot")
            .map(|r| PathBuf::from(r).join(r"System32\tar.exe"))
            .unwrap_or_else(|| "tar.exe".into());
        v.push((system, tar_args));
    } else if tar_family {
        v.push(("tar".into(), tar_args));
    }
    v
}

fn empty_dir(p: &Path) -> bool {
    fs::read_dir(p)
        .map(|mut rd| rd.next().is_none())
        .unwrap_or(true)
}

fn unpack_with_tool(archive: &Path, into: &Path, ext: &str, ctl: &Ctl) -> Result<(), String> {
    let mut failures = Vec::new();
    for (program, args) in extractors(archive, into, ext) {
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                failures.push(format!("{}: {e}", program.display()));
                continue;
            }
        };
        let status = loop {
            if ctl.cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CANCELLED.into());
            }
            match child.try_wait() {
                Ok(Some(s)) => break Some(s),
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => break None,
            }
        };
        if status.is_some_and(|s| s.success()) && !empty_dir(into) {
            return Ok(());
        }
        failures.push(format!("{} could not unpack it", name_of(&program)));
        // What a failed attempt left is no use to the next.
        let _ = fs::remove_dir_all(into);
        let _ = fs::create_dir_all(into);
    }
    if failures.is_empty() {
        let tool = if cfg!(windows) {
            "7-Zip (7-zip.org)"
        } else if ext == "rar" {
            "7-Zip, unrar or unar"
        } else {
            "7-Zip or bsdtar"
        };
        Err(format!(
            "Omnivores Rust opens .zip archives itself; for a .{ext} it needs {tool}. \
             Or unpack it yourself and add the folder."
        ))
    } else {
        Err(format!("{}.", failures.join("; ")))
    }
}

// ------------------------------------------------------------------------
// Finding games on this computer

/// Folders passed by: the system's own, and ones too big to be worth it.
const SKIP: [&str; 14] = [
    "windows",
    "$recycle.bin",
    "system volume information",
    "programdata",
    "appdata",
    "windowsapps",
    "node_modules",
    "proc",
    "sys",
    "dev",
    "snap",
    "usr",
    "lib",
    "__pycache__",
];

/// Where games are often found, each with how many folders deep to look:
/// old installs, Wine prefixes, Steam, the download folder, CDs and USB
/// sticks.
fn search_roots() -> Vec<(PathBuf, usize)> {
    let h = home();
    let mut v: Vec<(PathBuf, usize)> = Vec::new();
    for d in ["Desktop", "Downloads", "Documents", "Games"] {
        v.push((h.join(d), 3));
    }
    v.push((h.clone(), 2));
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var).map(PathBuf::from) {
                v.push((pf.join(r"Steam\steamapps\common"), 2));
                v.push((pf, 3));
            }
        }
        // Every drive: the top of a CD, D:\Games and the like.
        for letter in b'C'..=b'Z' {
            v.push((PathBuf::from(format!("{}:\\", letter as char)), 2));
        }
        v.push((PathBuf::from(r"C:\Games"), 3));
    } else {
        v.push((h.join(".wine/drive_c"), 4));
        // Lutris and Bottles keep a Wine prefix for each game.
        v.push((h.join("Games"), 5));
        v.push((h.join(".local/share/bottles/bottles"), 5));
        v.push((
            h.join(".var/app/com.usebottles.bottles/data/bottles/bottles"),
            5,
        ));
        v.push((h.join(".local/share/Steam/steamapps/common"), 2));
        v.push((
            h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common"),
            2,
        ));
        if let Some(user) = std::env::var_os("USER") {
            v.push((PathBuf::from("/media").join(&user), 3));
            v.push((PathBuf::from("/run/media").join(&user), 3));
        }
        v.push(("/media".into(), 2));
        v.push(("/mnt".into(), 3));
    }
    v
}

struct Search<'a> {
    found: Vec<PathBuf>,
    seen: HashSet<PathBuf>,
    visited: usize,
    deadline: Instant,
    ctl: &'a Ctl,
}

impl Search<'_> {
    fn done(&self) -> bool {
        self.ctl.cancelled() || self.visited > 200_000 || Instant::now() > self.deadline
    }

    /// Links are not followed: Wine's lead to the whole disk.
    fn dir(&mut self, dir: &Path, depth: usize) {
        if self.done() {
            return;
        }
        self.visited += 1;
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut next = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let Ok(ft) = e.file_type() else { continue };
            if name == "huntdat" && (ft.is_dir() || e.path().is_dir()) {
                if self.seen.insert(canonical(dir)) {
                    self.found.push(dir.to_path_buf());
                }
                return;
            }
            if ft.is_dir() && !name.starts_with('.') && !SKIP.contains(&name.as_str()) {
                next.push(e.path());
            }
        }
        if depth > 0 {
            next.sort();
            for d in next {
                self.dir(&d, depth - 1);
            }
        }
    }
}

/// Windows asks the player to insert a disk when a program looks at an
/// empty CD or card reader, unless the program says it will cope.
#[cfg(windows)]
fn no_disk_prompts() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadErrorMode(new: u32, old: *mut u32) -> i32;
    }
    const SEM_FAILCRITICALERRORS: u32 = 0x0001;
    const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
    // SAFETY: changes only how this thread's own errors are reported.
    unsafe {
        SetThreadErrorMode(
            SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX,
            std::ptr::null_mut(),
        );
    }
}

/// Games and mods on this computer that are not in the library yet.
pub fn search(library: &[PathBuf], ctl: &Ctl) -> Vec<Found> {
    #[cfg(windows)]
    no_disk_prompts();
    ctl.step("Looking for games on this computer…", 0);
    let mut s = Search {
        found: Vec::new(),
        seen: HashSet::new(),
        visited: 0,
        deadline: Instant::now() + Duration::from_secs(20),
        ctl,
    };
    for (root, depth) in search_roots() {
        if root.is_dir() {
            s.dir(&root, depth);
        }
    }
    s.found
        .into_iter()
        .filter(|p| !in_library(p, library))
        .flat_map(|p| look(&p, &name_of(&p), &p, library, false))
        .collect()
}

// ------------------------------------------------------------------------
// Into the library

/// Where an install goes: a game at the top of the library, a mod in its
/// game's mods folder (the library's own, if it has one) - under a name
/// nothing there has yet, and not one of `taken` either.
pub fn destination(
    library: &Path,
    name: &str,
    is_mod: bool,
    engine: Engine,
    taken: &[PathBuf],
) -> PathBuf {
    let dir = if is_mod {
        fs::read_dir(library)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .find(|(n, p)| {
                p.is_dir() && library::mods_dir(n).and_then(engine_from_name) == Some(engine)
            })
            .map(|(_, p)| p)
            .unwrap_or_else(|| library.join(format!("{} mods", engine.folder())))
    } else {
        library.to_path_buf()
    };
    let mut base = file_safe(name);
    if base.is_empty() || base.starts_with('.') {
        base = format!(
            "{} {}",
            engine.folder(),
            if is_mod { "mod" } else { "game" }
        );
    }
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(&base)
            } else {
                dir.join(format!("{base} ({n})"))
            }
        })
        .find(|p| {
            let n = name_of(p);
            find_ci(&dir, &n).is_none()
                && !taken
                    .iter()
                    .any(|t| t.parent() == p.parent() && name_of(t).eq_ignore_ascii_case(&n))
        })
        .expect("some name is free")
}

/// One thing to put into the library.
pub struct Plan {
    pub source: PathBuf,
    /// `source` is the inside of a HUNTDAT.
    pub bare: bool,
    /// A game to copy first, for a mod that carries only what it changes.
    pub base: Option<PathBuf>,
    pub dest: PathBuf,
    /// `source` is an unpacked archive's, and may be moved rather than
    /// copied.
    pub movable: bool,
}

/// What went into the library, and what stopped the rest.
pub struct Installed {
    pub done: Vec<PathBuf>,
    pub error: Option<String>,
}

pub fn install(plans: &[Plan], ctl: &Ctl) -> Installed {
    let total: u64 = plans
        .iter()
        .map(|p| tree_size(&p.source) + p.base.as_deref().map(tree_size).unwrap_or(0))
        .sum();
    ctl.step("Copying…", total);
    let mut done = Vec::new();
    for plan in plans {
        ctl.say(format!("Copying {}…", name_of(&plan.dest)));
        match install_one(plan, ctl) {
            Ok(()) => done.push(plan.dest.clone()),
            Err(e) => {
                let error = if ctl.cancelled() {
                    CANCELLED.to_string()
                } else {
                    format!("{}: {e}", name_of(&plan.dest))
                };
                return Installed {
                    done,
                    error: Some(error),
                };
            }
        }
    }
    Installed { done, error: None }
}

/// Builds the new folder under a hidden name beside where it goes and
/// renames it into place at the end, so the library never shows half a
/// game; whatever goes wrong, nothing is left behind.
fn install_one(plan: &Plan, ctl: &Ctl) -> io::Result<()> {
    let parent = plan
        .dest
        .parent()
        .ok_or_else(|| io::Error::other("no folder to put it in"))?;
    fs::create_dir_all(parent)?;
    if plan.dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is there already", plan.dest.display()),
        ));
    }
    let tmp = parent.join(work_name("install"));
    let result = (|| {
        // An unpacked archive is simply moved, if it is on the same drive;
        // should the last step fail it goes back, so nothing is lost.
        if plan.movable && plan.base.is_none() {
            let inner = if plan.bare {
                fs::create_dir_all(&tmp)?;
                tmp.join("HUNTDAT")
            } else {
                tmp.clone()
            };
            if fs::rename(&plan.source, &inner).is_ok() {
                ctl.advance(tree_size(&inner));
                return fs::rename(&tmp, &plan.dest).inspect_err(|_| {
                    let _ = fs::rename(&inner, &plan.source);
                });
            }
        }
        if let Some(base) = &plan.base {
            merge(base, &tmp, ctl)?;
        }
        let into = if plan.bare {
            find_ci(&tmp, "HUNTDAT").unwrap_or_else(|| tmp.join("HUNTDAT"))
        } else {
            tmp.clone()
        };
        merge(&plan.source, &into, ctl)?;
        ctl.check()?;
        fs::rename(&tmp, &plan.dest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result
}

/// Clears away working folders a launcher that was closed mid-way left in
/// the library.
pub fn sweep(library: &[PathBuf]) {
    let stale = |p: &Path| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > Duration::from_secs(3600))
    };
    let ours = |p: &Path| {
        let n = name_of(p);
        n.starts_with(".omnivores-unpack-") || n.starts_with(".omnivores-install-")
    };
    for root in library {
        let mut dirs = vec![root.clone()];
        dirs.extend(subdirs(root));
        for d in dirs {
            for p in fs::read_dir(&d)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
            {
                if ours(&p) && p.is_dir() && stale(&p) {
                    let _ = fs::remove_dir_all(&p);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary one, gone when
    /// dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(what: &str) -> Scratch {
            let p = std::env::temp_dir().join(format!(
                "omnivores-test-{what}-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn quiet() -> Ctl {
        Ctl::new(|| {})
    }

    /// Enough of a Carnivores 2 install to count as a whole game.
    fn retail(root: &Path) {
        put(root, "CARN2.EXE", "MZ");
        put(root, "HUNTDAT/_RES.TXT", "res");
        put(root, "HUNTDAT/MENU/MENUM.TGA", "menu");
        put(root, "HUNTDAT/SOUNDFX/a.wav", "wav");
        put(root, "HUNTDAT/AREAS/AREA1.MAP", "base map");
        put(root, "HUNTDAT/AREAS/AREA1.RSC", "base rsc");
    }

    #[test]
    fn whole_games_and_parts() {
        let s = Scratch::new("complete");
        retail(&s.0.join("Carnivores 2"));
        put(&s.0, "Part/HUNTDAT/AREAS/AREA1.MAP", "x");
        let a = analyse(std::slice::from_ref(&s.0), &s.0, &[], &quiet());
        assert_eq!(a.found.len(), 2);
        let game = a
            .found
            .iter()
            .find(|f| f.game.name == "Carnivores 2")
            .unwrap();
        assert!(game.complete(Engine::C2) && !game.game.is_mod && !game.bare);
        assert_eq!(game.game.engine, Engine::C2);
        let part = a.found.iter().find(|f| f.game.name == "Part").unwrap();
        assert!(!part.complete(part.game.engine) && part.game.is_mod);
        // Not the scan's guess of Carnivores 1 for a folder with no
        // renderers: most mods are for Carnivores 2.
        assert_eq!(part.game.engine, Engine::C2);
    }

    #[test]
    fn huntdat_chosen_for_its_game() {
        let s = Scratch::new("huntdat");
        retail(&s.0.join("C2"));
        let a = analyse(&[s.0.join("C2").join("HUNTDAT")], &s.0, &[], &quiet());
        assert_eq!(a.found.len(), 1);
        let f = &a.found[0];
        assert_eq!(f.game.path, s.0.join("C2"));
        assert!(f.complete(f.game.engine) && !f.bare && !f.game.is_mod);
    }

    /// The first game kept its creatures and weapons in its code, so its
    /// HUNTDAT has no _RES.TXT: it is a whole game all the same.
    #[test]
    fn first_game_whole_without_resource_list() {
        let s = Scratch::new("c1");
        let lib = s.0.join("lib");
        fs::create_dir_all(&lib).unwrap();
        let c1 = s.0.join("Carnivores");
        put(&c1, "HUNTSOFT.EXE", "MZ");
        put(&c1, "HUNTDAT/MENU/MENUM.TGA", "menu");
        put(&c1, "HUNTDAT/SOUNDFX/a.wav", "wav");
        put(&c1, "HUNTDAT/AREAS/AREA1.MAP", "map");
        put(&c1, "HUNTDAT/AREAS/AREA1.RSC", "rsc");
        // Nor does it need its program to be there to be known.
        let bare = s.0.join("Old CD");
        for rel in ["MENU/MENUM.TGA", "SOUNDFX/a.wav", "AREAS/AREA1.MAP"] {
            put(&bare.join("Carnivores"), &format!("HUNTDAT/{rel}"), "x");
        }
        let a = analyse(
            &[c1.clone(), bare.join("Carnivores")],
            &s.0,
            std::slice::from_ref(&lib),
            &quiet(),
        );
        assert_eq!(a.found.len(), 2);
        for f in &a.found {
            assert_eq!(f.game.engine, Engine::C1);
            assert!(f.complete(Engine::C1) && !f.game.is_mod && !f.bare);
            // Carnivores 2 and Ice Age could not start from it.
            assert!(!f.complete(Engine::C2) && !f.complete(Engine::Ice));
        }
        // It goes at the top of the library, not among the mods.
        assert_eq!(
            destination(&lib, "Carnivores", false, Engine::C1, &[]),
            lib.join("Carnivores")
        );
    }

    #[test]
    fn mod_over_its_game() {
        let s = Scratch::new("overlay");
        let lib = s.0.join("lib");
        retail(&lib.join("Carnivores 2"));
        // The mod spells its names its own way; the game must still end up
        // with one of each file.
        let m = s.0.join("New Area");
        put(&m, "huntdat/areas/area1.map", "mod map");
        put(&m, "huntdat/areas/AREA6.RSC", "new rsc");
        let found = analyse(
            std::slice::from_ref(&m),
            &s.0,
            std::slice::from_ref(&lib),
            &quiet(),
        );
        let f = &found.found[0];
        assert!(!f.complete(f.game.engine) && f.game.is_mod);
        let dest = destination(&lib, &f.game.name, true, Engine::C2, &[]);
        assert_eq!(dest, lib.join("Carnivores 2 mods").join("New Area"));
        let done = install(
            &[Plan {
                source: f.game.path.clone(),
                bare: f.bare,
                base: Some(lib.join("Carnivores 2")),
                dest: dest.clone(),
                movable: false,
            }],
            &quiet(),
        );
        assert!(done.error.is_none(), "{:?}", done.error);
        let areas = dest.join("HUNTDAT/AREAS");
        let mut names: Vec<String> = fs::read_dir(&areas)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["AREA1.MAP", "AREA1.RSC", "AREA6.RSC"]);
        assert_eq!(
            fs::read_to_string(areas.join("AREA1.MAP")).unwrap(),
            "mod map"
        );
        assert!(Contents::of(&dest.join("HUNTDAT")).complete(Engine::C2));
        // The source and the game it was laid over are as they were.
        assert!(m.join("huntdat/areas/area1.map").is_file());
        assert_eq!(
            fs::read_to_string(lib.join("Carnivores 2/HUNTDAT/AREAS/AREA1.MAP")).unwrap(),
            "base map"
        );
        // The library sees it as a Carnivores 2 mod.
        let games = library::scan_path(&lib);
        let g = games.iter().find(|g| g.path == dest).unwrap();
        assert!(g.is_mod && g.engine == Engine::C2 && !g.engine_guessed);
        // A second of the same name goes beside it.
        assert_eq!(
            destination(&lib, "new area", true, Engine::C2, &[]),
            lib.join("Carnivores 2 mods").join("new area (2)")
        );
    }

    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, text) in files {
            z.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(text.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn archives() {
        let s = Scratch::new("zip");
        let lib = s.0.join("lib");
        fs::create_dir_all(&lib).unwrap();
        // A mod made to be copied into HUNTDAT, with a Windows-style name
        // and one that tries to climb out.
        let z = s.0.join("Jungle Pack v2.zip");
        zip_of(
            &z,
            &[
                ("AREAS\\AREA7.MAP", "map"),
                ("AREAS/AREA7.RSC", "rsc"),
                ("../evil.txt", "no"),
            ],
        );
        let a = analyse(
            std::slice::from_ref(&z),
            &lib,
            std::slice::from_ref(&lib),
            &quiet(),
        );
        assert!(a.problems.is_empty(), "{:?}", a.problems);
        assert_eq!(a.found.len(), 1);
        let f = &a.found[0];
        assert!(f.bare && f.unpacked && !f.in_library);
        assert_eq!(f.game.name, "Jungle Pack v2");
        assert!(!s.0.join("evil.txt").exists() && !lib.join("evil.txt").exists());
        let dest = destination(&lib, &f.game.name, true, f.game.engine, &[]);
        let done = install(
            &[Plan {
                source: f.game.path.clone(),
                bare: true,
                base: None,
                dest: dest.clone(),
                movable: true,
            }],
            &quiet(),
        );
        assert!(done.error.is_none(), "{:?}", done.error);
        assert!(dest.join("HUNTDAT/AREAS/AREA7.MAP").is_file());
        for p in a.unpacked {
            let _ = fs::remove_dir_all(p);
        }
        // Nothing of the work is left in the library.
        let left: Vec<String> = fs::read_dir(&lib)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["Carnivores 2 mods"]);
    }

    #[test]
    fn not_archives() {
        let s = Scratch::new("bad");
        let exe = s.0.join("Setup.exe");
        fs::write(&exe, "MZ").unwrap();
        let txt = s.0.join("readme.txt");
        fs::write(&txt, "hello").unwrap();
        let a = analyse(&[exe, txt], &s.0, &[], &quiet());
        assert!(a.found.is_empty());
        assert_eq!(a.problems.len(), 2);
        assert!(a.problems[0].contains("program, not an archive"));
        assert!(a.unpacked.is_empty());
    }

    #[test]
    fn already_there_and_cancelled() {
        let s = Scratch::new("cancel");
        let lib = s.0.join("lib");
        retail(&lib.join("Carnivores 2"));
        let a = analyse(
            &[lib.join("Carnivores 2")],
            &s.0,
            std::slice::from_ref(&lib),
            &quiet(),
        );
        assert!(a.found[0].in_library);
        let ctl = quiet();
        ctl.cancel();
        let done = install(
            &[Plan {
                source: lib.join("Carnivores 2"),
                bare: false,
                base: None,
                dest: lib.join("Copy"),
                movable: false,
            }],
            &ctl,
        );
        assert_eq!(done.error.as_deref(), Some(CANCELLED));
        let left: Vec<String> = fs::read_dir(&lib)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["Carnivores 2"]);
    }

    #[test]
    fn paths_in_archives() {
        assert_eq!(
            safe_path("a/b\\c"),
            Some(PathBuf::from("a").join("b").join("c"))
        );
        assert_eq!(safe_path("/abs/x"), Some(PathBuf::from("abs").join("x")));
        assert_eq!(safe_path("a/../../x"), None);
        assert_eq!(safe_path("C:\\x"), None);
        assert_eq!(safe_path("./"), None);
        assert_eq!(archive_stem(Path::new("/d/Mod.tar.gz")), "Mod");
        assert_eq!(archive_stem(Path::new("/d/Mod v1.2.zip")), "Mod v1.2");
    }
}

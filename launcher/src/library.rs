//! Finding Carnivores installs and working out which engine each one needs.
//!
//! A game or mod is any directory holding a HUNTDAT folder. Which of the
//! three engines it runs on is decided from, in order of how much it can be
//! trusted:
//!
//!   1. A parent folder named "<game> mods", which states the answer
//!      outright. This is the layout the library uses: base games at the top
//!      level, with "Carnivores 2 mods", "Carnivores Ice Age mods" and so on
//!      beside them.
//!   2. The executable the install ships - CARN2.EXE, IceAge.exe,
//!      HUNT3DFX.EXE. A mod ships its own under its own name, so this only
//!      settles retail.
//!   3. The folder's own name.
//!   4. The .REN renderer modules. Carnivores 1 shipped one executable per
//!      renderer and has none; Carnivores 2 and Ice Age ship three.
//!
//! Anything still unresolved is assumed to be Carnivores 2, the engine most
//! mods target - and the answer can be corrected per entry.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Engine {
    C1,
    C2,
    Ice,
}

pub const ENGINES: [Engine; 3] = [Engine::C1, Engine::C2, Engine::Ice];

impl Engine {
    /// The id the settings file keeps it under.
    pub fn id(self) -> &'static str {
        match self {
            Engine::C1 => "c1",
            Engine::C2 => "c2",
            Engine::Ice => "ice",
        }
    }

    pub fn from_id(id: &str) -> Option<Engine> {
        ENGINES.into_iter().find(|e| e.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Engine::C1 => "Carnivores",
            Engine::C2 => "Carnivores 2",
            Engine::Ice => "Carnivores: Ice Age",
        }
    }

    /// The name of its folder in the library, and of its mods folder with
    /// " mods" after it: the library's own layout, and a name any file
    /// system takes.
    pub fn folder(self) -> &'static str {
        match self {
            Engine::C1 => "Carnivores",
            Engine::C2 => "Carnivores 2",
            Engine::Ice => "Carnivores Ice Age",
        }
    }

    /// The Rust rewrite's program for it.
    pub fn binary(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Engine::C1, true) => "carnivores1-rs.exe",
            (Engine::C2, true) => "carnivores2-rs.exe",
            (Engine::Ice, true) => "carnivores-iceage-rs.exe",
            (Engine::C1, false) => "carnivores1-rs",
            (Engine::C2, false) => "carnivores2-rs",
            (Engine::Ice, false) => "carnivores-iceage-rs",
        }
    }
}

/// Retail executable stems. A .exe that is not one of these belongs to a mod.
fn retail_exe(stem: &str) -> Option<Engine> {
    match stem {
        "carn2" => Some(Engine::C2),
        "iceage" => Some(Engine::Ice),
        "hunt" | "huntsoft" | "hunt3dfx" | "huntd3d" => Some(Engine::C1),
        _ => None,
    }
}

const IGNORED_EXES: [&str; 5] = ["setup", "uninst", "unins000", "altedit", "install"];

/// "Carnivores 2 mods", "ice age_mod" and the like: the name before the
/// word, if the folder is one.
pub fn mods_dir(name: &str) -> Option<&str> {
    let low = name.to_ascii_lowercase();
    let cut = if low.ends_with("mods") {
        4
    } else if low.ends_with("mod") {
        3
    } else {
        return None;
    };
    let head = &name[..name.len() - cut];
    let game = head.trim_end_matches([' ', '_', '-']);
    (game.len() < head.len()).then_some(game)
}

/// Map a human game name onto an engine.
pub fn engine_from_name(name: &str) -> Option<Engine> {
    let n = name.trim().to_lowercase();
    if n.contains("ice age") || n.contains("iceage") {
        return Some(Engine::Ice);
    }
    // A "2" standing as a word of its own.
    let b: Vec<char> = n.chars().collect();
    let word = |c: Option<&char>| c.map(|c| c.is_alphanumeric() || *c == '_').unwrap_or(false);
    let two = (0..b.len()).any(|i| {
        b[i] == '2' && !word(i.checked_sub(1).and_then(|j| b.get(j))) && !word(b.get(i + 1))
    });
    if two || n.contains("carnivores2") {
        return Some(Engine::C2);
    }
    if n.contains("carnivore") {
        return Some(Engine::C1);
    }
    None
}

/// One playable entry: a retail install or a mod.
#[derive(Clone, Debug)]
pub struct Game {
    pub path: PathBuf,
    pub name: String,
    pub engine: Engine,
    pub icon_source: Option<PathBuf>,
    pub is_mod: bool,
    /// False when the layout stated the engine.
    pub engine_guessed: bool,
    // From the saved settings.
    pub custom_name: Option<String>,
    pub custom_icon: Option<PathBuf>,
    pub custom_engine: Option<Engine>,
}

impl Game {
    pub fn key(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    pub fn title(&self) -> &str {
        self.custom_name.as_deref().unwrap_or(&self.name)
    }

    pub fn active_engine(&self) -> Engine {
        self.custom_engine.unwrap_or(self.engine)
    }

    pub fn subtitle(&self) -> String {
        let label = self.active_engine().label();
        if self.is_mod {
            return format!("Mod • {label}");
        }
        // Naming the engine under a retail install just repeats its title,
        // give or take the punctuation.
        let squash = |s: &str| {
            s.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        };
        if squash(label) == squash(self.title()) {
            "Game".into()
        } else {
            label.into()
        }
    }
}

fn listdir(path: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut v: Vec<(String, PathBuf)> = rd
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

pub fn has_huntdat(path: &Path) -> bool {
    listdir(path)
        .iter()
        .any(|(n, p)| n.eq_ignore_ascii_case("huntdat") && p.is_dir())
}

/// The file to take the icon from, and the mod's executable if there is
/// one. A loose .ico is the artwork the game shipped for itself so it wins;
/// otherwise the executable's resources are used, preferring a mod's own.
fn icon_source(path: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let (mut ico, mut exe, mut mod_exe) = (None, None, None);
    for (n, full) in listdir(path) {
        if !full.is_file() {
            continue;
        }
        let low = n.to_lowercase();
        if low.ends_with(".ico") {
            ico = ico.or(Some(full));
        } else if let Some(stem) = low.strip_suffix(".exe") {
            if IGNORED_EXES.contains(&stem) {
                continue;
            }
            exe = exe.or(Some(full.clone()));
            if retail_exe(stem).is_none() && mod_exe.is_none() {
                mod_exe = Some(full);
            }
        }
    }
    (ico.or(mod_exe.clone()).or(exe), mod_exe)
}

/// The engine implied by the install's own contents, and whether that is
/// certain.
fn engine_from_dir(path: &Path) -> (Option<Engine>, bool) {
    let names: Vec<String> = listdir(path)
        .into_iter()
        .map(|(n, _)| n.to_lowercase())
        .collect();
    for n in &names {
        if let Some(e) = n.strip_suffix(".exe").and_then(retail_exe) {
            return (Some(e), true);
        }
    }
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(e) = engine_from_name(&base) {
        return (Some(e), false);
    }
    // Carnivores 1 shipped a binary per renderer; the later engine ships the
    // renderers as separate .REN modules beside a single launcher.
    if !names.iter().any(|n| n.ends_with(".ren")) {
        return (Some(Engine::C1), false);
    }
    (None, false)
}

fn install(path: PathBuf, name: String, forced: Option<Engine>, in_collection: bool) -> Game {
    let (icon, mod_exe) = icon_source(&path);
    let (engine, sure) = match forced {
        Some(e) => (e, true),
        None => match engine_from_dir(&path) {
            (Some(e), sure) => (e, sure),
            (None, _) => (Engine::C2, false),
        },
    };
    Game {
        path,
        name,
        engine,
        icon_source: icon,
        // A mod either sits in a "<game> mods" folder or ships an
        // executable under a name no retail release used.
        is_mod: in_collection || mod_exe.is_some(),
        engine_guessed: !sure,
        custom_name: None,
        custom_icon: None,
        custom_engine: None,
    }
}

/// One install taken on its own, as the scan would see it: a "<game> mods"
/// folder around it still says which game it belongs to.
pub fn examine(path: &Path) -> Game {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let parent = path
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let game = mods_dir(&parent);
    install(
        path.to_path_buf(),
        name,
        game.and_then(engine_from_name),
        game.is_some(),
    )
}

/// Every install under one library path.
pub fn scan_path(root: &Path) -> Vec<Game> {
    fn walk(
        path: &Path,
        depth: usize,
        collection: bool,
        forced: Option<Engine>,
        found: &mut Vec<Game>,
    ) {
        if depth > 3 {
            return;
        }
        let mut dirs = listdir(path);
        dirs.sort_by_key(|(n, _)| n.to_lowercase());
        for (name, full) in dirs {
            if !full.is_dir() || name.starts_with('.') {
                continue;
            }
            if has_huntdat(&full) {
                found.push(install(full, name, forced, collection));
                continue;
            }
            // "Carnivores 2 mods" and friends: the folder names the engine
            // every install inside it runs on.
            match mods_dir(&name) {
                Some(game) => walk(
                    &full,
                    depth + 1,
                    true,
                    engine_from_name(game).or(forced),
                    found,
                ),
                None => walk(&full, depth + 1, collection, forced, found),
            }
        }
    }

    let mut found = Vec::new();
    if !root.is_dir() {
        return found;
    }
    if has_huntdat(root) {
        found.push(examine(root));
    } else {
        walk(root, 0, false, None, &mut found);
    }
    found
}

/// Every library path scanned, duplicates dropped, base games first and
/// then mods, alphabetically within each.
pub fn scan(paths: &[String]) -> Vec<Game> {
    let mut games = Vec::new();
    let mut seen = HashSet::new();
    for p in paths {
        for g in scan_path(Path::new(p)) {
            if seen.insert(g.key()) {
                games.push(g);
            }
        }
    }
    games
}

pub fn sort(games: &mut [Game]) {
    games.sort_by_key(|g| (g.is_mod, g.title().to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_mod_folders() {
        assert_eq!(engine_from_name("Carnivores Ice Age"), Some(Engine::Ice));
        assert_eq!(engine_from_name("Carnivores 2"), Some(Engine::C2));
        assert_eq!(engine_from_name("carnivores2"), Some(Engine::C2));
        assert_eq!(engine_from_name("C2"), None);
        assert_eq!(engine_from_name("Carnivores"), Some(Engine::C1));
        assert_eq!(mods_dir("Carnivores 2 mods"), Some("Carnivores 2"));
        assert_eq!(
            mods_dir("Carnivores Ice Age mods"),
            Some("Carnivores Ice Age")
        );
        assert_eq!(mods_dir("Carnivores_mod"), Some("Carnivores"));
        assert_eq!(mods_dir("Carnivoresmods"), None);
        assert_eq!(mods_dir("Mandibles"), None);
    }
}

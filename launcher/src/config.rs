//! Saved settings: where the library lives, where the engines are, and any
//! per-game title, icon or engine the player has overridden.
//!
//! Kept in `%APPDATA%\omnivores-rust\config.json` on Windows and
//! `~/.config/omnivores-rust/config.json` elsewhere. Nothing here ever writes inside a game folder - renames and
//! custom icons are the launcher's view of an install, not a change to it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::{Engine, Game, ENGINES};

/// The games folder looked in first: `%USERPROFILE%\Omnivores` on Windows,
/// `~/.omnivores` elsewhere.
pub fn default_library() -> PathBuf {
    if cfg!(windows) {
        home().join("Omnivores")
    } else {
        home().join(".omnivores")
    }
}

pub fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(home().join(".config")))
    };
    base.unwrap_or_else(home).join("omnivores-rust")
}

/// Where the three game programs are found when the settings do not say:
/// beside the launcher (they ship together), then on the PATH.
pub fn detect_engine(e: Engine) -> String {
    let exe = e.binary();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        dirs.push(d);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Override {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

impl Override {
    fn is_empty(&self) -> bool {
        self.name.is_none() && self.icon.is_none() && self.engine.is_none()
    }
}

/// The shortcuts made for one game, so they can be found and taken away
/// again whatever the entry has been renamed to since.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Shortcuts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub library_paths: Vec<String>,
    #[serde(default)]
    pub engines: BTreeMap<String, String>,
    #[serde(default)]
    pub overrides: BTreeMap<String, Override>,
    #[serde(default = "default_icon_size")]
    pub icon_size: u32,
    /// Whether to offer to install the launcher while it has no menu entry.
    #[serde(default = "default_true")]
    pub offer_install: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shortcuts: BTreeMap<String, Shortcuts>,
}

fn default_icon_size() -> u32 {
    56
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            library_paths: Vec::new(),
            engines: BTreeMap::new(),
            overrides: BTreeMap::new(),
            icon_size: default_icon_size(),
            offer_install: true,
            shortcuts: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        config_dir().join("config.json")
    }

    pub fn load() -> Config {
        let mut c: Config = std::fs::read(Self::path())
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        c.icon_size = c.icon_size.clamp(32, 128);
        // Fill in any engine the settings do not pin down, or whose program
        // has gone - a download folder deleted once the launcher was
        // installed, say.
        for e in ENGINES {
            let gone = c
                .engines
                .get(e.id())
                .map(|s| s.is_empty() || !Path::new(s).is_file())
                .unwrap_or(true);
            if gone {
                let found = detect_engine(e);
                if !found.is_empty() || !c.engines.contains_key(e.id()) {
                    c.engines.insert(e.id().into(), found);
                }
            }
        }
        c
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        std::fs::create_dir_all(config_dir())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
        std::fs::rename(&tmp, &path)
    }

    pub fn engine_path(&self, e: Engine) -> String {
        self.engines.get(e.id()).cloned().unwrap_or_default()
    }

    // ---------------------------------------------------------- overrides

    pub fn apply_overrides(&self, games: &mut [Game]) {
        for g in games {
            let o = self.overrides.get(&g.key()).cloned().unwrap_or_default();
            g.custom_name = o.name;
            g.custom_icon = o.icon.map(PathBuf::from);
            g.custom_engine = o.engine.as_deref().and_then(Engine::from_id);
        }
    }

    pub fn set_override(&mut self, game: &Game, f: impl FnOnce(&mut Override)) {
        let o = self.overrides.entry(game.key()).or_default();
        f(o);
        if o.is_empty() {
            self.overrides.remove(&game.key());
        }
    }

    /// Copy a chosen image into the launcher's own store, so the entry keeps
    /// its picture if the file later moves.
    pub fn store_custom_icon(&self, game: &Game, source: &Path) -> std::io::Result<PathBuf> {
        let dir = config_dir().join("icons");
        std::fs::create_dir_all(&dir)?;
        let stem: String = game
            .key()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string();
        let stem: String = stem
            .chars()
            .rev()
            .take(120)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let ext = source
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_else(|| ".png".into());
        let dest = dir.join(stem + &ext);
        std::fs::copy(source, &dest)?;
        Ok(dest)
    }
}

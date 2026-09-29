//! Finding the game's files.
//!
//! The data was made on Windows, where names are not case-sensitive, and the
//! games spell them however they like ("HUNTDAT\\AREAS\\AREA1" for a file
//! that is "area1.MAP" on disk in Ice Age). Every lookup goes through here.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, bevy::prelude::Resource)]
pub struct DataRoot(pub PathBuf);

impl DataRoot {
    /// A file under the game folder, matched without regard to case in any
    /// component. `rel` may use either slash.
    pub fn find(&self, rel: &str) -> Option<PathBuf> {
        find_ci(&self.0, rel)
    }

    pub fn find_or_err(&self, rel: &str) -> Result<PathBuf, String> {
        self.find(rel).ok_or_else(|| {
            format!(
                "{} not found in {}",
                rel.replace('\\', "/"),
                self.0.display()
            )
        })
    }

    /// Files in a folder (case-insensitive folder name) whose extension is
    /// `ext` (case-insensitive), sorted by name.
    pub fn list(&self, dir: &str, ext: &str) -> Vec<PathBuf> {
        let Some(d) = self.find(dir) else {
            return Vec::new();
        };
        let Ok(rd) = std::fs::read_dir(&d) else {
            return Vec::new();
        };
        let mut v: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .map(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
                    .unwrap_or(false)
            })
            .collect();
        v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        v
    }
}

pub fn find_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for comp in rel
        .split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
    {
        let exact = cur.join(comp);
        if exact.exists() {
            cur = exact;
            continue;
        }
        let rd = std::fs::read_dir(&cur).ok()?;
        let found = rd
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(comp))?;
        cur = found.path();
    }
    Some(cur)
}

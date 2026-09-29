//! Hunter profiles: a name, the credit earned, the rank, the statistics of
//! the last hunt and of all of them, and the trophies.
//!
//! Kept in the games' own files, trophy0N.sav in the game folder, one per
//! slot, and shared with the original games. Those append their options
//! after the record (and Carnivores 1 keeps one in the record's
//! spare words); the rewrite keeps its options in its own file, so it
//! writes only the record and leaves all of that as it found it.

use std::path::{Path, PathBuf};

use crate::game::GameKind;
use crate::paths::DataRoot;

/// Profile slots (trophy00.sav .. trophy07.sav).
pub const SLOTS: usize = 8;
/// Trophies kept per profile.
pub const BODIES: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub shots_made: i32,
    pub success: i32,
    /// Metres walked.
    pub path: f32,
    /// Seconds.
    pub time: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrophyItem {
    /// Creature type; 0 is an empty slot.
    pub ctype: i32,
    pub weapon: i32,
    pub phase: i32,
    pub height: i32,
    pub weight: i32,
    pub score: i32,
    /// Year << 20 | month << 10 | day.
    pub date: i32,
    /// Hours << 10 | minutes.
    pub time: i32,
    pub scale: f32,
    pub range: f32,
}

/// The first game's hunt equipment (scent, camouflage, radar, tranquilliser),
/// which it keeps with its options after the record: saved as a hunt
/// starts, and back on the weapon screen when the hunter is picked.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kit {
    pub scent: bool,
    pub camo: bool,
    pub radar: bool,
    pub tranq: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub slot: usize,
    pub score: i32,
    pub rank: i32,
    pub last: Stats,
    pub total: Stats,
    pub bodies: [TrophyItem; BODIES],
    /// Carnivores 1 only: the equipment its file holds, when it holds it.
    pub kit: Option<Kit>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            name: String::new(),
            slot: 0,
            score: 0,
            rank: 0,
            last: Stats::default(),
            total: Stats::default(),
            bodies: [TrophyItem::default(); BODIES],
            kit: None,
        }
    }
}

/// The profiles of one game folder (the game itself, or a mod: each has
/// its own hunters, as in the originals).
#[derive(Clone, Debug)]
pub struct Store {
    kind: GameKind,
    game: DataRoot,
}

impl Store {
    pub fn new(kind: GameKind, root: &DataRoot) -> Store {
        Store {
            kind,
            game: root.clone(),
        }
    }

    /// A slot's file: the one there, whatever the case of its name, or
    /// where a new one goes.
    fn file(&self, slot: usize) -> PathBuf {
        let name = format!("trophy0{slot}.sav");
        self.game
            .find(&name)
            .unwrap_or_else(|| self.game.0.join(name))
    }

    pub fn load(&self, slot: usize) -> Option<Profile> {
        let data = std::fs::read(self.game.find(&format!("trophy0{slot}.sav"))?).ok()?;
        let mut p = Profile::parse(&data)?;
        p.slot = slot;
        if self.kind == GameKind::Carnivores {
            p.kit = data.get(C1_KIT_AT..C1_KIT_AT + 16).map(|b| {
                let on = |i: usize| b[i * 4..i * 4 + 4] != [0; 4];
                Kit {
                    scent: on(0),
                    camo: on(1),
                    radar: on(2),
                    tranq: on(3),
                }
            });
        }
        (!p.name.is_empty()).then_some(p)
    }

    /// Every slot, with the profile in it if there is one.
    pub fn scan(&self, slots: usize) -> Vec<Option<Profile>> {
        (0..slots).map(|s| self.load(s)).collect()
    }

    pub fn save(&self, p: &mut Profile) {
        p.rank = Profile::earned_rank(p.score);
        let f = self.file(p.slot);
        let old = std::fs::read(&f).ok().filter(|o| o.len() >= RECORD);
        let c1 = self.kind == GameKind::Carnivores;
        let mut out = match old {
            Some(old) => p.record_over(self.kind, Some(&old)),
            // A new hunter in the first game starts with the options it
            // had in memory, the ones of the hunter loaded before, at resolution 3.
            None if c1 => match self.options_donor(p.slot) {
                Some(d) => {
                    let mut out = p.record_over(self.kind, Some(&d));
                    out[RECORD + 12..RECORD + 16].copy_from_slice(&3i32.to_le_bytes());
                    out
                }
                None => p.record_over(self.kind, None),
            },
            None => p.record_over(self.kind, None),
        };
        if let (true, Some(k)) = (c1 && out.len() >= C1_KIT_AT + 16, p.kit) {
            for (i, on) in [k.scent, k.camo, k.radar, k.tranq].into_iter().enumerate() {
                let o = C1_KIT_AT + i * 4;
                out[o..o + 4].copy_from_slice(&(on as i32).to_le_bytes());
            }
        }
        if let Err(e) = write_file(&f, &out) {
            bevy::log::warn!("{}: {e}", f.display());
        }
    }

    /// Another hunter's file with the first game's options after the
    /// record, the first slot's first (the one the game loads on start).
    fn options_donor(&self, slot: usize) -> Option<Vec<u8>> {
        (0..SLOTS)
            .filter(|&s| s != slot)
            .filter_map(|s| self.game.find(&format!("trophy0{s}.sav")))
            .filter_map(|f| std::fs::read(f).ok())
            .find(|d| d.len() >= C1_KIT_AT + 16)
    }

    /// Takes a hunter off the list: the file goes, as it did in the
    /// originals.
    pub fn delete(&self, slot: usize) {
        if let Some(f) = self.game.find(&format!("trophy0{slot}.sav")) {
            if let Err(e) = std::fs::remove_file(&f) {
                bevy::log::warn!("{}: {e}", f.display());
            }
        }
    }
}

/// Writes a file whole or not at all: beside it first, then over it.
fn write_file(f: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = f.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, f).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

// ----------------------------------------------------------------------
// The profiles earlier builds of the rewrite kept to themselves
// ----------------------------------------------------------------------

/// Where earlier builds kept their own copies of the profiles:
/// `$XDG_DATA_HOME/carnivores-rs` (`~/.local/share/...`), or
/// `%APPDATA%\carnivores-rs` on Windows.
fn old_base() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("carnivores-rs"))
}

/// The folder an earlier build kept a game folder's profiles in.
fn old_dir(kind: GameKind, root: &DataRoot) -> Option<PathBuf> {
    let path = std::fs::canonicalize(&root.0).unwrap_or_else(|_| root.0.clone());
    let name: String = path
        .file_name()
        .map(|n| {
            n.to_string_lossy()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == ' ' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| "game".into());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.to_string_lossy().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    Some(
        old_base()?
            .join(kind.id())
            .join(format!("{name}-{:08x}", h as u32)),
    )
}

/// Brings the copies an earlier build kept to itself back into the game
/// folder, once: a hunter only there is written there; one in both keeps
/// every trophy either has (with its credit), the larger totals and the
/// later last hunt. The copies are then set aside (the folder renamed
/// `.migrated`), with the game folder's files as they were beside them.
pub fn migrate(kind: GameKind, root: &DataRoot) {
    let Some(dir) = old_dir(kind, root) else {
        return;
    };
    if !dir.is_dir() {
        return;
    }
    let store = Store::new(kind, root);
    let slots = if kind == GameKind::Carnivores { 6 } else { 7 };
    let mut before = Vec::new();
    for slot in 0..SLOTS {
        let own = dir.join(format!("trophy0{slot}.sav"));
        let Some(mut mine) = std::fs::read(&own).ok().and_then(|d| Profile::parse(&d)) else {
            continue;
        };
        if mine.name.is_empty() {
            continue;
        }
        mine.slot = slot;
        let game_file = store.game.find(&format!("trophy0{slot}.sav"));
        if let Some(f) = &game_file {
            if let Ok(d) = std::fs::read(f) {
                before.push((format!("trophy0{slot}.sav"), d));
            }
        }
        let mut theirs = store.load(slot);
        if theirs.as_ref().is_some_and(|t| t.name != mine.name) {
            // Someone else has the slot now; the copy moves to a free one.
            match (0..slots).find(|&s| store.load(s).is_none()) {
                Some(s) => {
                    mine.slot = s;
                    theirs = None;
                }
                None => {
                    eprintln!(
                        "profile '{}' has no free slot; left in {}",
                        mine.name,
                        dir.display()
                    );
                    continue;
                }
            }
        }
        let mut merged = match theirs {
            None => mine,
            Some(t) => {
                let newer = |a: &Path| std::fs::metadata(a).and_then(|m| m.modified()).ok();
                let mine_newer = match (newer(&own), game_file.as_deref().and_then(newer)) {
                    (Some(a), Some(b)) => a > b,
                    _ => false,
                };
                Profile::merge(t, &mine, mine_newer)
            }
        };
        store.save(&mut merged);
        eprintln!(
            "profile '{}' brought back into {} (slot {})",
            merged.name,
            root.0.display(),
            merged.slot
        );
    }
    let mut done = dir.as_os_str().to_owned();
    done.push(".migrated");
    let done = PathBuf::from(done);
    if std::fs::rename(&dir, &done).is_ok() {
        for (name, d) in before {
            let _ = std::fs::write(done.join(format!("game-{name}")), d);
        }
    }
}

const RECORD: usize = 128 + 12 + 32 + BODIES * 56;
/// Where the first game's equipment sits after the record: past seven
/// options, its sixteen keys and the mouse reversal.
const C1_KIT_AT: usize = RECORD + 7 * 4 + 16 * 4 + 4;
/// Where the trophies start in the record.
const BODY_AT: usize = 128 + 12 + 32;

struct Rd<'a>(&'a [u8], usize);

impl Rd<'_> {
    fn i32(&mut self) -> i32 {
        let v = self
            .0
            .get(self.1..self.1 + 4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0);
        self.1 += 4;
        v
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.i32() as u32)
    }
    fn stats(&mut self) -> Stats {
        Stats {
            shots_made: self.i32(),
            success: self.i32(),
            path: self.f32(),
            time: self.f32(),
        }
    }
}

impl Profile {
    /// The rank the score has earned: 100 for advanced, 300
    /// for expert.
    pub fn earned_rank(score: i32) -> i32 {
        if score >= 300 {
            2
        } else if score >= 100 {
            1
        } else {
            0
        }
    }

    pub fn parse(data: &[u8]) -> Option<Profile> {
        if data.len() < 140 {
            return None;
        }
        let name_end = data[..128].iter().position(|&b| b == 0).unwrap_or(128);
        let name: String = data[..name_end].iter().map(|&b| b as char).collect();
        let mut r = Rd(data, 128);
        let slot = r.i32().clamp(0, SLOTS as i32 - 1) as usize;
        let score = r.i32();
        let rank = r.i32();
        let last = r.stats();
        let total = r.stats();
        let mut bodies = [TrophyItem::default(); BODIES];
        for b in bodies.iter_mut() {
            b.ctype = r.i32();
            b.weapon = r.i32();
            b.phase = r.i32();
            b.height = r.i32();
            b.weight = r.i32();
            b.score = r.i32();
            b.date = r.i32();
            b.time = r.i32();
            b.scale = r.f32();
            b.range = r.f32();
            for _ in 0..4 {
                r.i32();
            }
        }
        Some(Profile {
            name,
            slot,
            score,
            rank,
            last,
            total,
            bodies,
            kit: None,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RECORD);
        let mut name = [0u8; 128];
        for (i, c) in self.name.chars().take(127).enumerate() {
            name[i] = if (c as u32) < 256 { c as u8 } else { b'?' };
        }
        out.extend_from_slice(&name);
        let mut i = |v: i32| out.extend_from_slice(&v.to_le_bytes());
        i(self.slot as i32);
        i(self.score);
        i(Profile::earned_rank(self.score));
        for s in [self.last, self.total] {
            i(s.shots_made);
            i(s.success);
            i(s.path.to_bits() as i32);
            i(s.time.to_bits() as i32);
        }
        for b in &self.bodies {
            for v in [
                b.ctype, b.weapon, b.phase, b.height, b.weight, b.score, b.date, b.time,
            ] {
                i(v);
            }
            i(b.scale.to_bits() as i32);
            i(b.range.to_bits() as i32);
            for _ in 0..4 {
                i(0);
            }
        }
        out
    }

    /// The record written over what a file held: the file's spare words
    /// and whatever followed the record (other versions' options) are
    /// kept. A new Carnivores 1 file starts with its colour key option on,
    /// as that game's own does (in the first body's spare word).
    fn record_over(&self, kind: GameKind, old: Option<&[u8]>) -> Vec<u8> {
        let mut out = self.to_bytes();
        match old.filter(|o| o.len() >= RECORD) {
            Some(old) => {
                for i in 0..BODIES {
                    let o = BODY_AT + i * 56 + 40;
                    out[o..o + 16].copy_from_slice(&old[o..o + 16]);
                }
                out.extend_from_slice(&old[RECORD..]);
            }
            None if kind == GameKind::Carnivores => {
                let o = BODY_AT + 52;
                out[o..o + 4].copy_from_slice(&1i32.to_le_bytes());
            }
            None => {}
        }
        out
    }

    /// One hunter's two records made one: every trophy either has, the
    /// credit for the ones only `other` has, the larger totals, and the
    /// last hunt of the newer.
    pub fn merge(mut self, other: &Profile, other_newer: bool) -> Profile {
        let same = |a: &TrophyItem, b: &TrophyItem| {
            (
                a.ctype, a.weapon, a.score, a.weight, a.height, a.date, a.time,
            ) == (
                b.ctype, b.weapon, b.score, b.weight, b.height, b.date, b.time,
            )
        };
        let mut added = 0;
        for b in other.bodies.iter().filter(|b| b.ctype != 0) {
            if self.bodies.iter().any(|a| same(a, b)) {
                continue;
            }
            if self.add_trophy(*b).is_some() {
                added += b.score;
            }
        }
        self.score = (self.score + added).max(other.score);
        self.rank = Profile::earned_rank(self.score);
        let t = &mut self.total;
        t.shots_made = t.shots_made.max(other.total.shots_made);
        t.success = t.success.max(other.total.success);
        t.path = t.path.max(other.total.path);
        t.time = t.time.max(other.total.time);
        if other_newer {
            self.last = other.last;
        }
        self
    }

    /// Puts a trophy on the first free stand (23 of the 24).
    pub fn add_trophy(&mut self, t: TrophyItem) -> Option<usize> {
        let i = self.bodies.iter().take(23).position(|b| b.ctype == 0)?;
        self.bodies[i] = t;
        Some(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut p = Profile {
            name: "Hunter".into(),
            slot: 3,
            score: 150,
            ..Default::default()
        };
        p.last = Stats {
            shots_made: 4,
            success: 2,
            path: 812.5,
            time: 300.0,
        };
        p.bodies[0] = TrophyItem {
            ctype: 7,
            weapon: 2,
            score: 12,
            scale: 1.1,
            range: 40.5,
            date: (2026 << 20) | (9 << 10) | 27,
            ..Default::default()
        };
        let b = p.to_bytes();
        assert_eq!(b.len(), RECORD);
        let q = Profile::parse(&b).unwrap();
        assert_eq!(q.name, "Hunter");
        assert_eq!(q.slot, 3);
        assert_eq!(q.rank, 1);
        assert_eq!(q.last, p.last);
        assert_eq!(q.bodies[0], p.bodies[0]);
    }

    fn scratch(name: &str) -> DataRoot {
        let d = std::env::temp_dir().join(format!("carn-profile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        DataRoot(d)
    }

    #[test]
    fn saving_keeps_the_options_after_the_record() {
        let root = scratch("tail");
        // A file with options after the record.
        let mut p = Profile {
            name: "Hunter".into(),
            score: 20,
            ..Default::default()
        };
        let mut file = p.to_bytes();
        let o = BODY_AT + 3 * 56 + 44;
        file[o..o + 4].copy_from_slice(&7i32.to_le_bytes());
        let tail: Vec<u8> = (0..160u8).collect();
        file.extend_from_slice(&tail);
        std::fs::write(root.0.join("TROPHY00.SAV"), &file).unwrap();

        let store = Store::new(GameKind::Carnivores2, &root);
        let mut q = store.load(0).unwrap();
        assert_eq!(q.score, 20);
        q.score = 120;
        q.add_trophy(TrophyItem {
            ctype: 9,
            score: 100,
            ..Default::default()
        });
        store.save(&mut q);
        let back = std::fs::read(root.0.join("TROPHY00.SAV")).unwrap();
        assert_eq!(back.len(), RECORD + tail.len());
        assert_eq!(&back[RECORD..], &tail[..], "options kept");
        assert_eq!(&back[o..o + 4], &7i32.to_le_bytes(), "spare words kept");
        let r = store.load(0).unwrap();
        assert_eq!((r.score, r.rank, r.bodies[0].ctype), (120, 1, 9));

        store.delete(0);
        assert!(store.load(0).is_none());
        assert!(!root.0.join("TROPHY00.SAV").exists());
        let _ = std::fs::remove_dir_all(&root.0);

        // A first hunter's file, with no other to take options from, is the
        // record alone; Carnivores 1 marks its colour key on.
        let root = scratch("alone");
        p.slot = 2;
        Store::new(GameKind::Carnivores, &root).save(&mut p);
        let new = std::fs::read(root.0.join("trophy02.sav")).unwrap();
        assert_eq!(new.len(), RECORD);
        assert_eq!(&new[BODY_AT + 52..BODY_AT + 56], &1i32.to_le_bytes());
        let _ = std::fs::remove_dir_all(&root.0);
    }

    #[test]
    fn the_first_games_equipment_goes_with_the_hunter() {
        let root = scratch("kit");
        let store = Store::new(GameKind::Carnivores, &root);
        // A hunter saved with 1676 - 1516 bytes of options after the
        // record, the equipment among them.
        let mut file = Profile {
            name: "Old".into(),
            ..Default::default()
        }
        .to_bytes();
        file.resize(1676, 0);
        file[RECORD + 12..RECORD + 16].copy_from_slice(&5i32.to_le_bytes());
        file[C1_KIT_AT + 8..C1_KIT_AT + 12].copy_from_slice(&1i32.to_le_bytes());
        std::fs::write(root.0.join("trophy00.sav"), &file).unwrap();
        let mut old = store.load(0).unwrap();
        let radar_only = Kit {
            radar: true,
            ..Default::default()
        };
        assert_eq!(old.kit, Some(radar_only));
        old.kit = Some(Kit {
            scent: true,
            tranq: true,
            ..Default::default()
        });
        store.save(&mut old);
        let k = store.load(0).unwrap().kit.unwrap();
        assert!(k.scent && k.tranq && !k.radar && !k.camo);
        assert_eq!(
            std::fs::read(root.0.join("trophy00.sav")).unwrap().len(),
            1676
        );

        // A new one takes those options, at resolution 3, with its own kit.
        let mut new = Profile {
            name: "New".into(),
            slot: 1,
            kit: Some(Kit::default()),
            ..Default::default()
        };
        store.save(&mut new);
        let d = std::fs::read(root.0.join("trophy01.sav")).unwrap();
        assert_eq!(d.len(), 1676);
        assert_eq!(&d[RECORD + 12..RECORD + 16], &3i32.to_le_bytes());
        assert_eq!(store.load(1).unwrap().kit, Some(Kit::default()));
        let _ = std::fs::remove_dir_all(&root.0);
    }

    #[test]
    fn merging_keeps_every_trophy_and_the_later_hunt() {
        let t = |ctype, score| TrophyItem {
            ctype,
            score,
            date: 5,
            ..Default::default()
        };
        let mut game = Profile {
            name: "Hunter".into(),
            score: 6,
            ..Default::default()
        };
        game.bodies[0] = t(6, 6);
        game.total = Stats {
            shots_made: 45,
            success: 1,
            path: 1261.0,
            time: 798.0,
        };
        game.last.shots_made = 1;
        let mut mine = game.clone();
        mine.bodies[1] = t(4, 3);
        mine.score = 9;
        mine.total = Stats {
            shots_made: 62,
            success: 2,
            path: 1573.0,
            time: 936.0,
        };
        mine.last.shots_made = 6;
        let m = game.clone().merge(&mine, false);
        assert_eq!(m.score, 9);
        assert_eq!(m.bodies[1].ctype, 4);
        assert_eq!(m.total, mine.total);
        assert_eq!(m.last.shots_made, 1, "the game's own hunt was later");
        let m = game.merge(&mine, true);
        assert_eq!(m.last.shots_made, 6);
    }
}

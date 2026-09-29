//! The player's options, kept in a small text file per game.
//!
//! The originals keep them in the profile (trophyNN.sav) inside the game
//! folder. The rewrite keeps its own under the user's configuration folder,
//! so it can be played beside other versions of the games without either
//! rewriting the other's options.

use bevy::prelude::*;
use std::path::PathBuf;

use crate::game::GameKind;
use crate::keymap::{Binding, KeyMap, ACTS};

/// The frame-limit steps offered; 0 is off.
pub const FRAME_LIMITS: [u32; 11] = [0, 30, 60, 75, 100, 120, 144, 165, 200, 240, 300];
pub const FOV_MIN: i32 = 50;
pub const FOV_MAX: i32 = 120;

#[derive(Clone, Debug, Resource)]
pub struct Settings {
    /// Vertical field of view in degrees. The original projection is
    /// 2*atan(0.6) = 61.93.
    pub fov: i32,
    /// Frames per second cap, 0 for none.
    pub fps_limit: u32,
    pub vsync: bool,
    /// Anti-aliasing samples: 0 (off), 2, 4 or 8.
    pub msaa: u32,
    /// 0..255; the view radius is 42 + (view_range / 8) * 2 cells.
    pub view_range: i32,
    pub show_fps: bool,
    pub fullscreen: bool,
    /// Window size when not full screen.
    pub window_w: u32,
    pub window_h: u32,
    /// 0..255, 128 as authored (Carnivores 2 and Ice Age).
    pub brightness: i32,
    /// 0 dawn, 1 day, 2 night (Carnivores 2 and Ice Age). Not kept: the
    /// hunt screen sets it, and each start is day again (`dtm=` on the
    /// command line picks another).
    pub day_night: i32,
    /// 0..255
    pub mouse_sens: i32,
    pub mouse_invert: bool,
    /// 0 low, 1 normal, 2 high, 3 best (trilinear and anisotropic).
    pub textures: i32,
    /// Moving cloud shadows on the ground.
    pub clouds: bool,
    /// Creatures' shadows on the ground.
    pub shadows: bool,
    /// 0..255
    pub volume: i32,
    pub sound: bool,
    /// Run and crouch: hold the key (true) or press to toggle.
    pub run_hold: bool,
    pub crouch_hold: bool,
    /// Imperial units in the HUD.
    pub imperial: bool,
    /// The creatures: how aggressive, how many, how keen their senses,
    /// 0..255 with 128 as the games shipped (the first game: 0..2).
    pub agres: i32,
    pub dens: i32,
    pub sens: i32,
    /// Ground fogs.
    pub fog: bool,
    /// Far objects drawn as flat sprites, as the games shipped (Carnivores
    /// 2 and Ice Age), and from how many cells away.
    pub obj_lod: bool,
    pub obj_lod_dist: i32,
    /// Radar marks: a larger, clearer marker (true), or each game's
    /// own small dot.
    pub radar_clear: bool,
    /// The developer keys (Shift+F fog, Shift+C cloud shadows, Shift+L fly,
    /// Shift+M models, F9 leave at once, F10 where you are).
    pub debug_keys: bool,
    pub keys: KeyMap,
}

impl Settings {
    pub fn defaults(kind: GameKind) -> Settings {
        Settings {
            fov: 62,
            fps_limit: 0,
            vsync: true,
            msaa: 0,
            view_range: 128,
            show_fps: false,
            fullscreen: true,
            window_w: 1280,
            window_h: 800,
            brightness: 128,
            day_night: 1,
            mouse_sens: 128,
            mouse_invert: false,
            textures: 3,
            clouds: true,
            shadows: true,
            volume: 200,
            sound: true,
            run_hold: false,
            // Ice Age crouched on a toggle; the other two while held.
            crouch_hold: kind != GameKind::IceAge,
            imperial: true,
            agres: if kind == GameKind::Carnivores { 1 } else { 128 },
            dens: if kind == GameKind::Carnivores { 1 } else { 128 },
            sens: if kind == GameKind::Carnivores { 1 } else { 128 },
            fog: true,
            obj_lod: true,
            obj_lod_dist: 24,
            radar_clear: true,
            debug_keys: false,
            keys: KeyMap::default(),
        }
    }

    /// Where far objects turn into sprites, in world units; 0 for never
    /// (the first game had no sprites).
    pub fn object_lod(&self, kind: GameKind) -> f32 {
        if self.obj_lod && kind != GameKind::Carnivores {
            self.obj_lod_dist as f32 * 256.0
        } else {
            0.0
        }
    }

    /// View radius in cells.
    pub fn view_radius(&self) -> i32 {
        42 + (self.view_range.clamp(0, 255) / 8) * 2
    }

    pub fn path(kind: GameKind) -> Option<PathBuf> {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }?;
        Some(
            base.join("carnivores-rs")
                .join(format!("{}.cfg", kind.id())),
        )
    }

    pub fn load(kind: GameKind) -> Settings {
        let mut s = Settings::defaults(kind);
        let Some(p) = Self::path(kind) else { return s };
        let Ok(text) = std::fs::read_to_string(&p) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            s.set(k.trim(), v.trim());
        }
        // Earlier builds kept the time of day; the games did not.
        s.day_night = 1;
        s.clamp();
        s
    }

    pub fn save(&self, kind: GameKind) {
        let Some(p) = Self::path(kind) else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let b = |v: bool| if v { 1 } else { 0 };
        let text = format!(
            "# {} (Rust rewrite) options\n\
             fov={}\nfps_limit={}\nvsync={}\nmsaa={}\nview_range={}\nshow_fps={}\n\
             fullscreen={}\nwindow_w={}\nwindow_h={}\nbrightness={}\n\
             mouse_sens={}\nmouse_invert={}\ntextures={}\nclouds={}\nshadows={}\nvolume={}\nsound={}\n\
             run_hold={}\ncrouch_hold={}\nimperial={}\nagres={}\ndens={}\nsens={}\nfog={}\nobj_lod={}\nobj_lod_dist={}\nradar_clear={}\ndebug_keys={}\n{}",
            kind.title(),
            self.fov,
            self.fps_limit,
            b(self.vsync),
            self.msaa,
            self.view_range,
            b(self.show_fps),
            b(self.fullscreen),
            self.window_w,
            self.window_h,
            self.brightness,
            self.mouse_sens,
            b(self.mouse_invert),
            self.textures,
            b(self.clouds),
            b(self.shadows),
            self.volume,
            b(self.sound),
            b(self.run_hold),
            b(self.crouch_hold),
            b(self.imperial),
            self.agres,
            self.dens,
            self.sens,
            b(self.fog),
            b(self.obj_lod),
            self.obj_lod_dist,
            b(self.radar_clear),
            b(self.debug_keys),
            ACTS.iter()
                .zip(self.keys.0.iter())
                .map(|(a, k)| format!("key_{}={}\n", a.key(), k.name()))
                .collect::<String>(),
        );
        let _ = std::fs::write(&p, text);
    }

    /// The options a game folder's own profile carries (the
    /// originals' settings after the record, and any extended ones after
    /// those), for a first start with no options file of our own. Reads
    /// the first profile there is; true if one was found.
    pub fn import_save(&mut self, kind: GameKind, root: &crate::paths::DataRoot) -> bool {
        let Some(data) = (0..8).find_map(|s| {
            root.find(&format!("trophy0{s}.sav"))
                .and_then(|p| std::fs::read(p).ok())
        }) else {
            return false;
        };
        let c1 = kind == GameKind::Carnivores;
        let mut at = 128 + 12 + 32 + 24 * 56;
        let mut next = || {
            let v = data
                .get(at..at + 4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            at += 4;
            v
        };
        let (agres, dens, sens) = (next(), next(), next());
        let _res = next();
        let (fog, text) = (next(), next());
        let view_r = if c1 { None } else { next() };
        let shadows = next();
        let (ms_sens, bright) = if c1 { (None, None) } else { (next(), next()) };
        let nkeys = match kind {
            GameKind::Carnivores => 16,
            GameKind::IceAge => 18,
            _ => 17,
        };
        let keys: Vec<Option<i32>> = (0..nkeys).map(|_| next()).collect();
        let reverse = next();
        let (_scent, _camo, _radar, _tranq) = (next(), next(), next(), next());
        if !c1 {
            let _alpha = next();
        }
        let sys = next();
        if !c1 {
            let (_sound, _render) = (next(), next());
        }
        // The extended options, in their order (the first game's differs).
        let (vsync, fps, debug) = (next(), next(), next());
        let (blips, fov) = if c1 {
            let f = next();
            (next(), f)
        } else {
            (next(), next())
        };
        let full = next();
        let ms_sens = if c1 { next() } else { ms_sens };
        let (sprint, crouch, msaa) = (next(), next(), next());
        let (obj_lod, obj_dist) = if c1 {
            (None, None)
        } else {
            let (_tlod, _tdist) = (next(), next());
            (next(), next())
        };
        let show_fps = next();

        let b = |v: i32| v != 0;
        if let Some(v) = agres {
            self.agres = v;
        }
        if let Some(v) = dens {
            self.dens = v;
        }
        if let Some(v) = sens {
            self.sens = v;
        }
        if let Some(v) = fog {
            self.fog = b(v);
        }
        // Stored as Low, High, Auto, Best; onto Low, Normal, High, Best.
        if let Some(v) = text {
            self.textures = [0, 2, 2, 3][v.clamp(0, 3) as usize];
        }
        if let Some(v) = view_r {
            self.view_range = v;
        }
        if let Some(v) = shadows {
            self.shadows = b(v);
        }
        if let Some(v) = ms_sens {
            self.mouse_sens = v;
        }
        if let Some(v) = bright {
            self.brightness = v;
        }
        for (a, k) in crate::keymap::ACTS
            .iter()
            .filter(|a| !c1 || **a != crate::keymap::Act::ChangeCall)
            .zip(&keys)
        {
            if let Some(k) = k {
                self.keys.set(*a, crate::keymap::Binding::from_vk(*k));
            }
        }
        if let Some(v) = reverse {
            self.mouse_invert = b(v);
        }
        if let Some(v) = sys {
            self.imperial = b(v);
        }
        if let Some(v) = vsync {
            self.vsync = b(v);
        }
        if let Some(v) = fps {
            self.fps_limit = FRAME_LIMITS.get(v.max(0) as usize).copied().unwrap_or(0);
        }
        if let Some(v) = debug {
            self.debug_keys = b(v);
        }
        if let Some(v) = blips {
            self.radar_clear = b(v);
        }
        if let Some(v) = fov {
            self.fov = v;
        }
        if let Some(v) = full {
            self.fullscreen = b(v);
        }
        if let Some(v) = sprint {
            self.run_hold = v == 1;
        }
        if let Some(v) = crouch {
            self.crouch_hold = v == 1;
        }
        if let Some(v) = msaa {
            self.msaa = [0, 2, 4, 8][v.clamp(0, 3) as usize];
        }
        if let Some(v) = obj_lod {
            self.obj_lod = b(v);
        }
        if let Some(v) = obj_dist {
            self.obj_lod_dist = v;
        }
        if let Some(v) = show_fps {
            self.show_fps = b(v);
        }
        self.clamp();
        if c1 {
            self.agres = self.agres.clamp(0, 2);
            self.dens = self.dens.clamp(0, 2);
            self.sens = self.sens.clamp(0, 2);
        }
        true
    }

    /// Sets one option from its text form, as in the file or `key=value`
    /// on the command line. Unknown keys are ignored.
    pub fn set(&mut self, k: &str, v: &str) -> bool {
        let i = |v: &str| v.parse::<i64>().unwrap_or(0);
        let bl = |v: &str| matches!(v, "1" | "true" | "on" | "yes");
        match k {
            "fov" => self.fov = i(v) as i32,
            "fps_limit" | "fps" => self.fps_limit = i(v).max(0) as u32,
            "vsync" => self.vsync = bl(v),
            "msaa" => self.msaa = i(v) as u32,
            "view_range" => self.view_range = i(v) as i32,
            "show_fps" => self.show_fps = bl(v),
            "fullscreen" => self.fullscreen = bl(v),
            "window_w" => self.window_w = i(v) as u32,
            "window_h" => self.window_h = i(v) as u32,
            "brightness" => self.brightness = i(v) as i32,
            "day_night" | "dtm" => self.day_night = i(v) as i32,
            "mouse_sens" => self.mouse_sens = i(v) as i32,
            "mouse_invert" => self.mouse_invert = bl(v),
            "textures" => self.textures = i(v) as i32,
            "clouds" => self.clouds = bl(v),
            "shadows" => self.shadows = bl(v),
            "volume" => self.volume = i(v) as i32,
            "sound" => self.sound = bl(v),
            "run_hold" => self.run_hold = bl(v),
            "crouch_hold" => self.crouch_hold = bl(v),
            "imperial" => self.imperial = bl(v),
            "agres" => self.agres = i(v) as i32,
            "dens" => self.dens = i(v) as i32,
            "sens" => self.sens = i(v) as i32,
            "fog" => self.fog = bl(v),
            "obj_lod" => self.obj_lod = bl(v),
            "obj_lod_dist" => self.obj_lod_dist = i(v) as i32,
            "radar_clear" => self.radar_clear = bl(v),
            "debug_keys" => self.debug_keys = bl(v),
            _ => {
                let Some(a) = k
                    .strip_prefix("key_")
                    .and_then(|n| ACTS.iter().find(|a| a.key() == n))
                else {
                    return false;
                };
                self.keys.set(*a, Binding::parse(v));
            }
        }
        true
    }

    pub fn clamp(&mut self) {
        self.fov = self.fov.clamp(FOV_MIN, FOV_MAX);
        self.msaa = match self.msaa {
            0 | 1 => 0,
            2 | 3 => 2,
            4..=7 => 4,
            _ => 8,
        };
        self.view_range = self.view_range.clamp(0, 255);
        self.window_w = self.window_w.clamp(320, 16384);
        self.window_h = self.window_h.clamp(240, 16384);
        self.brightness = self.brightness.clamp(0, 255);
        self.day_night = self.day_night.clamp(0, 2);
        self.mouse_sens = self.mouse_sens.clamp(0, 255);
        self.textures = self.textures.clamp(0, 3);
        self.volume = self.volume.clamp(0, 255);
        self.fps_limit = self.fps_limit.min(1000);
        self.agres = self.agres.clamp(0, 255);
        self.dens = self.dens.clamp(0, 255);
        self.sens = self.sens.clamp(0, 255);
        self.obj_lod_dist = self.obj_lod_dist.clamp(12, 64);
    }
}

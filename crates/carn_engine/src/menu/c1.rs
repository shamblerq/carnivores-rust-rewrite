//! The first game's menus: profiles, the licence, the
//! main screen, statistics, and a hunt chosen in two steps - the location
//! and the kind of creature on one screen, the weapon and the equipment on
//! the next - with the further areas, the larger creatures and the sniper
//! rifle kept for the higher ranks. Options, credits and the quit prompt as
//! in its successors.

use std::collections::HashMap;
use std::sync::Arc;

use super::{load_pic, Align, Art, Face, Frame, HuntSetup, In, Outcome, Pic};
use crate::game::GameKind;
use crate::keymap::{Act, Binding};
use crate::paths::DataRoot;
use crate::profile::{Kit, Profile, Store};
use crate::settings::{Settings, FOV_MAX, FOV_MIN, FRAME_LIMITS};

/// Profile slots the first game offered.
const SLOTS: usize = 6;

// COLORREF 0x00BBGGRR, as the original wrote them.
const fn cref(c: u32) -> [u8; 3] {
    [
        (c & 0xFF) as u8,
        ((c >> 8) & 0xFF) as u8,
        ((c >> 16) & 0xFF) as u8,
    ]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Register,
    Delete,
    License,
    Main,
    Location,
    Weapon,
    Options,
    Credits,
    Quit,
    Stats,
}

impl Page {
    fn art(self) -> &'static str {
        match self {
            Page::Register => "register",
            Page::Delete => "delete",
            Page::License => "waiver",
            Page::Main => "main",
            Page::Location => "location",
            Page::Weapon => "weapons",
            Page::Options => "options",
            Page::Credits => "credits",
            Page::Quit => "quit",
            Page::Stats => "stats",
        }
    }
}

/// A 24-bit Windows bitmap (the location pictures), bottom row first.
fn load_bmp(root: &DataRoot, rel: &str) -> Option<Pic> {
    let d = std::fs::read(root.find(rel)?).ok()?;
    if d.len() < 54 || &d[..2] != b"BM" {
        return None;
    }
    let u32at = |i: usize| u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]);
    let off = u32at(10) as usize;
    let (w, h) = (u32at(18) as i32, u32at(22) as i32);
    if u16::from_le_bytes([d[28], d[29]]) != 24 || w <= 0 || h <= 0 {
        return None;
    }
    let stride = ((w * 3 + 3) & !3) as usize;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h as usize {
        let src = off + (h as usize - 1 - y) * stride;
        for x in 0..w as usize {
            let s = src + x * 3;
            let px = d.get(s..s + 3)?;
            let o = (y * w as usize + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Some(Pic {
        w,
        h,
        rgba: Arc::new(rgba),
    })
}

fn text_lines(root: &DataRoot, rel: &str) -> Vec<String> {
    let Some(p) = root.find(rel) else {
        return Vec::new();
    };
    let Ok(b) = std::fs::read(p) else {
        return Vec::new();
    };
    b.split(|&c| c == b'\n')
        .map(|l| {
            l.iter()
                .filter(|&&c| c != b'\r')
                .map(|&c| c as char)
                .collect()
        })
        .collect()
}

/// The options: three panels of rows, as the game lays them out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Agres,
    Dens,
    Sens,
    Fov,
    Units,
    Blips,
    Debug,
    ShowFps,
    Res,
    Full,
    VSync,
    Fps,
    Shadows,
    Fog,
    Textures,
    Msaa,
    Key(Act),
    RunMode,
    CrouchMode,
    Reverse,
    MouseSens,
}

const OPT_ROW_H: i32 = 21;
const MSSENS_X: i32 = 470;
const MSSENS_Y: i32 = 558;
const MSSENS_W: i32 = 258;
const SLIDER_W: i32 = 118;

/// A panel: where its labels end, its first row, and its rows.
type Panel = (i32, i32, Vec<(&'static str, Row)>);

fn panels() -> [Panel; 3] {
    let game = vec![
        ("Agressivity", Row::Agres),
        ("Density", Row::Dens),
        ("Sensitivity", Row::Sens),
        ("Field of view", Row::Fov),
        ("Measurement", Row::Units),
        ("Radar blips", Row::Blips),
        ("Debug keys", Row::Debug),
        ("Show FPS", Row::ShowFps),
    ];
    let mut ctrl: Vec<(&'static str, Row)> = [
        Act::Forward,
        Act::Backward,
        Act::TurnUp,
        Act::TurnDown,
        Act::TurnLeft,
        Act::TurnRight,
        Act::Fire,
        Act::GetWeapon,
        Act::StepLeft,
        Act::StepRight,
        Act::Strafe,
        Act::Jump,
        Act::Run,
        Act::Crouch,
        Act::Call,
        Act::Binoculars,
    ]
    .iter()
    .map(|a| (a.label(), Row::Key(*a)))
    .collect();
    ctrl.push(("Run mode", Row::RunMode));
    ctrl.push(("Crouch mode", Row::CrouchMode));
    ctrl.push(("Reverse mouse", Row::Reverse));
    ctrl.push(("Mouse sensitivity", Row::MouseSens));
    let video = vec![
        ("Resolution", Row::Res),
        ("Full screen", Row::Full),
        ("V-Sync", Row::VSync),
        ("Frame limit", Row::Fps),
        ("Shadows", Row::Shadows),
        ("Fog", Row::Fog),
        ("Textures", Row::Textures),
        ("Anti-aliasing", Row::Msaa),
    ];
    [(200, 84, game), (610, 90, ctrl), (200, 350, video)]
}

const RESOLUTIONS: &[(u32, u32)] = &[
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1280, 1024),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1080),
    (2560, 1440),
    (3440, 1440),
    (3840, 2160),
];

fn row_y(p: &Panel, l: usize) -> i32 {
    if p.2[l].1 == Row::MouseSens {
        MSSENS_Y
    } else {
        p.1 + l as i32 * OPT_ROW_H
    }
}

fn row_x0(p: &Panel, l: usize) -> i32 {
    if p.2[l].1 == Row::MouseSens {
        MSSENS_X - 16
    } else {
        p.0
    }
}

pub struct Menu {
    page: Page,
    entered: Option<Page>,
    store: Store,
    profile: Option<Profile>,
    profiles: Vec<Option<Profile>>,
    cur: Option<usize>,
    typing: String,
    area: usize,
    dino: usize,
    weapon: usize,
    observer: bool,
    tranq: bool,
    scent: bool,
    camo: bool,
    radar: bool,
    opt_mode: usize,
    opt_line: usize,
    wait_key: Option<Act>,
    drag: Option<(usize, usize)>,
    pics: HashMap<String, Option<Pic>>,
    texts: HashMap<String, Vec<String>>,
    dinos: Vec<crate::sim::DinoInfo>,
    weapons: Vec<crate::sim::script::WeapInfo>,
}

/// The location and weapon screens' choices. The game keeps them in its
/// state (area, kind, weapon, observer mode and the equipment), so a hunt and a return leave them as they were.
#[derive(Clone, Copy, Debug)]
pub struct Choices {
    area: usize,
    dino: usize,
    weapon: usize,
    observer: bool,
    kit: Kit,
}

impl Menu {
    pub fn new(root: &DataRoot, _settings: &Settings, profile: Option<Profile>) -> Menu {
        Menu {
            page: if profile.is_some() {
                Page::Main
            } else {
                Page::Register
            },
            entered: None,
            store: Store::new(GameKind::Carnivores, root),
            profile,
            profiles: Vec::new(),
            cur: None,
            typing: String::new(),
            area: 0,
            dino: 0,
            weapon: 0,
            observer: false,
            tranq: false,
            scent: false,
            camo: false,
            radar: false,
            opt_mode: 0,
            opt_line: 0,
            wait_key: None,
            drag: None,
            pics: HashMap::new(),
            texts: HashMap::new(),
            dinos: crate::sim::c1::dinos(),
            weapons: crate::weapons::c1_weapons(),
        }
    }

    fn pic(&mut self, root: &DataRoot, rel: &str) -> Option<Pic> {
        self.pics
            .entry(rel.to_string())
            .or_insert_with(|| {
                if rel.to_ascii_uppercase().ends_with(".BMP") {
                    load_bmp(root, rel)
                } else {
                    load_pic(root, rel)
                }
            })
            .clone()
    }

    fn text(&mut self, root: &DataRoot, rel: &str) -> Vec<String> {
        self.texts
            .entry(rel.to_string())
            .or_insert_with(|| text_lines(root, rel))
            .clone()
    }

    fn rank(&self) -> i32 {
        self.profile
            .as_ref()
            .map(|p| Profile::earned_rank(p.score))
            .unwrap_or(0)
    }

    /// What the rank opens up: the last kind and the
    /// last area that may be picked.
    fn limits(&self) -> (usize, usize) {
        match self.rank() {
            0 => (3, 2),
            1 => (5, 4),
            _ => (6, 5),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        i: &In,
        art: Option<&Art>,
        art_key: &str,
        root: &DataRoot,
        settings: &mut Settings,
        current: &mut super::CurrentProfile,
    ) -> (String, Frame, Outcome) {
        let want = self.page.art().to_string();
        if art_key != want {
            return (want, Frame::default(), Outcome::Stay);
        }
        if self.entered != Some(self.page) {
            self.entered = Some(self.page);
            if self.page == Page::Register {
                self.profiles = self.store.scan(SLOTS);
                // The name field holds the current hunter's name, as the
                // original's showed the profile it had loaded.
                if self.typing.is_empty() {
                    if let Some((s, p)) = self
                        .profiles
                        .iter()
                        .enumerate()
                        .find_map(|(s, p)| p.as_ref().map(|p| (s, p)))
                    {
                        self.cur = Some(s);
                        self.typing = p.name.clone();
                    }
                }
            }
            if self.page == Page::Options {
                self.wait_key = None;
                self.drag = None;
            }
        }
        let hover = art.map(|a| a.id(i.x, i.y)).unwrap_or(0);
        let mut f = Frame::default();
        if hover != 0 {
            f.lit.push(hover);
        }
        let mut out = Outcome::Stay;
        let stats_c = cref(0x003070A0);
        match self.page {
            Page::Register => self.register(i, hover, &mut f, current),
            Page::Delete => {
                if i.click && hover == 1 {
                    if let Some(s) = self.cur {
                        self.store.delete(s);
                    }
                    self.page = Page::Register;
                }
                if (i.click && hover == 2) || i.esc {
                    self.page = Page::Register;
                }
                let c = cref(0x00B08030);
                let name = self
                    .cur
                    .and_then(|s| self.profiles.get(s).cloned().flatten())
                    .map(|p| p.name)
                    .unwrap_or_default();
                f.text(
                    Face::Big,
                    290,
                    370,
                    "Do you want to delete player",
                    c,
                    Align::Left,
                );
                f.text(Face::Big, 300, 394, format!("'{name}' ?"), c, Align::Left);
            }
            Page::License => {
                if i.click && hover == 1 {
                    self.page = Page::Main;
                }
                if i.click && hover == 2 {
                    if let Some(p) = &self.profile {
                        self.store.delete(p.slot);
                    }
                    self.profile = None;
                    current.0 = None;
                    self.page = Page::Register;
                }
            }
            Page::Main => {
                if i.click {
                    match hover {
                        1 => self.page = Page::Location,
                        2 => {
                            self.page = Page::Options;
                            self.opt_mode = 0;
                            self.opt_line = 0;
                        }
                        3 => {
                            out = Outcome::Hunt(HuntSetup {
                                project: "HUNTDAT/AREAS/trophy".into(),
                                trophy: true,
                                ..Default::default()
                            });
                        }
                        4 => self.page = Page::Credits,
                        5 => self.page = Page::Quit,
                        6 => self.page = Page::Stats,
                        _ => {}
                    }
                }
                if i.esc {
                    self.page = Page::Quit;
                }
                self.main_stats(&mut f, stats_c);
            }
            Page::Stats => {
                if i.click || i.esc || i.enter {
                    self.page = Page::Main;
                }
                self.stats(&mut f, settings.imperial);
                self.main_stats(&mut f, stats_c);
            }
            Page::Credits => {
                if i.click || i.esc || i.enter {
                    self.page = Page::Main;
                }
            }
            Page::Quit => {
                if (i.click && hover == 1) || i.enter {
                    out = Outcome::Quit;
                }
                if (i.click && hover == 2) || i.esc {
                    self.page = Page::Main;
                }
                self.main_stats(&mut f, stats_c);
            }
            Page::Location => self.location(i, hover, &mut f, root, settings),
            Page::Weapon => {
                if let Some(o) = self.weapon_page(i, hover, &mut f, root) {
                    out = o;
                }
            }
            Page::Options => {
                let was_waiting = self.wait_key.is_some();
                self.options(i, hover, &mut f, settings);
                if (i.click && hover == 4) || (i.esc && !was_waiting) {
                    settings.save(GameKind::Carnivores);
                    self.page = Page::Main;
                }
            }
        }
        if matches!(out, Outcome::Hunt(_)) {
            // The profile is saved as the hunt starts, the equipment with it.
            let kit = self.kit();
            if let Some(p) = &mut self.profile {
                p.kit = Some(kit);
                self.store.save(p);
                current.0 = Some(p.clone());
            }
        }
        (want, f, out)
    }

    fn register(&mut self, i: &In, hover: u8, f: &mut Frame, current: &mut super::CurrentProfile) {
        for c in &i.chars {
            if self.typing.chars().count() < 19 && c.is_ascii() {
                self.typing.push(*c);
            }
        }
        if i.backspace {
            self.typing.pop();
        }
        const X: i32 = 320;
        const Y: i32 = 370;
        let mut ok = i.enter || (i.click && hover == 1);
        if i.click {
            for s in 0..SLOTS {
                let y = Y + s as i32 * 20;
                if i.x > X && i.x < X + 200 && i.y > y && i.y < y + 18 {
                    if let Some(p) = self.profiles.get(s).cloned().flatten() {
                        if self.cur == Some(s) {
                            ok = true;
                        }
                        self.cur = Some(s);
                        self.typing = p.name;
                    }
                }
            }
        }
        if i.click
            && hover == 2
            && self
                .cur
                .and_then(|s| self.profiles.get(s).cloned().flatten())
                .is_some()
        {
            self.page = Page::Delete;
        } else if ok && !self.typing.trim().is_empty() {
            // The name's own profile, or a new one in the
            // first free slot.
            let name = self.typing.trim().to_string();
            let found = self
                .profiles
                .iter()
                .position(|p| p.as_ref().map(|p| p.name == name).unwrap_or(false));
            if let Some(s) = found {
                let p = self.profiles[s].clone().expect("found");
                // The equipment the hunter last went out with.
                if let Some(k) = p.kit {
                    self.set_kit(k);
                }
                self.profile = Some(p.clone());
                current.0 = Some(p);
                self.page = Page::Main;
            } else if let Some(s) = self.profiles.iter().position(|p| p.is_none()) {
                // A new hunter starts without radar, scent or camouflage
                // (the tranquilizer stays as it was).
                self.radar = false;
                self.scent = false;
                self.camo = false;
                let mut p = Profile {
                    name,
                    slot: s,
                    kit: Some(self.kit()),
                    ..Default::default()
                };
                self.store.save(&mut p);
                self.profile = Some(p.clone());
                current.0 = Some(p);
                self.page = Page::License;
            }
        }
        let mut typed = self.typing.clone();
        if i.time_ms % 800 > 300 {
            typed.push('_');
        }
        f.text(Face::Big, 330, 326, typed, cref(0x00309070), Align::Left);
        for s in 0..SLOTS {
            let y = Y + s as i32 * 20;
            match self.profiles.get(s).and_then(|p| p.as_ref()) {
                None => f.text(Face::Big, X, y, "...", cref(0x003070A0), Align::Left),
                Some(p) => {
                    let c = if self.cur == Some(s) {
                        cref(0x005090E0)
                    } else {
                        cref(0x003070A0)
                    };
                    f.text(Face::Big, X, y, p.name.clone(), c, Align::Left);
                    let r =
                        ["Nov", "Adv", "Exp"][Profile::earned_rank(p.score).clamp(0, 2) as usize];
                    f.text(Face::Big, X + 146, y, r, c, Align::Left);
                }
            }
        }
    }

    fn main_stats(&self, f: &mut Frame, c: [u8; 3]) {
        let Some(p) = &self.profile else { return };
        f.text(Face::Big, 90, 9, p.name.clone(), c, Align::Left);
        f.text(Face::Big, 540, 9, p.score.to_string(), c, Align::Left);
        f.text(
            Face::Big,
            344,
            9,
            ["Novice", "Advanced", "Expert"][Profile::earned_rank(p.score).clamp(0, 2) as usize],
            c,
            Align::Left,
        );
    }

    fn stats(&self, f: &mut Frame, imperial: bool) {
        let Some(p) = &self.profile else { return };
        let c = cref(0x00209090);
        let time = |t: f32| {
            let t = t as i32;
            format!("{}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
        };
        let mut row = |y: i32, a: &str, b: String| {
            f.text(Face::Midd, 718, y, format!("{a}  "), c, Align::Right);
            f.text(Face::Midd, 718, y, b, c, Align::Left);
        };
        let l = &p.last;
        row(
            78,
            "Path travelled",
            if imperial {
                format!("{:.0} ft.", l.path / 0.3)
            } else {
                format!("{:.0} m.", l.path)
            },
        );
        row(98, "Time hunted", time(l.time));
        row(118, "Shots made", l.shots_made.to_string());
        row(138, "Shots succeed", l.success.to_string());
        let t = &p.total;
        let path = match (t.path < 1000.0, imperial) {
            (true, true) => format!("{:.0} ft.", t.path / 0.3),
            (true, false) => format!("{:.0} m.", t.path),
            (false, true) => format!("{:.1} miles.", t.path / 1667.0),
            (false, false) => format!("{:.1} km.", t.path / 1000.0),
        };
        row(208, "Path travelled", path);
        row(228, "Time hunted", time(t.time));
        row(248, "Shots made", t.shots_made.to_string());
        row(268, "Shots succeed", t.success.to_string());
        let ratio = if t.shots_made > 0 {
            t.success * 100 / t.shots_made
        } else {
            100
        };
        row(288, "Succes ratio", format!("{ratio}%"));
    }

    /// A 0..2 meter in the first game's style: a grey frame on a black
    /// shadow with quarter ticks, and a green fill.
    fn slider(f: &mut Frame, x: i32, y: i32, l: f32, w: i32) {
        let y = y + 13;
        for (o, c) in [(1, [0, 0, 0]), (0, [0x9F, 0x9F, 0x9F])] {
            let (x, y) = (x + o, y + o);
            f.rects.push((x, y - 9, w + 2, 1, c));
            f.rects.push((x, y, w + 2, 1, c));
            for tx in [x, x + w, x + w / 2, x + w / 4, x + w * 3 / 4] {
                f.rects.push((tx, y - 8, 1, 9, c));
            }
        }
        let fill = ((w - 2) as f32 * l.clamp(0.0, 2.0) / 2.0) as i32;
        f.rects.push((x + 2, y - 5, fill, 4, [0, 0, 0]));
        f.rects.push((x + 1, y - 6, fill, 4, [0x3F, 0xAF, 0x3F]));
    }

    fn location(&mut self, i: &In, hover: u8, f: &mut Frame, root: &DataRoot, settings: &Settings) {
        let (max_dino, max_area) = self.limits();
        if i.click {
            match hover {
                1 => self.area = (self.area + 1) % 6,
                2 => self.area = (self.area + 5) % 6,
                3 => self.observer = !self.observer,
                4 => self.dino = (self.dino + 1) % 7,
                5 => self.dino = (self.dino + 6) % 7,
                7 => self.page = Page::Main,
                8 if self.dino <= max_dino && self.area <= max_area => self.page = Page::Weapon,
                _ => {}
            }
        }
        if i.esc {
            self.page = Page::Main;
        }
        if self.observer {
            f.lit.push(3);
        }
        let n = self.area + 1;
        let land = if self.area > max_area {
            format!("HUNTDAT/MENU/LANDPIC/AREA{n}_NO.BMP")
        } else {
            format!("HUNTDAT/MENU/LANDPIC/AREA{n}.BMP")
        };
        if let Some(p) = self.pic(root, &land) {
            f.blits.push((p, 143, 71));
        }
        let names = ["PARA", "PACH", "STEG", "ALLO", "TRIC", "VELO", "TREX"];
        let base = names[self.dino];
        let locked = (self.dino >= 4 && max_dino < 4) || (self.dino == 6 && max_dino < 6);
        let pic = if locked {
            format!("HUNTDAT/MENU/DINOPIC/M{base}_NO.TGA")
        } else if hover == 6 {
            format!("HUNTDAT/MENU/DINOPIC/M{base}_ON.TGA")
        } else {
            format!("HUNTDAT/MENU/DINOPIC/M{base}.TGA")
        };
        if let Some(p) = self.pic(root, &pic) {
            f.blits.push((p, 401, 64));
        }
        let ext = if settings.imperial { "TXU" } else { "TXM" };
        let c = cref(0x209F85);
        for (t, l) in self
            .text(root, &format!("HUNTDAT/MENU/DINOPIC/{base}.{ext}"))
            .iter()
            .enumerate()
        {
            f.text(
                Face::Midd,
                420,
                330 + t as i32 * 16,
                l.clone(),
                c,
                Align::Left,
            );
        }
        if let Some(d) = self.dinos.get(self.dino + 4).cloned() {
            for (k, (label, v)) in [
                ("Sight", d.look_k),
                ("Scent", d.smell_k),
                ("Hearing", d.hear_k),
            ]
            .iter()
            .enumerate()
            {
                let y = 450 + k as i32 * 20;
                f.text(Face::Midd, 520, y, *label, c, Align::Right);
                Menu::slider(f, 526, y, v * 2.0, 120);
            }
        }
        let c = cref(0x809F25);
        let lines = if hover == 3 {
            self.text(root, "HUNTDAT/MENU/WEPPIC/OBSERVE.NFO")
        } else {
            self.text(root, &format!("HUNTDAT/MENU/LANDPIC/AREA{n}.TXT"))
        };
        for (t, l) in lines.iter().enumerate() {
            f.text(
                Face::Midd,
                50,
                330 + t as i32 * 16,
                l.clone(),
                c,
                Align::Left,
            );
        }
    }

    fn weapon_page(
        &mut self,
        i: &In,
        hover: u8,
        f: &mut Frame,
        root: &DataRoot,
    ) -> Option<Outcome> {
        if self.dino == 6 {
            self.tranq = false;
        }
        let mut out = None;
        if i.click {
            match hover {
                1 => self.weapon = (self.weapon + 1) % 3,
                2 => self.weapon = (self.weapon + 2) % 3,
                3 => self.tranq = !self.tranq && self.dino != 6,
                4 => self.scent = !self.scent,
                5 => self.camo = !self.camo,
                6 => self.radar = !self.radar,
                7 => self.page = Page::Location,
                8 if !(self.rank() < 1 && self.weapon > 1) => {
                    out = Some(Outcome::Hunt(self.setup()));
                }
                _ => {}
            }
        }
        if i.esc {
            self.page = Page::Location;
        }
        for (on, id) in [
            (self.tranq, 3),
            (self.scent, 4),
            (self.camo, 5),
            (self.radar, 6),
        ] {
            if on {
                f.lit.push(id);
            }
        }
        let n = self.weapon + 1;
        let pic = if n == 3 && self.rank() < 1 {
            "HUNTDAT/MENU/WEPPIC/WEAPON3A.TGA".to_string()
        } else {
            format!("HUNTDAT/MENU/WEPPIC/WEAPON{n}.TGA")
        };
        if let Some(p) = self.pic(root, &pic) {
            f.blits.push((p, 120, 120));
        }
        let c = cref(0xB09F45);
        for (t, l) in self
            .text(root, &format!("HUNTDAT/MENU/WEPPIC/WEAPON{n}.TXT"))
            .iter()
            .enumerate()
        {
            f.text(
                Face::Midd,
                60,
                330 + t as i32 * 16,
                l.clone(),
                c,
                Align::Left,
            );
        }
        if let Some(w) = self.weapons.get(self.weapon).cloned() {
            let rows = [
                ("Fire power:", w.power),
                ("Shot precision:", w.prec),
                ("Volume", 2.0 - w.loud),
                ("Rate of fire:", w.rate),
            ];
            for (k, (label, v)) in rows.iter().enumerate() {
                let y = 454 + k as i32 * 20;
                f.text(Face::Midd, 160, y, *label, c, Align::Right);
                Menu::slider(f, 166, y, *v, 120);
            }
        }
        let nfo = match hover {
            3 => Some("TRANQ"),
            4 => Some("SCENT"),
            5 => Some("CAMOFLAG"),
            6 => Some("RADAR"),
            _ => None,
        };
        if let Some(n) = nfo {
            for (t, l) in self
                .text(root, &format!("HUNTDAT/MENU/WEPPIC/{n}.NFO"))
                .iter()
                .enumerate()
            {
                f.text(
                    Face::Midd,
                    420,
                    330 + t as i32 * 16,
                    l.clone(),
                    c,
                    Align::Left,
                );
            }
        }
        out
    }

    fn kit(&self) -> Kit {
        Kit {
            scent: self.scent,
            camo: self.camo,
            radar: self.radar,
            tranq: self.tranq,
        }
    }

    fn set_kit(&mut self, k: Kit) {
        self.scent = k.scent;
        self.camo = k.camo;
        self.radar = k.radar;
        self.tranq = k.tranq;
    }

    /// What the location and weapon screens have chosen, to keep for the
    /// next visit.
    pub fn choices(&self) -> Choices {
        Choices {
            area: self.area,
            dino: self.dino,
            weapon: self.weapon,
            observer: self.observer,
            kit: self.kit(),
        }
    }

    /// The choices of the last visit, back on their screens.
    pub fn restore(&mut self, c: &Choices) {
        self.area = c.area;
        self.dino = c.dino;
        self.weapon = c.weapon;
        self.observer = c.observer;
        self.set_kit(c.kit);
    }

    fn setup(&self) -> HuntSetup {
        HuntSetup {
            project: format!("HUNTDAT/AREAS/AREA{}", self.area + 1),
            target_dino: 1 << self.dino,
            weapons: 1 << self.weapon,
            tranq: self.tranq,
            scent: self.scent,
            camo: self.camo,
            radar: self.radar,
            observer: self.observer,
            ..Default::default()
        }
    }

    fn value(r: Row, s: &Settings) -> String {
        let hml = |v: i32| ["Low", "Medium", "High"][v.clamp(0, 2) as usize].to_string();
        let on = |b: bool| if b { "On" } else { "Off" }.to_string();
        match r {
            Row::Agres => hml(s.agres),
            Row::Dens => hml(s.dens),
            Row::Sens => hml(s.sens),
            Row::Fov => format!("{}\u{b0}", s.fov),
            Row::Units => if s.imperial { "US" } else { "Metric" }.into(),
            Row::Blips => if s.radar_clear { "Clear" } else { "Original" }.into(),
            Row::Debug => on(s.debug_keys),
            Row::ShowFps => on(s.show_fps),
            Row::Res => format!("{}x{}", s.window_w, s.window_h),
            Row::Full => on(s.fullscreen),
            Row::VSync => on(s.vsync),
            Row::Fps => {
                if s.fps_limit == 0 {
                    "Off".into()
                } else {
                    s.fps_limit.to_string()
                }
            }
            Row::Shadows => on(s.shadows),
            Row::Fog => on(s.fog),
            Row::Textures => {
                ["Low", "High", "Auto", "Best"][s.textures.clamp(0, 3) as usize].into()
            }
            Row::Msaa => match s.msaa {
                2 => "2x".into(),
                4 => "4x".into(),
                8 => "8x".into(),
                _ => "Off".into(),
            },
            Row::Key(a) => s.keys.get(a).name(),
            Row::RunMode => if s.run_hold { "Hold" } else { "Toggle" }.into(),
            Row::CrouchMode => if s.crouch_hold { "Hold" } else { "Toggle" }.into(),
            Row::Reverse => on(s.mouse_invert),
            Row::MouseSens => s.mouse_sens.to_string(),
        }
    }

    /// A click steps the row.
    fn step(r: Row, s: &mut Settings) {
        match r {
            Row::Agres => s.agres = (s.agres + 1) % 3,
            Row::Dens => s.dens = (s.dens + 1) % 3,
            Row::Sens => s.sens = (s.sens + 1) % 3,
            Row::Units => s.imperial = !s.imperial,
            Row::Blips => s.radar_clear = !s.radar_clear,
            Row::Debug => s.debug_keys = !s.debug_keys,
            Row::ShowFps => s.show_fps = !s.show_fps,
            Row::Res => {
                let i = RESOLUTIONS
                    .iter()
                    .position(|&r| r == (s.window_w, s.window_h))
                    .map(|i| i + 1)
                    .unwrap_or(0)
                    % RESOLUTIONS.len();
                (s.window_w, s.window_h) = RESOLUTIONS[i];
            }
            Row::Full => s.fullscreen = !s.fullscreen,
            Row::VSync => s.vsync = !s.vsync,
            Row::Fps => {
                let i = FRAME_LIMITS
                    .iter()
                    .position(|&f| f == s.fps_limit)
                    .unwrap_or(0);
                s.fps_limit = FRAME_LIMITS[(i + 1) % FRAME_LIMITS.len()];
            }
            Row::Shadows => s.shadows = !s.shadows,
            Row::Fog => s.fog = !s.fog,
            Row::Textures => s.textures = (s.textures + 1) % 4,
            Row::Msaa => {
                s.msaa = match s.msaa {
                    0 => 2,
                    2 => 4,
                    4 => 8,
                    _ => 0,
                }
            }
            Row::RunMode => s.run_hold = !s.run_hold,
            Row::CrouchMode => s.crouch_hold = !s.crouch_hold,
            Row::Reverse => s.mouse_invert = !s.mouse_invert,
            _ => {}
        }
        s.clamp();
    }

    fn options(&mut self, i: &In, hover: u8, f: &mut Frame, s: &mut Settings) {
        let ps = panels();
        // The tab of the panel being worked on is drawn lit.
        f.lit.push(self.opt_mode as u8 + 1);
        let slider = |m: usize, l: usize, s: &mut Settings| {
            let p = &ps[m];
            let r = p.2[l].1;
            let w = if r == Row::MouseSens {
                MSSENS_W - 2
            } else {
                SLIDER_W
            };
            let t = ((i.x - (row_x0(p, l) + 16)) as f32 / w as f32).clamp(0.0, 1.0);
            if r == Row::Fov {
                s.fov = FOV_MIN + (t * (FOV_MAX - FOV_MIN) as f32 + 0.5) as i32;
            } else {
                s.mouse_sens = (t * 255.0 + 0.5) as i32;
            }
        };
        if let Some(a) = self.wait_key {
            if i.esc {
                self.wait_key = None;
            } else if let Some(b) = i.button {
                s.keys.set(a, Binding::Mouse(b));
                self.wait_key = None;
            } else if let Some(k) = i.key.filter(|k| Binding::bindable(*k)) {
                s.keys.set(a, Binding::Key(k));
                self.wait_key = None;
            }
        } else {
            if !i.held {
                self.drag = None;
            }
            if let Some((m, l)) = self.drag {
                slider(m, l, s);
            } else if i.click {
                if (1..=3).contains(&hover) {
                    self.opt_mode = hover as usize - 1;
                    self.opt_line = 0;
                }
                for (m, p) in ps.iter().enumerate() {
                    for l in 0..p.2.len() {
                        let (x0, y0) = (row_x0(p, l), row_y(p, l));
                        let (xl, xr) = if p.2[l].1 == Row::MouseSens {
                            (x0 - 170, x0 + 16 + MSSENS_W + 50)
                        } else {
                            (x0 - 120, x0 + 120)
                        };
                        if i.x > xl && i.x < xr && i.y > y0 && i.y < y0 + OPT_ROW_H {
                            self.opt_mode = m;
                            self.opt_line = l;
                            match p.2[l].1 {
                                Row::Fov | Row::MouseSens => {
                                    slider(m, l, s);
                                    self.drag = Some((m, l));
                                }
                                Row::Key(a) => self.wait_key = Some(a),
                                r => Menu::step(r, s),
                            }
                        }
                    }
                }
            }
        }
        for (m, p) in ps.iter().enumerate() {
            for (l, (label, r)) in p.2.iter().enumerate() {
                let (x0, y0) = (row_x0(p, l), row_y(p, l));
                let lc = if m == self.opt_mode && l == self.opt_line {
                    cref(0x00a0d0f0)
                } else {
                    cref(0x005282b2)
                };
                f.text(Face::Big, x0, y0, *label, lc, Align::Right);
                let c = cref(0xB0B0A0);
                let x = x0 + 16;
                match r {
                    Row::Fov => {
                        Menu::slider(
                            f,
                            x,
                            y0,
                            (s.fov - FOV_MIN) as f32 * 2.0 / (FOV_MAX - FOV_MIN) as f32,
                            120,
                        );
                        f.text(Face::Big, x + 132, y0, Menu::value(*r, s), c, Align::Left);
                    }
                    Row::MouseSens => {
                        Menu::slider(f, x, y0, s.mouse_sens as f32 * 2.0 / 255.0, MSSENS_W);
                        f.text(
                            Face::Big,
                            x + MSSENS_W + 12,
                            y0,
                            Menu::value(*r, s),
                            c,
                            Align::Left,
                        );
                    }
                    Row::Key(a) if self.wait_key == Some(*a) => {
                        f.text(Face::Big, x, y0, "<?>", c, Align::Left)
                    }
                    _ => f.text(Face::Big, x, y0, Menu::value(*r, s), c, Align::Left),
                }
            }
        }
    }
}

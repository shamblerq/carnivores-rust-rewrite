//! Carnivores 2's and Ice Age's menus, rebuilt from their artwork (after
//! the Carn2-Menu project): hunter
//! profiles, the waiver, the main screen, statistics, the hunt screen with
//! its price lists, options, credits and the quit prompt.
//!
//! The options screen keeps the originals' three panels and puts the
//! rewrite's own settings among them: field of view in the game panel,
//! frame limit, V-Sync and the window in the video panel, anti-aliasing,
//! cloud shadows and the FPS readout on its second page.

use std::sync::OnceLock;

use bevy::input::keyboard::KeyCode;

use super::{load_pic, Align, Art, Face, Frame, HuntSetup, In, Outcome, Pic};
use crate::game::GameKind;
use crate::keymap::{Act, Binding, ACTS};
use crate::paths::DataRoot;
use crate::profile::{Profile, Store, SLOTS};
use crate::settings::{Settings, FOV_MAX, FOV_MIN, FRAME_LIMITS};

const C_ITEM: [u8; 3] = [206, 198, 150];
const C_LOCKED: [u8; 3] = [124, 124, 116];
const C_SELECTED: [u8; 3] = [255, 255, 10];
const C_TEXT: [u8; 3] = [239, 228, 176];
const C_OPT_LABEL: [u8; 3] = [196, 142, 86];
const C_OPT_VALUE: [u8; 3] = [214, 206, 186];
const C_OPT_HOVER: [u8; 3] = [30, 239, 30];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Register,
    Delete,
    Waiver,
    Main,
    Stats,
    Hunt,
    Options,
    Credits,
    Quit,
}

impl Page {
    fn art(self) -> &'static str {
        match self {
            Page::Register => "register",
            Page::Delete => "delete",
            Page::Waiver => "waiver",
            Page::Main => "main",
            Page::Stats => "stats",
            Page::Hunt => "hunt",
            Page::Options => "options",
            Page::Credits => "credits",
            Page::Quit => "quit",
        }
    }
}

struct Entry {
    name: String,
    price: i32,
    desc: Vec<String>,
    pic_path: String,
    pic: OnceLock<Option<Pic>>,
    /// The area's map number, the creature's AI id, the weapon's or the
    /// accessory's index.
    index: usize,
    available: bool,
}

impl Entry {
    fn pic(&self, root: &DataRoot) -> Option<Pic> {
        self.pic
            .get_or_init(|| load_pic(root, &self.pic_path))
            .clone()
    }
}

fn text_lines(root: &DataRoot, rel: &str) -> Vec<String> {
    let Some(p) = root.find(rel) else {
        return Vec::new();
    };
    let Ok(b) = std::fs::read(p) else {
        return Vec::new();
    };
    let t: String = b.iter().map(|&c| c as char).collect();
    let mut v: Vec<String> = t
        .split('\n')
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();
    while v.last().map(|l| l.is_empty()).unwrap_or(false) {
        v.pop();
    }
    v
}

// Hunt screen lists: areas, creatures, weapons, equipment.
const LIST: [(i32, i32, i32, i32); 4] = [
    (10, 382, 190, 542),
    (210, 382, 390, 542),
    (410, 382, 590, 542),
    (610, 382, 790, 542),
];
const ROW_H: i32 = 16;
const ROWS: i32 = 10;

// ---- options ----------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opt {
    Agres,
    Dens,
    Sens,
    ViewR,
    Fov,
    Units,
    Res,
    Full,
    VSync,
    Fps,
    Shadows,
    Fog,
    Textures,
    Brightness,
    Msaa,
    Clouds,
    ShowFps,
    Volume,
    ObjLod,
    ObjDist,
    Blips,
    Debug,
    RunMode,
    CrouchMode,
    Reverse,
    MouseSens,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    /// A slider, with a readout in this unit where there is one.
    Slider(Opt, Option<&'static str>),
    /// A slider with its own bar: x, width.
    Wide(Opt, i32, i32),
    Choice(Opt),
    Key(Act),
    Page(&'static str),
}

struct Panel {
    x0: i32,
    y0: i32,
    vx: i32,
    vw: i32,
    hit_l: i32,
    hit_r: i32,
    row_h: i32,
}

const P_GAME: Panel = Panel {
    x0: 210,
    y0: 80,
    vx: 222,
    vw: 150,
    hit_l: 40,
    hit_r: 380,
    row_h: 24,
};
const P_VIDEO: Panel = Panel {
    x0: 210,
    y0: 336,
    vx: 222,
    vw: 150,
    hit_l: 40,
    hit_r: 380,
    row_h: 21,
};
const P_CTRL: Panel = Panel {
    x0: 604,
    y0: 80,
    vx: 616,
    vw: 150,
    hit_l: 424,
    hit_r: 762,
    row_h: 24,
};

const GAME_ROWS: &[(&str, Row)] = &[
    ("Agressivity", Row::Slider(Opt::Agres, None)),
    ("Density", Row::Slider(Opt::Dens, None)),
    ("Sensitivity", Row::Slider(Opt::Sens, None)),
    ("View range", Row::Slider(Opt::ViewR, None)),
    ("Field of view", Row::Slider(Opt::Fov, Some("\u{b0}"))),
    ("Measurement", Row::Choice(Opt::Units)),
    ("Radar blips", Row::Choice(Opt::Blips)),
    ("Debug keys", Row::Choice(Opt::Debug)),
];

const VIDEO_ROWS: &[(&str, Row)] = &[
    ("Resolution", Row::Choice(Opt::Res)),
    ("Full screen", Row::Choice(Opt::Full)),
    ("V-Sync", Row::Choice(Opt::VSync)),
    ("Frame limit", Row::Choice(Opt::Fps)),
    ("Shadows", Row::Choice(Opt::Shadows)),
    ("Fog", Row::Choice(Opt::Fog)),
    ("Textures", Row::Choice(Opt::Textures)),
    ("Brightness", Row::Slider(Opt::Brightness, None)),
    ("More", Row::Page(">>")),
];

const VIDEO2_ROWS: &[(&str, Row)] = &[
    ("Anti-aliasing", Row::Choice(Opt::Msaa)),
    ("Object LOD", Row::Choice(Opt::ObjLod)),
    ("Object distance", Row::Slider(Opt::ObjDist, Some(""))),
    ("Cloud shadows", Row::Choice(Opt::Clouds)),
    ("Show FPS", Row::Choice(Opt::ShowFps)),
    ("Volume", Row::Slider(Opt::Volume, None)),
    ("Back", Row::Page("<<")),
];

fn ctrl_rows() -> Vec<(&'static str, Row)> {
    let mut v: Vec<(&'static str, Row)> = ACTS[..14]
        .iter()
        .map(|a| (a.label(), Row::Key(*a)))
        .collect();
    v.push(("Run mode", Row::Choice(Opt::RunMode)));
    v.push(("Crouch mode", Row::Choice(Opt::CrouchMode)));
    for a in &ACTS[14..] {
        v.push((a.label(), Row::Key(*a)));
    }
    v.push(("Reverse mouse", Row::Choice(Opt::Reverse)));
    v.push(("Mouse sensitivity", Row::Wide(Opt::MouseSens, 454, 256)));
    v
}

/// Window sizes offered (the window's own size is kept in the list).
const RESOLUTIONS: &[(u32, u32)] = &[
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1280, 1024),
    (1366, 768),
    (1600, 900),
    (1680, 1050),
    (1920, 1080),
    (1920, 1200),
    (2560, 1080),
    (2560, 1440),
    (3440, 1440),
    (3840, 2160),
];

fn names(o: Opt, s: &Settings) -> Vec<String> {
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect();
    match o {
        Opt::Units => v(&["Metric", "Imperial"]),
        Opt::Blips => v(&["Original", "Clear"]),
        Opt::Res => {
            let mut r: Vec<String> = RESOLUTIONS
                .iter()
                .map(|(w, h)| format!("{w}x{h}"))
                .collect();
            if !RESOLUTIONS.contains(&(s.window_w, s.window_h)) {
                r.push(format!("{}x{}", s.window_w, s.window_h));
            }
            r
        }
        Opt::Fps => FRAME_LIMITS
            .iter()
            .map(|&f| {
                if f == 0 {
                    "Off".to_string()
                } else {
                    f.to_string()
                }
            })
            .collect(),
        Opt::Textures => v(&["Low", "Normal", "High", "Best"]),
        Opt::Msaa => v(&["Off", "2x", "4x", "8x"]),
        Opt::RunMode | Opt::CrouchMode => v(&["Toggle", "Hold"]),
        _ => v(&["Off", "On"]),
    }
}

fn range(o: Opt) -> (i32, i32) {
    match o {
        Opt::Fov => (FOV_MIN, FOV_MAX),
        Opt::ObjDist => (12, 64),
        _ => (0, 255),
    }
}

fn get(o: Opt, s: &Settings) -> i32 {
    let b = |v: bool| v as i32;
    match o {
        Opt::Agres => s.agres,
        Opt::Dens => s.dens,
        Opt::Sens => s.sens,
        Opt::ViewR => s.view_range,
        Opt::Fov => s.fov,
        Opt::Units => b(s.imperial),
        Opt::Res => RESOLUTIONS
            .iter()
            .position(|&r| r == (s.window_w, s.window_h))
            .unwrap_or(RESOLUTIONS.len()) as i32,
        Opt::Full => b(s.fullscreen),
        Opt::VSync => b(s.vsync),
        Opt::Fps => FRAME_LIMITS
            .iter()
            .position(|&f| f == s.fps_limit)
            .unwrap_or(0) as i32,
        Opt::Shadows => b(s.shadows),
        Opt::Fog => b(s.fog),
        Opt::Textures => s.textures,
        Opt::Brightness => s.brightness,
        Opt::Msaa => [0, 2, 4, 8].iter().position(|&m| m == s.msaa).unwrap_or(0) as i32,
        Opt::Clouds => b(s.clouds),
        Opt::ShowFps => b(s.show_fps),
        Opt::Volume => s.volume,
        Opt::ObjLod => b(s.obj_lod),
        Opt::ObjDist => s.obj_lod_dist,
        Opt::Blips => b(s.radar_clear),
        Opt::Debug => b(s.debug_keys),
        Opt::RunMode => b(s.run_hold),
        Opt::CrouchMode => b(s.crouch_hold),
        Opt::Reverse => b(s.mouse_invert),
        Opt::MouseSens => s.mouse_sens,
    }
}

fn set(o: Opt, s: &mut Settings, v: i32) {
    let b = v != 0;
    match o {
        Opt::Agres => s.agres = v,
        Opt::Dens => s.dens = v,
        Opt::Sens => s.sens = v,
        Opt::ViewR => s.view_range = v,
        Opt::Fov => s.fov = v,
        Opt::Units => s.imperial = b,
        Opt::Res => {
            if let Some(&(w, h)) = RESOLUTIONS.get(v as usize) {
                s.window_w = w;
                s.window_h = h;
            }
        }
        Opt::Full => s.fullscreen = b,
        Opt::VSync => s.vsync = b,
        Opt::Fps => s.fps_limit = FRAME_LIMITS[(v as usize).min(FRAME_LIMITS.len() - 1)],
        Opt::Shadows => s.shadows = b,
        Opt::Fog => s.fog = b,
        Opt::Textures => s.textures = v,
        Opt::Brightness => s.brightness = v,
        Opt::Msaa => s.msaa = [0, 2, 4, 8][(v as usize).min(3)],
        Opt::Clouds => s.clouds = b,
        Opt::ShowFps => s.show_fps = b,
        Opt::Volume => s.volume = v,
        Opt::ObjLod => s.obj_lod = b,
        Opt::ObjDist => s.obj_lod_dist = v,
        Opt::Blips => s.radar_clear = b,
        Opt::Debug => s.debug_keys = b,
        Opt::RunMode => s.run_hold = b,
        Opt::CrouchMode => s.crouch_hold = b,
        Opt::Reverse => s.mouse_invert = b,
        Opt::MouseSens => s.mouse_sens = v,
    }
    s.clamp();
}

fn slider_bar(p: &Panel, r: Row) -> (i32, i32) {
    match r {
        Row::Wide(_, x, w) => (x, w),
        Row::Slider(_, Some(_)) => (p.vx, p.vw - 52),
        _ => (p.vx, p.vw),
    }
}

fn label_right(p: &Panel, r: Row) -> i32 {
    match r {
        Row::Wide(_, x, _) => x - 12,
        _ => p.x0,
    }
}

fn panel_row(p: &Panel, rows: &[(&str, Row)], x: i32, y: i32) -> Option<usize> {
    if y < p.y0 - 4 {
        return None;
    }
    let row = ((y - (p.y0 - 4)) / p.row_h) as usize;
    let (label, r) = rows.get(row)?;
    let (l, rr) = match r {
        Row::Wide(_, bx, bw) => (label_right(p, *r) - 9 * label.len() as i32, bx + bw + 52),
        _ => (p.hit_l, p.hit_r),
    };
    (x >= l && x <= rr).then_some(row)
}

pub struct Menu {
    page: Page,
    entered: Option<Page>,
    store: Store,
    profile: Option<Profile>,
    profiles: Vec<Option<Profile>>,
    slot: usize,
    typing: String,
    start_credits: i32,
    areas: Vec<Entry>,
    dinos: Vec<Entry>,
    weapons: Vec<Entry>,
    access: Vec<Entry>,
    /// (power, precision, loudness) per weapon; (look, hear, smell) per creature.
    weapon_stats: Vec<(f32, f32, f32)>,
    dino_stats: Vec<(f32, f32, f32)>,
    picked_area: usize,
    picked_dino: Vec<bool>,
    picked_weapon: Vec<bool>,
    picked_access: Vec<bool>,
    offset: [usize; 4],
    info: Option<(usize, usize)>,
    time_of_day: i32,
    tranq: bool,
    observer: bool,
    video_page: bool,
    wait_key: Option<Act>,
    drag: Option<(u8, usize)>,
}

/// The hunt screen's choices. The menu sets them up once and
/// keeps them from one visit to the next (statics), so a hunt and a return
/// leave them as they were; a new start begins afresh.
#[derive(Clone, Debug)]
pub struct Choices {
    area: usize,
    dino: Vec<bool>,
    weapon: Vec<bool>,
    access: Vec<bool>,
    time_of_day: i32,
    tranq: bool,
    observer: bool,
}

impl Menu {
    pub fn new(
        root: &DataRoot,
        kind: GameKind,
        settings: &Settings,
        profile: Option<Profile>,
    ) -> Menu {
        let (dinos, script) = crate::creatures::load_dinos(root, kind).unwrap_or_default();
        let mut m = Menu {
            page: if profile.is_some() {
                Page::Main
            } else {
                Page::Register
            },
            entered: None,
            store: Store::new(kind, root),
            profile,
            profiles: Vec::new(),
            slot: 0,
            typing: String::new(),
            start_credits: if script.has_prices {
                script.start_credits
            } else {
                100
            },
            areas: Vec::new(),
            dinos: Vec::new(),
            weapons: Vec::new(),
            access: Vec::new(),
            weapon_stats: Vec::new(),
            dino_stats: Vec::new(),
            picked_area: 0,
            picked_dino: Vec::new(),
            picked_weapon: Vec::new(),
            picked_access: Vec::new(),
            offset: [0; 4],
            info: None,
            time_of_day: 1,
            tranq: false,
            observer: false,
            video_page: false,
            wait_key: None,
            drag: None,
        };
        let entry = |name: String, price: i32, desc: &str, pic: String, index: usize| Entry {
            name,
            price,
            desc: text_lines(root, desc),
            pic_path: pic,
            pic: OnceLock::new(),
            index,
            available: true,
        };

        // Areas: as many as the price table prices, or the maps present.
        let present = |n: usize| root.find(&format!("HUNTDAT/AREAS/AREA{n}.MAP")).is_some();
        let mut ns: Vec<usize> = if script.has_prices && !script.area_prices.is_empty() {
            (1..=script.area_prices.len().min(10)).collect()
        } else {
            (1..=10).filter(|&n| present(n)).collect()
        };
        if ns.is_empty() {
            ns = (1..=5).collect();
        }
        for n in ns {
            let desc = format!("HUNTDAT/MENU/TXT/AREA{n}.TXT");
            let first = text_lines(root, &desc)
                .into_iter()
                .next()
                .filter(|l| !l.is_empty());
            let mut e = entry(
                first.unwrap_or_else(|| format!("Area {n}")),
                script.area_prices.get(n - 1).copied().unwrap_or(0),
                &desc,
                format!("HUNTDAT/MENU/PICS/AREA{n}.TGA"),
                n,
            );
            e.available = present(n);
            m.areas.push(e);
        }

        // The hunt: kinds with a behaviour from 10, in the order of their
        // ids (Carnivores 2's slots) or of the list (Ice Age).
        let mut hunt: Vec<usize> = (0..dinos.len()).filter(|&i| dinos[i].ai >= 10).collect();
        if kind == GameKind::Carnivores2 {
            hunt.sort_by_key(|&i| dinos[i].ai);
            hunt.dedup_by_key(|i| dinos[*i].ai);
        }
        let ext = if settings.imperial { "TXU" } else { "TXM" };
        for (slot, &i) in hunt.iter().enumerate() {
            let d = &dinos[i];
            let n = d.ai - 9;
            m.dinos.push(entry(
                d.name.clone(),
                script.dino_prices.get(slot).copied().unwrap_or(0),
                &format!("HUNTDAT/MENU/TXT/DINO{n}.{ext}"),
                format!("HUNTDAT/MENU/PICS/DINO{n}.TGA"),
                d.ai as usize,
            ));
            m.dino_stats.push((d.look_k, d.hear_k, d.smell_k));
        }

        let wn = if script.weapon_prices.is_empty() {
            script.weapons.len()
        } else {
            script.weapons.len().min(script.weapon_prices.len())
        };
        for (w, wi) in script.weapons.iter().take(wn).enumerate() {
            m.weapons.push(entry(
                wi.name.clone(),
                script.weapon_prices.get(w).copied().unwrap_or(0),
                &format!("HUNTDAT/MENU/TXT/WEAPON{}.TXT", w + 1),
                format!("HUNTDAT/MENU/PICS/WEAPON{}.TGA", w + 1),
                w,
            ));
            m.weapon_stats.push((wi.power, wi.prec, wi.loud));
        }

        for (a, (name, info)) in [
            ("Camouflage", "CAMOFLAG"),
            ("Radar", "RADAR"),
            ("Cover scent", "SCENT"),
            ("Double ammo", "DOUBLE"),
        ]
        .iter()
        .enumerate()
        {
            m.access.push(entry(
                name.to_string(),
                script.access_prices.get(a).copied().unwrap_or(0),
                &format!("HUNTDAT/MENU/TXT/{info}.NFO"),
                format!("HUNTDAT/MENU/PICS/EQUIP{}.TGA", a + 1),
                a,
            ));
        }
        m.picked_dino = vec![false; m.dinos.len()];
        m.picked_weapon = vec![false; m.weapons.len()];
        m.picked_access = vec![false; m.access.len()];
        m
    }

    fn debit(&self) -> i32 {
        let mut d = self
            .areas
            .get(self.picked_area)
            .map(|a| a.price)
            .unwrap_or(0);
        for (list, picked) in [
            (&self.dinos, &self.picked_dino),
            (&self.weapons, &self.picked_weapon),
            (&self.access, &self.picked_access),
        ] {
            d += list
                .iter()
                .zip(picked.iter())
                .filter(|(_, p)| **p)
                .map(|(e, _)| e.price)
                .sum::<i32>();
        }
        d
    }

    fn score(&self) -> i32 {
        self.profile.as_ref().map(|p| p.score).unwrap_or(0)
    }

    fn enter(&mut self, page: Page) {
        match page {
            Page::Register => {
                self.profiles = self.store.scan(SLOTS);
                self.typing.clear();
            }
            Page::Hunt => {
                self.offset = [0; 4];
                self.info = Some((0, self.picked_area));
                if !self.picked_weapon.iter().any(|&p| p) {
                    if let Some(p) = self.picked_weapon.first_mut() {
                        *p = true;
                    }
                }
                if !self.picked_dino.iter().any(|&p| p) {
                    if let Some(p) = self.picked_dino.first_mut() {
                        *p = true;
                    }
                }
            }
            Page::Options => {
                self.video_page = false;
                self.wait_key = None;
                self.drag = None;
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        i: &In,
        art: Option<&Art>,
        art_key: &str,
        root: &DataRoot,
        kind: GameKind,
        settings: &mut Settings,
        current: &mut super::CurrentProfile,
    ) -> (String, Frame, Outcome) {
        let want = self.page.art().to_string();
        if art_key != want {
            return (want, Frame::default(), Outcome::Stay);
        }
        if self.entered != Some(self.page) {
            self.entered = Some(self.page);
            self.enter(self.page);
        }
        let hover = art.map(|a| a.id(i.x, i.y)).unwrap_or(0);
        let mut f = Frame::default();
        if hover != 0 {
            f.lit.push(hover);
        }
        let mut out = Outcome::Stay;
        match self.page {
            Page::Register => self.register(i, hover, &mut f, current),
            Page::Delete => {
                if i.click && hover == 1 {
                    self.store.delete(self.slot);
                    self.page = Page::Register;
                }
                if (i.click && hover == 2) || i.esc {
                    self.page = Page::Register;
                }
                let name = self
                    .profiles
                    .get(self.slot)
                    .and_then(|p| p.as_ref())
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                f.text(
                    Face::Midd,
                    290,
                    370,
                    "Do you want to delete player",
                    [48, 128, 176],
                    Align::Left,
                );
                f.text(
                    Face::Midd,
                    300,
                    394,
                    format!("'{name}' ?"),
                    [48, 128, 176],
                    Align::Left,
                );
            }
            Page::Waiver => {
                if i.click && hover == 1 {
                    self.page = Page::Main;
                }
                if i.click && hover == 2 {
                    self.store.delete(self.slot);
                    self.profile = None;
                    current.0 = None;
                    self.page = Page::Register;
                }
            }
            Page::Main => {
                if i.click && hover != 0 {
                    match hover {
                        1 => self.page = Page::Hunt,
                        2 => self.page = Page::Options,
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
                self.draw_main(&mut f);
            }
            Page::Stats => {
                if i.click || i.esc {
                    self.page = Page::Main;
                }
                self.draw_stats(&mut f, settings.imperial);
                self.draw_main(&mut f);
            }
            Page::Hunt => {
                if let Some(o) = self.hunt(i, hover, &mut f, root, settings) {
                    out = o;
                    if let Some(p) = &mut self.profile {
                        self.store.save(p);
                        current.0 = Some(p.clone());
                    }
                }
                if i.esc {
                    self.page = Page::Main;
                }
            }
            Page::Options => {
                let was_waiting = self.wait_key.is_some();
                self.options(i, hover, &mut f, settings);
                if (i.click && hover == 4) || (i.esc && !was_waiting) {
                    settings.save(kind);
                    self.page = Page::Main;
                }
            }
            Page::Credits => {
                if i.click || i.esc {
                    self.page = Page::Main;
                }
            }
            Page::Quit => {
                if i.click && hover == 1 {
                    out = Outcome::Quit;
                }
                if (i.click && hover == 2) || i.esc {
                    self.page = Page::Main;
                }
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
        let rows = 7.min(SLOTS);
        if i.click && i.x >= 308 && i.x <= 408 && i.y >= Y - 2 && i.y < Y - 2 + rows as i32 * 16 {
            self.slot = ((i.y - (Y - 2)) / 16) as usize;
            self.typing.clear();
        } else {
            let ok = (i.click && hover == 1) || i.enter;
            let rem = (i.click && hover == 2) || i.delete;
            let used = self.profiles.get(self.slot).and_then(|p| p.clone());
            if ok {
                if let Some(p) = used {
                    self.profile = Some(p.clone());
                    current.0 = Some(p);
                    self.page = Page::Main;
                } else if !self.typing.trim().is_empty() {
                    let mut p = Profile {
                        name: self.typing.trim().to_string(),
                        slot: self.slot,
                        score: self.start_credits,
                        ..Default::default()
                    };
                    self.store.save(&mut p);
                    self.profile = Some(p.clone());
                    current.0 = Some(p);
                    self.page = Page::Waiver;
                }
            } else if rem && used.is_some() {
                self.page = Page::Delete;
            }
        }
        let mut typed = self.typing.clone();
        if i.time_ms % 800 > 300 {
            typed.push('_');
        }
        f.text(Face::Small, 315, 326, typed, C_TEXT, Align::Left);
        for r in 0..rows {
            let c = if r == self.slot {
                [255, 170, 10]
            } else {
                C_ITEM
            };
            let n = self
                .profiles
                .get(r)
                .and_then(|p| p.as_ref())
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "...".into());
            f.text(Face::Small, X, Y + r as i32 * 16, n, c, Align::Left);
        }
    }

    fn draw_main(&self, f: &mut Frame) {
        let name = self
            .profile
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Hunter".into());
        f.text(Face::Big, 90, 9, name, C_TEXT, Align::Left);
        f.text(
            Face::Big,
            592,
            9,
            self.score().to_string(),
            C_TEXT,
            Align::Right,
        );
    }

    fn draw_stats(&self, f: &mut Frame, imperial: bool) {
        let Some(p) = &self.profile else { return };
        let (l, r) = (606, 788);
        let time = |t: f32| {
            let t = t as i32;
            format!("{}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
        };
        let acc = |s: &crate::profile::Stats| {
            if s.shots_made > 0 {
                s.success * 100 / s.shots_made
            } else {
                0
            }
        };
        let mut row = |y: i32, a: &str, b: String| {
            f.text(Face::Midd, l, y, a, C_TEXT, Align::Left);
            f.text(Face::Midd, r, y, b, C_TEXT, Align::Right);
        };
        let last = &p.last;
        row(
            78,
            "Path travelled",
            if imperial {
                format!("{:.0} ft.", last.path / 0.3)
            } else {
                format!("{:.0} m.", last.path)
            },
        );
        row(98, "Time hunted", time(last.time));
        row(118, "Shots made", last.shots_made.to_string());
        row(138, "Accuracy", format!("{}%", acc(last)));
        let tot = &p.total;
        let path = if tot.path < 1000.0 {
            if imperial {
                format!("{:.0} ft.", tot.path / 0.3)
            } else {
                format!("{:.0} m.", tot.path)
            }
        } else if imperial {
            format!("{:.2} miles.", tot.path / 1667.0)
        } else {
            format!("{:.2} km.", tot.path / 1000.0)
        };
        row(208, "Path travelled", path);
        row(228, "Time hunted", time(tot.time));
        row(248, "Shots made", tot.shots_made.to_string());
        row(268, "Accuracy", format!("{}%", acc(tot)));
        row(
            288,
            "Rank:",
            ["Novice", "Advanced", "Expert"][Profile::earned_rank(p.score).clamp(0, 2) as usize]
                .to_string(),
        );
    }

    fn list_at(&self, list: usize, count: usize, x: i32, y: i32) -> Option<usize> {
        let (l, t, r, b) = LIST[list];
        if x < l || x > r || y < t || y > b {
            return None;
        }
        let row = (y - t) / ROW_H;
        if !(0..ROWS).contains(&row) {
            return None;
        }
        let i = self.offset[list] + row as usize;
        (i < count).then_some(i)
    }

    fn hunt(
        &mut self,
        i: &In,
        hover: u8,
        f: &mut Frame,
        root: &DataRoot,
        settings: &mut Settings,
    ) -> Option<Outcome> {
        let credit = self.score() - self.debit();
        let counts = [
            self.areas.len(),
            self.dinos.len(),
            self.weapons.len(),
            self.access.len(),
        ];
        if i.wheel != 0 {
            for c in 0..4 {
                let (l, t, r, b) = LIST[c];
                if i.x >= l && i.x <= r && i.y >= t && i.y <= b {
                    let max = counts[c].saturating_sub(ROWS as usize) as i32;
                    self.offset[c] = (self.offset[c] as i32 - i.wheel).clamp(0, max) as usize;
                }
            }
        }
        for (c, &count) in counts.iter().enumerate() {
            let Some(n) = self.list_at(c, count, i.x, i.y) else {
                continue;
            };
            self.info = Some((c, n));
            if !i.click {
                break;
            }
            match c {
                0 => {
                    let refund = self
                        .areas
                        .get(self.picked_area)
                        .map(|a| a.price)
                        .unwrap_or(0);
                    let e = &self.areas[n];
                    if e.available && credit + refund >= e.price {
                        self.picked_area = n;
                    }
                }
                _ => {
                    let (list, picked) = match c {
                        1 => (&self.dinos, &mut self.picked_dino),
                        2 => (&self.weapons, &mut self.picked_weapon),
                        _ => (&self.access, &mut self.picked_access),
                    };
                    if picked[n] {
                        picked[n] = false;
                    } else if credit >= list[n].price {
                        picked[n] = true;
                    }
                }
            }
            break;
        }
        let credit = self.score() - self.debit();
        if i.click && hover != 0 {
            match hover {
                1..=3 => {
                    self.time_of_day = hover as i32 - 1;
                }
                5 => self.tranq = !self.tranq,
                6 => self.observer = !self.observer,
                7 => self.page = Page::Main,
                8 => {
                    let any_d = self.picked_dino.iter().any(|&p| p);
                    let any_w = self.picked_weapon.iter().any(|&p| p);
                    if any_d && any_w && credit >= 0 {
                        return Some(Outcome::Hunt(self.setup(settings)));
                    }
                }
                _ => {}
            }
        }
        f.lit.push(self.time_of_day as u8 + 1);
        if self.tranq {
            f.lit.push(5);
        }
        if self.observer {
            f.lit.push(6);
        }

        // The picture and the words for what the pointer is over.
        let info = self.info.and_then(|(c, n)| {
            let list = match c {
                0 => &self.areas,
                1 => &self.dinos,
                2 => &self.weapons,
                _ => &self.access,
            };
            list.get(n).map(|e| (c, n, e))
        });
        if let Some((c, n, e)) = info {
            if let Some(p) = e.pic(root) {
                f.blits.push((p, 38, 73));
            }
            let first = if c == 0 { 1 } else { 0 };
            for (k, l) in e.desc.iter().skip(first).enumerate() {
                f.text(
                    Face::Midd,
                    424,
                    96 + k as i32 * 16,
                    l.clone(),
                    C_TEXT,
                    Align::Left,
                );
            }
            let bars = match c {
                1 => self.dino_stats.get(n).map(|s| {
                    (
                        ["Sight:", "Hearing:", "Scent:"],
                        [s.0 * 2.0, s.1 * 2.0, s.2 * 2.0],
                    )
                }),
                2 => self
                    .weapon_stats
                    .get(n)
                    .map(|s| (["Power:", "Accuracy:", "Volume:"], [s.0, s.1, s.2])),
                _ => None,
            };
            if let Some((labels, v)) = bars {
                for k in 0..3 {
                    let y = 210 + k as i32 * 16;
                    f.text(Face::Midd, 424, y, labels[k], C_TEXT, Align::Left);
                    // A 0..2 meter with quarter ticks.
                    let (bx, by, bw, bh) = (504, y + 5, 120, 8);
                    f.rects.push((bx, by, bw, bh, [0x20, 0x20, 0x20]));
                    let fill = (((bw - 2) as f32 * v[k].clamp(0.0, 2.0)) / 2.0) as i32;
                    if fill > 0 {
                        f.rects
                            .push((bx + 1, by + 1, fill, bh - 2, [0x3F, 0xAF, 0x3F]));
                    }
                    for q in 1..4 {
                        f.rects
                            .push((bx + bw * q / 4, by, 1, bh, [0x9F, 0x9F, 0x9F]));
                    }
                }
            }
        }
        f.text(
            Face::Big,
            336,
            38,
            self.score().to_string(),
            C_TEXT,
            Align::Left,
        );
        f.text(
            Face::Big,
            464,
            38,
            credit.to_string(),
            if credit < 0 { [255, 80, 80] } else { C_TEXT },
            Align::Right,
        );
        for (c, &(l, t, r, _)) in LIST.iter().enumerate() {
            let (list, picked): (&Vec<Entry>, Option<&Vec<bool>>) = match c {
                0 => (&self.areas, None),
                1 => (&self.dinos, Some(&self.picked_dino)),
                2 => (&self.weapons, Some(&self.picked_weapon)),
                _ => (&self.access, Some(&self.picked_access)),
            };
            for row in 0..ROWS as usize {
                let n = self.offset[c] + row;
                let Some(e) = list.get(n) else { break };
                let is = picked.map(|p| p[n]).unwrap_or(n == self.picked_area);
                let mut col = C_ITEM;
                if !is && credit < e.price {
                    col = C_LOCKED;
                }
                if !e.available {
                    col = C_LOCKED;
                }
                if is {
                    col = C_SELECTED;
                }
                let y = t + row as i32 * ROW_H;
                f.text(Face::Small, l + 4, y, e.name.clone(), col, Align::Left);
                if e.price > 0 {
                    f.text(
                        Face::Small,
                        r - 4,
                        y,
                        e.price.to_string(),
                        col,
                        Align::Right,
                    );
                }
            }
        }
        None
    }

    /// What the hunt screen has chosen, to keep for the next visit.
    pub fn choices(&self) -> Choices {
        Choices {
            area: self.picked_area,
            dino: self.picked_dino.clone(),
            weapon: self.picked_weapon.clone(),
            access: self.picked_access.clone(),
            time_of_day: self.time_of_day,
            tranq: self.tranq,
            observer: self.observer,
        }
    }

    /// The choices of the last visit, back on the hunt screen.
    pub fn restore(&mut self, c: &Choices) {
        if c.area < self.areas.len() {
            self.picked_area = c.area;
        }
        for (to, from) in [
            (&mut self.picked_dino, &c.dino),
            (&mut self.picked_weapon, &c.weapon),
            (&mut self.picked_access, &c.access),
        ] {
            if to.len() == from.len() {
                to.clone_from(from);
            }
        }
        self.time_of_day = c.time_of_day;
        self.tranq = c.tranq;
        self.observer = c.observer;
    }

    fn setup(&self, settings: &mut Settings) -> HuntSetup {
        settings.day_night = self.time_of_day;
        let area = self
            .areas
            .get(self.picked_area)
            .map(|a| a.index)
            .unwrap_or(1);
        let mut s = HuntSetup {
            project: format!("HUNTDAT/AREAS/AREA{area}"),
            tranq: self.tranq,
            observer: self.observer,
            ..Default::default()
        };
        for (e, p) in self.dinos.iter().zip(&self.picked_dino) {
            if *p && e.index < 32 {
                s.target_dino |= 1 << e.index;
            }
        }
        for (e, p) in self.weapons.iter().zip(&self.picked_weapon) {
            if *p && e.index < 32 {
                s.weapons |= 1 << e.index;
            }
        }
        let acc = |i: usize| self.picked_access.get(i).copied().unwrap_or(false);
        s.camo = acc(0);
        s.radar = acc(1);
        s.scent = acc(2);
        s.double_ammo = acc(3);
        s
    }

    fn options(&mut self, i: &In, _hover: u8, f: &mut Frame, s: &mut Settings) {
        let ctrl = ctrl_rows();
        let video: &[(&str, Row)] = if self.video_page {
            VIDEO2_ROWS
        } else {
            VIDEO_ROWS
        };
        let panels: [(&Panel, &[(&str, Row)]); 3] =
            [(&P_GAME, GAME_ROWS), (&P_VIDEO, video), (&P_CTRL, &ctrl)];

        if let Some(a) = self.wait_key {
            // The next key or button takes the slot; Esc leaves it.
            if i.esc {
                self.wait_key = None;
            } else if let Some(b) = i.button {
                s.keys.set(a, Binding::Mouse(b));
                self.wait_key = None;
            } else if let Some(k) = i
                .key
                .filter(|k| Binding::bindable(*k) && *k != KeyCode::Escape)
            {
                s.keys.set(a, Binding::Key(k));
                self.wait_key = None;
            }
        } else {
            if !i.held {
                self.drag = None;
            }
            let apply_slider = |p: &Panel, r: Row, s: &mut Settings| {
                let (Row::Slider(o, _) | Row::Wide(o, _, _)) = r else {
                    return;
                };
                let (x, w) = slider_bar(p, r);
                let (lo, hi) = range(o);
                let v = ((i.x - x) as f32 / w as f32).clamp(0.0, 1.0);
                set(o, s, lo + (v * (hi - lo) as f32 + 0.5) as i32);
            };
            if let Some((pi, row)) = self.drag {
                let (p, rows) = panels[pi as usize];
                if let Some((_, r)) = rows.get(row) {
                    apply_slider(p, *r, s);
                }
            } else if i.click {
                for (pi, (p, rows)) in panels.iter().enumerate() {
                    let Some(row) = panel_row(p, rows, i.x, i.y) else {
                        continue;
                    };
                    let r = rows[row].1;
                    match r {
                        Row::Slider(..) | Row::Wide(..) => {
                            apply_slider(p, r, s);
                            self.drag = Some((pi as u8, row));
                        }
                        Row::Page(_) => self.video_page = !self.video_page,
                        Row::Choice(o) => {
                            let n = names(o, s).len() as i32;
                            set(o, s, (get(o, s) + 1).rem_euclid(n.max(1)));
                        }
                        Row::Key(a) => self.wait_key = Some(a),
                    }
                    break;
                }
            }
        }

        let video: &[(&str, Row)] = if self.video_page {
            VIDEO2_ROWS
        } else {
            VIDEO_ROWS
        };
        let panels: [(&Panel, &[(&str, Row)]); 3] =
            [(&P_GAME, GAME_ROWS), (&P_VIDEO, video), (&P_CTRL, &ctrl)];
        for (p, rows) in panels {
            let hover_row = panel_row(p, rows, i.x, i.y);
            for (n, (label, r)) in rows.iter().enumerate() {
                let y = p.y0 + n as i32 * p.row_h;
                let lc = if hover_row == Some(n) {
                    C_OPT_HOVER
                } else {
                    C_OPT_LABEL
                };
                f.text(Face::Opt, label_right(p, *r), y, *label, lc, Align::Right);
                match *r {
                    Row::Slider(o, _) | Row::Wide(o, _, _) => {
                        let (lo, hi) = range(o);
                        let v = get(o, s).clamp(lo, hi);
                        let (x, w) = slider_bar(p, *r);
                        let t = if hi > lo {
                            (v - lo) as f32 / (hi - lo) as f32
                        } else {
                            0.0
                        };
                        f.rects.push((x, y + 8, w, 8, [0x20, 0x20, 0x20]));
                        f.rects.push((
                            x + ((w - 6) as f32 * t) as i32,
                            y + 8,
                            6,
                            8,
                            [239, 228, 176],
                        ));
                        // A readout where the row has one, or its own bar.
                        let unit = match *r {
                            Row::Slider(_, u) => u,
                            _ => Some(""),
                        };
                        if let Some(unit) = unit {
                            f.text(
                                Face::Opt,
                                x + w + 52,
                                y,
                                format!("{v}{unit}"),
                                C_OPT_VALUE,
                                Align::Right,
                            );
                        }
                    }
                    Row::Choice(o) => {
                        let ns = names(o, s);
                        let v = (get(o, s).max(0) as usize).min(ns.len().saturating_sub(1));
                        f.text(
                            Face::Opt,
                            p.vx,
                            y,
                            ns.get(v).cloned().unwrap_or_default(),
                            C_OPT_VALUE,
                            Align::Left,
                        );
                    }
                    Row::Key(a) => {
                        let t = if self.wait_key == Some(a) {
                            "<?>".to_string()
                        } else {
                            s.keys.get(a).name()
                        };
                        f.text(Face::Opt, p.vx, y, t, C_OPT_VALUE, Align::Left);
                    }
                    Row::Page(t) => f.text(Face::Opt, p.vx, y, t, C_OPT_VALUE, Align::Left),
                }
            }
        }
    }
}

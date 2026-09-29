//! The game itself: creatures and what they do, independent of drawing.
//!
//! It keeps the games' own rules: their state numbers, timers in
//! milliseconds, the same constants and the same order of operations. The
//! renderer only reads the result.

pub mod c1;
pub mod c2ai;
pub mod fx;
pub mod script;
pub mod ship;

use std::f32::consts::PI;
use std::sync::Arc;

use bevy::math::Vec3;
use bevy::prelude::Resource;
use carn_formats::car::CharacterInfo;
use carn_formats::rsc::OF_ANIMATED;

use crate::area::Area;
use crate::game::GameKind;

pub const PMORPHTIME: i32 = 256;
pub const CS_ONWATER: i32 = 0x0001_0000;
pub const MAX_HEALTH: i32 = 128_000;

pub const HUNT_EAT: i32 = 0;
pub const HUNT_BREATH: i32 = 1;
pub const HUNT_FALL: i32 = 2;
pub const HUNT_KILL: i32 = 3;

// Carnivores 2 behaviours.
pub const AI_HUNTER: i32 = 0;
pub const AI_MOSH: i32 = 1;
pub const AI_GALL: i32 = 2;
pub const AI_DIMOR: i32 = 3;
pub const AI_PTERA: i32 = 4;
pub const AI_DIMET: i32 = 5;
pub const AI_BRACH: i32 = 6;
pub const AI_PARA: i32 = 10;
pub const AI_ANKY: i32 = 11;
pub const AI_STEGO: i32 = 12;
pub const AI_ALLO: i32 = 13;
pub const AI_CHASM: i32 = 14;
pub const AI_VELO: i32 = 15;
pub const AI_SPINO: i32 = 16;
pub const AI_CERAT: i32 = 17;
pub const AI_TREX: i32 = 18;

/// Animation slots: what a behaviour asks for, mapped per creature to the
/// animation in its model.
pub mod slot {
    pub const RUN: usize = 0;
    pub const WALK: usize = 1;
    pub const SLIDE: usize = 2;
    pub const SWIM: usize = 3;
    pub const JUMP: usize = 4;
    pub const DIE: usize = 5;
    pub const EAT: usize = 6;
    pub const SLEEP: usize = 7;
    pub const IDLE1: usize = 8;
    pub const IDLE2: usize = 9;
    pub const IDLE3: usize = 10;
    pub const IDLE4: usize = 11;
    pub const FLY: usize = 12;
    pub const GLIDE: usize = 13;
    pub const FALL: usize = 14;
    pub const ROAR: usize = 15;
    pub const LOOK1: usize = 16;
    pub const LOOK2: usize = 17;
    pub const SMELL1: usize = 18;
    pub const SMELL2: usize = 19;
    pub const COUNT: usize = 20;
}

#[derive(Clone, Debug)]
pub struct DinoInfo {
    pub name: String,
    pub file: String,
    pub pic: String,
    pub ai: i32,
    pub mass: f32,
    pub length: f32,
    pub radius: f32,
    pub health0: i32,
    pub base_score: i32,
    pub smell_k: f32,
    pub hear_k: f32,
    pub look_k: f32,
    pub sh_delta: f32,
    pub scale0: i32,
    pub scale_a: i32,
    pub danger: bool,
    /// Model animation for each slot, -1 for none.
    pub anim: [i32; slot::COUNT],
}

impl Default for DinoInfo {
    fn default() -> Self {
        DinoInfo {
            name: String::new(),
            file: String::new(),
            pic: String::new(),
            ai: 0,
            mass: 0.0,
            length: 0.0,
            radius: 0.0,
            health0: 0,
            base_score: 0,
            smell_k: 0.0,
            hear_k: 0.0,
            look_k: 0.0,
            sh_delta: 0.0,
            // The games' defaults.
            scale0: 800,
            scale_a: 600,
            danger: false,
            anim: [-1; slot::COUNT],
        }
    }
}

impl DinoInfo {
    /// Retail's animation layout for each behaviour.
    pub fn set_retail_layout(&mut self) {
        use slot::*;
        let a = &mut self.anim;
        *a = [-1; COUNT];
        let set = |a: &mut [i32; COUNT], l: &[(usize, i32)]| {
            for &(s, v) in l {
                a[s] = v;
            }
        };
        match self.ai {
            AI_MOSH => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (DIE, 2),
                    (IDLE1, 3),
                    (IDLE2, 4),
                    (SLEEP, 5),
                ],
            ),
            AI_GALL => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (SLIDE, 2),
                    (DIE, 3),
                    (IDLE1, 4),
                    (IDLE2, 5),
                    (SLEEP, 6),
                ],
            ),
            AI_DIMOR | AI_PTERA => set(a, &[(FLY, 0), (GLIDE, 1), (FALL, 2), (DIE, 3)]),
            AI_DIMET => set(
                a,
                &[
                    (WALK, 0),
                    (RUN, 1),
                    (IDLE1, 2),
                    (IDLE2, 3),
                    (DIE, 4),
                    (SLEEP, 5),
                ],
            ),
            AI_BRACH => set(a, &[(WALK, 0), (IDLE1, 1), (IDLE2, 2), (IDLE3, 3)]),
            AI_PARA => set(
                a,
                &[
                    (WALK, 0),
                    (RUN, 1),
                    (IDLE1, 2),
                    (IDLE2, 3),
                    (DIE, 4),
                    (SLEEP, 5),
                ],
            ),
            AI_ANKY => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (IDLE1, 2),
                    (IDLE2, 3),
                    (DIE, 4),
                    (SLEEP, 5),
                ],
            ),
            AI_STEGO => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (DIE, 2),
                    (IDLE1, 3),
                    (IDLE2, 4),
                    (SLEEP, 5),
                ],
            ),
            AI_ALLO => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (SWIM, 2),
                    (SLIDE, 3),
                    (JUMP, 4),
                    (DIE, 5),
                    (EAT, 6),
                    (SLEEP, 7),
                ],
            ),
            AI_CHASM => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (IDLE1, 2),
                    (IDLE2, 3),
                    (IDLE3, 4),
                    (DIE, 5),
                    (SLEEP, 6),
                ],
            ),
            AI_VELO => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (SLIDE, 2),
                    (SWIM, 3),
                    (JUMP, 4),
                    (DIE, 5),
                    (EAT, 6),
                    (SLEEP, 7),
                ],
            ),
            AI_SPINO => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (SLIDE, 2),
                    (SWIM, 1),
                    (IDLE1, 3),
                    (IDLE2, 4),
                    (JUMP, 5),
                    (DIE, 6),
                    (EAT, 7),
                    (SLEEP, 8),
                ],
            ),
            AI_CERAT => set(
                a,
                &[
                    (WALK, 0),
                    (RUN, 1),
                    (SWIM, 0),
                    (IDLE1, 2),
                    (IDLE2, 3),
                    (IDLE3, 4),
                    (DIE, 5),
                    (SLEEP, 6),
                    (EAT, 7),
                ],
            ),
            AI_TREX => set(
                a,
                &[
                    (RUN, 0),
                    (WALK, 1),
                    (ROAR, 2),
                    (SWIM, 3),
                    (LOOK1, 4),
                    (LOOK2, 5),
                    (SMELL1, 6),
                    (SMELL2, 7),
                    (DIE, 8),
                    (EAT, 9),
                    (SLEEP, 10),
                ],
            ),
            _ => set(a, &[(WALK, 0)]),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Character {
    pub ctype: usize,
    pub ai: i32,
    pub state_f: i32,
    pub state: i32,
    pub no_way_cnt: i32,
    pub no_find_cnt: i32,
    pub afraid_time: i32,
    pub tgtime: i32,
    pub pp_morph_time: i32,
    pub prev_phase: i32,
    pub prev_pf_time: i32,
    pub phase: i32,
    pub ftime: i32,
    pub vspeed: f32,
    pub rspeed: f32,
    pub bend: f32,
    pub scale: f32,
    pub slide: i32,
    pub slidex: f32,
    pub slidez: f32,
    pub tgx: f32,
    pub tgz: f32,
    pub pos: Vec3,
    pub tgalpha: f32,
    pub alpha: f32,
    pub beta: f32,
    pub tggamma: f32,
    pub gamma: f32,
    pub lookx: f32,
    pub lookz: f32,
    pub health: i32,
    pub blood_time: i32,
    pub blood_ttime: i32,
    /// Taken away (the ship lifted it); no longer simulated or drawn.
    pub removed: bool,
    /// On the ship's hook: drawn, but no longer animated.
    pub frozen: bool,
}

/// The C runtime's rand() as the games' compiler had it: 0..32767.
#[derive(Clone, Debug)]
pub struct Rng(pub u32);

impl Rng {
    pub fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214013).wrapping_add(2531011);
        ((self.0 >> 16) & 0x7FFF) as i32
    }
    /// -r..=r.
    pub fn si(&mut self, r: i32) -> i32 {
        ((self.rand() as i64 * (r as i64 * 2 + 1)) / 32767 - r as i64) as i32
    }
    /// 0..=r.
    pub fn r(&mut self, r: i32) -> i32 {
        if r <= 0 {
            return 0;
        }
        self.rand() % (r + 1)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Wind {
    pub alpha: f32,
    pub speed: f32,
    pub nv: Vec3,
}

/// What the creatures know of the hunter this frame.
#[derive(Clone, Debug, Default)]
pub struct HunterView {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub head_y: f32,
    /// Where the eye is and which way it looks.
    pub cam: Vec3,
    pub cam_alpha: f32,
    pub underwater: bool,
}

/// The camera circling the hunter's body after a death.
#[derive(Clone, Debug, Default)]
pub struct DemoPoint {
    /// 0 off; 1 closing in; from 2, milliseconds spent circling.
    pub time: i32,
    pub pos: Vec3,
    pub cindex: usize,
}

/// A sound the simulation wants played.
#[derive(Clone, Debug)]
pub enum SoundRef {
    /// A creature's own sound: its type and the index in its .CAR.
    Char(usize, usize),
    /// The hunter's scream on being killed.
    Scream,
    /// One of the drop ship's sounds.
    Ship(usize),
    /// A dinosaur call: its number (callN_x.wav) and which of the three.
    Call(i32, usize),
}

/// The hunt's tally.
#[derive(Clone, Debug, Default)]
pub struct HuntStats {
    pub success: i32,
    pub shots_made: i32,
    pub path: f32,
    pub time: f32,
}

#[derive(Clone, Debug)]
pub struct SoundEvent {
    pub sound: SoundRef,
    /// Where in the world, or None for a sound at the hunter.
    pub pos: Option<Vec3>,
    pub volume: i32,
}

#[derive(Resource)]
pub struct Sim {
    pub kind: GameKind,
    pub dinos: Vec<DinoInfo>,
    /// The loaded model and animations for each creature type.
    pub chinfo: Vec<Option<Arc<CharacterInfo>>>,
    pub chars: Vec<Character>,
    pub rng: Rng,
    pub time_dt: i32,
    pub real_time: i32,
    pub takt: u32,
    pub hunter: HunterView,
    pub my_health: i32,
    pub wind: Wind,
    pub view_r: i32,
    pub opt_agres: i32,
    pub opt_dens: i32,
    pub opt_sens: i32,
    pub day_night: i32,
    pub trophy: bool,
    pub tranq: bool,
    pub demo: DemoPoint,
    pub sounds: Vec<SoundEvent>,
    /// The line of text at the top left, and how long it has left
    /// (two seconds).
    pub message: Option<(String, i32)>,
    /// Counting down to the ship taking the hunter away; the
    /// hunt ends when it reaches 0.
    pub exit_time: i32,
    pub exit_done: bool,
    /// The hunt slots chosen (bit per AI id from 10, as retail had it).
    pub target_dino: u32,
    /// Which creature the behaviour being run belongs to.
    pub cur: usize,
    /// Objects between two points, from the last trace_look.
    pub objects_on_look: i32,
    /// Creatures killed that are worth a trophy, for the ship to fetch.
    pub kills: Vec<usize>,
    pub ship: ship::Ship,
    pub ship_tasks: Vec<usize>,
    pub ship_info: Option<Arc<CharacterInfo>>,
    pub hunt_stats: HuntStats,
    pub score: i32,
    pub trophies: Vec<ship::Trophy>,
    /// How long the last trophy's card stays up, ms.
    pub trophy_time: i32,
    /// The weapon in the hunter's hands, for the trophy record.
    pub current_weapon: usize,
    /// The first game's rules where they differ from Carnivores 2's.
    pub c1: bool,
    /// Which creature type is the hunter's body.
    pub hunter_ctype: usize,
    /// The creature type the call imitates, and the call's state: a lock between calls, which of the three recordings
    /// is next, and an answer on its way.
    pub call_ctype: Option<usize>,
    pub call_lock: i32,
    pub next_call: usize,
    pub answer: Option<(Vec3, i32, i32)>,
    /// How long the call's name stays up after changing it, ms.
    pub call_shown: i32,
    /// Particles, rings, blood spots and snow (fx.rs).
    pub fx: fx::Fx,
    /// -freeze: creatures stand still until they are hit (for testing).
    pub hold_still: bool,
    /// The hunt's equipment and modes: camouflage and cover scent make the
    /// hunter harder to see and smell, radar shows the hunted on the map;
    /// an observer is never noticed, never hurt and carries no weapon.
    pub camo: bool,
    pub scent: bool,
    pub radar: bool,
    pub observer: bool,
    /// The games' debug mode: the creatures take no notice of the hunter.
    pub debug: bool,
}

pub fn angle_difference(a: f32, b: f32) -> f32 {
    let mut a = (a - b).abs();
    if a > PI {
        a = 2.0 * PI - a;
    }
    a
}

pub fn corrected_alpha(a: f32, b: f32) -> f32 {
    let mut d = (a - b).abs();
    if d < PI {
        return (a + b) / 2.0;
    }
    d = a + PI * 2.0 - b;
    if d < 0.0 {
        d += 2.0 * PI;
    }
    if d > 2.0 * PI {
        d -= 2.0 * PI;
    }
    d
}

pub fn delta_func(a: &mut f32, b: f32, d: f32) {
    if b > *a {
        *a += d;
        if *a > b {
            *a = b;
        }
    } else {
        *a -= d;
        if *a < b {
            *a = b;
        }
    }
}

pub fn sgn(f: f32) -> f32 {
    if f < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// The heading of a vector, 0..2pi, found the way the originals did (a
/// ten-step binary search), so headings match theirs to the last bit that
/// matters.
pub fn find_vector_alpha(vx: f32, vy: f32) -> f32 {
    let adx = vx.abs();
    let ady = vy.abs();
    let mut alpha = PI / 4.0;
    let mut dalpha = PI / 8.0;
    for _ in 1..=10 {
        let (s, c) = alpha.sin_cos();
        alpha -= dalpha * sgn(adx * s - c * ady);
        dalpha /= 2.0;
    }
    if vx < 0.0 {
        if vy < 0.0 {
            alpha += PI;
        } else {
            alpha = PI - alpha;
        }
    } else if vy < 0.0 {
        alpha = 2.0 * PI - alpha;
    }
    alpha
}

pub fn wrap_2pi(a: &mut f32) {
    if *a < 0.0 {
        *a += 2.0 * PI;
    }
    if *a > 2.0 * PI {
        *a -= 2.0 * PI;
    }
}

impl Sim {
    fn ice(&self) -> bool {
        self.kind == GameKind::IceAge
    }

    /// The T-Rex's special rules (Carnivores 2 and the first game).
    pub fn is_trex(&self, c: &Character) -> bool {
        !self.ice() && c.ai == AI_TREX
    }

    /// A creature that flies (it is put down higher, and falls when shot).
    pub fn is_flyer(&self, ai: i32) -> bool {
        if self.ice() {
            ai == c2ai::ICE_ARCHEO
        } else {
            ai == AI_DIMOR || ai == AI_PTERA
        }
    }

    pub fn new(
        kind: GameKind,
        dinos: Vec<DinoInfo>,
        chinfo: Vec<Option<Arc<CharacterInfo>>>,
    ) -> Sim {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u32)
            .unwrap_or(1);
        Sim {
            kind,
            dinos,
            chinfo,
            chars: Vec::new(),
            rng: Rng(seed),
            time_dt: 16,
            real_time: 0,
            takt: 0,
            hunter: HunterView::default(),
            my_health: MAX_HEALTH,
            wind: Wind {
                alpha: 0.0,
                speed: 10.0,
                nv: Vec3::ZERO,
            },
            view_r: 74,
            opt_agres: 128,
            opt_dens: 128,
            opt_sens: 128,
            day_night: 1,
            trophy: false,
            tranq: false,
            demo: DemoPoint::default(),
            sounds: Vec::new(),
            message: None,
            exit_time: 0,
            exit_done: false,
            target_dino: 0,
            cur: 0,
            objects_on_look: 0,
            kills: Vec::new(),
            ship: ship::Ship::default(),
            ship_tasks: Vec::new(),
            ship_info: None,
            hunt_stats: HuntStats::default(),
            score: 0,
            trophies: Vec::new(),
            trophy_time: 0,
            current_weapon: 0,
            c1: kind == GameKind::Carnivores,
            hunter_ctype: if kind == GameKind::Carnivores {
                c1::HUNTER_CTYPE
            } else {
                0
            },
            call_ctype: None,
            call_lock: 0,
            next_call: 0,
            answer: None,
            call_shown: 0,
            fx: fx::Fx::default(),
            hold_still: false,
            camo: false,
            scent: false,
            radar: false,
            observer: false,
            debug: false,
        }
    }

    // ------------------------------------------------------------------
    // Animation bookkeeping
    // ------------------------------------------------------------------

    pub fn info(&self, c: &Character) -> Option<&CharacterInfo> {
        self.chinfo.get(c.ctype).and_then(|i| i.as_deref())
    }

    pub fn ani_count(&self, c: &Character) -> i32 {
        self.info(c).map(|i| i.animations.len() as i32).unwrap_or(0)
    }

    /// Length of an animation in milliseconds, never 0 (the games divide
    /// by it).
    pub fn ani_time(&self, c: &Character, phase: i32) -> i32 {
        self.info(c)
            .and_then(|i| i.animations.get(phase.max(0) as usize))
            .map(|a| a.ani_time.max(1))
            .unwrap_or(1000)
    }

    /// The animation a creature plays for a slot.
    pub fn danim(&self, c: &Character, s: usize) -> i32 {
        if s >= slot::COUNT {
            return 0;
        }
        let d = &self.dinos[c.ctype];
        let mut s = s;
        if s > slot::IDLE1 && s <= slot::IDLE4 {
            while s > slot::IDLE1 && d.anim[s] < 0 {
                s -= 1;
            }
        }
        let a = d.anim[s];
        if a < 0 || a >= self.ani_count(c) {
            return 0;
        }
        a
    }

    pub fn process_prev_phase(&self, c: &mut Character) {
        c.pp_morph_time += self.time_dt;
        if c.pp_morph_time > PMORPHTIME {
            c.prev_phase = c.phase;
        }
        c.prev_pf_time += self.time_dt;
        c.prev_pf_time %= self.ani_time(c, c.prev_phase);
    }

    /// A creature's animation sound, as the phase starts.
    pub fn activate_fx(&mut self, c: &Character) {
        if c.ai != AI_HUNTER && self.hunter.underwater {
            return;
        }
        let Some(info) = self.info(c) else { return };
        let fx = info
            .anifx
            .get(c.phase.max(0) as usize)
            .copied()
            .unwrap_or(-1);
        if fx < 0 || fx as usize >= info.sounds.len() {
            return;
        }
        let d = (Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z) - c.pos).length();
        if d > 68.0 * 256.0 {
            return;
        }
        self.sounds.push(SoundEvent {
            sound: SoundRef::Char(c.ctype, fx as usize),
            pos: Some(c.pos),
            volume: 256,
        });
    }

    pub fn reset_character(&mut self, c: &mut Character) {
        let d = &self.dinos[c.ctype];
        c.ai = d.ai;
        c.state = 0;
        c.state_f = 0;
        c.phase = 0;
        c.ftime = 0;
        c.prev_phase = 0;
        c.prev_pf_time = 0;
        c.pp_morph_time = 0;
        c.beta = 0.0;
        c.gamma = 0.0;
        c.tggamma = 0.0;
        c.bend = 0.0;
        c.rspeed = 0.0;
        c.afraid_time = 0;
        c.blood_ttime = 0;
        c.blood_time = 0;
        c.lookx = c.alpha.cos();
        c.lookz = c.alpha.sin();
        c.health = d.health0;
        if self.opt_agres > 128 {
            c.health = c.health * self.opt_agres / 128;
        }
        let (s0, sa) = (d.scale0, d.scale_a);
        c.scale = (s0 + self.rng.r(sa)) as f32 / 1000.0;
    }

    /// The trophy room's mounts: each trophy on the stand of
    /// its slot, facing into the room, standing still.
    pub fn place_trophies(&mut self, a: &Area, bodies: &[crate::profile::TrophyItem]) {
        self.chars.clear();
        for (slot, b) in bodies.iter().enumerate() {
            let ct = b.ctype as usize;
            if b.ctype <= 0
                || ct >= self.dinos.len()
                || self.chinfo.get(ct).map(|i| i.is_none()).unwrap_or(true)
            {
                continue;
            }
            let Some(&(lx, lz)) = a.landings.get(slot) else {
                continue;
            };
            let alpha = match slot {
                0..=5 => PI / 2.0,
                6..=11 => PI,
                12..=17 => PI * 3.0 / 2.0,
                _ => 0.0,
            };
            let mut c = Character {
                ctype: ct,
                alpha,
                ..Default::default()
            };
            self.reset_character(&mut c);
            c.state = slot as i32;
            c.scale = b.scale;
            let (x, z) = (lx as f32 * 256.0 + 128.0, lz as f32 * 256.0 + 128.0);
            c.pos = Vec3::new(x, a.land_h(x, z), z);
            self.chars.push(c);
        }
    }

    /// The mount the hunter stands in front of: its slot.
    pub fn trophy_in_front(&self) -> Option<usize> {
        let p = Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z);
        let mut found = None;
        for c in &self.chars {
            let at = c.pos + Vec3::new(c.lookx, 0.0, c.lookz) * 256.0 * 2.5;
            if (at - p).length() < 148.0 {
                found = Some(c.state.max(0) as usize);
            }
        }
        found
    }

    // ------------------------------------------------------------------
    // Where a creature can go
    // ------------------------------------------------------------------

    /// Whether a creature may be put at a spot:
    /// not in water, not on too steep ground, not in an object.
    pub fn check_place_collision_p(&self, a: &Area, v: &mut Vec3) -> bool {
        let ccx = v.x as i32 / 256;
        let ccz = v.z as i32 / 256;
        let lim = a.size - 16;
        if ccx < 4 || ccz < 4 || ccx > lim || ccz > lim {
            return true;
        }
        let f = |x: i32, z: i32| a.map.fmap[a.idx(x, z)];
        let fm = f(ccx - 1, ccz)
            | f(ccx, ccz - 1)
            | f(ccx - 1, ccz - 1)
            | f(ccx, ccz)
            | f(ccx, ccz + 1)
            | f(ccx + 1, ccz)
            | f(ccx + 1, ccz + 1);
        let (wbit, nobit) = match a.engine {
            carn_formats::Engine::C1 => (
                carn_formats::map::C1_FM_WATER,
                carn_formats::map::C1_FM_NOWAY,
            ),
            carn_formats::Engine::C2 => (
                carn_formats::map::C2_FM_WATER,
                carn_formats::map::C2_FM_NOWAY,
            ),
        };
        if fm & (wbit | nobit) != 0 {
            return true;
        }
        let h = a.land_h(v.x, v.z);
        v.y = h;
        for (dx, dz) in [
            (-164.0, -164.0),
            (164.0, -164.0),
            (-164.0, 164.0),
            (164.0, 164.0),
        ] {
            if (a.land_h(v.x + dx, v.z + dz) - h).abs() > 160.0 {
                return true;
            }
        }
        self.objects_block(a, *v, ccx, ccz)
    }

    fn objects_block(&self, a: &Area, v: Vec3, ccx: i32, ccz: i32) -> bool {
        for z in -2..=2 {
            for x in -2..=2 {
                let ob = a.map.omap[a.idx(ccx + x, ccz + z)];
                if ob == 255 {
                    continue;
                }
                let r = a.rsc.objects[ob as usize].info.radius;
                if r < 10 {
                    continue;
                }
                let cr = r as f32 + 64.0;
                let oz = (ccz + z) as f32 * 256.0 + 128.0;
                let ox = (ccx + x) as f32 * 256.0 + 128.0;
                if ((ox - v.x).powi(2) + (oz - v.z).powi(2)).sqrt() < cr {
                    return true;
                }
            }
        }
        false
    }

    fn water_around(&self, a: &Area, ccx: i32, ccz: i32) -> bool {
        let w = |x: i32, z: i32| a.map.deep_water(a.idx(x, z));
        w(ccx - 1, ccz)
            || w(ccx, ccz - 1)
            || w(ccx - 1, ccz - 1)
            || w(ccx, ccz)
            || w(ccx, ccz + 1)
            || w(ccx + 1, ccz)
            || w(ccx + 1, ccz + 1)
    }

    /// For the path probe: `wc` keeps it off water,
    /// `mc` out of objects.
    pub fn check_place_collision(&self, a: &Area, v: &mut Vec3, wc: bool, mc: bool) -> bool {
        let ccx = v.x as i32 / 256;
        let ccz = v.z as i32 / 256;
        let lim = a.size - 6;
        if ccx < 4 || ccz < 4 || ccx > lim || ccz > lim {
            return true;
        }
        if wc && self.water_around(a, ccx, ccz) {
            return true;
        }
        let h = a.land_h(v.x, v.z);
        if !a.map.deep_water(a.idx(ccx, ccz)) && (h - v.y).abs() > 64.0 {
            return true;
        }
        v.y = h;
        for (dx, dz) in [(-64.0, -64.0), (64.0, -64.0), (-64.0, 64.0), (64.0, 64.0)] {
            if (a.land_h(v.x + dx, v.z + dz) - h).abs() > 100.0 {
                return true;
            }
        }
        mc && self.objects_block(a, *v, ccx, ccz)
    }

    /// The movement test (no objects).
    pub fn check_place_collision2(&self, a: &Area, v: &mut Vec3, wc: bool) -> bool {
        let ccx = v.x as i32 / 256;
        let ccz = v.z as i32 / 256;
        let lim = a.size - 6;
        if ccx < 4 || ccz < 4 || ccx > lim || ccz > lim {
            return true;
        }
        if wc && self.water_around(a, ccx, ccz) {
            return true;
        }
        let h = a.land_h(v.x, v.z);
        v.y = h;
        for (dx, dz) in [(-64.0, -64.0), (64.0, -64.0), (-64.0, 64.0), (64.0, 64.0)] {
            if (a.land_h(v.x + dx, v.z + dz) - h).abs() > 100.0 {
                return true;
            }
        }
        false
    }

    pub fn check_possible_path(&self, a: &Area, c: &Character, wc: bool, mc: bool) -> i32 {
        let mut p = c.pos;
        let (lz, lx) = c.tgalpha.sin_cos();
        let mut n = 0;
        for _ in 0..20 {
            p.x += lx * 64.0;
            p.z += lz * 64.0;
            if self.check_place_collision(a, &mut p, wc, mc) {
                n += 1;
            }
        }
        n
    }

    /// Turns the wanted heading to the nearest clear one.
    pub fn look_for_a_way(&self, a: &Area, c: &mut Character, wc: bool, mc: bool) {
        let alpha = c.tgalpha;
        let mut dalpha = 15.0f32;
        let mut afound = alpha;
        let mut maxp = 16;
        if self.check_possible_path(a, c, wc, mc) == 0 {
            c.no_way_cnt = 0;
            return;
        }
        c.no_way_cnt += 1;
        for i in 0..12 {
            for sign in [1.0f32, -1.0] {
                c.tgalpha = alpha + sign * dalpha * PI / 180.0;
                let curp = self.check_possible_path(a, c, wc, mc) + (i >> 1);
                if curp == 0 {
                    return;
                }
                if curp < maxp {
                    maxp = curp;
                    afound = c.tgalpha;
                }
            }
            dalpha += 15.0;
        }
        c.tgalpha = afound;
    }

    pub fn move_character(
        &self,
        a: &Area,
        c: &mut Character,
        dx: f32,
        dz: f32,
        wc: bool,
        _mc: bool,
    ) {
        let mut p = c.pos;
        if self.check_place_collision2(a, &mut p, wc) {
            c.pos.x += dx / 2.0;
            c.pos.z += dz / 2.0;
            return;
        }
        p.x += dx;
        p.z += dz;
        if !self.check_place_collision2(a, &mut p, wc) {
            c.pos = p;
            return;
        }
        p = c.pos;
        p.x += dx / 2.0;
        p.z += dz / 2.0;
        if !self.check_place_collision2(a, &mut p, wc) {
            c.pos = p;
        }
        p = c.pos;
        p.x += dx / 4.0;
        p.z += dz / 4.0;
        c.pos = p;
    }

    pub fn set_new_target_place(&mut self, a: &Area, c: &mut Character, r: f32) {
        let mut r = r;
        let mut tr = 0;
        let hi = (a.size - 6) as f32 * 256.0;
        loop {
            let mut p = Vec3::ZERO;
            p.x = (c.pos.x + self.rng.si(r as i32) as f32).clamp(512.0, hi);
            p.z = (c.pos.z + self.rng.si(r as i32) as f32).clamp(512.0, hi);
            p.y = a.land_h(p.x, p.z);
            tr += 1;
            if tr < 128 && (p.x - c.pos.x).abs() + (p.z - c.pos.z).abs() < r / 2.0 {
                continue;
            }
            r += 512.0;
            if tr < 256 && self.check_place_collision_p(a, &mut p) {
                continue;
            }
            c.tgtime = 0;
            c.tgx = p.x;
            c.tgz = p.z;
            return;
        }
    }

    pub fn set_new_target_place_brahi(&mut self, a: &Area, c: &mut Character, r: f32) {
        let mut tr = 0;
        let hi = (a.size - 6) as f32 * 256.0;
        loop {
            let mut p = Vec3::ZERO;
            p.x = (c.pos.x + self.rng.si(r as i32) as f32).clamp(512.0, hi);
            p.z = (c.pos.z + self.rng.si(r as i32) as f32).clamp(512.0, hi);
            tr += 1;
            if tr < 16 && (p.x - c.pos.x).abs() + (p.z - c.pos.z).abs() < r / 2.0 {
                continue;
            }
            p.y = a.land_h(p.x, p.z);
            let wy = a.land_up_h(p.x, p.z) - p.y;
            if tr < 128 && (!(200.0..=400.0).contains(&wy)) {
                continue;
            }
            c.tgtime = 0;
            c.tgx = p.x;
            c.tgz = p.z;
            return;
        }
    }

    /// Puts a creature that has fallen far behind back into play ahead of
    /// the hunter.
    pub fn replace_character_forward(&mut self, a: &Area, c: &mut Character) -> bool {
        let al = self.hunter.cam_alpha + self.rng.si(2048) as f32 / 2048.0;
        let (sa, ca) = al.sin_cos();
        let base = if self.c1 { 36 } else { self.view_r };
        let d1 = (base + self.rng.r(10)) as f32 * 256.0;
        let d2 = (base + self.rng.r(10)) as f32 * 256.0;
        let mut p = Vec3::new(self.hunter.x + sa * d1, 0.0, self.hunter.z - ca * d2);
        p.y = a.land_h(p.x, p.z);
        let hi = if self.c1 {
            500.0 * 256.0
        } else {
            (a.size - 24) as f32 * 256.0
        };
        if p.x < 16.0 * 256.0 || p.z < 16.0 * 256.0 || p.x > hi || p.z > hi {
            return false;
        }
        if self.check_place_collision_p(a, &mut p) {
            return false;
        }
        c.state = 0;
        c.pos = p;
        self.set_new_target_place(a, c, 2048.0);
        if self.is_flyer(c.ai) && (self.c1 || self.ice() || c.ai == AI_DIMOR) {
            c.pos.y += 1048.0;
        }
        true
    }

    /// Pitch and roll from the ground under the creature, and its height.
    pub fn think_y_beta_gamma(
        &self,
        a: &Area,
        c: &mut Character,
        blook: f32,
        glook: f32,
        blim: f32,
        glim: f32,
    ) {
        c.pos.y = a.land_h(c.pos.x, c.pos.z);
        let hlook = a.land_h(c.pos.x + c.lookx * blook, c.pos.z + c.lookz * blook);
        let hlook2 = a.land_h(c.pos.x - c.lookx * blook, c.pos.z - c.lookz * blook);
        delta_func(
            &mut c.beta,
            (hlook2 - hlook) / (blook * 3.2),
            self.time_dt as f32 / 800.0,
        );
        c.beta = c.beta.clamp(-blim, blim);
        let hlook = a.land_h(c.pos.x + c.lookz * glook, c.pos.z - c.lookx * glook);
        let hlook2 = a.land_h(c.pos.x - c.lookz * glook, c.pos.z + c.lookx * glook);
        c.tggamma = ((hlook - hlook2) / (glook * 3.2)).clamp(-glim, glim);
    }

    /// Switches to a new phase, keeping the old one for the blend.
    pub fn begin_morph(&self, c: &mut Character, old_phase: i32, old_ftime: i32) {
        if c.pp_morph_time > 128 {
            c.prev_phase = old_phase;
            c.prev_pf_time = old_ftime;
            c.pp_morph_time = 0;
        }
    }

    // ------------------------------------------------------------------
    // The hunter's death
    // ------------------------------------------------------------------

    /// The hunter dies: his body joins the creatures, killed
    /// by `killer` or by the world, and the camera starts to circle it.
    /// A line at the top left for two seconds.
    pub fn add_message(&mut self, text: &str) {
        self.message = Some((text.to_string(), 2000));
    }

    pub fn add_dead_body(&mut self, a: &Area, killer: Option<&Character>, phase: i32) {
        if self.my_health == 0 {
            return;
        }
        if self.exit_time > 0 {
            self.add_message("Transportation cancelled.");
        }
        self.exit_time = 0;
        let mut body = Character {
            ctype: self.hunter_ctype,
            alpha: self.hunter.cam_alpha,
            ..Default::default()
        };
        self.reset_character(&mut body);
        if phase != HUNT_BREATH {
            self.sounds.push(SoundEvent {
                sound: SoundRef::Scream,
                pos: None,
                volume: 256,
            });
        }
        body.health = 0;
        self.my_health = 0;
        if let Some(k) = killer {
            let ice = self.ice();
            let pl = match k.ai {
                _ if ice => 170.0,
                AI_SPINO if !self.c1 => 200.0,
                AI_CERAT if !self.c1 => 320.0,
                AI_TREX => 0.0,
                _ => 170.0,
            };
            body.pos.x = k.pos.x + k.lookx * pl * k.scale;
            body.pos.z = k.pos.z + k.lookz * pl * k.scale;
            body.pos.y = a.land_qh(body.pos.x, body.pos.z, self.hunter.y);
        } else {
            body.pos = Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z);
        }
        body.phase = phase;
        body.prev_phase = phase;
        self.activate_fx(&body);
        self.demo = DemoPoint {
            time: 1,
            pos: body.pos,
            cindex: self.chars.len(),
        };
        self.chars.push(body);
    }

    fn animate_hunt_dead(&mut self, a: &Area, c: &mut Character) {
        self.process_prev_phase(c);
        c.ftime += self.time_dt;
        let at = self.ani_time(c, c.phase);
        if c.ftime >= at {
            c.ftime = if c.phase == 2 { at - 1 } else { 0 };
            if c.phase == 1 {
                c.ftime = 0;
                c.phase = 2;
            }
            self.activate_fx(c);
        }
        let h = a.land_h(c.pos.x, c.pos.z);
        delta_func(&mut c.pos.y, h, self.time_dt as f32 / 5.0);
        if c.phase == 2 && c.pos.y > h + 3.0 {
            c.ftime = 0;
        }
        if c.pos.y < h + 256.0 {
            let blook = 256.0;
            let hlook = a.land_h(c.pos.x + c.lookx * blook, c.pos.z + c.lookz * blook);
            let hlook2 = a.land_h(c.pos.x - c.lookx * blook, c.pos.z - c.lookz * blook);
            delta_func(
                &mut c.beta,
                (hlook2 - hlook) / (blook * 3.2),
                self.time_dt as f32 / 1800.0,
            );
            c.beta = c.beta.clamp(-0.4, 0.4);
            let glook = 256.0;
            let hlook = a.land_h(c.pos.x + c.lookz * glook, c.pos.z - c.lookx * glook);
            let hlook2 = a.land_h(c.pos.x - c.lookz * glook, c.pos.z + c.lookx * glook);
            c.tggamma = ((hlook - hlook2) / (glook * 3.2)).clamp(-0.4, 0.4);
            delta_func(&mut c.gamma, c.tggamma, self.time_dt as f32 / 1800.0);
        }
        // Killed by the T-Rex, the body stays in its jaws.
        if let Some(k) = self.chars.get(self.demo.cindex) {
            if k.ai == AI_TREX && !k.removed {
                c.pos = k.pos;
                c.ftime = k.ftime;
                c.beta = k.beta;
                c.gamma = k.gamma;
            }
        }
    }

    /// The camera after a death: closes in on the
    /// body, then circles it. Returns the eye, its heading and pitch.
    pub fn demo_camera(&mut self, a: &Area, cam: &mut Vec3, alpha: &mut f32, beta: &mut f32) {
        let dt = self.time_dt as f32;
        let Some(target) = self.chars.get(self.demo.cindex) else {
            return;
        };
        let mut dpos = target.pos;
        let mut base = 824.0;
        dpos.y += 256.0;
        if target.ai == AI_TREX {
            dpos.y += 512.0;
            base = 1424.0;
        }
        self.demo.pos = dpos;
        let pp = Vec3::new(dpos.x, cam.y, dpos.z);
        let l = (pp - *cam).length();
        if self.demo.time == 1 && l < base {
            self.demo.time = 2;
        }
        let nv = (dpos - *cam).normalize_or_zero();
        if self.demo.time == 1 {
            delta_func(&mut cam.x, dpos.x, nv.x.abs() * dt * 3.0);
            delta_func(&mut cam.z, dpos.z, nv.z.abs() * dt * 3.0);
        } else {
            self.demo.time += self.time_dt;
            *alpha += dt / 1224.0;
            let (sa, ca) = alpha.sin_cos();
            delta_func(&mut cam.x, dpos.x - sa * base, dt);
            delta_func(&mut cam.z, dpos.z + ca * base, dt);
        }
        let horiz = ((dpos.x - cam.x).powi(2) + (dpos.z - cam.z).powi(2)).sqrt();
        let mut b = find_vector_alpha(horiz, dpos.y - cam.y - 400.0);
        if b > PI {
            b -= 2.0 * PI;
        }
        delta_func(beta, -b, dt / 4000.0);
        let h = a.land_qh(cam.x, cam.z, cam.y);
        delta_func(&mut cam.y, h + 128.0, dt / 8.0);
        if cam.y < h + 80.0 {
            cam.y = h + 80.0;
        }
    }

    // ------------------------------------------------------------------
    // Senses
    // ------------------------------------------------------------------

    /// A loud noise at `pos` (a shot) startles everything within range.
    pub fn make_noise(&mut self, pos: Vec3, range: f32) {
        let ice = self.ice();
        for c in self.chars.iter_mut() {
            if c.health == 0 || c.removed {
                continue;
            }
            let l = (c.pos - pos).length();
            if l > range {
                continue;
            }
            if c.ai == AI_TREX && !ice {
                if c.state == 0 {
                    c.state = 2;
                }
            } else {
                c.afraid_time = ((10.0 + (range - l) / 256.0) as i32) * 1024;
                c.state = 2;
                c.no_find_cnt = 0;
            }
        }
    }

    /// Whether the line between two points is blocked by the ground; counts
    /// the objects close to it on the way.
    pub fn trace_look(&mut self, a: &Area, from: Vec3, to: Vec3) -> bool {
        let nv = (to - from).normalize_or_zero();
        let mut nvp = to - from;
        nvp.y = 0.0;
        let nvp = nvp.normalize_or_zero();
        self.objects_on_look = 0;
        let (axi, azi) = ((from.x / 256.0) as i32, (from.z / 256.0) as i32);
        let (bxi, bzi) = ((to.x / 256.0) as i32, (to.z / 256.0) as i32);
        // Cells from 2 to 510 in the first game, to 1010 in the later ones.
        let lim = if self.c1 { 510 } else { a.size - 14 };
        let mut tb = to;
        for zz in (azi.min(bzi) - 2)..=(azi.max(bzi) + 2) {
            for xx in (axi.min(bxi) - 2)..=(axi.max(bxi) + 2) {
                if xx < 2 || xx > lim || zz < 2 || zz > lim {
                    continue;
                }
                let v = |x: i32, z: i32| {
                    Vec3::new(
                        x as f32 * 256.0,
                        a.surface_steps(x, z) as f32 * a.hs,
                        z as f32 * 256.0,
                    )
                };
                let rev = a.map.reverse(a.idx(xx, zz));
                let (v0, v1, v2) = if rev {
                    (v(xx, zz), v(xx + 1, zz), v(xx, zz + 1))
                } else {
                    (v(xx, zz), v(xx + 1, zz), v(xx + 1, zz + 1))
                };
                if trace_plane(from, &mut tb, nv, v0, v1, v2) {
                    return true;
                }
                let (w0, w1, w2) = if rev {
                    (v2, v1, v(xx + 1, zz + 1))
                } else {
                    (v0, v2, v(xx, zz + 1))
                };
                if trace_plane(from, &mut tb, nv, w0, w1, w2) {
                    return true;
                }
                let ob = a.map.omap[a.idx(xx, zz)];
                if ob != 255 {
                    let mut o =
                        Vec3::new(xx as f32 * 256.0 + 128.0, tb.y, zz as f32 * 256.0 + 128.0);
                    let s1 = -(o - tb).dot(nv);
                    o.y = from.y;
                    let s2 = (o - from).dot(nv);
                    if s1 > 0.0 && s2 > 0.0 && point_to_vector_d(from, nvp, o) < 180.0 {
                        self.objects_on_look += 1;
                        if a.rsc.objects[ob as usize].info.radius > 32 {
                            self.objects_on_look += 1;
                        }
                    }
                }
            }
        }
        false
    }

    /// Whether any creature notices the hunter this frame, by sight or by
    /// smell.
    pub fn check_afraid(&mut self, a: &Area) {
        if self.my_health == 0
            || self.trophy
            || self.hunter.underwater
            || self.observer
            || self.debug
        {
            return;
        }
        let kmask = if self.camo { 1.5 } else { 1.0 };
        let kscent = if self.scent { 1.5 } else { 1.0 };
        let ppos = Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z);
        let wlook = self.wind.nv;
        for i in 0..self.chars.len() {
            let c = self.chars[i].clone();
            if c.health == 0 || c.removed || c.ai < 10 {
                continue;
            }
            if c.afraid_time != 0 || c.state == 1 {
                continue;
            }
            let mut rlook = ppos - c.pos;
            let c1 = self.c1;
            let mut kr = if c1 {
                rlook.length() / 256.0 / 30.0
            } else {
                // The view radius halved as a whole number.
                rlook.length() / 256.0 / (32.0 + (self.view_r / 2) as f32)
            };
            rlook = rlook.normalize_or_zero();
            kr *= if c1 {
                2.5 / (1.5 + self.opt_sens as f32)
            } else {
                2.5 / (1.5 + self.opt_sens as f32 / 128.0)
            };
            if kr > if c1 { 2.0 } else { 3.0 } {
                continue;
            }
            let clook = Vec3::new(c.lookx, 0.0, c.lookz);
            let kwind = wlook.dot(rlook) * self.wind.speed / 10.0;
            let klook = -clook.dot(rlook);
            let kstand = match (c1, self.hunter.head_y > 180.0) {
                (false, true) => 0.7,
                (false, false) => 1.2,
                (true, true) => 1.0,
                (true, false) => 1.4,
            };
            let d = &self.dinos[c.ctype];
            let (look_k, smell_k, is_trex) =
                (d.look_k.max(0.01), d.smell_k.max(0.01), self.is_trex(&c));
            let mut k_look = kr * ((klook + 3.0) / 3.0) * kstand * kmask;
            if klook > 0.3 {
                k_look *= 2.0;
            }
            if klook > 0.8 {
                k_look *= 2.0;
            }
            k_look /= look_k;
            let eye = Vec3::new(c.pos.x, c.pos.y + 220.0, c.pos.z);
            if k_look < 1.0
                && self.trace_look(
                    a,
                    eye,
                    Vec3::new(ppos.x, ppos.y + self.hunter.head_y / 2.0, ppos.z),
                )
            {
                k_look *= 1.3;
            }
            if k_look < 1.0
                && self.trace_look(
                    a,
                    eye,
                    Vec3::new(ppos.x, ppos.y + self.hunter.head_y, ppos.z),
                )
            {
                k_look = 2.0;
            }
            k_look *= 1.0 + self.objects_on_look as f32 / if c1 { 4.0 } else { 6.0 };
            let mut k_smell = if c1 {
                kr * ((kwind + 2.5) / 2.5) * ((klook + 4.0) / 4.0) * kscent
            } else {
                kr * ((kwind + 2.0) / 2.0) * ((klook + 3.0) / 3.0) * kscent
            };
            if kwind > 0.0 {
                k_smell *= 2.0;
            }
            k_smell /= smell_k;
            let kres = k_look.min(k_smell);
            if kres < 1.0 {
                let kres = kres.min(kr);
                let c = &mut self.chars[i];
                c.afraid_time = (1.0 / (kres + 0.1) * 10.0 * 1000.0) as i32;
                c.state = 2;
                if is_trex && k_look > k_smell {
                    c.state = 3;
                }
                c.no_find_cnt = 0;
            }
        }
    }

    // ------------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------------

    /// The number of a creature type's call recordings (callN_a.wav), if
    /// it has any: the hunted kinds, counted from the first.
    pub fn call_number(&self, ctype: usize) -> Option<i32> {
        let d = self.dinos.get(ctype)?;
        if self.c1 {
            (4..=10).contains(&ctype).then_some(ctype as i32 - 3)
        } else {
            (d.ai >= 10).then_some(d.ai - 9)
        }
    }

    /// The types a call can be made for, in order.
    /// The kinds whose call the hunter has: the ones picked for the hunt
    /// (changing the call skips the rest).
    pub fn callable(&self) -> Vec<usize> {
        (0..self.dinos.len())
            .filter(|&t| self.call_number(t).is_some() && self.chinfo[t].is_some())
            .filter(|&t| {
                let bit = if self.c1 {
                    t as i32 - 4
                } else {
                    self.dinos[t].ai
                };
                (0..32).contains(&bit) && self.target_dino & (1 << bit) != 0
            })
            .collect()
    }

    /// C: the next call.
    pub fn change_call(&mut self) {
        let list = self.callable();
        if list.is_empty() {
            return;
        }
        if self.call_shown > 0 {
            let i = self
                .call_ctype
                .and_then(|c| list.iter().position(|&t| t == c))
                .map(|i| i + 1)
                .unwrap_or(0);
            self.call_ctype = Some(list[i % list.len()]);
        } else if self.call_ctype.is_none() {
            self.call_ctype = Some(list[0]);
        }
        self.call_shown = 2048;
    }

    /// F: call. Creatures of the kind that are calm and within
    /// reach come toward the hunter, and the nearest may answer. A
    /// dangerous one's call scatters the small creatures.
    pub fn make_call(&mut self) {
        if self.hunter.underwater
            || self.trophy
            || self.observer
            || self.target_dino == 0
            || self.call_lock > 0
        {
            return;
        }
        let Some(ct) = self.call_ctype.or_else(|| self.callable().first().copied()) else {
            return;
        };
        self.call_ctype = Some(ct);
        let Some(num) = self.call_number(ct) else {
            return;
        };
        self.call_lock = 1024 * 3;
        self.next_call = (self.next_call + (self.real_time % 2) as usize + 1) % 3;
        self.sounds.push(SoundEvent {
            sound: SoundRef::Call(num, self.next_call),
            pos: None,
            volume: 256,
        });
        let danger = self.dinos[ct].danger;
        let ai = self.dinos[ct].ai;
        let hunter = Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z);
        // Who hears it: Carnivores 1 reaches 98 cells and answers from
        // within 200; the later games reach twice the view range.
        let (reach, mut dmin) = if self.c1 {
            (98.0 * 256.0, 200.0 * 256.0)
        } else {
            (self.view_r as f32 * 400.0, 512.0 * 256.0)
        };
        let mut best = None;
        for i in 0..self.chars.len() {
            if danger && self.chars[i].ai < 10 && self.chars[i].ai > 0 {
                let t = (10 + self.rng.r(5)) * 1024;
                let c = &mut self.chars[i];
                c.state = 2;
                c.afraid_time = t;
            }
            let c = &self.chars[i];
            let same = if self.c1 { c.ctype == ct } else { c.ai == ai };
            if !same || c.afraid_time != 0 || c.state != 0 || c.health == 0 || c.removed {
                continue;
            }
            let d = (hunter - c.pos).length();
            if d < reach {
                if self.rng.r(128) > 32 && d < dmin {
                    dmin = d;
                    best = Some(i);
                }
                let (ox, oz) = (self.rng.si(1800) as f32, self.rng.si(1800) as f32);
                let c = &mut self.chars[i];
                c.tgx = hunter.x + ox;
                c.tgz = hunter.z + oz;
            }
        }
        if let Some(i) = best {
            let at = hunter - (self.chars[i].pos - hunter) / -3.0;
            let t = 2000 + self.rng.r(2000);
            self.answer = Some((at, t, num));
        }
    }

    // ------------------------------------------------------------------
    // The frame
    // ------------------------------------------------------------------

    /// One frame of the world's life: the creatures, the wind and their
    /// senses.
    pub fn update(&mut self, a: &Area, dt_ms: i32) {
        self.time_dt = dt_ms.clamp(1, 1000);
        self.real_time += self.time_dt;
        self.takt = self.takt.wrapping_add(1);
        if self.my_health > 0 {
            self.my_health = (self.my_health + self.time_dt * 4).min(MAX_HEALTH);
        }

        if self.takt & 1 == 1 {
            self.wind.alpha += self.rng.si(16) as f32 / 4096.0;
            self.wind.speed += self.rng.si(400) as f32 / 6400.0;
        }
        self.wind.speed = self.wind.speed.clamp(4.0, 18.0);
        self.wind.nv = Vec3::new(self.wind.alpha.sin(), 0.0, -self.wind.alpha.cos());

        for ci in std::mem::take(&mut self.kills) {
            let w = self.current_weapon;
            self.add_ship_task(a, ci, w);
        }
        if self.trophy_time > 0 {
            self.trophy_time = (self.trophy_time - self.time_dt).max(0);
        }
        self.call_lock = (self.call_lock - self.time_dt).max(0);
        if let Some((_, t)) = self.message.as_mut() {
            *t -= self.time_dt;
            if *t <= 0 {
                self.message = None;
            }
        }
        if self.exit_time > 0 {
            self.exit_time -= self.time_dt;
            if self.exit_time <= 0 {
                self.exit_time = 0;
                self.exit_done = true;
            }
        }
        self.call_shown = (self.call_shown - self.time_dt).max(0);
        if let Some((at, t, num)) = self.answer {
            let t = t - self.time_dt;
            if t <= 0 {
                self.answer = None;
                let v = (self.rng.r(128) % 3) as usize;
                self.sounds.push(SoundEvent {
                    sound: SoundRef::Call(num, v),
                    pos: Some(at),
                    volume: 256,
                });
            } else {
                self.answer = Some((at, t, num));
            }
        }
        if !self.trophy {
            for i in 0..self.chars.len() {
                if self.chars[i].removed || self.chars[i].frozen {
                    continue;
                }
                self.cur = i;
                let mut c = std::mem::take(&mut self.chars[i]);
                c.tgtime += self.time_dt;
                if c.tgtime > 30 * 1000 {
                    self.set_new_target_place(a, &mut c, 2048.0);
                }
                self.bleed(a, &mut c);
                self.animate(a, &mut c);
                self.chars[i] = c;
            }
        }
        self.check_afraid(a);
        self.animate_ship(a);
        self.animate_fx(a);
    }

    fn animate(&mut self, a: &Area, c: &mut Character) {
        match self.kind {
            GameKind::Carnivores2 | GameKind::IceAge | GameKind::Carnivores => {
                c2ai::animate(self, a, c)
            }
        }
    }

    // ------------------------------------------------------------------
    // Placing the creatures
    // ------------------------------------------------------------------

    fn random_spot(&mut self, a: &Area, cx: f32, cz: f32, r: i32) -> Vec3 {
        let x = cx + self.rng.si(r) as f32;
        let z = cz + self.rng.si(r) as f32;
        Vec3::new(x, a.land_h(x, z), z)
    }

    fn new_char(&mut self, ctype: usize, pos: Vec3) -> Character {
        let mut c = Character {
            ctype,
            pos,
            tgx: pos.x,
            tgz: pos.z,
            ..Default::default()
        };
        self.reset_character(&mut c);
        c
    }

    /// Carnivores 2's placement: the small ambient
    /// creatures near the hunter, three brachiosaurs in their lakes, and
    /// the hunt itself away from him.
    pub fn place_characters_c2(&mut self, a: &Area) {
        self.chars.clear();
        let ctype_of = |ai: i32, dinos: &[DinoInfo]| dinos.iter().position(|d| d.ai == ai);
        let (px, pz) = (self.hunter.x, self.hunter.z);
        let center = (a.size / 2 * 256) as f32;

        let mut mc = 5 + self.opt_dens / 80;
        if self.day_night == 2 {
            mc /= 2;
        }
        let mut tr = 0;
        let ice = self.kind == GameKind::IceAge;
        for c in 0..mc {
            let ctype = 1 + (c % if ice { 2 } else { 5 }) as usize;
            if ctype >= self.dinos.len()
                || self.chinfo.get(ctype).map(|i| i.is_none()).unwrap_or(true)
            {
                continue;
            }
            let mut ok = false;
            let mut p = Vec3::ZERO;
            while tr <= 10240 {
                tr += 1;
                p = self.random_spot(a, px, pz, 10040);
                if !self.check_place_collision_p(a, &mut p) {
                    ok = true;
                    break;
                }
            }
            if !ok {
                break;
            }
            let mut ch = self.new_char(ctype, p);
            let up = if ice {
                ch.ai == c2ai::ICE_ARCHEO
            } else {
                ch.ai == AI_DIMET || ch.ai == AI_PTERA
            };
            if up {
                ch.pos.y += 2048.0;
            }
            self.chars.push(ch);
        }

        if ice {
            // No brachiosaurs in the ice.
        } else if let Some(brach) = ctype_of(AI_BRACH, &self.dinos) {
            if self.chinfo.get(brach).map(|i| i.is_some()).unwrap_or(false) {
                let mut tr = 0;
                for _ in 0..3 {
                    let mut found = None;
                    while tr <= 10240 {
                        tr += 1;
                        let x = center + self.rng.si(50 * 256) as f32 * 10.0;
                        let z = center + self.rng.si(50 * 256) as f32 * 10.0;
                        let y = a.land_h(x, z);
                        let wy = a.land_up_h(x, z) - y;
                        if (220.0..=380.0).contains(&wy) {
                            found = Some(Vec3::new(x, y, z));
                            break;
                        }
                    }
                    let Some(p) = found else { break };
                    let ch = self.new_char(brach, p);
                    self.chars.push(ch);
                }
            }
        }

        // The hunt: types with a behaviour from 10 up.
        let hunt: Vec<usize> = (0..self.dinos.len())
            .filter(|&i| {
                self.dinos[i].ai >= 10 && self.chinfo.get(i).map(|x| x.is_some()).unwrap_or(false)
            })
            .collect();
        if hunt.is_empty() {
            return;
        }
        let picked: Vec<usize> = hunt
            .iter()
            .copied()
            .filter(|&i| self.target_dino & (1 << self.dinos[i].ai) != 0)
            .collect();
        let mc = 8 + self.opt_dens / 30 + self.rng.r(6);
        let mut tr = 0;
        for c in 0..mc {
            let ctype = if c < 4 || picked.is_empty() {
                // Ice Age drew the opening few from the first seven kinds.
                let n = if ice { hunt.len().min(7) } else { hunt.len() };
                hunt[self.rng.r(n as i32 - 1) as usize]
            } else if ice && c >= 10 {
                picked[self.rng.r(picked.len() as i32 - 1) as usize]
            } else {
                picked[c as usize % picked.len()]
            };
            let mut found = None;
            while tr <= 10240 {
                tr += 1;
                let x = center + self.rng.si(50 * 256) as f32 * 10.0;
                let z = center + self.rng.si(50 * 256) as f32 * 10.0;
                let mut p = Vec3::new(x, a.land_h(x, z), z);
                if (p.x - px).abs() + (p.z - pz).abs() < 256.0 * 40.0 {
                    continue;
                }
                if self.check_place_collision_p(a, &mut p) {
                    continue;
                }
                found = Some(p);
                break;
            }
            let Some(p) = found else { break };
            let ch = self.new_char(ctype, p);
            self.chars.push(ch);
        }
    }
}

impl Sim {
    /// A creature's vertices this frame: its
    /// animation between two frames, blended into the one it is leaving,
    /// bent into its turn, pitched, rolled and scaled. In model space; the
    /// caller turns it by the heading and places it.
    pub fn morph(&self, c: &Character, out: &mut Vec<[f32; 3]>) {
        out.clear();
        let Some(info) = self.info(c) else { return };
        let vc = info.model.vertices.len();
        let frame = |phase: i32, ftime: i32| -> Option<(usize, usize, f32)> {
            let an = info.animations.get(phase.max(0) as usize)?;
            if an.frames == 0 || an.ani_time <= 0 || an.data.len() < an.frames * vc * 3 {
                return None;
            }
            let cur = ((an.frames as i64 - 1) * ftime.max(0) as i64 * 256) / an.ani_time as i64;
            let spline = (cur & 0xFF) as f32 / 256.0;
            let f = ((cur >> 8) as usize).min(an.frames - 1);
            Some((phase as usize, f, spline))
        };
        let Some((ph, f, k)) = frame(c.phase, c.ftime) else {
            out.extend(info.model.vertices.iter().map(|v| v.pos));
            return;
        };
        let pm = if c.phase != c.prev_phase && c.pp_morph_time < PMORPHTIME {
            frame(c.prev_phase, c.prev_pf_time)
        } else {
            None
        };
        let get = |ph: usize, f: usize, k: f32, v: usize| -> [f32; 3] {
            let an = &info.animations[ph];
            let n = (f + 1).min(an.frames - 1);
            let a = &an.data[(f * vc + v) * 3..];
            let b = &an.data[(n * vc + v) * 3..];
            let (k1, k2) = ((1.0 - k) / 8.0, k / 8.0);
            [
                a[0] as f32 * k1 + b[0] as f32 * k2,
                a[1] as f32 * k1 + b[1] as f32 * k2,
                -(a[2] as f32 * k1 + b[2] as f32 * k2),
            ]
        };
        let pmk1 = c.pp_morph_time as f32 / PMORPHTIME as f32;
        let pmk2 = 1.0 - pmk1;
        let scale = c.scale;
        let (sb, cb) = c.beta.sin_cos();
        let (sb, cb) = (sb * scale, cb * scale);
        let (sg, cg) = c.gamma.sin_cos();
        out.reserve(vc);
        for v in 0..vc {
            let [mut x, mut y, mut z] = get(ph, f, k, v);
            if let Some((pph, pf, pk)) = pm {
                let [px, py, pz] = get(pph, pf, pk, v);
                x = x * pmk1 + px * pmk2;
                y = y * pmk1 + py * pmk2;
                z = z * pmk1 + pz * pmk2;
            }
            let mut zz = z;
            let mut xx = cg * x - sg * y;
            let yy = cg * y + sg * x;
            let fi = if z > 0.0 {
                (z / 240.0).min(1.0)
            } else {
                (z / 380.0).max(-1.0)
            } * c.bend;
            let (bs, bc) = fi.sin_cos();
            let bx = bc * xx - bs * zz;
            let bz = bc * zz + bs * xx;
            zz = bz;
            xx = bx;
            out.push([xx * scale, cb * yy - sb * zz, cb * zz + sb * yy]);
        }
    }
}

/// What a shot hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Ground,
    Water,
    Model,
    /// A creature, and whether the face was a mortal one (head, heart).
    Char(usize, bool),
}

impl Sim {
    /// Follows a shot from `from` towards `to`: the nearest of
    /// the ground, the water, the objects and the creatures it meets, and
    /// the point just short of it.
    pub fn trace_shot(&self, a: &Area, from: Vec3, to: Vec3) -> (Option<Hit>, Vec3) {
        let nv = (to - from).normalize_or_zero();
        let mut tb = to;
        let mut res = None;
        let (axi, azi) = ((from.x / 256.0) as i32, (from.z / 256.0) as i32);
        let (bxi, bzi) = ((to.x / 256.0) as i32, (to.z / 256.0) as i32);
        // Cells from 2 to 510 in the first game, to 1010 in the later ones.
        let lim = if self.c1 { 510 } else { a.size - 14 };
        let g = |x: i32, z: i32| {
            Vec3::new(
                x as f32 * 256.0,
                a.surface_steps(x, z) as f32 * a.hs,
                z as f32 * 256.0,
            )
        };
        for zz in (azi.min(bzi) - 2)..=(azi.max(bzi) + 2) {
            for xx in (axi.min(bxi) - 2)..=(axi.max(bxi) + 2) {
                if xx < 2 || xx > lim || zz < 2 || zz > lim {
                    continue;
                }
                let i = a.idx(xx, zz);
                let rev = a.map.reverse(i);
                let (v0, v1, v2) = if rev {
                    (g(xx, zz), g(xx + 1, zz), g(xx, zz + 1))
                } else {
                    (g(xx, zz), g(xx + 1, zz), g(xx + 1, zz + 1))
                };
                if trace_plane(from, &mut tb, nv, v0, v1, v2) {
                    res = Some(Hit::Ground);
                }
                let (w0, w1, w2) = if rev {
                    (v2, v1, g(xx + 1, zz + 1))
                } else {
                    (v0, v2, g(xx, zz + 1))
                };
                if trace_plane(from, &mut tb, nv, w0, w1, w2) {
                    res = Some(Hit::Ground);
                }
                if a.engine == carn_formats::Engine::C2 && a.map.water(i) {
                    let wl = |x: i32, z: i32| {
                        let w = a.map.wmap[a.idx(x, z)] as usize;
                        let l = a.rsc.waters.get(w).map(|w| w.level).unwrap_or(0) as f32 * a.hs;
                        Vec3::new(x as f32 * 256.0, l, z as f32 * 256.0)
                    };
                    let (q0, q1, q2, q3) = (
                        wl(xx, zz),
                        wl(xx + 1, zz),
                        wl(xx + 1, zz + 1),
                        wl(xx, zz + 1),
                    );
                    if trace_plane(from, &mut tb, nv, q0, q1, q2) {
                        res = Some(Hit::Water);
                    }
                    if trace_plane(from, &mut tb, nv, q0, q2, q3) {
                        res = Some(Hit::Water);
                    }
                }
                let ob = a.map.omap[i];
                if ob != 255 && self.trace_model(a, from, &mut tb, nv, xx, zz, ob as usize) {
                    res = Some(Hit::Model);
                }
            }
        }
        let mut buf = Vec::new();
        for (ci, c) in self.chars.iter().enumerate() {
            if c.removed || point_to_vector_d(from, nv, c.pos) > 1024.0 {
                continue;
            }
            let Some(info) = self.info(c) else { continue };
            self.morph(c, &mut buf);
            if buf.len() != info.model.vertices.len() {
                continue;
            }
            let (sa, ca) = (FRAC_PI_2_F - c.alpha).sin_cos();
            let w: Vec<Vec3> = buf
                .iter()
                .map(|p| {
                    Vec3::new(
                        p[0] * ca + p[2] * sa + c.pos.x,
                        p[1] + c.pos.y,
                        p[2] * ca - p[0] * sa + c.pos.z,
                    )
                })
                .collect();
            for f in &info.model.faces {
                if f.keyed() {
                    continue;
                }
                let [i0, i1, i2] = f.v.map(|v| v as usize);
                if trace_plane(from, &mut tb, nv, w[i0], w[i1], w[i2]) {
                    res = Some(Hit::Char(ci, f.flags & carn_formats::model::SF_MORTAL != 0));
                }
            }
        }
        let l = if matches!(res, Some(Hit::Char(..))) {
            32.0
        } else {
            16.0
        };
        (res, tb - nv * l)
    }

    #[allow(clippy::too_many_arguments)]
    fn trace_model(
        &self,
        a: &Area,
        from: Vec3,
        tb: &mut Vec3,
        nv: Vec3,
        xx: i32,
        zz: i32,
        ob: usize,
    ) -> bool {
        let o = &a.rsc.objects[ob];
        let base = Vec3::new(
            xx as f32 * 256.0 + 128.0,
            a.land_oh(xx, zz),
            zz as f32 * 256.0 + 128.0,
        );
        if point_to_vector_d(from, nv, base + Vec3::Y * 700.0) > 1400.0 {
            return false;
        }
        let turn = a.map.object_turn(a.idx(xx, zz)) as f32 * FRAC_PI_2_F;
        let (sa, ca) = turn.sin_cos();
        let w: Vec<Vec3> = o
            .model
            .vertices
            .iter()
            .map(|v| {
                let p = v.pos;
                Vec3::new(
                    p[0] * ca + p[2] * sa + base.x,
                    p[1] + base.y,
                    p[2] * ca - p[0] * sa + base.z,
                )
            })
            .collect();
        let mut hit = false;
        for f in &o.model.faces {
            if f.keyed() {
                continue;
            }
            let [i0, i1, i2] = f.v.map(|v| v as usize);
            if trace_plane(from, tb, nv, w[i0], w[i1], w[i2]) {
                hit = true;
            }
        }
        hit
    }

    /// A shot's damage to creature `ci`.
    /// Returns true if it died of it.
    pub fn shot_damage(&mut self, a: &Area, ci: usize, mortal: bool, power: f32) -> bool {
        let Some(c) = self.chars.get_mut(ci) else {
            return false;
        };
        if c.health == 0 {
            return false;
        }
        // Held still for a test (-freeze) until it is hit.
        if self.hold_still {
            c.frozen = false;
        }
        if mortal {
            c.health = 0;
        } else {
            c.health = (c.health as f32 - power) as i32;
        }
        c.health = c.health.max(0);
        let (ai, ctype) = (c.ai, c.ctype);
        let killed = c.health == 0;
        if killed {
            if ai >= 10 {
                self.kills.push(ci);
            } else {
                self.add_secondary_one(a, ctype);
            }
        } else {
            let ice = self.ice();
            let c = &mut self.chars[ci];
            c.afraid_time = 60 * 1000;
            if ice {
                if c.state == 0 {
                    c.state = 2;
                }
                c.blood_ttime += 90000;
            } else {
                if ai != AI_TREX || c.state == 0 {
                    c.state = 2;
                }
                if ai != AI_BRACH {
                    c.blood_ttime += 90000;
                }
            }
        }
        let trex = self.is_trex(&self.chars[ci]);
        let c = &mut self.chars[ci];
        if trex {
            c.state = if c.state != 0 { 5 } else { 1 };
        }
        killed
    }

    /// A small creature shot dead is replaced by another out of sight.
    fn add_secondary_one(&mut self, a: &Area, ctype: usize) {
        if self.chars.len() > 64 {
            return;
        }
        for _ in 0..128 {
            let x = self.hunter.x + self.rng.si(20040) as f32;
            let z = self.hunter.z + self.rng.si(20040) as f32;
            let mut p = Vec3::new(x, a.land_h(x, z), z);
            if self.check_place_collision_p(a, &mut p) {
                continue;
            }
            if (p.x - self.hunter.x).abs() + (p.z - self.hunter.z).abs() < 256.0 * 40.0 {
                continue;
            }
            let c = self.new_char(ctype, p);
            self.chars.push(c);
            return;
        }
    }
}

const FRAC_PI_2_F: f32 = std::f32::consts::FRAC_PI_2;

/// Distance from a point to a line through `a` along unit `nv`.
pub fn point_to_vector_d(a: Vec3, nv: Vec3, p: Vec3) -> f32 {
    let d = p - a;
    let t = d.dot(nv);
    (d - nv * t).length()
}

/// Whether the segment from `a` to `*b` crosses a triangle; if so `*b`
/// moves to the hit.
pub fn trace_plane(ta: Vec3, tb: &mut Vec3, nv: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let pnv = (b - a).cross(c - a).normalize_or_zero();
    let sa = (ta - a).dot(pnv);
    let sb = (*tb - a).dot(pnv);
    if sa * sb > -1.0 {
        return false;
    }
    let scvn = nv.dot(pnv);
    if scvn == 0.0 {
        return false;
    }
    let scln = ((ta - a).dot(pnv) / scvn).abs();
    let hp = ta + nv * scln;
    if (b - a).cross(hp - a).dot(pnv) < 0.0 {
        return false;
    }
    if (c - b).cross(hp - b).dot(pnv) < 0.0 {
        return false;
    }
    if (a - c).cross(hp - c).dot(pnv) < 0.0 {
        return false;
    }
    if (hp - ta).length() < (*tb - ta).length() {
        *tb = hp;
        return true;
    }
    false
}

/// Whether an area object animates (for the renderer).
pub fn object_animated(flags: i32) -> bool {
    flags & OF_ANIMATED != 0
}

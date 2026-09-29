//! What the hunt throws into the air and leaves on the ground: the dust,
//! splash or blood a
//! shot kicks up, bubbles, rings on the water, the spots a wounded animal
//! bleeds, the snow, and the first game's flash where a shot lands.
//!
//! Where the games counted frames (a blood burst faded one step every fourth
//! frame; bubbles rose every 64th), this counts the time those frames took
//! at 60 a second, so the frame limit does not change how long things last.

use bevy::math::Vec3;
use std::f32::consts::PI;

use super::*;
use crate::area::Area;

pub const PART_BLOOD: u8 = 1;
pub const PART_WATER: u8 = 2;
pub const PART_GROUND: u8 = 3;
pub const PART_BUBBLE: u8 = 4;

/// A frame at 60 a second, for the effects the games timed in frames.
const FRAME_MS: i32 = 17;

#[derive(Clone, Copy, Debug, Default)]
pub struct Particle {
    pub pos: Vec3,
    pub speed: Vec3,
    /// Size (the games' R: a disc of R * 0.64 units).
    pub r: f32,
    pub landed: bool,
}

/// One burst: a handful of particles that share colours.
#[derive(Clone, Debug)]
pub struct Burst {
    pub kind: u8,
    /// Centre and rim colours as the games stored them.
    pub rgb: [u8; 3],
    pub rgb2: [u8; 3],
    /// Their alphas, 0..255.
    pub a1: i32,
    pub a2: i32,
    pub parts: Vec<Particle>,
    pub done: usize,
    fade_ms: i32,
}

/// A ring spreading on the water (WCircle).
#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub pos: Vec3,
    pub ftime: i32,
    pub scale: f32,
}

/// A spot of blood on the ground.
#[derive(Clone, Copy, Debug)]
pub struct BloodSpot {
    pub pos: Vec3,
    /// Milliseconds left.
    pub ltime: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Flake {
    pub pos: Vec3,
    /// The ground (or water) under it.
    pub hl: f32,
    /// 0 falling; then how long it has lain.
    pub ftime: i32,
}

/// The first game's flash where a shot lands (Explosion).
#[derive(Clone, Copy, Debug)]
pub struct Flash {
    pub pos: Vec3,
    pub ftime: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Fx {
    pub bursts: Vec<Burst>,
    pub rings: Vec<Ring>,
    pub blood: Vec<BloodSpot>,
    pub snow: Vec<Flake>,
    /// Ice Age snows on areas that are mostly white.
    pub snowing: bool,
    pub flashes: Vec<Flash>,
    /// How long the flash's animation runs (0: no flash model).
    pub flash_time: i32,
    bubble_ms: i32,
    swim_ms: i32,
}

/// Adds two colours a channel at a time, stopping at white.
fn color_sum(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [
        a[0].saturating_add(b[0]),
        a[1].saturating_add(b[1]),
        a[2].saturating_add(b[2]),
    ]
}

impl Sim {
    /// The water's colour at a spot, darkened and tinted as the splashes
    /// and bubbles use it.
    fn splash_color(&self, a: &Area, x: f32, z: f32) -> [u8; 3] {
        let i = a.idx((x / 256.0) as i32, (z / 256.0) as i32);
        let rgb = a
            .map
            .wmap
            .get(i)
            .and_then(|&w| a.rsc.waters.get(w as usize))
            .map(|w| w.fog_rgb)
            .unwrap_or(0);
        let c = [
            (rgb >> 16) as u8 >> 1,
            (rgb >> 8) as u8 >> 1,
            rgb as u8 >> 1,
        ];
        color_sum(c, [0x15, 0x20, 0x20])
    }

    /// `cnt` particles of a kind thrown up from a spot.
    pub fn add_elements(&mut self, a: &Area, p: Vec3, kind: u8, cnt: i32) {
        if self.fx.bursts.len() > 30 {
            self.fx.bursts.remove(0);
        }
        let (rgb, a1, rgb2, a2) = match kind {
            PART_BLOOD => ([0x60, 0, 0], 0xE0, [0x30, 0, 0], 0x20),
            PART_GROUND => ([0xF0, 0x9E, 0x55], 0xF0, [0xF0, 0x9E, 0x55], 0x10),
            PART_BUBBLE => {
                let c = self.splash_color(a, p.x, p.z);
                (color_sum(c, color_sum(c, c)), 0x70, color_sum(c, c), 0x40)
            }
            _ => {
                let c = self.splash_color(a, p.x, p.z);
                (color_sum(c, color_sum(c, c)), 0xB0, c, 0x40)
            }
        };
        let (sa, ca) = self.hunter.cam_alpha.sin_cos();
        let al = self.rng.si(128) as f32 / 128.0 * PI / 4.0;
        let (ss, cc) = al.sin_cos();
        let n = cnt.clamp(0, 30);
        let mut parts = Vec::with_capacity(n as usize);
        for e in 0..n {
            let r = (6 + self.rng.r(5)) as f32;
            let speed = match kind {
                PART_BLOOD => {
                    let v = (e * 6 + self.rng.r(96) + 220) as f32;
                    let x = ss * ca * v + self.rng.si(32) as f32;
                    let y = cc * (v * 3.0);
                    let z = ss * sa * v + self.rng.si(32) as f32;
                    Vec3::new(x, y, z)
                }
                PART_GROUND => {
                    let x = self.rng.si(52) as f32 - sa * 64.0;
                    let y = (self.rng.r(100) + 600 + e * 20) as f32;
                    let z = self.rng.si(52) as f32 + ca * 64.0;
                    Vec3::new(x, y, z)
                }
                PART_BUBBLE => {
                    let x = self.rng.si(40) as f32;
                    let y = (self.rng.r(140) + 20) as f32;
                    let z = self.rng.si(40) as f32;
                    Vec3::new(x, y, z)
                }
                _ => {
                    let x = self.rng.si(32) as f32;
                    let y = (self.rng.r(80) + 400 + e * 40) as f32;
                    let z = self.rng.si(32) as f32;
                    Vec3::new(x, y, z)
                }
            };
            parts.push(Particle {
                pos: p,
                speed,
                r,
                landed: false,
            });
        }
        self.fx.bursts.push(Burst {
            kind,
            rgb,
            rgb2,
            a1,
            a2,
            parts,
            done: 0,
            fade_ms: 0,
        });
    }

    /// A ring on the water's surface at a spot.
    pub fn add_wcircle(&mut self, a: &Area, x: f32, z: f32, scale: f32) {
        // The games kept a fixed table; this only stops a runaway.
        if self.fx.rings.len() >= 256 {
            self.fx.rings.remove(0);
        }
        self.fx.rings.push(Ring {
            pos: Vec3::new(x, a.land_up_h(x, z), z),
            ftime: 0,
            scale,
        });
    }

    /// A spot of blood under a wounded animal.
    pub fn add_blood_trail(&mut self, a: &Area, c: &Character) {
        if self.fx.blood.len() > 508 {
            self.fx.blood.remove(0);
        }
        let x = c.pos.x + self.rng.si(32) as f32;
        let z = c.pos.z + self.rng.si(32) as f32;
        self.fx.blood.push(BloodSpot {
            pos: Vec3::new(x, a.land_h(x, z) + 4.0, z),
            ltime: 210000,
        });
    }

    /// The first game's flash where a shot lands.
    pub fn add_flash(&mut self, p: Vec3) {
        if self.fx.flash_time > 0 && self.fx.flashes.len() < 32 {
            self.fx.flashes.push(Flash { pos: p, ftime: 0 });
        }
    }

    /// Where a shot lands: dust off the ground or a
    /// model, a splash and rings on water, blood from a creature.
    pub fn shot_impact(&mut self, a: &Area, hit: &Hit, p: Vec3, power: f32) {
        if self.c1 {
            self.add_flash(p);
            return;
        }
        match hit {
            Hit::Ground | Hit::Model => {
                self.add_elements(a, p, PART_GROUND, (6.0 + power * 4.0) as i32)
            }
            Hit::Water => {
                self.add_elements(a, p, PART_WATER, (4.0 + power * 3.0) as i32);
                self.add_wcircle(a, p.x, p.z, 1.2);
                self.add_wcircle(a, p.x, p.z, 1.2);
            }
            Hit::Char(..) => self.add_elements(a, p, PART_BLOOD, (4.0 + power * 4.0) as i32),
        }
    }

    /// A wounded animal bleeds for a while after each hit, faster the more
    /// it has been hit.
    pub fn bleed(&mut self, a: &Area, c: &mut Character) {
        if c.health == 0 || c.blood_ttime == 0 || self.c1 {
            return;
        }
        c.blood_ttime = (c.blood_ttime - self.time_dt).max(0);
        let k = ((20000.0 + c.blood_ttime as f32) / 90000.0).min(1.5);
        c.blood_time += (self.time_dt as f32 * k) as i32;
        if c.blood_time > 600 {
            c.blood_time = self.rng.r(228);
            self.add_blood_trail(a, c);
            if self.rng.r(128) > 96 {
                self.add_blood_trail(a, c);
            }
        }
    }

    /// A flyer shot out of the sky and hitting the water on its way down: rings where it goes in, bubbles when it reaches
    /// the bottom.
    pub fn fall_splash(&mut self, a: &Area, c: &Character, next_y: f32, landed: bool) {
        if self.c1 {
            return;
        }
        let wh = a.land_up_h(c.pos.x, c.pos.z);
        let lh = a.land_h(c.pos.x, c.pos.z);
        if wh <= lh {
            return;
        }
        if !landed && c.pos.y >= wh && next_y < wh {
            for s in [2.0, 2.5, 3.0, 3.5, 3.0] {
                let x = c.pos.x + self.rng.si(128) as f32;
                let z = c.pos.z + self.rng.si(128) as f32;
                self.add_wcircle(a, x, z, s);
            }
        }
        if landed {
            for _ in 0..3 {
                let x = c.pos.x + self.rng.si(128) as f32;
                let z = c.pos.z + self.rng.si(128) as f32;
                self.add_elements(a, Vec3::new(x, lh, z), PART_BUBBLE, 10);
            }
        }
    }

    /// Ice Age snows when most of the area is white (more than 200000 cells whose texture is brighter than a level set by the
    /// brightness option).
    pub fn init_fx(&mut self, a: &Area, brightness: i32, flash_time: i32) {
        self.fx = Fx {
            flash_time,
            ..Default::default()
        };
        if self.kind != GameKind::IceAge {
            return;
        }
        let sl = (100 * (brightness + 128)) >> 8;
        let white: Vec<bool> = a
            .rsc
            .textures
            .iter()
            .map(|t| {
                let [r, g, b] = carn_formats::color::mid_color(t);
                r as i32 > sl && g as i32 > sl && b as i32 > sl
            })
            .collect();
        let n = a
            .map
            .tmap1
            .iter()
            .filter(|&&t| {
                let t = if t as u32 == 0xFFFF { 1 } else { t as usize };
                white.get(t).copied().unwrap_or(false)
            })
            .count();
        self.fx.snowing = n > 200000;
        bevy::log::debug!(
            "{n} white cells: {}",
            if self.fx.snowing { "snow" } else { "no snow" }
        );
    }

    /// Water rings from the hunter: wading steps, going under or coming up,
    /// and swimming.
    pub fn hunter_rings(&mut self, a: &Area, step: bool, dived: bool, swim: bool) {
        if self.c1 {
            return;
        }
        let cam = self.hunter.cam;
        if step {
            self.add_wcircle(a, cam.x, cam.z, 1.2);
        }
        if dived {
            self.add_wcircle(a, cam.x, cam.z, 2.0);
        }
        if swim {
            self.fx.swim_ms += self.time_dt;
            if self.fx.swim_ms >= 32 * FRAME_MS {
                self.fx.swim_ms = 0;
                self.add_wcircle(a, cam.x, cam.z, 1.5);
            }
        } else {
            self.fx.swim_ms = 0;
        }
    }

    fn respawn_flake(&mut self, a: &Area, i: usize, scatter: bool) {
        let (px, pz) = (self.hunter.x, self.hunter.z);
        let x = px + self.rng.si(12 * 256) as f32;
        let z = pz + self.rng.si(12 * 256) as f32;
        let hl = a.land_up_h(x, z);
        let y = if scatter {
            hl + 256.0 + self.rng.r(12 * 256) as f32
        } else {
            hl + ((8 + self.rng.r(5)) * 256) as f32
        };
        self.fx.snow[i] = Flake {
            pos: Vec3::new(x, y, z),
            hl,
            ftime: 0,
        };
    }

    fn animate_snow(&mut self, a: &Area) {
        if !self.fx.snowing {
            return;
        }
        while self.fx.snow.len() < 2000 {
            let i = self.fx.snow.len();
            self.fx.snow.push(Flake::default());
            self.respawn_flake(a, i, true);
        }
        let dt = self.time_dt;
        let drift =
            self.wind.nv.normalize_or_zero() * ((4.0 + self.wind.speed) * 4.0 * dt as f32 / 1000.0);
        let (sa, ca) = self.hunter.cam_alpha.sin_cos();
        let (px, pz) = (self.hunter.x, self.hunter.z);
        for s in 0..self.fx.snow.len() {
            let mut f = self.fx.snow[s];
            // Keep the cloud with the hunter.
            if (f.pos.x - px).abs() > 14.0 * 256.0 || (f.pos.z - pz).abs() > 14.0 * 256.0 {
                f.pos.x = px + self.rng.si(12 * 256) as f32;
                f.pos.z = pz + self.rng.si(12 * 256) as f32;
                f.pos.y -= f.hl;
                f.hl = a.land_up_h(f.pos.x, f.pos.z);
                f.pos.y += f.hl;
            }
            if f.ftime == 0 {
                // A sway across the view, in whole units as Ice Age worked
                // it out, so it only shows when frames are long.
                let v = ((((self.real_time + s as i32 * 23) % 800) - 400) * dt / 16000) as f32;
                f.pos.x += ca * v;
                f.pos.z += sa * v;
                f.pos += drift;
                f.hl = a.land_up_h(f.pos.x, f.pos.z);
                f.pos.y -= dt as f32 * 192.0 / 1000.0;
                if f.pos.y < f.hl + 8.0 {
                    f.pos.y = f.hl + 8.0;
                    f.ftime = 1;
                }
                self.fx.snow[s] = f;
            } else {
                f.ftime += dt;
                f.pos.y -= dt as f32 * 3.0 / 1000.0;
                self.fx.snow[s] = f;
                if f.ftime > 2000 {
                    self.respawn_flake(a, s, false);
                }
            }
        }
    }

    /// The particles and the other effects, and the rings' spreading.
    pub fn animate_fx(&mut self, a: &Area) {
        let dt = self.time_dt;
        let mut new_rings: Vec<(f32, f32)> = Vec::new();
        let mut bursts = std::mem::take(&mut self.fx.bursts);
        for b in bursts.iter_mut() {
            match b.kind {
                PART_GROUND => {
                    b.a1 = (b.a1 - dt / 4).max(0);
                    b.a2 = (b.a2 - dt / 4).max(0);
                    if b.a1 == 0 && b.a2 == 0 {
                        b.parts.clear();
                    }
                }
                PART_WATER | PART_BUBBLE => {
                    if b.done == b.parts.len() {
                        b.parts.clear();
                    }
                }
                _ => {
                    if b.done == b.parts.len() {
                        b.fade_ms += dt;
                        while b.fade_ms >= 4 * FRAME_MS {
                            b.fade_ms -= 4 * FRAME_MS;
                            b.a1 = (b.a1 - 1).max(0);
                            b.a2 = (b.a2 - 1).max(0);
                        }
                        if b.a1 == 0 && b.a2 == 0 {
                            b.parts.clear();
                        }
                    }
                }
            }
            if b.parts.is_empty() {
                continue;
            }
            let k = dt as f32 / 1000.0;
            for p in b.parts.iter_mut() {
                if p.landed {
                    continue;
                }
                p.pos += p.speed * k;
                let h = a.land_up_h(p.pos.x, p.pos.z);
                let on_water = a.land_h(p.pos.x, p.pos.z) < h;
                if b.kind == PART_BUBBLE {
                    p.speed.y = (p.speed.y + 2.0 * 256.0 * k).min(824.0);
                    if p.pos.y > h {
                        new_rings.push((p.pos.x, p.pos.z));
                        b.done += 1;
                        p.landed = true;
                        if on_water {
                            p.pos.y -= 10240.0;
                        }
                    }
                } else {
                    p.speed.y -= 9.8 * 256.0 * k;
                    if p.pos.y < h {
                        if on_water {
                            new_rings.push((p.pos.x, p.pos.z));
                        }
                        b.done += 1;
                        p.landed = true;
                        if on_water {
                            p.pos.y -= 10240.0;
                        } else {
                            p.pos.y = h + 4.0;
                        }
                    }
                }
            }
        }
        bursts.retain(|b| !b.parts.is_empty());
        // Anything added while these moved goes after them.
        bursts.append(&mut self.fx.bursts);
        self.fx.bursts = bursts;
        for (x, z) in new_rings {
            self.add_wcircle(a, x, z, 0.6);
        }

        self.fx.blood.retain_mut(|b| {
            b.ltime -= dt;
            b.ltime > 0
        });

        self.animate_snow(a);

        // Now and then, bubbles somewhere ahead in deep water.
        self.fx.bubble_ms += dt;
        if self.fx.bubble_ms >= 64 * FRAME_MS && !self.c1 {
            self.fx.bubble_ms = 0;
            let al2 = self.hunter.cam_alpha + self.rng.si(60) as f32 * PI / 180.0;
            let l = (1024 + self.rng.r(3120)) as f32;
            let x = self.hunter.cam.x + al2.sin() * l;
            let z = self.hunter.cam.z - al2.cos() * l;
            if a.land_up_h(x, z) > a.land_h(x, z) + 256.0 {
                let n = 6 + self.rng.r(6);
                self.add_elements(a, Vec3::new(x, a.land_h(x, z), z), PART_BUBBLE, n);
            }
        }

        for r in self.fx.rings.iter_mut() {
            r.ftime += if r.scale > 1.0 {
                (dt as f32 * 3.0 / r.scale) as i32
            } else {
                dt * 3
            };
        }
        self.fx.rings.retain(|r| r.ftime < 2000);

        let ft = self.fx.flash_time;
        for f in self.fx.flashes.iter_mut() {
            f.ftime += dt;
        }
        self.fx.flashes.retain(|f| f.ftime < ft);
    }
}

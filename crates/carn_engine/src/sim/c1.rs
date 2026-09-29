//! Carnivores (the first game): its creature list, which it kept in code, and how it spreads them over an area.
//!
//! Its behaviours are the ancestors of Carnivores 2's and run through the
//! same code in c2ai.rs, with the first game's figures where they differ.

use bevy::math::Vec3;

use super::*;
use crate::area::Area;

/// The first game's pachycephalosaurus behaviour, which Carnivores 2 does
/// not have.
pub const AI_PACH: i32 = 20;

/// The hunter's body is creature type 11 in the first game.
pub const HUNTER_CTYPE: usize = 11;

fn dino(name: &str, file: &str, ai: i32) -> DinoInfo {
    let mut d = DinoInfo {
        name: name.into(),
        file: file.into(),
        ai,
        ..Default::default()
    };
    d.set_retail_layout();
    d
}

/// The game's table, in its type order: 0-2 the small
/// ones, 3 unused, 4-10 the hunt, 11 the hunter.
pub fn dinos() -> Vec<DinoInfo> {
    let mut v = vec![
        dino("Moschops", "MOSH.CAR", AI_MOSH),
        dino("Galimimus", "GALL.CAR", AI_GALL),
        dino("Dimorphodon", "DIMOR2.CAR", AI_DIMOR),
        dino("", "", -1),
        dino("Parasaurolophus", "PAR2.CAR", AI_PARA),
        dino("Pachycephalosaurus", "PACH.CAR", AI_PACH),
        dino("Stegosaurus", "STEGO.CAR", AI_STEGO),
        dino("Allosaurus", "ALLO.CAR", AI_ALLO),
        dino("Triceratops", "TRICER.CAR", AI_CHASM),
        dino("Velociraptor", "VELO2.CAR", AI_VELO),
        dino("T-Rex", "TIREX.CAR", AI_TREX),
        dino("Hunter", "HUNTER1.CAR", AI_HUNTER),
    ];
    let set = |d: &mut DinoInfo, health: i32, mass: f32| {
        d.health0 = health;
        d.mass = mass;
    };
    set(&mut v[0], 2, 0.15);
    set(&mut v[1], 2, 0.1);
    set(&mut v[2], 1, 0.05);
    let big = |d: &mut DinoInfo,
               mass: f32,
               length: f32,
               radius: f32,
               health: i32,
               score: i32,
               k: (f32, f32, f32),
               sh: f32| {
        d.mass = mass;
        d.length = length;
        d.radius = radius;
        d.health0 = health;
        d.base_score = score;
        d.smell_k = k.0;
        d.hear_k = k.1;
        d.look_k = k.2;
        d.sh_delta = sh;
    };
    big(&mut v[4], 1.5, 5.8, 320.0, 5, 6, (0.8, 1.0, 0.4), 48.0);
    big(&mut v[5], 0.8, 4.5, 280.0, 4, 8, (0.4, 0.8, 0.6), 36.0);
    big(&mut v[6], 7.0, 7.0, 480.0, 5, 7, (0.4, 0.8, 0.6), 128.0);
    big(&mut v[7], 0.5, 4.2, 256.0, 3, 12, (1.0, 0.3, 0.5), 32.0);
    v[7].scale0 = 1000;
    v[7].scale_a = 600;
    v[7].danger = true;
    big(&mut v[8], 3.0, 5.0, 512.0, 8, 9, (0.6, 0.5, 0.4), 148.0);
    big(&mut v[9], 0.3, 4.0, 256.0, 3, 16, (1.0, 0.5, 0.4), -24.0);
    v[9].scale_a = 400;
    v[9].danger = true;
    big(
        &mut v[10],
        6.0,
        12.0,
        400.0,
        1024,
        20,
        (0.85, 0.8, 0.8),
        168.0,
    );
    v[10].danger = true;
    // The first game's own animation orders where they differ from its
    // successor's (the RAP_, VEL_, TRI_, PAC_ constants).
    {
        use slot::*;
        let a = &mut v[5].anim;
        *a = [-1; COUNT];
        a[WALK] = 0;
        a[RUN] = 1;
        a[SLIDE] = 2;
        a[DIE] = 3;
        a[IDLE1] = 4;
        a[IDLE2] = 5;
        a[SLEEP] = 6;
        let a = &mut v[8].anim;
        *a = [-1; COUNT];
        a[RUN] = 0;
        a[WALK] = 1;
        a[IDLE1] = 2;
        a[IDLE2] = 3;
        a[IDLE3] = 4;
        a[DIE] = 5;
        a[SLEEP] = 6;
    }
    v
}

impl Sim {
    /// The first game's placement: a few small creatures near the hunter,
    /// then the hunt around the middle of the map, the first four of any
    /// kind and the rest the kind hunted.
    pub fn place_characters_c1(&mut self, a: &Area) {
        self.chars.clear();
        let (px, pz) = (self.hunter.x, self.hunter.z);
        let has = |s: &Sim, t: usize| s.chinfo.get(t).map(|c| c.is_some()).unwrap_or(false);
        for _ in 0..5 + self.opt_dens {
            let ctype = self.rng.r(2) as usize;
            if !has(self, ctype) {
                continue;
            }
            let mut found = None;
            for _ in 0..10240 {
                let x = px + self.rng.si(10040) as f32;
                let z = pz + self.rng.si(10040) as f32;
                let mut p = Vec3::new(x, a.land_h(x, z), z);
                if !self.check_place_collision_p(a, &mut p) {
                    found = Some(p);
                    break;
                }
            }
            let Some(p) = found else { break };
            let mut c = self.new_char(ctype, p);
            if ctype == 2 {
                c.pos.y += 2048.0;
            }
            self.chars.push(c);
        }
        // The kind hunted (0-6 for types 4-10), or any.
        let target = (0..7).find(|t| self.target_dino & (1 << t) != 0 && self.target_dino != !0);
        let mut mc = 10 + self.opt_dens * 2;
        if target == Some(6) {
            mc = 5 + self.opt_dens;
        }
        let centre = (a.size / 2 * 256) as f32;
        for c in 0..mc {
            let ctype = match target {
                Some(t) if c >= 4 => 4 + t as usize,
                _ => 4 + self.rng.r(if target.is_some() { 3 } else { 6 }) as usize,
            };
            if !has(self, ctype) {
                continue;
            }
            let mut found = None;
            for _ in 0..10240 {
                let x = centre + self.rng.si(20 * 256) as f32 * 10.0;
                let z = centre + self.rng.si(20 * 256) as f32 * 10.0;
                let mut p = Vec3::new(x, a.land_h(x, z), z);
                if (p.x - px).abs() + (p.z - pz).abs() < 256.0 * 40.0 {
                    continue;
                }
                if !self.check_place_collision_p(a, &mut p) {
                    found = Some(p);
                    break;
                }
            }
            let Some(p) = found else { break };
            let ch = self.new_char(ctype, p);
            self.chars.push(ch);
        }
    }
}

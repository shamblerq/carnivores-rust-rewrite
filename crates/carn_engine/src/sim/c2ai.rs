//! Carnivores 2's creature behaviours, and through them the first game's
//! and Ice Age's.
//!
//! Most of them are one of two shapes with different figures: the hunters
//! (allosaurus, velociraptor, spinosaurus, ceratosaurus) and the grazers
//! (parasaurolophus, ankylosaurus, stegosaurus, chasmosaurus, and the small
//! ones: gallimimus, moschops, dimetrodon). Each shape is written out once
//! here with its figures in a table; the T-Rex, the brachiosaurus and the
//! flyers have their own. Everything else - the order of the tests, the
//! timers, the randomness - is as in the original.

use std::f32::consts::PI;

use bevy::math::Vec3;

use super::slot::*;
use super::*;
use crate::area::Area;

pub fn animate(s: &mut Sim, a: &Area, c: &mut Character) {
    let alive = c.health > 0;
    if s.kind == crate::game::GameKind::IceAge {
        return ice(s, a, c, alive);
    }
    match c.ai {
        AI_HUNTER => s.animate_hunt_dead(a, c),
        AI_MOSH if alive => grazer(s, a, c, &MOSH),
        AI_GALL if alive => grazer(s, a, c, if s.c1 { &GALL_C1 } else { &GALL }),
        AI_DIMET if alive => grazer(s, a, c, &DIMET),
        AI_PARA if alive => grazer(s, a, c, &PARA),
        AI_ANKY if alive => grazer(s, a, c, &ANKY),
        AI_STEGO if alive => grazer(s, a, c, &STEGO),
        AI_CHASM if alive => grazer(s, a, c, if s.c1 { &TRIC_C1 } else { &CHASM }),
        super::c1::AI_PACH if alive => grazer(s, a, c, &PACH),
        AI_ALLO if alive => hunter(s, a, c, if s.c1 { &ALLO_C1 } else { &ALLO }),
        AI_VELO if alive => hunter(s, a, c, if s.c1 { &VELO_C1 } else { &VELO }),
        AI_SPINO if alive => hunter(s, a, c, &SPINO),
        AI_CERAT if alive => hunter(s, a, c, &CERAT),
        AI_TREX if alive => trex(s, a, c),
        AI_BRACH => brahi(s, a, c),
        AI_DIMOR | AI_PTERA if alive => dimor(s, a, c),
        AI_DIMOR | AI_PTERA => dimor_dead(s, a, c),
        AI_TREX => dead(s, a, c, 200.0, 196.0, false),
        _ if !alive => dead(s, a, c, 100.0, 96.0, true),
        _ => {}
    }
}

fn dt(s: &Sim) -> f32 {
    s.time_dt as f32
}

/// The heading to aim for: Carnivores 2 splits the difference with the
/// current one to steady it; the first game aimed straight.
fn aim(s: &Sim, c: &Character, dx: f32, dz: f32) -> f32 {
    if s.c1 {
        find_vector_alpha(dx, dz)
    } else {
        corrected_alpha(find_vector_alpha(dx, dz), c.alpha)
    }
}

/// How far behind the hunter a creature may fall before it is put back
/// ahead of him.
fn replace_dist(s: &Sim) -> f32 {
    if s.c1 {
        13240.0
    } else {
        (s.view_r + 20) as f32 * 256.0
    }
}

/// Turns towards `tgalpha` at the behaviour's rate and bends the body into
/// the turn; returns how far it still has to turn.
struct Turn {
    base: f32,
    per: f32,
    afraid_mul: f32,
    rdiv_afraid: f32,
    rdiv: f32,
    bend_div: f32,
    bend_max: f32,
    /// Bend towards the target / back, milliseconds per radian; the same
    /// figure twice for a creature that bends evenly.
    bend_in: f32,
    bend_out: f32,
    turn_div: f32,
}

fn turn(s: &Sim, c: &mut Character, t: &Turn, slow: bool) {
    let dalpha = (c.tgalpha - c.alpha).abs();
    let drspd = if dalpha > PI {
        2.0 * PI - dalpha
    } else {
        dalpha
    };
    let mut currspeed = if drspd > 0.02 {
        if c.tgalpha > c.alpha {
            t.base + drspd * t.per
        } else {
            -t.base - drspd * t.per
        }
    } else {
        0.0
    };
    if c.afraid_time != 0 {
        currspeed *= t.afraid_mul;
    }
    if dalpha > PI {
        currspeed = -currspeed;
    }
    if slow {
        currspeed /= 1.4;
    }
    let rdiv = if c.afraid_time != 0 {
        t.rdiv_afraid
    } else {
        t.rdiv
    };
    delta_func(&mut c.rspeed, currspeed, dt(s) / rdiv);
    let tgbend = (drspd / t.bend_div).min(t.bend_max) * sgn(currspeed);
    let bd = if tgbend.abs() > c.bend.abs() {
        t.bend_in
    } else {
        t.bend_out
    };
    delta_func(&mut c.bend, tgbend, dt(s) / bd);
    let rspd = c.rspeed * dt(s) / t.turn_div;
    if drspd < rspd.abs() {
        c.alpha = c.tgalpha;
    } else {
        c.alpha += rspd;
    }
    if c.alpha > PI * 2.0 {
        c.alpha -= PI * 2.0;
    }
    if c.alpha < 0.0 {
        c.alpha += PI * 2.0;
    }
}

fn drspd(c: &Character) -> f32 {
    let d = (c.tgalpha - c.alpha).abs();
    if d > PI {
        2.0 * PI - d
    } else {
        d
    }
}

/// Advances the animation clock; true when the animation came round.
fn tick(s: &Sim, c: &mut Character) -> bool {
    c.ftime += s.time_dt;
    let at = s.ani_time(c, c.phase);
    if c.ftime >= at {
        c.ftime %= at;
        return true;
    }
    false
}

/// The phase-change bookkeeping every behaviour ends its selection with:
/// the sound, the proportional clock for walk/run changes (`prop` is the
/// highest phase that takes part), the blend.
fn end_select(
    s: &mut Sim,
    c: &mut Character,
    old_phase: i32,
    old_ftime: i32,
    new_phase: bool,
    prop: i32,
) {
    if old_phase != c.phase || new_phase {
        s.activate_fx(c);
    }
    if old_phase != c.phase {
        if old_phase <= prop && c.phase <= prop {
            c.ftime = old_ftime * s.ani_time(c, c.phase) / s.ani_time(c, old_phase) + 64;
        } else if !new_phase {
            c.ftime = 0;
        }
        s.begin_morph(c, old_phase, old_ftime);
    }
    c.ftime %= s.ani_time(c, c.phase);
}

fn flee_from(c: &mut Character, pdx: f32, pdz: f32) {
    let nv = Vec3::new(pdx, 0.0, pdz).normalize_or_zero() * 2048.0;
    c.tgx = c.pos.x - nv.x;
    c.tgz = c.pos.z - nv.z;
    c.tgtime = 0;
}

fn set_on_water(a: &Area, c: &mut Character, depth: f32) {
    if a.land_up_h(c.pos.x, c.pos.z) - a.land_h(c.pos.x, c.pos.z) > depth * c.scale {
        c.state_f |= CS_ONWATER;
    } else {
        // The games clear every flag here, not just the water one.
        c.state_f = 0;
    }
}

// ---------------------------------------------------------------------------
// The hunters
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Wake {
    /// Keeps a jump going; otherwise starts a new phase.
    KeepJump,
    /// As KeepJump, and breaks into a run.
    KeepJumpRun,
    /// Always starts a new phase, running.
    Run,
}

struct HunterP {
    /// The first game's chase radius, in cells: base + aggression * mul
    /// (its aggression option ran 0-2).
    c1_chase: Option<(i32, i32)>,
    /// How sharp a turn at speed makes it skid.
    slide_angle: f32,
    /// How far ahead of its centre it measures the hunter from.
    off: f32,
    off_scaled: bool,
    wake: Wake,
    water_depth: f32,
    chase_mul: f32,
    agres_div: i32,
    jump: bool,
    slide: bool,
    idles: i32,
    eat_dist: f32,
    eat_dy: f32,
    calm_resets_fear: bool,
    wobble_div: f32,
    speed_run: f32,
    speed_jump: f32,
    speed_walk: f32,
    speed_swim: f32,
    bend_div: f32,
    swim_y: f32,
    ty: (f32, f32, f32, f32),
    gamma_walk: f32,
    gamma_run: f32,
}

const ALLO: HunterP = HunterP {
    c1_chase: None,
    slide_angle: PI * 2.0 / 3.0,
    off: 100.0,
    off_scaled: true,
    wake: Wake::KeepJump,
    water_depth: 180.0,
    chase_mul: 128.0,
    agres_div: 4,
    jump: true,
    slide: true,
    idles: 0,
    eat_dist: 256.0,
    eat_dy: 160.0,
    calm_resets_fear: false,
    wobble_div: 2.0,
    speed_run: 1.2,
    speed_jump: 1.1,
    speed_walk: 0.428,
    speed_swim: 0.4,
    bend_div: 2.0,
    swim_y: 200.0,
    ty: (64.0, 32.0, 0.5, 0.4),
    gamma_walk: 10.0,
    gamma_run: 8.0,
};

const VELO: HunterP = HunterP {
    c1_chase: None,
    slide_angle: PI * 2.0 / 3.0,
    off: 108.0,
    off_scaled: false,
    wake: Wake::KeepJump,
    water_depth: 140.0,
    chase_mul: 160.0,
    agres_div: 8,
    jump: true,
    slide: true,
    idles: 0,
    eat_dist: 256.0,
    eat_dy: 120.0,
    calm_resets_fear: true,
    wobble_div: 2.0,
    speed_run: 1.2,
    speed_jump: 1.1,
    speed_walk: 0.428,
    speed_swim: 0.4,
    bend_div: 3.0,
    swim_y: 160.0,
    ty: (48.0, 24.0, 0.5, 0.4),
    gamma_walk: 7.0,
    gamma_run: 5.0,
};

const SPINO: HunterP = HunterP {
    c1_chase: None,
    slide_angle: PI * 2.0 / 3.0,
    off: 108.0,
    off_scaled: false,
    wake: Wake::KeepJumpRun,
    water_depth: 140.0,
    chase_mul: 140.0,
    agres_div: 8,
    jump: true,
    slide: true,
    idles: 2,
    eat_dist: 300.0,
    eat_dy: 120.0,
    calm_resets_fear: true,
    wobble_div: 4.0,
    speed_run: 1.6,
    speed_jump: 1.4,
    speed_walk: 0.70,
    speed_swim: 0.5,
    bend_div: 3.0,
    swim_y: 160.0,
    ty: (98.0, 84.0, 0.4, 0.3),
    gamma_walk: 9.0,
    gamma_run: 6.0,
};

const CERAT: HunterP = HunterP {
    c1_chase: None,
    slide_angle: PI * 2.0 / 3.0,
    off: 108.0,
    off_scaled: false,
    wake: Wake::Run,
    water_depth: 140.0,
    chase_mul: 200.0,
    agres_div: 8,
    jump: false,
    slide: false,
    idles: 3,
    eat_dist: 350.0,
    eat_dy: 120.0,
    calm_resets_fear: true,
    wobble_div: 4.0,
    speed_run: 2.2,
    speed_jump: 0.0,
    speed_walk: 0.75,
    speed_swim: 0.0,
    bend_div: 3.0,
    swim_y: 160.0,
    ty: (348.0, 324.0, 0.5, 0.4),
    gamma_walk: 9.0,
    gamma_run: 6.0,
};

const ALLO_C1: HunterP = HunterP {
    c1_chase: Some((16, 6)),
    slide_angle: PI / 2.0,
    ty: (48.0, 24.0, 0.5, 0.4),
    ..ALLO
};

const VELO_C1: HunterP = HunterP {
    c1_chase: Some((18, 10)),
    slide_angle: PI / 2.0,
    ..VELO
};

fn is_idle(s: &Sim, c: &Character, n: i32) -> bool {
    (0..n).any(|i| c.phase == s.danim(c, IDLE1 + i as usize))
}

/// Allosaurus, velociraptor, spinosaurus, ceratosaurus.
fn hunter(s: &mut Sim, a: &Area, c: &mut Character, p: &HunterP) {
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let (hx, hy, hz) = (s.hunter.x, s.hunter.y, s.hunter.z);
    let (mut tdx, mut tdz, mut pdx, mut pdz, mut pdist);
    loop {
        tdx = c.tgx - c.pos.x;
        tdz = c.tgz - c.pos.z;
        let tdist = (tdx * tdx + tdz * tdz).sqrt();
        let off = if p.off_scaled { p.off * c.scale } else { p.off };
        pdx = hx - c.pos.x - c.lookx * off;
        pdz = hz - c.pos.z - c.lookz * off;
        pdist = (pdx * pdx + pdz * pdz).sqrt();
        if c.state == 2 {
            match p.wake {
                Wake::KeepJump | Wake::KeepJumpRun => {
                    if c.phase != s.danim(c, JUMP) {
                        new_phase = true;
                    }
                    c.state = 1;
                    if p.wake == Wake::KeepJumpRun {
                        c.phase = s.danim(c, RUN);
                    }
                }
                Wake::Run => {
                    new_phase = true;
                    c.state = 1;
                    c.phase = s.danim(c, RUN);
                }
            }
        }
        set_on_water(a, c, p.water_depth);
        if c.phase == s.danim(c, EAT) {
            break;
        }
        if s.my_health == 0 {
            c.state = 0;
        }
        if c.state != 0 {
            let chase = match p.c1_chase {
                Some((b, m)) => ((b + s.opt_agres * m) * 256) as f32,
                None => s.view_r as f32 * p.chase_mul + (s.opt_agres / p.agres_div) as f32,
            };
            if pdist > chase {
                flee_from(c, pdx, pdz);
                c.afraid_time -= s.time_dt;
                if c.afraid_time <= 0 {
                    c.afraid_time = 0;
                    c.state = 0;
                }
            } else {
                c.tgx = hx;
                c.tgz = hz;
                c.tgtime = 0;
            }
            if p.jump
                && c.state_f & CS_ONWATER == 0
                && pdist < 1324.0 * c.scale
                && pdist > 900.0 * c.scale
                && angle_difference(c.alpha, find_vector_alpha(pdx, pdz)) < 0.2
            {
                c.phase = s.danim(c, JUMP);
            }
            if pdist < p.eat_dist && (hy - c.pos.y - p.eat_dy).abs() < 256.0 {
                if c.state_f & CS_ONWATER == 0 {
                    c.vspeed /= 8.0;
                    c.state = 1;
                    c.phase = s.danim(c, EAT);
                }
                s.add_dead_body(a, Some(c), HUNT_EAT);
            }
        }
        if c.state == 0 {
            if p.calm_resets_fear {
                c.afraid_time = 0;
            }
            if tdist < 456.0 {
                s.set_new_target_place(a, c, 8048.0);
                continue;
            }
        }
        break;
    }

    // NOTHINK
    if pdist < 2048.0 {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = aim(s, c, tdx, tdz);
        if c.state != 0 && pdist > 1648.0 {
            c.tgalpha += (s.real_time as f32 / 824.0).sin() / p.wobble_div;
            wrap_2pi(&mut c.tgalpha);
        }
    }
    s.look_for_a_way(a, c, false, true);
    if c.no_way_cnt > 12 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 16 + s.rng.r(20);
    }
    wrap_2pi(&mut c.tgalpha);

    s.process_prev_phase(c);
    if tick(s, c) {
        new_phase = true;
    }

    let eat = s.danim(c, EAT);
    let jump = s.danim(c, JUMP);
    'select: {
        if c.phase == eat {
            break 'select;
        }
        if p.jump {
            if new_phase && old_phase == jump {
                c.phase = s.danim(c, RUN);
                break 'select;
            }
            if c.phase == jump {
                break 'select;
            }
        }
        if p.idles > 0 && new_phase {
            if c.state == 0 {
                if s.rng.r(128) > 110 {
                    let r = s.rng.r(p.idles - 1);
                    c.phase = s.danim(c, IDLE1 + r as usize);
                    break 'select;
                }
                c.phase = s.danim(c, WALK);
            } else {
                c.phase = s.danim(c, RUN);
            }
        }
        if p.idles == 0 || !is_idle(s, c, p.idles) {
            let d = (c.tgalpha - c.alpha).abs();
            c.phase = if c.state == 0 {
                s.danim(c, WALK)
            } else if d < 1.0 || d > 2.0 * PI - 1.0 {
                s.danim(c, RUN)
            } else {
                s.danim(c, WALK)
            };
        }
        if c.state_f & CS_ONWATER != 0 {
            c.phase = s.danim(c, SWIM);
        }
        if p.slide && c.slide > 40 {
            c.phase = s.danim(c, SLIDE);
        }
    }
    end_select(s, c, old_phase, old_ftime, new_phase, 3);

    let dr = drspd(c);
    let skip =
        c.phase == eat || (p.jump && c.phase == jump) || (p.idles > 0 && is_idle(s, c, p.idles));
    if !skip {
        let slow = c.state_f & CS_ONWATER != 0 || c.phase == s.danim(c, WALK);
        turn(
            s,
            c,
            &Turn {
                base: 0.6,
                per: 1.2,
                afraid_mul: 2.5,
                rdiv_afraid: 160.0,
                rdiv: 180.0,
                bend_div: p.bend_div,
                bend_max: PI / 5.0,
                bend_in: 800.0,
                bend_out: 600.0,
                turn_div: 1024.0,
            },
            slow,
        );
    }

    if p.slide
        && c.slide == 0
        && c.vspeed > 0.6
        && c.phase != jump
        && angle_difference(c.tgalpha, c.alpha) > p.slide_angle
    {
        c.slide = (c.vspeed * 700.0) as i32;
        c.slidex = c.lookx;
        c.slidez = c.lookz;
        c.vspeed = 0.0;
    }

    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == s.danim(c, RUN) {
        curspeed = p.speed_run;
    }
    if p.jump && c.phase == jump {
        curspeed = p.speed_jump;
    }
    if c.phase == s.danim(c, WALK) {
        curspeed = p.speed_walk;
    }
    if p.speed_swim > 0.0 && c.phase == s.danim(c, SWIM) {
        curspeed = p.speed_swim;
    }
    if c.phase == eat {
        curspeed = 0.0;
    }
    if p.slide && c.phase == s.danim(c, RUN) && c.slide != 0 {
        curspeed /= 8.0;
        if dr > PI / 2.0 {
            curspeed = 0.0;
        } else if dr > PI / 4.0 {
            curspeed *= 2.0 - 4.0 * dr / PI;
        }
    } else if dr > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * dr / PI;
    }
    delta_func(&mut c.vspeed, curspeed, dt(s) / 500.0);
    if p.jump && c.phase == jump {
        c.vspeed = 1.1;
    }
    let (lx, lz, v) = (c.lookx, c.lookz, c.vspeed * dt(s) * c.scale);
    s.move_character(a, c, lx * v, lz * v, false, true);
    if p.slide && c.slide != 0 {
        let k = c.slide as f32 / 600.0 * dt(s) * c.scale;
        let (sx, sz) = (c.slidex, c.slidez);
        s.move_character(a, c, sx * k, sz * k, false, true);
        c.slide = (c.slide - s.time_dt).max(0);
    }

    if c.state_f & CS_ONWATER != 0 {
        c.pos.y = a.land_up_h(c.pos.x, c.pos.z) - p.swim_y * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        let (b, g, bl, gl) = p.ty;
        s.think_y_beta_gamma(a, c, b, g, bl, gl);
    }
    c.tggamma += c.rspeed
        / if c.phase == s.danim(c, WALK) {
            p.gamma_walk
        } else {
            p.gamma_run
        };
    if p.jump && c.phase == jump {
        c.tggamma = 0.0;
    }
    delta_func(&mut c.gamma, c.tggamma, dt(s) / 1624.0);
}

// ---------------------------------------------------------------------------
// The grazers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Flee {
    /// Runs until its fear wears off, then wanders far (parasaurolophus,
    /// ankylosaurus, stegosaurus).
    Big,
    /// Keeps away until 4096 off; scares afresh inside 2048 (gallimimus).
    Gall,
    /// As Gall, but only a fresh scare restarts the run (moschops,
    /// dimetrodon).
    Mosh,
    /// Stands its ground: charges a hunter who comes close (chasmosaurus).
    Tric,
    /// A hunter: chases him while within `chase` view radii times this,
    /// else makes off (Ice Age's wolf).
    Pred(f32),
}

#[derive(Clone, Copy, PartialEq)]
enum IdleMode {
    /// Carnivores 2: the last idle may lead back to walking, else another.
    Standard,
    /// Back to walking on a roll over this, else any idle (Ice Age).
    Any(i32),
    /// Straight back to walking (Ice Age's smilodon).
    Walk,
}

struct GrazerP {
    flee: Flee,
    /// Kills a hunter it reaches (the chargers).
    kills: bool,
    /// Charges inside this many cells (chasmosaurus 16).
    charge_cells: i32,
    /// Starts looking for its way afresh when the hunter is this near
    /// (Ice Age); 0 for never.
    nofind_near: f32,
    /// Zig-zags in flight only beyond this distance (Ice Age); 0 always.
    wobble_far: f32,
    idle_mode: IdleMode,
    /// Settles into any of its idles, not just the first (Ice Age).
    idle_start_any: bool,
    /// The first game's charge radius for its triceratops, in cells.
    c1_charge: Option<(i32, i32)>,
    /// Keeps up with the hunter by being put back ahead of him.
    forward: bool,
    startle: f32,
    fear: (i32, i32),
    wander: f32,
    wobble: bool,
    noway: (i32, i32, i32),
    idles: i32,
    idle_stay: i32,
    idle_roll_div: bool,
    idle_start: i32,
    prop: i32,
    tbase: f32,
    tper: f32,
    rdiv: f32,
    bend_div: f32,
    bend_max: f32,
    bend_in: f32,
    bend_out: f32,
    turn_div: f32,
    speed_run: f32,
    speed_walk: f32,
    even_accel: bool,
    ty: (f32, f32, f32, f32),
    gamma_walk: f32,
    gamma_run: f32,
}

const PARA: GrazerP = GrazerP {
    flee: Flee::Big,
    kills: false,
    charge_cells: 16,
    nofind_near: 0.0,
    wobble_far: 0.0,
    idle_mode: IdleMode::Standard,
    idle_start_any: false,
    c1_charge: None,
    forward: false,
    startle: 1024.0,
    fear: (6, 8),
    wander: 8048.0,
    wobble: true,
    noway: (8, 44, 80),
    idles: 2,
    idle_stay: 64,
    idle_roll_div: false,
    idle_start: 120,
    prop: 2,
    tbase: 0.2,
    tper: 1.0,
    rdiv: 400.0,
    bend_div: 3.0,
    bend_max: PI / 2.0,
    bend_in: 1600.0,
    bend_out: 1200.0,
    turn_div: 612.0,
    speed_run: 1.6,
    speed_walk: 0.40,
    even_accel: false,
    ty: (128.0, 64.0, 0.6, 0.4),
    gamma_walk: 12.0,
    gamma_run: 8.0,
};

const ANKY: GrazerP = GrazerP {
    noway: (12, 32, 60),
    bend_div: 2.0,
    bend_max: PI / 3.0,
    bend_in: 2000.0,
    bend_out: 2000.0,
    speed_run: 0.75,
    speed_walk: 0.28,
    gamma_walk: 16.0,
    gamma_run: 10.0,
    ..PARA
};

const STEGO: GrazerP = GrazerP {
    speed_run: 0.96,
    speed_walk: 0.36,
    ..ANKY
};

const GALL: GrazerP = GrazerP {
    flee: Flee::Gall,
    forward: true,
    startle: 812.0,
    fear: (5, 5),
    wander: 2048.0,
    wobble: false,
    noway: (8, 8, 40),
    idle_stay: 76,
    idle_roll_div: true,
    tbase: 0.8,
    tper: 1.4,
    rdiv: 260.0,
    bend_div: 3.0,
    bend_max: PI / 2.0,
    bend_in: 800.0,
    bend_out: 400.0,
    turn_div: 1024.0,
    speed_run: 0.9,
    speed_walk: 0.32,
    even_accel: true,
    ty: (64.0, 32.0, 0.7, 0.4),
    ..PARA
};

const MOSH: GrazerP = GrazerP {
    flee: Flee::Mosh,
    noway: (8, 8, 80),
    prop: 1,
    bend_div: 2.0,
    speed_run: 0.6,
    speed_walk: 0.3,
    ..GALL
};

const DIMET: GrazerP = GrazerP { ..MOSH };

const CHASM: GrazerP = GrazerP {
    flee: Flee::Tric,
    kills: true,
    noway: (8, 48, 80),
    idles: 3,
    idle_start: 124,
    prop: 1,
    bend_div: 3.5,
    speed_run: 1.2,
    speed_walk: 0.30,
    ty: (128.0, 64.0, 0.6, 0.3),
    ..PARA
};

const TRIC_C1: GrazerP = GrazerP {
    c1_charge: Some((10, 6)),
    ..CHASM
};

const GALL_C1: GrazerP = GrazerP {
    speed_walk: 0.36,
    ..GALL
};

/// The first game's pachycephalosaurus: a parasaurolophus
/// that wanders less far and gives up on a blocked way sooner.
const PACH: GrazerP = GrazerP {
    wander: 6048.0,
    noway: (12, 32, 60),
    ..PARA
};

/// Parasaurolophus, ankylosaurus, stegosaurus, chasmosaurus, gallimimus,
/// moschops, dimetrodon.
fn grazer(s: &mut Sim, a: &Area, c: &mut Character, p: &GrazerP) {
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    if c.afraid_time != 0 {
        c.afraid_time = (c.afraid_time - s.time_dt).max(0);
    }
    if c.state == 2 {
        new_phase = true;
        c.state = 1;
    }
    let (hx, hy, hz) = (s.hunter.x, s.hunter.y, s.hunter.z);
    let (mut tdx, mut tdz, mut pdx, mut pdz, mut pdist);
    loop {
        tdx = c.tgx - c.pos.x;
        tdz = c.tgz - c.pos.z;
        let tdist = (tdx * tdx + tdz * tdz).sqrt();
        let off = if matches!(p.flee, Flee::Tric | Flee::Pred(_)) {
            300.0 * c.scale
        } else {
            0.0
        };
        pdx = hx - c.pos.x - c.lookx * off;
        pdz = hz - c.pos.z - c.lookz * off;
        pdist = (pdx * pdx + pdz * pdz).sqrt();

        if let Flee::Pred(_) = p.flee {
            if s.my_health == 0 {
                c.state = 0;
            }
        }
        if c.state != 0 {
            match p.flee {
                Flee::Pred(mul) => {
                    if pdist > s.view_r as f32 * mul + (s.opt_agres / 8) as f32 {
                        flee_from(c, pdx, pdz);
                        c.afraid_time -= s.time_dt;
                        if c.afraid_time <= 0 {
                            c.afraid_time = 0;
                            c.state = 0;
                        }
                    } else {
                        c.tgx = hx;
                        c.tgz = hz;
                        c.tgtime = 0;
                    }
                }
                Flee::Big => {
                    if c.afraid_time == 0 {
                        c.state = 0;
                        s.set_new_target_place(a, c, p.wander);
                        continue;
                    }
                    flee_from(c, pdx, pdz);
                }
                Flee::Gall => {
                    if c.afraid_time == 0 {
                        if pdist < 2048.0 {
                            c.state = 1;
                            c.afraid_time = (5 + s.rng.r(5)) * 1024;
                        }
                        if pdist > 4096.0 {
                            c.state = 0;
                            s.set_new_target_place(a, c, 2048.0);
                            continue;
                        }
                    }
                    flee_from(c, pdx, pdz);
                }
                Flee::Mosh => {
                    if c.afraid_time == 0 {
                        if pdist < 2048.0 {
                            c.afraid_time = (5 + s.rng.r(5)) * 1024;
                        }
                        if c.afraid_time == 0 && pdist > 4096.0 {
                            c.state = 0;
                            s.set_new_target_place(a, c, 2048.0);
                            continue;
                        }
                    }
                    flee_from(c, pdx, pdz);
                }
                Flee::Tric => {
                    if pdist < 6000.0 {
                        c.afraid_time = 8000;
                    }
                    if c.afraid_time == 0 {
                        c.state = 0;
                        s.set_new_target_place(a, c, p.wander);
                        continue;
                    }
                    let charge = match p.c1_charge {
                        Some((b, m)) => ((b + s.opt_agres * m) * 256) as f32,
                        None => (256 * p.charge_cells + s.opt_agres / 8) as f32,
                    };
                    if pdist > charge {
                        flee_from(c, pdx, pdz);
                    } else {
                        c.tgx = hx;
                        c.tgz = hz;
                        c.tgtime = 0;
                    }
                }
            }
        }
        if p.kills && s.my_health != 0 && pdist < 300.0 && (hy - c.pos.y - 160.0).abs() < 256.0 {
            c.state = 0;
            s.add_dead_body(a, Some(c), HUNT_EAT);
        }
        if p.forward && pdist > replace_dist(s) && s.replace_character_forward(a, c) {
            continue;
        }
        if c.state == 0 {
            c.afraid_time = 0;
            if !matches!(p.flee, Flee::Tric | Flee::Pred(_)) && pdist < p.startle {
                c.state = 1;
                c.afraid_time = (p.fear.0 + s.rng.r(p.fear.1)) * 1024;
                c.phase = s.danim(c, RUN);
                continue;
            }
            if tdist < 456.0 {
                s.set_new_target_place(a, c, p.wander);
                continue;
            }
        }
        break;
    }

    if p.nofind_near > 0.0 && pdist < p.nofind_near {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt > 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = aim(s, c, tdx, tdz);
        if p.wobble && c.afraid_time != 0 && (p.wobble_far == 0.0 || pdist > p.wobble_far) {
            c.tgalpha += (s.real_time as f32 / 1024.0).sin() / 3.0;
            wrap_2pi(&mut c.tgalpha);
        }
    }
    s.look_for_a_way(a, c, true, true);
    if c.no_way_cnt > p.noway.0 {
        c.no_way_cnt = 0;
        c.no_find_cnt = p.noway.1 + s.rng.r(p.noway.2);
    }
    wrap_2pi(&mut c.tgalpha);

    s.process_prev_phase(c);
    if tick(s, c) {
        new_phase = true;
    }

    if new_phase {
        if c.state == 0 {
            let last_idle = s.danim(c, IDLE1 + p.idles as usize - 1);
            if is_idle(s, c, p.idles) {
                match p.idle_mode {
                    IdleMode::Standard => {
                        if s.rng.r(128) > p.idle_stay && c.phase == last_idle {
                            c.phase = s.danim(c, WALK);
                        } else {
                            let r = if p.idle_roll_div {
                                s.rng.r(3) / 3
                            } else {
                                s.rng.r(p.idles - 1)
                            };
                            c.phase = s.danim(c, IDLE1 + r as usize);
                        }
                    }
                    IdleMode::Any(k) => {
                        if s.rng.r(128) > k {
                            c.phase = s.danim(c, WALK);
                        } else {
                            let r = s.rng.r(p.idles - 1);
                            c.phase = s.danim(c, IDLE1 + r as usize);
                        }
                    }
                    IdleMode::Walk => c.phase = s.danim(c, WALK),
                }
            } else if s.rng.r(128) > p.idle_start {
                let r = if p.idle_start_any {
                    s.rng.r(p.idles - 1)
                } else {
                    0
                };
                c.phase = s.danim(c, IDLE1 + r as usize);
            } else {
                c.phase = s.danim(c, WALK);
            }
        } else if c.afraid_time != 0 {
            c.phase = s.danim(c, RUN);
        } else {
            c.phase = s.danim(c, WALK);
        }
    }
    end_select(s, c, old_phase, old_ftime, new_phase, p.prop);

    let dr = drspd(c);
    if !is_idle(s, c, p.idles) {
        // Only walking slows the turn: the games test the behaviour state
        // against the water flag here, which never matches.
        let slow = c.phase == s.danim(c, WALK);
        turn(
            s,
            c,
            &Turn {
                base: p.tbase,
                per: p.tper,
                afraid_mul: 1.5,
                rdiv_afraid: p.rdiv,
                rdiv: p.rdiv,
                bend_div: p.bend_div,
                bend_max: p.bend_max,
                bend_in: p.bend_in,
                bend_out: p.bend_out,
                turn_div: p.turn_div,
            },
            slow,
        );
    }

    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == s.danim(c, RUN) {
        curspeed = p.speed_run;
    }
    if c.phase == s.danim(c, WALK) {
        curspeed = p.speed_walk;
    }
    if dr > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * dr / PI;
    }
    curspeed *= c.scale;
    if p.even_accel || curspeed > c.vspeed {
        delta_func(&mut c.vspeed, curspeed, dt(s) / 1024.0);
    } else {
        delta_func(&mut c.vspeed, curspeed, dt(s) / 256.0);
    }
    let (lx, lz, v) = (c.lookx, c.lookz, c.vspeed * dt(s));
    s.move_character(a, c, lx * v, lz * v, true, true);
    let (b, g, bl, gl) = p.ty;
    s.think_y_beta_gamma(a, c, b, g, bl, gl);
    c.tggamma += c.rspeed
        / if c.phase == s.danim(c, WALK) {
            p.gamma_walk
        } else {
            p.gamma_run
        };
    delta_func(&mut c.gamma, c.tggamma, dt(s) / 2048.0);
}

// ---------------------------------------------------------------------------
// The T-Rex
// ---------------------------------------------------------------------------

fn trex(s: &mut Sim, a: &Area, c: &mut Character) {
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let (hx, hy, hz) = (s.hunter.x, s.hunter.y, s.hunter.z);
    let (mut tdx, mut tdz, mut pdist);
    loop {
        tdx = c.tgx - c.pos.x;
        tdz = c.tgz - c.pos.z;
        let tdist = (tdx * tdx + tdz * tdz).sqrt();
        let pdx = hx - c.pos.x - c.lookx * 108.0;
        let pdz = hz - c.pos.z - c.lookz * 108.0;
        pdist = (pdx * pdx + pdz * pdz).sqrt();
        let palpha = find_vector_alpha(pdx, pdz);
        if c.state == 5 {
            new_phase = true;
            c.state = 1;
            c.phase = s.danim(c, WALK);
            c.ftime = 0;
            c.tgx = hx;
            c.tgz = hz;
            continue;
        }
        set_on_water(a, c, 560.0);
        if c.phase == s.danim(c, EAT) {
            break;
        }
        if s.my_health == 0 {
            c.state = 0;
        }
        if c.state != 0 {
            c.tgx = hx;
            c.tgz = hz;
            c.tgtime = 0;
            if c.state > 1 && angle_difference(c.alpha, palpha) < 0.4 {
                c.phase = if c.state == 2 {
                    let r = s.rng.r(1) as usize;
                    s.danim(c, LOOK2 + r)
                } else {
                    let r = s.rng.r(1) as usize;
                    s.danim(c, SMELL1 + r)
                };
                c.state = 1;
                c.rspeed = 0.0;
            }
            let reach = if s.c1 { 256.0 } else { 380.0 };
            if pdist < reach && (hy - c.pos.y).abs() < 256.0 {
                c.vspeed /= 8.0;
                c.state = 1;
                c.phase = s.danim(c, EAT);
                s.add_dead_body(a, Some(c), HUNT_KILL);
                let n = s.chars.len() - 1;
                s.chars[n].scale = c.scale;
                s.chars[n].alpha = c.alpha;
                c.bend = 0.0;
                s.demo.cindex = s.cur;
            }
        }
        if c.state == 0 && tdist < 1224.0 {
            s.set_new_target_place(a, c, 8048.0);
            continue;
        }
        break;
    }

    if pdist < 2048.0 {
        c.no_find_cnt = 0;
    }
    if c.no_find_cnt != 0 {
        c.no_find_cnt -= 1;
    } else {
        c.tgalpha = aim(s, c, tdx, tdz);
        if c.state != 0 && pdist > 5648.0 {
            c.tgalpha += (s.real_time as f32 / 824.0).sin() / 6.0;
            wrap_2pi(&mut c.tgalpha);
        }
    }
    let calm = c.state == 0;
    s.look_for_a_way(a, c, false, calm);
    if c.no_way_cnt > 12 {
        c.no_way_cnt = 0;
        c.no_find_cnt = 16 + s.rng.r(20);
    }
    wrap_2pi(&mut c.tgalpha);
    s.process_prev_phase(c);

    let look_mode = [LOOK1, LOOK2, SMELL1, SMELL2]
        .iter()
        .any(|&sl| c.phase == s.danim(c, sl));
    if tick(s, c) {
        new_phase = true;
    }
    let eat = s.danim(c, EAT);
    let roar = s.danim(c, ROAR);
    'select: {
        if c.phase == eat {
            break 'select;
        }
        if !new_phase && c.phase == roar {
            break 'select;
        }
        if c.state == 0 && new_phase && s.rng.r(128) > 110 {
            c.phase = if s.rng.r(128) > 64 {
                let r = s.rng.r(1) as usize;
                s.danim(c, LOOK2 + r)
            } else {
                let r = s.rng.r(1) as usize;
                s.danim(c, SMELL1 + r)
            };
            break 'select;
        }
        if !new_phase && look_mode {
            break 'select;
        }
        if c.state != 0 && new_phase && look_mode {
            c.phase = roar;
            break 'select;
        }
        let d = (c.tgalpha - c.alpha).abs();
        c.phase = if c.state == 0 || c.state > 1 {
            s.danim(c, WALK)
        } else if d < 1.0 || d > 2.0 * PI - 1.0 {
            s.danim(c, RUN)
        } else {
            s.danim(c, WALK)
        };
        if c.state_f & CS_ONWATER != 0 {
            c.phase = s.danim(c, SWIM);
        }
    }
    end_select(s, c, old_phase, old_ftime, new_phase, 1);

    let dr = drspd(c);
    if !(c.phase == roar || c.phase == eat || look_mode) {
        let st = c.state;
        let dalpha = (c.tgalpha - c.alpha).abs();
        let mut currspeed = if dr > 0.02 {
            if c.tgalpha > c.alpha {
                0.7 + dr * 1.4
            } else {
                -0.7 - dr * 1.4
            }
        } else {
            0.0
        };
        if c.afraid_time != 0 {
            currspeed *= 2.5;
        }
        if dalpha > PI {
            currspeed = -currspeed;
        }
        delta_func(
            &mut c.rspeed,
            currspeed,
            dt(s) / if st != 0 { 440.0 } else { 620.0 },
        );
        let tgbend = (dr / 2.0).min(PI / 6.0) * sgn(currspeed);
        delta_func(&mut c.bend, tgbend, dt(s) / 1800.0);
        let rspd = c.rspeed * dt(s) / 1024.0;
        if dr < rspd.abs() {
            c.alpha = c.tgalpha;
        } else {
            c.alpha += rspd;
        }
        if c.alpha > PI * 2.0 {
            c.alpha -= PI * 2.0;
        }
        if c.alpha < 0.0 {
            c.alpha += PI * 2.0;
        }
    }

    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == s.danim(c, RUN) {
        curspeed = 2.49;
    }
    if c.phase == s.danim(c, WALK) {
        curspeed = 0.76;
    }
    if c.phase == s.danim(c, SWIM) {
        curspeed = 0.70;
    }
    if dr > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * dr / PI;
    }
    delta_func(&mut c.vspeed, curspeed, dt(s) / 200.0);
    let (lx, lz, v) = (c.lookx, c.lookz, c.vspeed * dt(s) * c.scale);
    s.move_character(a, c, lx * v, lz * v, false, true);
    if c.state_f & CS_ONWATER != 0 {
        c.pos.y = a.land_up_h(c.pos.x, c.pos.z) - 540.0 * c.scale;
        c.beta /= 2.0;
        c.tggamma = 0.0;
    } else {
        s.think_y_beta_gamma(a, c, 348.0, 324.0, 0.5, 0.4);
    }
    c.tggamma += c.rspeed
        / if c.phase == s.danim(c, WALK) {
            16.0
        } else {
            12.0
        };
    delta_func(&mut c.gamma, c.tggamma, dt(s) / 2024.0);
}

// ---------------------------------------------------------------------------
// The brachiosaurus: wades its lakes and grazes, and nothing disturbs it.
// ---------------------------------------------------------------------------

fn brahi(s: &mut Sim, a: &Area, c: &mut Character) {
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let (mut tdx, mut tdz);
    loop {
        c.tgtime = 0;
        tdx = c.tgx - c.pos.x;
        tdz = c.tgz - c.pos.z;
        if (tdx * tdx + tdz * tdz).sqrt() < 256.0 {
            s.set_new_target_place_brahi(a, c, 2048.0);
            continue;
        }
        break;
    }
    c.tgalpha = find_vector_alpha(tdx, tdz);
    wrap_2pi(&mut c.tgalpha);
    s.process_prev_phase(c);
    if tick(s, c) {
        new_phase = true;
    }
    let walk = s.danim(c, WALK);
    if new_phase {
        if c.phase > walk {
            if s.rng.r(128) > 90 {
                c.phase = walk;
            } else {
                let r = s.rng.r(2) as usize;
                c.phase = s.danim(c, IDLE1 + r);
            }
        } else if s.rng.r(128) > 64 {
            c.phase = s.danim(c, IDLE1);
        } else {
            c.phase = walk;
        }
    }
    if old_phase != c.phase || new_phase {
        s.activate_fx(c);
    }
    if old_phase != c.phase {
        s.begin_morph(c, old_phase, old_ftime);
    }
    c.ftime %= s.ani_time(c, c.phase);

    let dr = drspd(c);
    if c.phase <= walk {
        let dalpha = (c.tgalpha - c.alpha).abs();
        let mut currspeed = if dr > 0.02 {
            if c.tgalpha > c.alpha {
                0.2 + dr * 0.2
            } else {
                -0.2 - dr * 0.2
            }
        } else {
            0.0
        };
        if dalpha > PI {
            currspeed = -currspeed;
        }
        delta_func(&mut c.rspeed, currspeed, dt(s) / 600.0);
        let tgbend = (dr / 4.0).min(PI / 4.0) * sgn(currspeed);
        delta_func(&mut c.bend, tgbend, dt(s) / 3200.0);
        let rspd = c.rspeed * dt(s) / 1024.0;
        if dr < rspd.abs() {
            c.alpha = c.tgalpha;
        } else {
            c.alpha += rspd;
        }
        if c.alpha > PI * 2.0 {
            c.alpha -= PI * 2.0;
        }
        if c.alpha < 0.0 {
            c.alpha += PI * 2.0;
        }
    }
    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = if c.phase == walk { 0.2 } else { 0.0 };
    if dr > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * dr / PI;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dt(s) / 1024.0);
    c.pos.x += c.lookx * c.vspeed * dt(s);
    c.pos.z += c.lookz * c.vspeed * dt(s);
    s.think_y_beta_gamma(a, c, 256.0, 128.0, 0.1, 0.2);
    delta_func(&mut c.gamma, c.tggamma, dt(s) / 4048.0);
}

// ---------------------------------------------------------------------------
// The flyers (dimorphodon, pteranodon)
// ---------------------------------------------------------------------------

fn dimor(s: &mut Sim, a: &Area, c: &mut Character) {
    let mut new_phase = false;
    let old_phase = c.phase;
    let old_ftime = c.ftime;
    let (hx, hz) = (s.hunter.x, s.hunter.z);
    let (mut tdx, mut tdz);
    loop {
        tdx = c.tgx - c.pos.x;
        tdz = c.tgz - c.pos.z;
        let tdist = (tdx * tdx + tdz * tdz).sqrt();
        let pdist = ((hx - c.pos.x).powi(2) + (hz - c.pos.z).powi(2)).sqrt();
        if pdist > replace_dist(s) && s.replace_character_forward(a, c) {
            continue;
        }
        if tdist < 1024.0 {
            s.set_new_target_place(a, c, 4048.0);
            continue;
        }
        break;
    }
    c.tgalpha = aim(s, c, tdx, tdz);
    wrap_2pi(&mut c.tgalpha);
    s.process_prev_phase(c);
    if tick(s, c) {
        new_phase = true;
    }
    let (fly, glide) = (s.danim(c, FLY), s.danim(c, GLIDE));
    if new_phase {
        let h = a.land_h(c.pos.x, c.pos.z);
        if c.phase == fly {
            if c.pos.y > h + 2800.0 {
                c.phase = glide;
            }
        } else if c.phase == glide && c.pos.y < h + 1800.0 {
            c.phase = fly;
        }
    }
    if (old_phase != c.phase || new_phase) && (s.rng.rand() & 1023) > 980 {
        s.activate_fx(c);
    }
    if old_phase != c.phase {
        if !new_phase {
            c.ftime = 0;
        }
        s.begin_morph(c, old_phase, old_ftime);
    }
    c.ftime %= s.ani_time(c, c.phase);

    let dr = drspd(c);
    turn(
        s,
        c,
        &Turn {
            base: 0.6,
            per: 1.2,
            afraid_mul: 1.0,
            rdiv_afraid: 460.0,
            rdiv: 460.0,
            bend_div: 2.0,
            bend_max: PI / 2.0,
            bend_in: 800.0,
            bend_out: 400.0,
            turn_div: 1024.0,
        },
        false,
    );
    c.lookx = c.alpha.cos();
    c.lookz = c.alpha.sin();
    let mut curspeed = 0.0;
    if c.phase == fly {
        curspeed = 1.5;
    }
    if c.phase == glide {
        curspeed = 1.3;
    }
    if dr > PI / 2.0 {
        curspeed *= 2.0 - 2.0 * dr / PI;
    }
    let h = a.land_h(c.pos.x, c.pos.z);
    if c.phase == fly {
        delta_func(&mut c.pos.y, h + 4048.0, dt(s) / 6.0);
    } else {
        delta_func(&mut c.pos.y, h, dt(s) / 16.0);
    }
    if c.pos.y < h + 236.0 {
        c.pos.y = h + 256.0;
    }
    curspeed *= c.scale;
    delta_func(&mut c.vspeed, curspeed, dt(s) / 2024.0);
    c.pos.x += c.lookx * c.vspeed * dt(s);
    c.pos.z += c.lookz * c.vspeed * dt(s);
    c.tggamma = (c.rspeed / 4.0).clamp(-PI / 6.0, PI / 6.0);
    delta_func(&mut c.gamma, c.tggamma, dt(s) / 2048.0);
}

fn dimor_dead(s: &mut Sim, a: &Area, c: &mut Character) {
    let (fall, die) = (s.danim(c, FALL), s.danim(c, DIE));
    if c.phase != fall && c.phase != die {
        let (op, of) = (c.phase, c.ftime);
        s.begin_morph(c, op, of);
        c.ftime = 0;
        c.phase = fall;
        c.rspeed = 0.0;
        s.activate_fx(c);
        return;
    }
    s.process_prev_phase(c);
    c.ftime += s.time_dt;
    let at = s.ani_time(c, c.phase);
    if c.ftime >= at {
        if c.phase == die {
            c.ftime = at - 1;
        } else {
            c.ftime %= at;
        }
    }
    delta_func(
        &mut c.vspeed,
        0.0,
        dt(s) / if c.phase == die { 400.0 } else { 1200.0 },
    );
    c.pos.x += c.lookx * c.vspeed * dt(s);
    c.pos.z += c.lookz * c.vspeed * dt(s);
    if c.phase == fall {
        let lh = a.land_h(c.pos.x, c.pos.z);
        s.fall_splash(a, c, c.pos.y + c.rspeed * dt(s) / 1024.0, false);
        c.pos.y += c.rspeed * dt(s) / 1024.0;
        c.rspeed -= dt(s) * 2.56;
        if c.pos.y < lh {
            c.pos.y = lh;
            s.fall_splash(a, c, c.pos.y, true);
            let (op, of) = (c.phase, c.ftime);
            s.begin_morph(c, op, of);
            c.phase = die;
            c.ftime = 0;
            s.activate_fx(c);
        }
    } else {
        s.think_y_beta_gamma(a, c, 140.0, 126.0, 0.6, 0.5);
        delta_func(&mut c.gamma, c.tggamma, dt(s) / 1600.0);
    }
}

/// Every other death (AnimateXxxDead): fall, lie still - or with the
/// tranquilliser, go to sleep - and slide to a stop.
fn dead(s: &mut Sim, a: &Area, c: &mut Character, blook: f32, glook: f32, can_sleep: bool) {
    let (die, sleep) = (s.danim(c, DIE), s.danim(c, SLEEP));
    let dying = c.phase == die || (can_sleep && c.phase == sleep);
    if !dying {
        let (op, of) = (c.phase, c.ftime);
        s.begin_morph(c, op, of);
        c.ftime = 0;
        c.phase = die;
        s.activate_fx(c);
    } else {
        s.process_prev_phase(c);
        c.ftime += s.time_dt;
        let at = s.ani_time(c, c.phase);
        if c.ftime >= at {
            if can_sleep && s.tranq {
                c.ftime = 0;
                c.phase = sleep;
                s.activate_fx(c);
            } else {
                c.ftime = at - 1;
            }
        }
    }
    delta_func(&mut c.vspeed, 0.0, dt(s) / 800.0);
    c.pos.x += c.lookx * c.vspeed * dt(s);
    c.pos.z += c.lookz * c.vspeed * dt(s);
    s.think_y_beta_gamma(a, c, blook, glook, 0.6, 0.5);
    delta_func(&mut c.gamma, c.tggamma, dt(s) / 1600.0);
}

// ---------------------------------------------------------------------------
// Ice Age: the same shapes with its own animals.
// ---------------------------------------------------------------------------

pub const ICE_PIG: i32 = 1;
pub const ICE_ARCHEO: i32 = 2;
pub const ICE_BRONT: i32 = 10;
pub const ICE_HOG: i32 = 11;
pub const ICE_WOLF: i32 = 12;
pub const ICE_RHINO: i32 = 13;
pub const ICE_DIATR: i32 = 14;
pub const ICE_DEER: i32 = 15;
pub const ICE_SMILO: i32 = 16;
pub const ICE_MAMM: i32 = 17;
pub const ICE_BEAR: i32 = 18;

/// Ice Age's animation orders (the PIG_, BRO_, DIA_, DER_, MAM_ constants).
pub fn ice_layout(d: &mut DinoInfo) {
    let a = &mut d.anim;
    *a = [-1; COUNT];
    let set = |a: &mut [i32; COUNT], l: &[(usize, i32)]| {
        for &(s, v) in l {
            a[s] = v;
        }
    };
    match d.ai {
        ICE_PIG | ICE_BRONT | ICE_HOG | ICE_WOLF | ICE_RHINO | ICE_SMILO | ICE_BEAR => set(
            a,
            &[
                (WALK, 0),
                (RUN, 1),
                (IDLE1, 2),
                (IDLE2, 3),
                (IDLE3, 4),
                (DIE, 5),
                (SLEEP, 6),
            ],
        ),
        ICE_ARCHEO => set(a, &[(FLY, 0), (GLIDE, 1), (FALL, 2), (DIE, 3)]),
        ICE_DIATR => set(
            a,
            &[
                (WALK, 0),
                (RUN, 1),
                (SWIM, 1),
                (IDLE1, 2),
                (IDLE2, 3),
                (JUMP, 4),
                (SLIDE, 5),
                (EAT, 6),
                (DIE, 7),
                (SLEEP, 8),
            ],
        ),
        ICE_DEER | ICE_MAMM => set(
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
        _ => set(a, &[(WALK, 0)]),
    }
}

const PIG: GrazerP = GrazerP {
    idles: 3,
    idle_mode: IdleMode::Any(96),
    speed_walk: 0.2,
    ..MOSH
};

const ICE_CHARGER: GrazerP = GrazerP {
    nofind_near: 2048.0,
    wobble_far: 12.0 * 256.0,
    ..CHASM
};

const BRONT: GrazerP = GrazerP {
    speed_run: 1.536,
    speed_walk: 0.336,
    ..ICE_CHARGER
};

const HOG: GrazerP = GrazerP {
    charge_cells: 20,
    tbase: 0.3,
    tper: 1.4,
    speed_run: 1.152,
    speed_walk: 0.224,
    ..ICE_CHARGER
};

const RHINO: GrazerP = GrazerP {
    charge_cells: 18,
    tbase: 0.3,
    tper: 1.2,
    speed_run: 1.152,
    speed_walk: 0.336,
    ..ICE_CHARGER
};

const WOLF: GrazerP = GrazerP {
    flee: Flee::Pred(140.0),
    idle_mode: IdleMode::Any(64),
    idle_start_any: true,
    tbase: 0.4,
    tper: 1.5,
    speed_run: 2.048,
    speed_walk: 0.32,
    ..ICE_CHARGER
};

const BEAR: GrazerP = GrazerP {
    charge_cells: 20,
    nofind_near: 2048.0,
    speed_run: 1.792,
    speed_walk: 0.48,
    ..CHASM
};

const SMILO: GrazerP = GrazerP {
    idle_mode: IdleMode::Walk,
    idle_start_any: true,
    tbase: 0.3,
    tper: 1.5,
    speed_run: 2.048,
    speed_walk: 0.64,
    ..ICE_CHARGER
};

const DEER: GrazerP = GrazerP {
    nofind_near: 2048.0,
    speed_run: 1.536,
    speed_walk: 0.32,
    ..ANKY
};

const MAMM: GrazerP = GrazerP {
    nofind_near: 3048.0,
    speed_run: 1.28,
    speed_walk: 0.64,
    ..ANKY
};

const DIATR: HunterP = HunterP {
    speed_run: 1.28,
    speed_jump: 1.2,
    speed_walk: 0.448,
    speed_swim: 0.6,
    ..SPINO
};

fn ice(s: &mut Sim, a: &Area, c: &mut Character, alive: bool) {
    match c.ai {
        AI_HUNTER => s.animate_hunt_dead(a, c),
        ICE_PIG if alive => grazer(s, a, c, &PIG),
        ICE_ARCHEO if alive => dimor(s, a, c),
        ICE_ARCHEO => dimor_dead(s, a, c),
        ICE_BRONT if alive => grazer(s, a, c, &BRONT),
        ICE_HOG if alive => grazer(s, a, c, &HOG),
        ICE_WOLF if alive => grazer(s, a, c, &WOLF),
        ICE_RHINO if alive => grazer(s, a, c, &RHINO),
        ICE_DIATR if alive => hunter(s, a, c, &DIATR),
        ICE_DEER if alive => grazer(s, a, c, &DEER),
        ICE_SMILO if alive => grazer(s, a, c, &SMILO),
        ICE_MAMM if alive => grazer(s, a, c, &MAMM),
        ICE_BEAR if alive => grazer(s, a, c, &BEAR),
        // The yeti and the poacher had no behaviour of their own.
        _ if !alive => dead(s, a, c, 100.0, 96.0, true),
        _ => {}
    }
}

//! The hunter: walking, running, crouching, jumping, swimming, and the
//! camera on his head, in the games' own units and with their constants.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::area::Area;
use crate::game::GameKind;
use crate::settings::Settings;

#[derive(Resource, Debug, Clone)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Heading: 0 looks toward -z (north), increasing turns right.
    pub alpha: f32,
    /// Pitch: positive looks down.
    pub beta: f32,
    pub vspeed: f32,
    pub sspeed: f32,
    pub yspeed: f32,
    pub head_y: f32,
    pub run: bool,
    pub crouch: bool,
    pub swim: bool,
    pub underwater: bool,
    pub underwater_t: f32,
    pub on_water: bool,
    stepdy: f32,
    stepdd: f32,
    rav: f32,
    rbv: f32,
    /// Where the eye is, and the view angles, after bob and sway.
    pub cam: Vec3,
    pub cam_alpha: f32,
    pub cam_beta: f32,
    /// Extra field-of-view factor (under water, binoculars); 1 normally.
    pub zoom_w: f32,
    pub zoom_h: f32,
    /// Real time in milliseconds, the games' clock.
    pub real_time: f32,
    pub noclip: bool,
    pub fly: bool,
    /// A footstep to play this frame: in water, and how loud (0..256).
    pub footstep: Option<(bool, i32)>,
    /// A line for the top left, from a key that toggles a mode.
    pub message: Option<&'static str>,
    /// The kick of a shot: how far the head is thrown back.
    pub head_back: f32,
}

impl Player {
    pub fn new(area: &Area) -> Player {
        let (lx, lz) = if area.trophy {
            (76, 70)
        } else {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as usize)
                .unwrap_or(0);
            area.landings[t % area.landings.len()]
        };
        Self::at(area, (lx * 256 + 128) as f32, (lz * 256 + 128) as f32, 0.0)
    }

    pub fn at(area: &Area, x: f32, z: f32, alpha: f32) -> Player {
        let y = area.land_qh(x, z, 0.0);
        Player {
            x,
            y,
            z,
            alpha,
            beta: 0.0,
            vspeed: 0.0,
            sspeed: 0.0,
            yspeed: 0.0,
            head_y: 220.0,
            run: false,
            crouch: false,
            swim: false,
            underwater: false,
            underwater_t: 0.0,
            on_water: false,
            stepdy: 0.0,
            stepdd: 0.0,
            rav: 0.0,
            rbv: 0.0,
            cam: Vec3::new(x, y + 220.0, z),
            cam_alpha: alpha,
            cam_beta: 0.0,
            zoom_w: 1.0,
            zoom_h: 1.0,
            real_time: 0.0,
            noclip: false,
            fly: false,
            footstep: None,
            message: None,
            head_back: 0.0,
        }
    }

    fn slide(&mut self, area: &Area) {
        if self.noclip || self.underwater {
            return;
        }
        let ch = area.land_qh_no_obj(self.x, self.z);
        let mut mh = ch;
        let mut sd = 0;
        let probes = [
            (-16.0, 0.0),
            (16.0, 0.0),
            (0.0, -16.0),
            (0.0, 16.0),
            (-12.0, -12.0),
            (12.0, -12.0),
            (-12.0, 12.0),
            (12.0, 12.0),
        ];
        for (k, (dx, dz)) in probes.iter().enumerate() {
            let h = area.land_qh_no_obj(self.x + dx, self.z + dz);
            if h < mh {
                mh = h;
                sd = k + 1;
            }
        }
        if mh < ch - 16.0 {
            let mut delta = (ch - mh) / 4.0;
            match sd {
                1 => self.x -= delta,
                2 => self.x += delta,
                3 => self.z -= delta,
                4 => self.z += delta,
                _ => {}
            }
            delta *= 0.7;
            match sd {
                5 => {
                    self.x -= delta;
                    self.z -= delta;
                }
                6 => {
                    self.x += delta;
                    self.z -= delta;
                }
                7 => {
                    self.x -= delta;
                    self.z += delta;
                }
                8 => {
                    self.x += delta;
                    self.z += delta;
                }
                _ => {}
            }
        }
    }
}

/// What the keys asked for this frame.
#[derive(Default, Clone, Copy, Debug)]
pub struct Controls {
    pub forward: bool,
    pub backward: bool,
    pub sleft: bool,
    pub sright: bool,
    pub left: bool,
    pub right: bool,
    pub look_up: bool,
    pub look_down: bool,
    pub jump: bool,
    pub down: bool,
    pub mouse: Vec2,
    /// The strafe key: the mouse steps aside instead of turning.
    pub strafe: bool,
    /// A weapon is out, so the hunter walks.
    pub weapon_up: bool,
    /// Debug mode's Ctrl: a flat speed ahead, or back with the back key.
    pub boost: Option<f32>,
}

pub fn read_controls(
    input: &crate::keymap::Input,
    mouse: &AccumulatedMouseMotion,
    player: &mut Player,
    settings: &Settings,
) -> Controls {
    use crate::keymap::Act;
    let km = &settings.keys;
    let k = |a: Act| km.held(a, input);
    if settings.run_hold {
        player.run = k(Act::Run);
    } else if km.pressed(Act::Run, input) {
        player.run = !player.run;
        player.message = Some(if player.run {
            "Run mode is ON"
        } else {
            "Run mode is OFF"
        });
    }
    if settings.crouch_hold {
        player.crouch = k(Act::Crouch);
    } else if km.pressed(Act::Crouch, input) {
        player.crouch = !player.crouch;
        player.message = Some(if player.crouch {
            "Crouch mode is ON"
        } else {
            "Crouch mode is OFF"
        });
    }
    // Strafe turns the turning keys into side steps.
    let strafe = k(Act::Strafe);
    Controls {
        forward: k(Act::Forward),
        backward: k(Act::Backward),
        sleft: k(Act::StepLeft) || (strafe && k(Act::TurnLeft)),
        sright: k(Act::StepRight) || (strafe && k(Act::TurnRight)),
        left: !strafe && k(Act::TurnLeft),
        right: !strafe && k(Act::TurnRight),
        look_up: k(Act::TurnUp),
        look_down: k(Act::TurnDown),
        jump: k(Act::Jump),
        down: player.crouch,
        mouse: mouse.delta,
        strafe,
        weapon_up: false,
        boost: None,
    }
}

/// One frame of the hunter's movement. `dt_ms` is the frame time in
/// milliseconds.
pub fn step(p: &mut Player, area: &Area, c: &Controls, settings: &Settings, dt_ms: f32) {
    let dt = dt_ms / 1000.0;
    p.real_time += dt_ms;
    let rt = p.real_time;

    // Mouse look, smoothed the way the original did it.
    let sens = (settings.mouse_sens + 64) as f32 / 600.0 / 192.0;
    let my = if settings.mouse_invert {
        -c.mouse.y
    } else {
        c.mouse.y
    };
    p.rav += c.mouse.x * sens;
    p.rbv += my * sens;
    if c.strafe {
        p.sspeed += p.rav * 10.0;
    } else {
        p.alpha += p.rav;
    }
    p.beta += p.rbv;
    p.rav /= 2.0 + dt_ms / 20.0;
    p.rbv /= 2.0 + dt_ms / 20.0;

    if !(c.forward || c.backward) {
        p.vspeed = if p.vspeed > 0.0 {
            (p.vspeed - dt * 2.0).max(0.0)
        } else {
            (p.vspeed + dt * 2.0).min(0.0)
        };
    }
    if !(c.sleft || c.sright) {
        p.sspeed = if p.sspeed > 0.0 {
            (p.sspeed - dt * 2.0).max(0.0)
        } else {
            (p.sspeed + dt * 2.0).min(0.0)
        };
    }
    if c.forward {
        p.vspeed += if p.vspeed > 0.0 { dt } else { dt * 4.0 };
    }
    if c.backward {
        p.vspeed -= if p.vspeed < 0.0 { dt } else { dt * 4.0 };
    }
    if c.sright {
        p.sspeed += if p.sspeed > 0.0 { dt } else { dt * 4.0 };
    }
    if c.sleft {
        p.sspeed -= if p.sspeed < 0.0 { dt } else { dt * 4.0 };
    }
    // Running needs the run mode, the hunter upright (Ice Age allows a
    // little stoop) and no weapon out; anything else is a walk.
    let upright = if area.kind == GameKind::IceAge {
        p.head_y > 190.0
    } else {
        p.head_y == 220.0
    };
    let lim = if p.swim {
        0.25
    } else if p.run && upright && !c.weapon_up {
        0.7
    } else {
        0.3
    };
    p.vspeed = p.vspeed.clamp(-lim, lim);
    p.sspeed = p.sspeed.clamp(-lim, lim);
    if let Some(b) = c.boost {
        p.vspeed = if c.backward { -b } else { b };
    }

    if c.jump && p.yspeed == 0.0 && !p.swim && p.y <= area.land_qh(p.x, p.z, p.y) {
        p.yspeed = 600.0 + p.vspeed.abs() * 600.0;
    }

    if c.right {
        p.alpha += dt * 1.5;
    }
    if c.left {
        p.alpha -= dt * 1.5;
    }
    if c.look_up {
        p.beta -= dt;
    }
    if c.look_down {
        p.beta += dt;
    }

    let (sa, ca) = p.alpha.sin_cos();
    let (sb, cb) = p.beta.sin_cos();
    let mut nv = Vec3::new(sa, 0.0, -ca);
    let sv = nv;
    if p.underwater || p.fly {
        nv = Vec3::new(nv.x * cb, -sb, nv.z * cb);
    }
    let nv = nv * dt_ms * p.vspeed;
    let sv = Vec3::new(sv.x, 0.0, sv.z) * dt_ms * p.sspeed;
    let mvi = 1 + (dt_ms as i32) / 16;
    for _ in 0..mvi {
        let k = mvi as f32;
        p.x += nv.x / k;
        p.y += nv.y / k;
        p.z += nv.z / k;
        p.x -= sv.z / k;
        p.z += sv.x / k;
        if !p.noclip {
            let (mut x, mut z) = (p.x, p.z);
            area.check_collision(&mut x, &mut z, p.y);
            p.x = x;
            p.z = z;
        }
        if p.y <= area.land_qh_no_obj(p.x, p.z) + 16.0 {
            p.slide(area);
            p.slide(area);
        }
    }

    // The kick of the last shot wears off.
    if p.head_back != 0.0 {
        p.head_back -= dt * (80.0 + (32.0 - (p.head_back - 32.0).abs()) * 4.0);
        if p.head_back <= 0.0 {
            p.head_back = 0.0;
        }
    }

    // Standing up and crouching.
    if c.down || p.underwater {
        p.head_y = p.head_y.max(110.0);
        p.head_y -= dt * (60.0 + (p.head_y - 110.0) * 5.0);
        p.head_y = p.head_y.max(110.0);
    } else {
        p.head_y = p.head_y.min(220.0);
        p.head_y += dt * (60.0 + (220.0 - p.head_y) * 5.0);
        p.head_y = p.head_y.min(220.0);
    }

    let h = area.land_qh(p.x, p.z, p.y);
    let hu = area.land_ceil_h(p.x, p.z, p.y) - 64.0;
    let hwater = area.land_up_h(p.x, p.z);

    if !p.underwater {
        if p.y > h {
            p.yspeed -= dt * 3000.0;
        }
    } else if p.yspeed < 0.0 {
        p.yspeed = (p.yspeed + dt * 4000.0).min(0.0);
    }
    if p.fly {
        p.yspeed = 0.0;
    }
    p.y += p.yspeed * dt;

    if p.y + p.head_y > hu {
        if p.yspeed > 0.0 {
            p.yspeed = -1.0;
        }
        p.y = hu - p.head_y;
        if p.y < h {
            p.y = h;
            p.head_y = (hu - p.y).max(110.0);
        }
    }

    if p.y < h {
        if p.yspeed < -600.0 {
            p.footstep = Some((false, 64));
        }
        if p.yspeed < -800.0 {
            p.head_y += p.yspeed / 100.0;
        }
        if p.y < h - 80.0 {
            p.y = h - 80.0;
        }
        p.y += (h - p.y + 32.0) * dt * 4.0;
        if p.y > h {
            p.y = h;
        }
        p.yspeed = 0.0;
    }

    p.swim = false;
    if !p.underwater && c.jump && p.y < hwater - 148.0 {
        p.swim = true;
        p.y = hwater - 148.0;
        p.yspeed = 0.0;
    }

    let prev = p.stepdy;
    p.stepdy = if p.swim {
        (rt / 360.0).sin() * 20.0
    } else {
        (p.vspeed.abs() + p.sspeed.abs()).min(1.0) * (rt / 80.0).sin() * 22.0
    };
    let d = p.stepdy - prev;
    // The first game has the one step sound, in water or out, and steps
    // on under water too.
    let c1 = area.kind == GameKind::Carnivores;
    if (c1 || !p.underwater) && p.y < h + 64.0 && d < 0.0 && p.stepdd >= 0.0 {
        p.footstep = if p.on_water && !c1 {
            Some((true, 64 + (p.vspeed * 30.0) as i32))
        } else {
            Some((false, 24 + (p.vspeed * 50.0) as i32))
        };
    }
    p.stepdd = d;

    p.beta = p.beta.clamp(-1.26, 1.46);

    let (head_alpha, head_beta) = (p.head_back / 20000.0, -p.head_back / 10000.0);
    p.cam_alpha = p.alpha + head_alpha;
    p.cam_beta = p.beta + head_beta;
    p.cam = Vec3::new(
        p.x - sa * p.head_back,
        p.y + p.head_y + p.stepdy,
        p.z + ca * p.head_back,
    );

    p.on_water = area.land_up_h(p.cam.x, p.cam.z) > area.land_h(p.cam.x, p.cam.z);
    let up = area.land_up_h(p.cam.x, p.cam.z);
    // In the first game flying counts as being under water.
    let fly_under = c1 && p.fly;
    if p.underwater {
        p.underwater = up - 4.0 >= p.cam.y || fly_under;
        if !p.underwater {
            p.head_y += 20.0;
            p.cam.y += 20.0;
        }
    } else {
        p.underwater = up + 28.0 >= p.cam.y || fly_under;
        if p.underwater {
            p.head_y -= 20.0;
            p.cam.y -= 20.0;
        }
    }
    if !p.underwater {
        p.underwater_t = 0.0;
    } else {
        p.underwater_t = (p.underwater_t + dt_ms).min(512.0);
    }

    // Under water the view swims (the projection squeezes and sways).
    if p.underwater {
        let ut = (p.underwater_t / 512.0 * std::f32::consts::FRAC_PI_2).sin();
        p.zoom_w = 1.0 + ((1.0 + (rt / 180.0).cos()) / 30.0 + (1.0 - ut) / 1.5) / 1.25;
        p.zoom_h = 1.0 + ((1.0 + (rt / 180.0).sin()) / 30.0 - (1.0 - ut) / 16.0) / 1.25;
        p.cam_alpha += (rt / 360.0).cos() / 120.0;
        p.cam_beta += (rt / 360.0).sin() / 100.0;
        p.cam.y -= (rt / 360.0).sin() * 4.0;
    } else {
        p.zoom_w = 1.0;
        p.zoom_h = 1.0;
    }

    if p.swim {
        p.cam_beta -= (rt / 360.0).cos() / 80.0;
        p.x += dt * 32.0;
        p.z += dt * 32.0;
    }
    if p.noclip {
        p.cam.y += 1024.0;
    }
    p.cam_beta = p.cam_beta.clamp(-1.26, 1.46);
}

/// The camera's rotation for the view angles.
pub fn view_rotation(alpha: f32, beta: f32) -> Quat {
    Quat::from_rotation_y(-alpha) * Quat::from_rotation_x(-beta)
}

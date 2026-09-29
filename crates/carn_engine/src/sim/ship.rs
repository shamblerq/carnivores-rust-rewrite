//! The drop ship that comes for a trophy: flies in, lowers its hook, winches
//! the body up and carries it off.

use std::f32::consts::PI;

use bevy::math::Vec3;

use super::*;
use crate::area::Area;

#[derive(Clone, Debug)]
pub struct Ship {
    /// -1 idle; 0 flying to the body; 3 lowering; 2 winching up; 1 leaving.
    pub state: i32,
    pub pos: Vec3,
    pub tgpos: Vec3,
    pub retpos: Vec3,
    pub alpha: f32,
    pub tgalpha: f32,
    pub speed: f32,
    pub rspeed: f32,
    pub delta_y: f32,
    pub ftime: i32,
    pub cindex: Option<usize>,
}

impl Default for Ship {
    fn default() -> Self {
        Ship {
            state: -1,
            pos: Vec3::ZERO,
            tgpos: Vec3::ZERO,
            retpos: Vec3::ZERO,
            alpha: 0.0,
            tgalpha: 0.0,
            speed: 0.0,
            rspeed: 0.0,
            delta_y: 2048.0,
            ftime: 0,
            cindex: None,
        }
    }
}

/// A trophy taken this hunt.
#[derive(Clone, Debug)]
pub struct Trophy {
    pub ctype: usize,
    pub scale: f32,
    pub weapon: usize,
    pub score: i32,
    pub phase: i32,
    /// Hours << 10 | minutes.
    pub time: i32,
    /// Year << 20 | month << 10 | day.
    pub date: i32,
    /// In the games' own distance unit (units / 64).
    pub range: f32,
}

impl Sim {
    fn ship_anim_time(&self) -> i32 {
        self.ship_info
            .as_ref()
            .and_then(|s| s.animations.first())
            .map(|a| a.ani_time.max(1))
            .unwrap_or(1000)
    }

    fn ship_fx(&mut self, i: usize, at: Option<Vec3>) {
        self.sounds.push(SoundEvent {
            sound: SoundRef::Ship(i),
            pos: at,
            volume: 256,
        });
    }

    /// A trophy creature has died: note the trophy and send for the ship.
    pub fn add_ship_task(&mut self, a: &Area, ci: usize, weapon: usize) {
        let c = self.chars[ci].clone();
        let dry = a.land_up_h(c.pos.x, c.pos.z) - a.land_h(c.pos.x, c.pos.z) < 100.0;
        if dry && !self.tranq {
            self.ship_tasks.push(ci);
            self.ship_fx(3, None);
        }
        self.hunt_stats.success += 1;
        let d = &self.dinos[c.ctype];
        let mut score = d.base_score as f32;
        if self.hunt_stats.success > 1 {
            score *= 1.0 + self.hunt_stats.success as f32 / 10.0;
        }
        if self.target_dino & (1 << c.ai.clamp(0, 31)) == 0 {
            score /= 2.0;
        }
        if self.tranq {
            score *= 1.25;
        }
        if self.radar {
            score *= 0.70;
        }
        if self.scent {
            score *= 0.80;
        }
        if self.camo {
            score *= 0.85;
        }
        self.score += score as i32;
        if !self.tranq {
            let (time, date) = local_time();
            let hunter = Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z);
            self.trophies.push(Trophy {
                ctype: c.ctype,
                scale: c.scale,
                weapon,
                score: score as i32,
                phase: self.real_time & 3,
                time,
                date,
                range: (c.pos - hunter).length() / 64.0,
            });
            self.trophy_time = 20 * 1000;
        }
    }

    fn init_ship(&mut self, a: &Area, ci: usize) {
        let c = &self.chars[ci];
        let (hx, hz) = (self.hunter.x, self.hunter.z);
        let mut ship = Ship {
            delta_y: 2048.0 + self.dinos[c.ctype].sh_delta * c.scale,
            ..Default::default()
        };
        ship.pos.x = hx - 90.0 * 256.0;
        if ship.pos.x < 256.0 {
            ship.pos.x = hx + 90.0 * 256.0;
        }
        ship.pos.z = hz - 90.0 * 256.0;
        if ship.pos.z < 256.0 {
            ship.pos.z = hz + 90.0 * 256.0;
        }
        ship.pos.y = a.land_up_h(ship.pos.x, ship.pos.z) + ship.delta_y + 1024.0;
        ship.tgpos = Vec3::new(c.pos.x, 0.0, c.pos.z);
        ship.tgpos.y = a.land_up_h(c.pos.x, c.pos.z) + ship.delta_y;
        ship.state = 0;
        ship.retpos = ship.pos;
        ship.cindex = Some(ci);
        ship.ftime = 0;
        ship.alpha = self.ship.alpha;
        self.ship = ship;
    }

    /// The ship's frame.
    pub fn animate_ship(&mut self, a: &Area) {
        if self.ship.state == -1 {
            if !self.ship_tasks.is_empty() {
                let ci = self.ship_tasks.remove(0);
                self.init_ship(a, ci);
            }
            return;
        }
        let dt = self.time_dt as f32;
        let am = self.ship_anim_time();
        let mut tdt = self.time_dt;
        if self.ship.ftime != 0 {
            if self.ship.ftime < 500 {
                tdt = self.time_dt * (self.ship.ftime + 48) / 548;
            }
            if am - self.ship.ftime < 500 {
                tdt = self.time_dt * (am - self.ship.ftime + 48) / 548;
            }
            tdt = tdt.max(2);
        }
        let sh = &mut self.ship;
        let l = (sh.tgpos - sh.pos).length();
        let l2 = ((sh.tgpos.x - sh.pos.x).powi(2) * 2.0).sqrt();
        sh.pos.y += 0.3 * (self.real_time as f32 / 256.0).cos();
        sh.tgalpha = find_vector_alpha(sh.tgpos.x - sh.pos.x, sh.tgpos.z - sh.pos.z);
        let dalpha = (sh.tgalpha - sh.alpha).abs();
        let drspd = if dalpha > PI {
            2.0 * PI - dalpha
        } else {
            dalpha
        };

        let hunter = Vec3::new(self.hunter.x, self.hunter.y, self.hunter.z);
        if sh.state != 0
            && sh.speed > 1.0
            && l < 4000.0
            && (hunter - sh.pos).length() < (self.view_r + 2) as f32 * 256.0
        {
            sh.tgpos.x += sh.alpha.cos() * 256.0 * 6.0;
            sh.tgpos.z += sh.alpha.sin() * 256.0 * 6.0;
            sh.tgpos.y = a.land_up_h(sh.tgpos.x, sh.tgpos.z) + sh.delta_y;
            sh.tgpos.y = sh.tgpos.y.max(a.land_up_h(sh.pos.x, sh.pos.z) + sh.delta_y);
        }

        if sh.state == 3 {
            sh.ftime += tdt;
            if sh.ftime >= am {
                sh.ftime = am - 1;
                sh.state = 2;
                let at = sh.pos;
                self.ship_fx(4, None);
                self.ship_fx(1, Some(at));
            }
            return;
        }

        if sh.state != 0 {
            let hold = sh.pos.y - 650.0 - (sh.delta_y - 2048.0);
            if let Some(ci) = sh.cindex {
                if let Some(c) = self.chars.get_mut(ci) {
                    delta_func(&mut c.pos.y, hold, tdt as f32 / 3.0);
                    delta_func(&mut c.beta, 0.0, dt / 4048.0);
                    delta_func(&mut c.gamma, 0.0, dt / 4048.0);
                }
            }
            let sh = &mut self.ship;
            if sh.state == 2 {
                sh.ftime = (sh.ftime - tdt).max(0);
                let up = sh
                    .cindex
                    .and_then(|ci| self.chars.get(ci))
                    .map(|c| (c.pos.y - hold).abs() < 1.0)
                    .unwrap_or(true);
                if sh.ftime == 0 && up {
                    sh.state = 1;
                    let at = sh.pos;
                    self.ship_fx(5, None);
                    self.ship_fx(2, Some(at));
                }
                return;
            }
        }

        let sh = &mut self.ship;
        let mut vspeed = (1.0 + l / 128.0).min(24.0);
        if sh.state != 0 {
            vspeed = 24.0;
        }
        if dalpha.abs() > 0.4 {
            vspeed = 0.0;
        }
        let was = sh.speed;
        if vspeed > sh.speed {
            delta_func(&mut sh.speed, vspeed, dt / 200.0);
        } else {
            sh.speed = vspeed;
        }
        if sh.speed > 0.0 && was == 0.0 {
            let at = sh.pos;
            self.ship_fx(2, Some(at));
        }

        let sh = &mut self.ship;
        let mut step = dt * sh.speed / 16.0;
        if dalpha.abs() < 0.4 {
            if step < l {
                if step > l2 {
                    step = l2 * 0.5;
                }
                if l2 < 0.1 {
                    step = 0.0;
                }
                sh.pos.x += sh.alpha.cos() * step;
                sh.pos.z += sh.alpha.sin() * step;
            } else if sh.state != 0 {
                sh.state = -1;
                if let Some(ci) = sh.cindex {
                    if let Some(c) = self.chars.get_mut(ci) {
                        c.removed = true;
                    }
                }
                return;
            } else {
                sh.pos = sh.tgpos;
                sh.state = 3;
                sh.ftime = 1;
                sh.tgpos = sh.retpos;
                sh.tgpos.y = a.land_up_h(sh.tgpos.x, sh.tgpos.z) + sh.delta_y;
                sh.tgpos.y = sh.tgpos.y.max(a.land_up_h(sh.pos.x, sh.pos.z) + sh.delta_y);
                let at = sh.pos;
                if let Some(ci) = sh.cindex {
                    if let Some(c) = self.chars.get_mut(ci) {
                        c.frozen = true;
                    }
                }
                self.ship_fx(1, Some(at));
            }
        }

        let sh = &mut self.ship;
        let h = a.land_up_h(sh.pos.x, sh.pos.z);
        delta_func(&mut sh.pos.y, sh.tgpos.y, dt / 4.0);
        if sh.pos.y < h + 1024.0 {
            if sh.state != 0 {
                if let Some(c) = sh.cindex.and_then(|ci| self.chars.get_mut(ci)) {
                    c.pos.y += h + 1024.0 - sh.pos.y;
                }
            }
            sh.pos.y = h + 1024.0;
        }

        let mut currspeed = if sh.tgalpha > sh.alpha {
            0.1 + drspd.abs() / 2.0
        } else {
            -0.1 - drspd.abs() / 2.0
        };
        if dalpha.abs() > PI {
            currspeed = -currspeed;
        }
        delta_func(&mut sh.rspeed, currspeed, dt / 420.0);
        let rspd = sh.rspeed * dt / 1024.0;
        if drspd.abs() < rspd.abs() {
            sh.alpha = sh.tgalpha;
            sh.rspeed /= 2.0;
        } else {
            sh.alpha += rspd;
            if sh.state != 0 {
                if let Some(c) = sh.cindex.and_then(|ci| self.chars.get_mut(ci)) {
                    c.alpha += rspd;
                }
            }
        }
        if sh.alpha < 0.0 {
            sh.alpha += PI * 2.0;
        }
        if sh.alpha > PI * 2.0 {
            sh.alpha -= PI * 2.0;
        }

        if sh.state != 0 {
            if let Some(c) = sh.cindex.and_then(|ci| self.chars.get_mut(ci)) {
                c.pos.x = sh.pos.x;
                c.pos.z = sh.pos.z;
            }
            if l > 1000.0 {
                sh.tgpos.y += dt / 12.0;
            }
        } else if let Some(c) = sh.cindex.and_then(|ci| self.chars.get(ci)) {
            sh.tgpos.x = c.pos.x;
            sh.tgpos.z = c.pos.z;
            sh.tgpos.y = a.land_up_h(sh.tgpos.x, sh.tgpos.z) + sh.delta_y;
            sh.tgpos.y = sh.tgpos.y.max(a.land_up_h(sh.pos.x, sh.pos.z) + sh.delta_y);
        }
    }
}

/// The local time as the trophy records keep it.
fn local_time() -> (i32, i32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Days since 1970 to a civil date (Howard Hinnant's algorithm), in UTC:
    // the record is only ever shown, never compared.
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, min) = (sod / 3600, (sod / 60) % 60);
    (
        ((h << 10) + min) as i32,
        (((y << 20) + (m << 10) + d) as i32),
    )
}

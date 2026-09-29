//! A loaded hunting area: the resource and map files, prepared the way the
//! original loader left them, and the height and
//! collision queries the game code asks of the ground.
//!
//! Everything here is in the games' own units: a map cell is 256 units
//! across, x runs along a map row and z down the rows, y is up.

use bevy::prelude::Resource;
use carn_formats::map::{Map, C2_FM_WATER, C2_FM_WATER2};
use carn_formats::model::{Model, SF_DOUBLE_SIDE, SF_OPACITY};
use carn_formats::rsc::{self, LoadOptions, Rsc};
use carn_formats::Engine;

use crate::game::GameKind;
use crate::paths::DataRoot;

#[derive(Clone, Copy, Debug, Default)]
pub struct Bound {
    pub cx: f32,
    pub cy: f32,
    /// Half extents; a < 0 marks an unused slot.
    pub a: f32,
    pub b: f32,
    pub y1: f32,
    pub y2: f32,
}

/// Per object type: what the loader worked out beyond the file.
#[derive(Clone, Debug)]
pub struct ObjectExtra {
    /// Vertex light for each of the four quarter-turns (Carnivores uses [0]).
    pub vlight: [Vec<f32>; 4],
    /// Collision boxes for the objects that have them (Carnivores 2).
    pub bounds: [Bound; 8],
}

#[derive(Resource)]
pub struct Area {
    pub kind: GameKind,
    pub engine: Engine,
    /// The project name the area was loaded by, e.g. "HUNTDAT/AREAS/AREA1".
    pub name: String,
    pub rsc: Rsc,
    pub map: Map,
    pub size: i32,
    /// Units per height-map step.
    pub hs: f32,
    pub opt: LoadOptions,
    pub extra: Vec<ObjectExtra>,
    pub landings: Vec<(i32, i32)>,
    /// The engine's 32x32 table of random values (0..1023), used to vary
    /// object light and water motion from cell to cell.
    pub random: Vec<i32>,
    pub trophy: bool,
    /// Sun direction for the object lighting.
    pub sun: [f32; 3],
}

/// Deterministic stand-in for the C runtime's rand(), which the tables were
/// filled from; the exact values never mattered, only that they vary.
pub struct Lcg(pub u32);
impl Lcg {
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(214013).wrapping_add(2531011);
        (self.0 >> 16) & 0x7FFF
    }
}

impl Area {
    pub fn load(
        root: &DataRoot,
        kind: GameKind,
        project: &str,
        opt: LoadOptions,
    ) -> Result<Area, String> {
        let engine = kind.engine();
        let rsc_path = root.find_or_err(&format!("{project}.rsc"))?;
        let map_path = root.find_or_err(&format!("{project}.map"))?;
        let rsc = Rsc::load(&rsc_path, engine, opt).map_err(|e| e.to_string())?;
        let map = Map::load(&map_path, engine, opt.day_night).map_err(|e| e.to_string())?;
        let trophy = project.to_ascii_lowercase().contains("trophy");

        let sun = match engine {
            Engine::C1 => [1000.0, 300.0, -1000.0],
            Engine::C2 => match opt.day_night {
                0 => [-4048.0, 2048.0, -4048.0],
                2 => [3048.0, 3048.0, 3048.0],
                _ => [-2048.0, 4048.0, -2048.0],
            },
        };

        let mut lcg = Lcg(12345);
        let random = (0..32 * 32)
            .map(|_| (lcg.next() as i32 * 1024) / 32768)
            .collect();

        let size = map.size as i32;
        let mut area = Area {
            kind,
            engine,
            name: project.to_string(),
            rsc,
            map,
            size,
            hs: engine.height_scale(),
            opt,
            extra: Vec::new(),
            landings: Vec::new(),
            random,
            trophy,
            sun,
        };
        area.extra = area
            .rsc
            .objects
            .iter()
            .map(|o| area.object_extra(o))
            .collect();
        area.fix_fogs();
        area.create_tmap();
        area.render_light_map();
        Ok(area)
    }

    // ------------------------------------------------------------------
    // Load-time preparation
    // ------------------------------------------------------------------

    fn object_extra(&self, o: &rsc::RscObject) -> ObjectExtra {
        let mut vlight: [Vec<f32>; 4] = Default::default();
        if o.info.flags & rsc::OF_NO_LIGHT != 0 {
            for v in vlight.iter_mut() {
                *v = vec![0.0; o.model.vertices.len()];
            }
        } else {
            vlight = calc_lights(&o.model, self.engine, self.sun);
        }
        let mut bounds = [Bound {
            a: -1.0,
            ..Default::default()
        }; 8];
        if self.engine == Engine::C2 && o.info.flags & rsc::OF_BOUND != 0 {
            bounds = calc_bound_box(&o.model);
        }
        ObjectExtra { vlight, bounds }
    }

    /// A fog whose top is above ground level also covers the low ground
    /// around the zones marked with it.
    fn fix_fogs(&mut self) {
        if self.rsc.fogs.len() < 2 || self.rsc.fogs[1].y_begin <= 1.0 {
            return;
        }
        let yb = self.rsc.fogs[1].y_begin;
        let hs = (self.size / 2) as usize;
        let n = self.size as usize;
        for y in 0..hs - 2 {
            for x in 0..hs - 2 {
                if self.map.fogsmap[y * hs + x] != 0 {
                    continue;
                }
                let mut low = false;
                for dy in 0..3 {
                    for dx in 0..3 {
                        let h =
                            self.map.hmap[(y * 2 + dy).min(n - 1) * n + (x * 2 + dx).min(n - 1)];
                        if (h as f32) < yb {
                            low = true;
                        }
                    }
                }
                if low {
                    self.map.fogsmap[y * hs + x] = 1;
                }
            }
        }
    }

    fn create_tmap(&mut self) {
        let n = self.size as usize;
        let m = &mut self.map;
        match self.engine {
            Engine::C1 => {
                for t in m.tmap1.iter_mut().chain(m.tmap2.iter_mut()) {
                    if *t == 255 {
                        *t = 1;
                    }
                }
            }
            Engine::C2 => {
                for t in m.tmap1.iter_mut().chain(m.tmap2.iter_mut()) {
                    if *t == 0xFFFF {
                        *t = 1;
                    }
                }
                // The shore: dry cells next to water join its body, so the
                // surface reaches the bank, and are nudged up a step where
                // they sit exactly at the water line.
                for y in 1..n - 1 {
                    for x in 1..n - 1 {
                        let i = y * n + x;
                        if m.fmap[i] & C2_FM_WATER != 0 {
                            continue;
                        }
                        let nb = [
                            (x + 1, y),
                            (x, y + 1),
                            (x - 1, y),
                            (x, y - 1),
                            (x - 1, y - 1),
                            (x + 1, y - 1),
                            (x - 1, y + 1),
                            (x + 1, y + 1),
                        ];
                        for (nx, ny) in nb {
                            let j = ny * n + nx;
                            if m.fmap[j] & C2_FM_WATER != 0 {
                                m.fmap[i] |= C2_FM_WATER2;
                                m.wmap[i] = m.wmap[j];
                            }
                        }
                        if m.fmap[i] & C2_FM_WATER2 != 0 {
                            let w = m.wmap[i] as usize;
                            if let Some(we) = self.rsc.waters.get(w) {
                                if m.hmap[i] as i32 == we.level {
                                    m.hmap[i] = m.hmap[i].saturating_add(1);
                                }
                            }
                        }
                    }
                }
                for i in 0..n * n {
                    if !m.water(i) {
                        m.wmap[i] = 255;
                    }
                }
            }
        }

        // Landing points, and where each object stands.
        let mut landings = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                if self.map.omap[i] == 254 {
                    landings.push((x as i32, y as i32));
                    self.map.omap[i] = 255;
                }
                let ob = self.map.omap[i];
                if ob == 255 || ob as usize >= self.rsc.objects.len() {
                    if ob != 255 {
                        self.map.omap[i] = 255;
                    }
                    self.map.hmapo[i] = if self.engine == Engine::C1 { 48 } else { 0 };
                    continue;
                }
                let info = &self.rsc.objects[ob as usize].info;
                let (flags, gr) = (info.flags, info.gr_rad);
                match self.engine {
                    Engine::C1 => {
                        if flags & rsc::OF_PLACE_USER != 0 {
                            self.map.hmapo[i] = self.map.hmapo[i].saturating_add(48);
                        }
                        if flags & rsc::OF_PLACE_GROUND != 0 {
                            self.map.hmapo[i] = self.object_h(x as i32, y as i32, gr);
                        }
                        if flags & rsc::OF_PLACE_WATER != 0 {
                            self.map.hmapo[i] = self.object_h_water(x as i32, y as i32);
                        }
                    }
                    Engine::C2 => {
                        if flags & rsc::OF_PLACE_GROUND != 0 {
                            self.map.hmapo[i] = self.object_h(x as i32, y as i32, gr);
                        }
                    }
                }
            }
        }
        if landings.is_empty() {
            landings.push((self.size / 2, self.size / 2));
        }
        if self.trophy {
            landings.clear();
            for x in 0..6 {
                landings.push((69 + x * 3, 66));
            }
            for y in 0..6 {
                landings.push((87, 69 + y * 3));
            }
            for x in 0..6 {
                landings.push((84 - x * 3, 87));
            }
            for y in 0..6 {
                landings.push((66, 84 - y * 3));
            }
        }
        self.landings = landings;
    }

    fn object_h(&self, x: i32, y: i32, r: i32) -> u8 {
        let px = (x * 256 + 128) as f32;
        let py = (y * 256 + 128) as f32;
        let r = r as f32;
        let mut hr = self.land_h(px, py);
        for (dx, dy) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
            hr = hr.min(self.land_h(px + dx, py + dy));
        }
        hr += 15.0;
        let steps = (hr / self.hs) as i32 + if self.engine == Engine::C1 { 48 } else { 0 };
        steps.clamp(0, 255) as u8
    }

    fn object_h_water(&self, x: i32, y: i32) -> u8 {
        let m = &self.map;
        let i = m.cidx(x, y);
        let h = if m.reverse(i) {
            (m.hmap[m.cidx(x + 1, y)] as i32 + m.hmap[m.cidx(x, y + 1)] as i32) / 2
        } else {
            (m.hmap[i] as i32 + m.hmap[m.cidx(x + 1, y + 1)] as i32) / 2
        };
        (h + 48).clamp(0, 255) as u8
    }

    /// Objects darken the ground under and beside them.
    fn render_light_map(&mut self) {
        let n = self.size;
        let dn = self.opt.day_night;
        for y in 1..n - 1 {
            for x in 1..n - 1 {
                let ob = self.map.omap[(y * n + x) as usize];
                if ob == 255 {
                    continue;
                }
                let info = self.rsc.objects[ob as usize].info.clone();
                match self.engine {
                    Engine::C2 => {
                        let s = if dn == 2 { -1 } else { 1 };
                        let mut l = info.line_length / 128;
                        if dn != 1 {
                            l = info.line_length / 70;
                        }
                        if l > 0 {
                            self.shadow_circle(
                                x * 256 + 128,
                                y * 256 + 128,
                                256,
                                info.l_intensity * 2,
                            );
                        }
                        for i in 1..l {
                            self.add_shadow(x + i * s, y + i * s, info.l_intensity);
                        }
                        let l = info.line_length * 2;
                        self.shadow_circle(
                            x * 256 + 128 + l * s,
                            y * 256 + 128 + l * s,
                            info.circle_rad * 2,
                            info.c_intensity * 4,
                        );
                    }
                    Engine::C1 => {
                        let l = info.line_length / 128;
                        if l > 0 {
                            self.shadow_circle(
                                x * 256 + 128,
                                y * 256 + 128,
                                256,
                                info.l_intensity / 2,
                            );
                        }
                        for i in 1..l {
                            self.add_shadow(x + i, y + i, info.l_intensity);
                        }
                        let l = info.line_length * 2;
                        self.shadow_circle(
                            x * 256 + 128 + l,
                            y * 256 + 128 + l,
                            info.circle_rad * 2,
                            info.c_intensity,
                        );
                    }
                }
            }
        }
    }

    fn add_shadow(&mut self, x: i32, y: i32, d: i32) {
        if x < 0 || y < 0 || x >= self.size || y >= self.size {
            return;
        }
        let i = (y * self.size + x) as usize;
        let l = self.map.lmap[i] as i32;
        self.map.lmap[i] = match self.engine {
            // Carnivores stores darkness: shadow adds to it.
            Engine::C1 => (l + d).min(56),
            Engine::C2 => (l - d).max(32),
        } as u8;
    }

    fn shadow_circle(&mut self, x: i32, y: i32, r: i32, d: i32) {
        if r <= 0 {
            return;
        }
        let (cx, cy) = (x / 256, y / 256);
        let cr = 1 + r / 256;
        for yy in -cr..=cr {
            for xx in -cr..=cr {
                let tx = (cx + xx) * 256;
                let ty = (cy + yy) * 256;
                let dist = (((tx - x) * (tx - x) + (ty - y) * (ty - y)) as f32).sqrt() as i32;
                if dist > r {
                    continue;
                }
                self.add_shadow(cx + xx, cy + yy, d * (r - dist) / r);
            }
        }
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        self.map.cidx(x, y)
    }

    /// Ground height in steps at a map vertex: Carnivores keeps it in the
    /// second height map, offset by 48.
    #[inline]
    fn ground_steps(&self, x: i32, y: i32) -> i32 {
        let i = self.idx(x, y);
        match self.engine {
            Engine::C1 => self.map.hmap2[i] as i32 - 48,
            Engine::C2 => self.map.hmap[i] as i32,
        }
    }

    /// The height the terrain mesh is drawn at, in steps.
    #[inline]
    pub fn surface_steps(&self, x: i32, y: i32) -> i32 {
        self.map.hmap[self.idx(x, y)] as i32
    }

    fn interp(&self, x: f32, y: f32, h: impl Fn(i32, i32) -> i32) -> f32 {
        let (xi, yi) = (x.max(0.0) as i32, y.max(0.0) as i32);
        let (cx, cy) = (xi / 256, yi / 256);
        let (dx, dy) = (xi % 256, yi % 256);
        let h1 = h(cx, cy);
        let mut h2 = h(cx + 1, cy);
        let mut h3 = h(cx + 1, cy + 1);
        let mut h4 = h(cx, cy + 1);
        let mut h1m = h1;
        if self.map.reverse(self.idx(cx, cy)) {
            if 256 - dx > dy {
                h3 = h2 + h4 - h1;
            } else {
                h1m = h2 + h4 - h3;
            }
        } else if dx > dy {
            h4 = h1 + h3 - h2;
        } else {
            h2 = h1 + h3 - h4;
        }
        ((h1m * (256 - dx) + h2 * dx) * (256 - dy) + (h4 * (256 - dx) + h3 * dx) * dy) as f32
            / 65536.0
    }

    /// Height of the ground.
    pub fn land_h(&self, x: f32, z: f32) -> f32 {
        self.interp(x, z, |a, b| self.ground_steps(a, b)) * self.hs
    }

    /// Height of the drawn surface: the water's top where there is water.
    pub fn land_up_h(&self, x: f32, z: f32) -> f32 {
        match self.engine {
            Engine::C1 => self.interp(x, z, |a, b| self.surface_steps(a, b)) * self.hs,
            Engine::C2 => {
                let i = self.idx(x as i32 / 256, z as i32 / 256);
                if !self.map.water(i) {
                    return self.land_h(x, z);
                }
                let w = self.map.wmap[i] as usize;
                self.rsc
                    .waters
                    .get(w)
                    .map(|w| w.level as f32 * self.hs)
                    .unwrap_or_else(|| self.land_h(x, z))
            }
        }
    }

    /// Where the object on a cell stands.
    pub fn land_oh(&self, cx: i32, cy: i32) -> f32 {
        let h = self.map.hmapo[self.idx(cx, cy)] as f32;
        match self.engine {
            Engine::C1 => (h - 48.0) * self.hs,
            Engine::C2 => h * self.hs,
        }
    }

    /// Highest ground in a ring around a point: what the hunter's feet rest
    /// on, not counting objects.
    pub fn land_qh_no_obj(&self, x: f32, z: f32) -> f32 {
        let mut h = self.land_h(x, z);
        for (dx, dz) in [
            (-90.0, -90.0),
            (90.0, -90.0),
            (-90.0, 90.0),
            (90.0, 90.0),
            (128.0, 0.0),
            (-128.0, 0.0),
            (0.0, 128.0),
            (0.0, -128.0),
        ] {
            h = h.max(self.land_h(x + dx, z + dz));
        }
        h
    }

    fn object_at(&self, cx: i32, cz: i32) -> Option<usize> {
        let ob = self.map.omap[self.idx(cx, cz)];
        (ob != 255).then_some(ob as usize)
    }

    /// The ground including the tops of objects the hunter can stand on.
    pub fn land_qh(&self, x: f32, z: f32, player_y: f32) -> f32 {
        let mut h = self.land_qh_no_obj(x, z);
        let (ccx, ccz) = (x as i32 / 256, z as i32 / 256);
        let r = if self.engine == Engine::C1 { 2 } else { 4 };
        for dz in -r..=r {
            for dx in -r..=r {
                let Some(ob) = self.object_at(ccx + dx, ccz + dz) else {
                    continue;
                };
                let info = &self.rsc.objects[ob].info;
                let cr = info.radius as f32 - 1.0;
                let oz = (ccz + dz) as f32 * 256.0 + 128.0;
                let ox = (ccx + dx) as f32 * 256.0 + 128.0;
                let land_y = self.land_oh(ccx + dx, ccz + dz);
                if self.engine == Engine::C1 {
                    if info.y_hi as f32 + land_y < h
                        || info.y_hi as f32 + land_y > player_y + 128.0
                        || info.y_lo as f32 + land_y > player_y + 256.0
                    {
                        continue;
                    }
                    let d = ((ox - x).powi(2) + (oz - z).powi(2)).sqrt();
                    if d < cr {
                        h = info.y_hi as f32 + land_y;
                    }
                    continue;
                }
                if info.flags & rsc::OF_BOUND != 0 {
                    let turn = self.map.object_turn(self.idx(ccx + dx, ccz + dz));
                    if let Some(hh) =
                        point_on_bound(x, z, ox, oz, land_y, &self.extra[ob].bounds, turn, player_y)
                    {
                        if h < land_y + hh {
                            h = land_y + hh;
                        }
                    }
                    continue;
                }
                if info.y_hi as f32 + land_y < h || info.y_hi as f32 + land_y > player_y + 128.0 {
                    continue;
                }
                let d = if info.flags & rsc::OF_CIRCLE != 0 {
                    ((ox - x).powi(2) + (oz - z).powi(2)).sqrt()
                } else {
                    (ox - x).abs().max((oz - z).abs())
                };
                if d < cr {
                    h = info.y_hi as f32 + land_y;
                }
            }
        }
        h
    }

    /// The lowest overhang above the hunter; Carnivores has
    /// no overhangs.
    pub fn land_ceil_h(&self, x: f32, z: f32, player_y: f32) -> f32 {
        let mut h = self.land_h(x, z) + 20480.0;
        if self.engine == Engine::C1 {
            return h;
        }
        let (ccx, ccz) = (x as i32 / 256, z as i32 / 256);
        for dz in -4..=4 {
            for dx in -4..=4 {
                let Some(ob) = self.object_at(ccx + dx, ccz + dz) else {
                    continue;
                };
                let info = &self.rsc.objects[ob].info;
                let cr = info.radius as f32 - 1.0;
                let oz = (ccz + dz) as f32 * 256.0 + 128.0;
                let ox = (ccx + dx) as f32 * 256.0 + 128.0;
                let land_y = self.land_oh(ccx + dx, ccz + dz);
                if info.flags & rsc::OF_BOUND != 0 {
                    let turn = self.map.object_turn(self.idx(ccx + dx, ccz + dz));
                    if let Some(hh) = point_under_bound(
                        x,
                        z,
                        ox,
                        oz,
                        land_y,
                        &self.extra[ob].bounds,
                        turn,
                        player_y,
                    ) {
                        if h > land_y + hh {
                            h = land_y + hh;
                        }
                    }
                    continue;
                }
                if info.y_lo as f32 + land_y > h || (info.y_lo as f32 + land_y) < player_y + 100.0 {
                    continue;
                }
                let d = if info.flags & rsc::OF_CIRCLE != 0 {
                    ((ox - x).powi(2) + (oz - z).powi(2)).sqrt()
                } else {
                    (ox - x).abs().max((oz - z).abs())
                };
                if d < cr {
                    h = info.y_lo as f32 + land_y;
                }
            }
        }
        h
    }

    /// Pushes a point at the hunter's height out of the objects around it
    /// and keeps it inside the playable part of the map.
    pub fn check_collision(&self, cx: &mut f32, cz: &mut f32, player_y: f32) {
        let lo = 36.0 * 256.0;
        let hi = (self.size - 44) as f32 * 256.0;
        *cx = cx.clamp(lo, hi);
        *cz = cz.clamp(lo, hi);
        let (ccx, ccz) = (*cx as i32 / 256, *cz as i32 / 256);
        let r = if self.engine == Engine::C1 { 2 } else { 4 };
        for dz in -r..=r {
            for dx in -r..=r {
                let Some(ob) = self.object_at(ccx + dx, ccz + dz) else {
                    continue;
                };
                let info = &self.rsc.objects[ob].info;
                let cr = info.radius as f32;
                let oz = (ccz + dz) as f32 * 256.0 + 128.0;
                let ox = (ccx + dx) as f32 * 256.0 + 128.0;
                let land_y = self.land_oh(ccx + dx, ccz + dz);
                let bound = self.engine == Engine::C2 && info.flags & rsc::OF_BOUND != 0;
                if !bound
                    && (info.y_hi as f32 + land_y < player_y + 128.0
                        || info.y_lo as f32 + land_y > player_y + 256.0)
                {
                    continue;
                }
                if bound {
                    let turn = self.map.object_turn(self.idx(ccx + dx, ccz + dz));
                    check_bound_collision(
                        cx,
                        cz,
                        ox,
                        oz,
                        land_y,
                        &self.extra[ob].bounds,
                        turn,
                        player_y,
                    );
                } else if self.engine == Engine::C1 || info.flags & rsc::OF_CIRCLE != 0 {
                    let d = ((ox - *cx).powi(2) + (oz - *cz).powi(2)).sqrt();
                    if d < cr && d > 0.0 {
                        *cx -= (ox - *cx) * (cr - d) / d;
                        *cz -= (oz - *cz) * (cr - d) / d;
                    }
                } else {
                    let d = (ox - *cx).abs().max((oz - *cz).abs());
                    if d < cr && d > 0.0 {
                        if (ox - *cx).abs() > (oz - *cz).abs() {
                            *cx -= (ox - *cx) * (cr - d) / d;
                        } else {
                            *cz -= (oz - *cz) * (cr - d) / d;
                        }
                    }
                }
            }
        }
    }

    /// Light (0..255, brighter is higher) of the ground at a point, for
    /// Carnivores 2's ground-lit objects.
    pub fn land_light(&self, x: f32, z: f32) -> f32 {
        self.interp_plain(x, z, |a, b| self.map.lmap[self.idx(a, b)] as i32)
    }

    fn interp_plain(&self, x: f32, y: f32, h: impl Fn(i32, i32) -> i32) -> f32 {
        let (xi, yi) = (x.max(0.0) as i32, y.max(0.0) as i32);
        let (cx, cy) = (xi / 256, yi / 256);
        let (dx, dy) = (xi % 256, yi % 256);
        let (h1, h2, h3, h4) = (h(cx, cy), h(cx + 1, cy), h(cx + 1, cy + 1), h(cx, cy + 1));
        ((h1 * (256 - dx) + h2 * dx) * (256 - dy) + (h4 * (256 - dx) + h3 * dx) * dy) as f32
            / 65536.0
    }

    /// The fog zone the camera's cell is in.
    pub fn fog_zone(&self, x: f32, z: f32) -> usize {
        self.map.fog_at(x as i32 / 256, z as i32 / 256) as usize
    }

    pub fn ambient_zone(&self, x: f32, z: f32) -> usize {
        self.map.amb_at(x as i32 / 256, z as i32 / 256) as usize
    }

    /// Light an object on a cell gets, before its per-vertex light:
    /// Carnivores 2's mlight (64..192, brightness) or Carnivores' 9..42
    /// darkness turned into the same brightness scale.
    pub fn object_light(&self, x: i32, y: i32) -> f32 {
        let i = self.idx(x, y);
        let rnd = self.random[((y & 31) * 32 + (x & 31)) as usize];
        let ob = self.map.omap[i];
        match self.engine {
            Engine::C2 => {
                let info = &self.rsc.objects[ob as usize].info;
                let l = if info.flags & rsc::OF_DEF_LIGHT != 0 {
                    info.def_light
                } else if info.flags & rsc::OF_GRND_LIGHT != 0 {
                    128
                } else {
                    -(rnd >> 5) + (self.map.lmap[i] as i32 >> 1) + 96
                };
                l.clamp(64, 192) as f32
            }
            Engine::C1 => {
                let rm = rnd >> 7;
                let l0 = self.map.lmap[i] as i32;
                let l3 = self.map.lmap[self.idx(x + 1, y + 1)] as i32;
                let ml = (rm + ((l0 + l3) >> 2)).clamp(9, 42);
                (255 - ml * 4) as f32
            }
        }
    }
}

/// Per-vertex light from the sun for a model, in the same
/// 0..255 scale the vertex colours use, added to the object's light.
pub fn calc_lights(model: &Model, engine: Engine, sun: [f32; 3]) -> [Vec<f32>; 4] {
    let vc = model.vertices.len();
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 {
            [v[0] / l, v[1] / l, v[2] / l]
        } else {
            v
        }
    };
    let slight = match engine {
        Engine::C2 => norm([-sun[0], -sun[1] / 2.0, -sun[2]]),
        Engine::C1 => norm([-1000.0, -300.0, 1000.0]),
    };
    let fnorm: Vec<[f32; 3]> = model
        .faces
        .iter()
        .map(|f| {
            let a = model.raw_pos(f.v[0] as usize);
            let b = model.raw_pos(f.v[1] as usize);
            let c = model.raw_pos(f.v[2] as usize);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            norm([
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ])
        })
        .collect();
    // Summed normal per vertex over the faces that count for lighting.
    let skip = match engine {
        Engine::C2 => SF_DOUBLE_SIDE,
        Engine::C1 => SF_OPACITY,
    };
    let mut vn = vec![[0f32; 3]; vc];
    let mut used = vec![0u32; vc];
    for (f, n) in model.faces.iter().zip(&fnorm) {
        if f.flags & skip != 0 {
            continue;
        }
        for &v in &f.v {
            let v = v as usize;
            vn[v][0] += n[0];
            vn[v][1] += n[1];
            vn[v][2] += n[2];
            used[v] += 1;
        }
    }
    let mut out: [Vec<f32>; 4] = Default::default();
    for (vt, o) in out.iter_mut().enumerate() {
        let (sa, ca) = ((vt as f32) * std::f32::consts::FRAC_PI_2).sin_cos();
        *o = (0..vc)
            .map(|v| {
                if used[v] == 0 {
                    return 0.0;
                }
                let n = norm(vn[v]);
                match engine {
                    Engine::C2 => {
                        let rv = [n[0] * sa + n[2] * ca, n[1], n[2] * sa - n[0] * ca];
                        (rv[0] * slight[0] + rv[1] * slight[1] + rv[2] * slight[2]) * 64.0
                    }
                    Engine::C1 => {
                        let c = n[0] * slight[0] + n[1] * slight[1] + n[2] * slight[2];
                        ((c - 0.40) * 60.0).trunc()
                    }
                }
            })
            .collect();
    }
    out
}

/// Collision boxes around each bone of a model.
pub fn calc_bound_box(model: &Model) -> [Bound; 8] {
    let mut out = [Bound {
        a: -1.0,
        ..Default::default()
    }; 8];
    for (o, b) in out.iter_mut().enumerate() {
        let mut first = true;
        let (mut x1, mut x2, mut y1, mut y2, mut z1, mut z2) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
        for v in &model.vertices {
            if v.hide != 0 || v.owner as usize != o {
                continue;
            }
            let p = v.pos;
            if first {
                x1 = p[0] - 1.0;
                x2 = p[0] + 1.0;
                y1 = p[1] - 1.0;
                y2 = p[1] + 1.0;
                z1 = p[2] - 1.0;
                z2 = p[2] + 1.0;
                first = false;
            }
            x1 = x1.min(p[0]);
            x2 = x2.max(p[0]);
            y1 = y1.min(p[1]);
            y2 = y2.max(p[1]);
            z1 = z1.min(p[2]);
            z2 = z2.max(p[2]);
        }
        if first {
            continue;
        }
        x1 -= 72.0;
        x2 += 72.0;
        z1 -= 72.0;
        z2 += 72.0;
        *b = Bound {
            cx: (x1 + x2) / 2.0,
            cy: (z1 + z2) / 2.0,
            a: (x2 - x1) / 2.0,
            b: (z2 - z1) / 2.0,
            y1,
            y2,
        };
    }
    out
}

fn turned(b: &Bound, turn: u32) -> (f32, f32, f32, f32) {
    let (sa, ca) = (turn as f32 * std::f32::consts::FRAC_PI_2).sin_cos();
    let ccx = b.cx * ca + b.cy * sa;
    let ccy = b.cy * ca - b.cx * sa;
    let (a, bb) = if turn & 1 == 1 {
        (b.b, b.a)
    } else {
        (b.a, b.b)
    };
    (ccx, ccy, a, bb)
}

#[allow(clippy::too_many_arguments)]
fn point_on_bound(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    oy: f32,
    bounds: &[Bound; 8],
    turn: u32,
    player_y: f32,
) -> Option<f32> {
    let (px, py) = (px - cx, py - cy);
    let mut h = None;
    for b in bounds {
        if b.a < 0.0 || b.y2 + oy > player_y + 128.0 {
            continue;
        }
        let (ccx, ccy, a, bb) = turned(b, turn);
        if (px - ccx).abs() < a && (py - ccy).abs() < bb {
            h = Some(h.map_or(b.y2, |v: f32| v.max(b.y2)));
        }
    }
    h
}

#[allow(clippy::too_many_arguments)]
fn point_under_bound(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    oy: f32,
    bounds: &[Bound; 8],
    turn: u32,
    player_y: f32,
) -> Option<f32> {
    let (px, py) = (px - cx, py - cy);
    let mut h = None;
    for b in bounds {
        if b.a < 0.0 || b.y1 + oy < player_y + 128.0 {
            continue;
        }
        let (ccx, ccy, a, bb) = turned(b, turn);
        if (px - ccx).abs() < a && (py - ccy).abs() < bb {
            h = Some(h.map_or(b.y1, |v: f32| v.min(b.y1)));
        }
    }
    h
}

#[allow(clippy::too_many_arguments)]
fn check_bound_collision(
    px: &mut f32,
    py: &mut f32,
    cx: f32,
    cy: f32,
    oy: f32,
    bounds: &[Bound; 8],
    turn: u32,
    player_y: f32,
) {
    let ppx = *px - cx;
    let ppy = *py - cy;
    for b in bounds {
        if b.a < 0.0 || b.y2 + oy < player_y + 128.0 || b.y1 + oy > player_y + 256.0 {
            continue;
        }
        let (ccx, ccy, a, bb) = turned(b, turn);
        let (w, h) = (a + 2.0, bb + 2.0);
        let dw = (ppx - ccx).abs() - w;
        let dh = (ppy - ccy).abs() - h;
        if dw > 0.0 || dh > 0.0 {
            continue;
        }
        if dw > dh {
            *px = cx + ccx + w * (ppx - ccx).signum();
        } else {
            *py = cy + ccy + h * (ppy - ccy).signum();
        }
    }
}

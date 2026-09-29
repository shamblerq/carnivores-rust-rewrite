//! An area's map (.MAP): per-cell layers over the whole square area.
//!
//! Carnivores stores 512x512 cells with one-byte layers; Carnivores 2 and Ice
//! Age 1024x1024 with two-byte texture and flag layers, three light maps (one
//! per time of day) and a water-body index. Both are read into the same
//! struct; the flag bits differ between the two and are read through the
//! helpers here rather than by value.

use crate::reader::{FormatError, Reader, Result};
use crate::Engine;

/// Carnivores 2 flag bits.
pub const C2_FM_WATER: u16 = 0x0080;
pub const C2_FM_WATER2: u16 = 0x8000;
pub const C2_FM_NOWAY: u16 = 0x0020;
pub const C2_FM_REVERSE: u16 = 0x0010;
pub const C2_FM_WATER_A: u16 = 0x8080;

/// Carnivores flag bits.
pub const C1_FM_WATER: u16 = 0x80;
pub const C1_FM_REVERSE: u16 = 0x40;
pub const C1_FM_NOWAY: u16 = 0x20;

#[derive(Clone, Debug)]
pub struct Map {
    pub engine: Engine,
    pub size: usize,
    /// Ground height, in height-map steps. In Carnivores this is the visible
    /// surface, water included; `hmap2` is the ground under it.
    pub hmap: Vec<u8>,
    /// Texture of the cell's first triangle. In Carnivores the second
    /// triangle takes `tmap2`; Carnivores 2 uses `tmap2` for the coarse
    /// far-away ground.
    pub tmap1: Vec<u16>,
    pub tmap2: Vec<u16>,
    /// Object on the cell, 255 for none (254 marks a landing point).
    pub omap: Vec<u8>,
    pub fmap: Vec<u16>,
    /// Light, per vertex. Carnivores 2: brightness 0..255 for the chosen time
    /// of day. Carnivores: darkness 0..63.
    pub lmap: Vec<u8>,
    /// Carnivores 2: which water body the cell belongs to, 255 for none.
    pub wmap: Vec<u8>,
    /// Height the object on the cell stands at.
    pub hmapo: Vec<u8>,
    /// Carnivores: ground height + 48 (under water, the bottom).
    pub hmap2: Vec<u8>,
    /// Fog zone per 2x2 cells (index into the resource file's fogs).
    pub fogsmap: Vec<u8>,
    /// Ambient sound zone per 2x2 cells.
    pub ambmap: Vec<u8>,
}

impl Map {
    pub fn load(path: &std::path::Path, engine: Engine, day_night: i32) -> Result<Map> {
        let data =
            std::fs::read(path).map_err(|e| FormatError(format!("{}: {e}", path.display())))?;
        Self::parse(&data, engine, day_night)
            .map_err(|e| FormatError(format!("{}: {e}", path.display())))
    }

    pub fn parse(data: &[u8], engine: Engine, day_night: i32) -> Result<Map> {
        let n = engine.map_size();
        let nn = n * n;
        let h = nn / 4;
        let mut r = Reader::new(data);
        match engine {
            Engine::C1 => {
                let hmap = r.u8_vec(nn)?;
                let tmap1 = r.bytes(nn)?.iter().map(|&b| b as u16).collect();
                let tmap2 = r.bytes(nn)?.iter().map(|&b| b as u16).collect();
                let omap = r.u8_vec(nn)?;
                let fmap = r.bytes(nn)?.iter().map(|&b| b as u16).collect();
                let lmap = r.u8_vec(nn)?;
                let hmap2 = r.u8_vec(nn)?;
                let hmapo = r.u8_vec(nn)?;
                let fogsmap = r.u8_vec(h)?;
                let ambmap = r.u8_vec(h)?;
                Ok(Map {
                    engine,
                    size: n,
                    hmap,
                    tmap1,
                    tmap2,
                    omap,
                    fmap,
                    lmap,
                    wmap: vec![255; nn],
                    hmapo,
                    hmap2,
                    fogsmap,
                    ambmap,
                })
            }
            Engine::C2 => {
                let hmap = r.u8_vec(nn)?;
                let tmap1 = r.u16_vec(nn)?;
                let tmap2 = r.u16_vec(nn)?;
                let omap = r.u8_vec(nn)?;
                let fmap = r.u16_vec(nn)?;
                let dn = day_night.clamp(0, 2) as usize;
                r.skip(nn * dn)?;
                let lmap = r.u8_vec(nn)?;
                r.skip(nn * (2 - dn))?;
                let wmap = r.u8_vec(nn)?;
                let hmapo = r.u8_vec(nn)?;
                let fogsmap = r.u8_vec(h)?;
                let ambmap = r.u8_vec(h)?;
                Ok(Map {
                    engine,
                    size: n,
                    hmap,
                    tmap1,
                    tmap2,
                    omap,
                    fmap,
                    lmap,
                    wmap,
                    hmapo,
                    hmap2: Vec::new(),
                    fogsmap,
                    ambmap,
                })
            }
        }
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.size + x
    }

    /// Cell coordinates clamped into the map.
    #[inline]
    pub fn cidx(&self, x: i32, y: i32) -> usize {
        let m = self.size as i32 - 1;
        self.idx(x.clamp(0, m) as usize, y.clamp(0, m) as usize)
    }

    #[inline]
    pub fn reverse(&self, i: usize) -> bool {
        match self.engine {
            Engine::C1 => self.fmap[i] & C1_FM_REVERSE != 0,
            Engine::C2 => self.fmap[i] & C2_FM_REVERSE != 0,
        }
    }

    /// The cell is water (Carnivores 2: including the shore ring the loader
    /// adds).
    #[inline]
    pub fn water(&self, i: usize) -> bool {
        match self.engine {
            Engine::C1 => self.fmap[i] & C1_FM_WATER != 0,
            Engine::C2 => self.fmap[i] & C2_FM_WATER_A != 0,
        }
    }

    /// Water proper, not the shore ring.
    #[inline]
    pub fn deep_water(&self, i: usize) -> bool {
        match self.engine {
            Engine::C1 => self.fmap[i] & C1_FM_WATER != 0,
            Engine::C2 => self.fmap[i] & C2_FM_WATER != 0,
        }
    }

    #[inline]
    pub fn noway(&self, i: usize) -> bool {
        self.fmap[i] & C2_FM_NOWAY != 0
    }

    /// The quarter-turn the cell's texture is rotated by.
    #[inline]
    pub fn tdir(&self, i: usize) -> u32 {
        (self.fmap[i] & 3) as u32
    }

    /// The quarter-turn the object on the cell is rotated by (Carnivores 2).
    #[inline]
    pub fn object_turn(&self, i: usize) -> u32 {
        match self.engine {
            Engine::C1 => 0,
            Engine::C2 => ((self.fmap[i] >> 2) & 3) as u32,
        }
    }

    /// Fog zone of a cell.
    #[inline]
    pub fn fog_at(&self, x: i32, y: i32) -> u8 {
        let hs = (self.size / 2) as i32;
        let (fx, fy) = ((x >> 1).clamp(0, hs - 1), (y >> 1).clamp(0, hs - 1));
        self.fogsmap[(fy * hs + fx) as usize]
    }

    #[inline]
    pub fn amb_at(&self, x: i32, y: i32) -> u8 {
        let hs = (self.size / 2) as i32;
        let (fx, fy) = ((x >> 1).clamp(0, hs - 1), (y >> 1).clamp(0, hs - 1));
        self.ambmap[(fy * hs + fx) as usize]
    }
}

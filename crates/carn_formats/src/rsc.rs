//! An area's resource file (.RSC): its ground textures, the objects placed
//! on the map, the sky, fogs, sounds and (Carnivores 2) water bodies.

use crate::color;
use crate::model::{Animation, Model};
use crate::reader::{FormatError, Reader, Result};
use crate::Engine;

/// Object flags (the object record's flags word).
pub const OF_PLACE_WATER: i32 = 1;
pub const OF_PLACE_GROUND: i32 = 2;
pub const OF_PLACE_USER: i32 = 4;
pub const OF_CIRCLE: i32 = 8;
pub const OF_BOUND: i32 = 16;
pub const OF_NO_BMP: i32 = 32;
pub const OF_NO_LIGHT: i32 = 64;
pub const OF_DEF_LIGHT: i32 = 128;
pub const OF_GRND_LIGHT: i32 = 256;
pub const OF_NO_SOFT: i32 = 512;
pub const OF_NO_SOFT2: i32 = 1024;
pub const OF_ANIMATED: i32 = 0x8000_0000u32 as i32;

#[derive(Clone, Debug, Default)]
pub struct ObjInfo {
    /// Collision radius (already doubled, as the loader did).
    pub radius: i32,
    /// Height of the underside and the top above the object's base.
    pub y_lo: i32,
    pub y_hi: i32,
    /// The shadow the object casts into the light map.
    pub line_length: i32,
    pub l_intensity: i32,
    pub circle_rad: i32,
    pub c_intensity: i32,
    pub flags: i32,
    /// How far around the object the ground is sampled to seat it.
    pub gr_rad: i32,
    pub def_light: i32,
    /// Largest horizontal distance of a vertex from the centre.
    pub bound_r: f32,
}

impl ObjInfo {
    fn read(r: &mut Reader) -> Result<ObjInfo> {
        let radius = r.i32()? * 2;
        let y_lo = r.i32()? * 2;
        let y_hi = r.i32()? * 2;
        let line_length = (r.i32()? / 128) * 128;
        let l_intensity = r.i32()?;
        let circle_rad = r.i32()?;
        let c_intensity = r.i32()?;
        let flags = r.i32()?;
        let gr_rad = r.i32()?;
        let def_light = r.i32()?;
        let _last_ani_time = r.i32()?;
        let _bound_r = r.f32()?;
        r.skip(16)?;
        Ok(ObjInfo {
            radius,
            y_lo,
            y_hi,
            line_length,
            l_intensity,
            circle_rad,
            c_intensity,
            flags,
            gr_rad,
            def_light,
            bound_r: 0.0,
        })
    }
}

/// The flat stand-in an object is drawn as far away (Carnivores 2).
#[derive(Clone, Debug)]
pub struct BmpModel {
    /// 128x128, 5-5-5, texel 0 transparent.
    pub texture: Vec<u16>,
    /// Corners in model space: top-left, top-right, bottom-right, bottom-left.
    pub quad: [[f32; 3]; 4],
}

#[derive(Clone, Debug)]
pub struct RscObject {
    pub info: ObjInfo,
    pub model: Model,
    pub bmp: Option<BmpModel>,
    pub anim: Option<Animation>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FogEntity {
    /// 0x00RRGGBB
    pub rgb: u32,
    /// Top of the fog, in height-map steps.
    pub y_begin: f32,
    /// Breathing it hurts.
    pub mortal: bool,
    pub transp: f32,
    pub limit: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Trd {
    pub number: i32,
    pub volume: i32,
    /// Average seconds between plays.
    pub freq: i32,
    /// Reverb environment (Carnivores 2).
    pub envir: u16,
    /// Daytime-only sound (Carnivores 2): dropped at night.
    pub flags: u16,
}

#[derive(Clone, Debug, Default)]
pub struct Ambient {
    /// The looped background, 16-bit mono 22050 Hz.
    pub sound: Vec<i16>,
    /// All sixteen slots; the first `count` are in use, and slot 0 also
    /// carries the area's reverb and timing even when none are.
    pub rdata: Vec<Trd>,
    pub count: usize,
    pub volume: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WaterEntity {
    pub tindex: i32,
    /// Surface height, in height-map steps.
    pub level: i32,
    pub transp: f32,
    /// 0x00RRGGBB; replaced at load by the mean colour of its texture.
    pub fog_rgb: u32,
}

#[derive(Clone, Debug)]
pub struct Rsc {
    pub engine: Engine,
    /// 128x128 5-5-5 each. Index 0 is never 0 in a texel (the loader bumps
    /// 0 to 1) so ground textures are never keyed.
    pub textures: Vec<Vec<u16>>,
    /// Horizon (fade) colour for dawn, day and night. Carnivores has one.
    pub fade_rgb: [[i32; 3]; 3],
    pub trans_rgb: [[i32; 3]; 3],
    pub objects: Vec<RscObject>,
    /// 256x256 each; dawn, day and night for Carnivores 2, one for Carnivores.
    pub skies: Vec<Vec<u16>>,
    /// 128x128 cloud shadow map.
    pub skymap: Vec<u8>,
    /// Index 0 is "no fog"; the file's entries start at 1.
    pub fogs: Vec<FogEntity>,
    pub random_sounds: Vec<Vec<i16>>,
    pub ambients: Vec<Ambient>,
    pub waters: Vec<WaterEntity>,
}

fn pcm(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// Options that change what is loaded.
#[derive(Clone, Copy, Debug)]
pub struct LoadOptions {
    /// 0..255, 128 = as authored (Carnivores 2 only).
    pub brightness: i32,
    /// 0 dawn, 1 day, 2 night (Carnivores 2 only).
    pub day_night: i32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions {
            brightness: 128,
            day_night: 1,
        }
    }
}

impl Rsc {
    pub fn load(path: &std::path::Path, engine: Engine, opt: LoadOptions) -> Result<Rsc> {
        let data =
            std::fs::read(path).map_err(|e| FormatError(format!("{}: {e}", path.display())))?;
        Self::parse(&data, engine, opt).map_err(|e| FormatError(format!("{}: {e}", path.display())))
    }

    pub fn parse(data: &[u8], engine: Engine, opt: LoadOptions) -> Result<Rsc> {
        let c2 = engine == Engine::C2;
        let mut r = Reader::new(data);
        let tc = r.count("texture", 4096)?;
        let mc = r.count("object", 1024)?;

        let mut fade_rgb = [[0i32; 3]; 3];
        let mut trans_rgb = [[0i32; 3]; 3];
        if c2 {
            for row in fade_rgb.iter_mut() {
                for c in row.iter_mut() {
                    *c = r.i32()?;
                }
            }
            for row in trans_rgb.iter_mut() {
                for c in row.iter_mut() {
                    *c = r.i32()?;
                }
            }
        } else {
            let mut sky = [0i32; 3];
            let mut trans = [0i32; 3];
            for c in sky.iter_mut() {
                *c = r.i32()?;
            }
            for c in trans.iter_mut() {
                *c = r.i32()?;
            }
            fade_rgb = [sky; 3];
            trans_rgb = [trans; 3];
        }

        let mut textures = Vec::with_capacity(tc);
        for _ in 0..tc {
            let mut t = r.u16_vec(128 * 128)?;
            for p in t.iter_mut() {
                if *p == 0 {
                    *p = 1;
                }
            }
            if c2 {
                color::brighten(&mut t, opt.brightness, opt.day_night);
            }
            textures.push(t);
        }

        let mut objects = Vec::with_capacity(mc);
        for _ in 0..mc {
            let mut info = ObjInfo::read(&mut r)?;
            let mut model = Model::read_with_objects(&mut r)?;
            if c2 {
                color::brighten(&mut model.texture, opt.brightness, opt.day_night);
            }
            let bmp = if c2 {
                Some(read_bmp_model(&mut r, &model, opt)?)
            } else {
                None
            };
            let anim = if info.flags & OF_ANIMATED != 0 {
                let vc = r.count("vertex", 1 << 16)?;
                let _ = r.i32()?;
                let kps = r.i32()?.max(1);
                let frames = r.count("frame", 1 << 16)? + 1;
                let data = r.i16_vec(vc * frames * 3)?;
                if vc != model.vertices.len() {
                    return Err(FormatError("object animation vertex count mismatch".into()));
                }
                Some(Animation {
                    name: String::new(),
                    kps,
                    frames,
                    ani_time: (frames as i32 * 1000) / kps,
                    data,
                })
            } else {
                None
            };
            info.bound_r = model
                .vertices
                .iter()
                .map(|v| (v.pos[0] * v.pos[0] + v.pos[2] * v.pos[2]).sqrt())
                .fold(0.0, f32::max);
            objects.push(RscObject {
                info,
                model,
                bmp,
                anim,
            });
        }

        let skies = if c2 {
            let mut s = Vec::new();
            for _ in 0..3 {
                let mut t = r.u16_vec(256 * 256)?;
                color::brighten(&mut t, opt.brightness, opt.day_night);
                s.push(t);
            }
            s
        } else {
            vec![r.u16_vec(256 * 256)?]
        };
        let skymap = r.u8_vec(128 * 128)?;

        let fc = r.count("fog", 255)?;
        let mut fogs = vec![FogEntity::default()];
        for _ in 0..fc {
            let rgb = r.u32()? & 0xFF_FFFF;
            let y_begin = r.f32()?;
            let mortal = r.i32()? != 0;
            let transp = r.f32()?;
            let limit = r.f32()?;
            fogs.push(FogEntity {
                rgb,
                y_begin,
                mortal,
                transp,
                limit,
            });
        }

        let rc = r.count("sound", 1024)?;
        let mut random_sounds = Vec::with_capacity(rc);
        for _ in 0..rc {
            let len = r.count("sound byte", 1 << 26)?;
            random_sounds.push(pcm(r.bytes(len)?));
        }

        let ac = r.count("ambient", 256)?;
        let mut ambients = Vec::with_capacity(ac);
        for _ in 0..ac {
            let len = r.count("sound byte", 1 << 26)?;
            let sound = pcm(r.bytes(len)?);
            let mut all = Vec::with_capacity(16);
            for _ in 0..16 {
                let number = r.i32()?;
                let volume = r.i32()?;
                let freq = r.i32()?;
                let (envir, flags) = if c2 {
                    (r.u16()?, r.u16()?)
                } else {
                    r.i32()?;
                    (0, 0)
                };
                all.push(Trd {
                    number,
                    volume,
                    freq,
                    envir,
                    flags,
                });
            }
            let count = r.count("random sound", 16)?;
            let volume = r.i32()?;
            ambients.push(Ambient {
                sound,
                rdata: all,
                count,
                volume,
            });
        }

        let mut waters = Vec::new();
        if c2 {
            let wc = r.count("water", 255)?;
            for _ in 0..wc {
                let tindex = r.i32()?;
                let level = r.i32()?;
                let transp = r.f32()?;
                let _fog = r.u32()?;
                let fog_rgb = textures
                    .get(tindex.max(0) as usize)
                    .map(|t| {
                        let [r, g, b] = color::mid_color(t);
                        ((r as u32) << 16) | ((g as u32) << 8) | b as u32
                    })
                    .unwrap_or(0);
                waters.push(WaterEntity {
                    tindex,
                    level,
                    transp,
                    fog_rgb,
                });
            }
        }

        Ok(Rsc {
            engine,
            textures,
            fade_rgb,
            trans_rgb,
            objects,
            skies,
            skymap,
            fogs,
            random_sounds,
            ambients,
            waters,
        })
    }

    /// The horizon colour for the time of day, after the brightness option; night drops red and blue for the night-vision green.
    pub fn sky_fade(&self, opt: LoadOptions) -> [u8; 3] {
        let i = if self.engine == Engine::C2 {
            opt.day_night.clamp(0, 2) as usize
        } else {
            0
        };
        let mut c = self.fade_rgb[i];
        if self.engine == Engine::C2 {
            if opt.day_night == 2 {
                c[0] = 0;
                c[2] = 0;
            }
            for v in c.iter_mut() {
                *v = (*v * (opt.brightness + 128) / 256).min(255);
            }
        }
        [
            c[0].clamp(0, 255) as u8,
            c[1].clamp(0, 255) as u8,
            c[2].clamp(0, 255) as u8,
        ]
    }

    /// The clear sky's colour for the time of day: the games
    /// compared the sky round the sun with it to tell how clouded the sun
    /// was. Adjusted as the horizon colour is.
    pub fn sky_trans(&self, opt: LoadOptions) -> [u8; 3] {
        let i = if self.engine == Engine::C2 {
            opt.day_night.clamp(0, 2) as usize
        } else {
            0
        };
        let mut c = self.trans_rgb[i];
        if self.engine == Engine::C2 {
            if opt.day_night == 2 {
                c[0] = 0;
                c[2] = 0;
            }
            for v in c.iter_mut() {
                *v = (*v * (opt.brightness + 128) / 256).min(255);
            }
        }
        [
            c[0].clamp(0, 255) as u8,
            c[1].clamp(0, 255) as u8,
            c[2].clamp(0, 255) as u8,
        ]
    }

    pub fn sky(&self, opt: LoadOptions) -> &[u16] {
        let i = if self.engine == Engine::C2 {
            opt.day_night.clamp(0, 2) as usize
        } else {
            0
        };
        &self.skies[i.min(self.skies.len() - 1)]
    }
}

fn read_bmp_model(r: &mut Reader, model: &Model, opt: LoadOptions) -> Result<BmpModel> {
    let mut texture = r.u16_vec(128 * 128)?;
    color::brighten(&mut texture, opt.brightness, opt.day_night);
    // The quad spans the model's x and y extents. The loader
    // seeds the top from vertex 0's x rather than its y - kept, as it only
    // ever makes the quad a little taller.
    let (mut mxx, mut mnx, mut mxy, mut mny) = (0.5f32, -0.5f32, 0.5f32, -0.5f32);
    if let Some(v0) = model.vertices.first() {
        mxx = v0.pos[0] + 0.5;
        mnx = v0.pos[0] - 0.5;
        mxy = v0.pos[0] + 0.5;
        mny = v0.pos[1] - 0.5;
    }
    for v in &model.vertices {
        mxx = mxx.max(v.pos[0]);
        mnx = mnx.min(v.pos[0]);
        mxy = mxy.max(v.pos[1]);
        mny = mny.min(v.pos[1]);
    }
    Ok(BmpModel {
        texture,
        quad: [
            [mnx, mxy, 0.0],
            [mxx, mxy, 0.0],
            [mxx, mny, 0.0],
            [mnx, mny, 0.0],
        ],
    })
}

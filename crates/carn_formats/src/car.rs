//! Characters (.CAR): a model with its vertex animations and sounds.
//! Creatures, weapons, the hunter's body and the drop ship are all these.

use crate::color;
use crate::model::{Animation, Model};
use crate::reader::{FormatError, Reader, Result};

#[derive(Clone, Debug)]
pub struct CharacterInfo {
    pub name: String,
    pub model: Model,
    pub animations: Vec<Animation>,
    /// 16-bit mono 22050 Hz.
    pub sounds: Vec<Vec<i16>>,
    /// For each animation, the sound played as it starts, or -1.
    pub anifx: [i32; 64],
}

impl CharacterInfo {
    /// `brightness` is Carnivores 2's option (128 = as authored); pass None
    /// for Carnivores, which had none.
    pub fn load(path: &std::path::Path, brightness: Option<(i32, i32)>) -> Result<CharacterInfo> {
        let data =
            std::fs::read(path).map_err(|e| FormatError(format!("{}: {e}", path.display())))?;
        Self::parse(&data, brightness).map_err(|e| FormatError(format!("{}: {e}", path.display())))
    }

    pub fn parse(data: &[u8], brightness: Option<(i32, i32)>) -> Result<CharacterInfo> {
        let mut r = Reader::new(data);
        let name = r.name(32)?;
        let ani_count = r.count("animation", 64)?;
        let sfx_count = r.count("sound", 64)?;
        let (mut model, vc) = Model::read_car(&mut r)?;
        if let Some((b, dn)) = brightness {
            color::brighten(&mut model.texture, b, dn);
        }

        let mut animations = Vec::with_capacity(ani_count);
        for _ in 0..ani_count {
            let aname = r.name(32)?;
            let kps = r.i32()?.max(1);
            let frames = r.count("frame", 1 << 16)?;
            let data = r.i16_vec(vc * frames * 3)?;
            animations.push(Animation {
                name: aname,
                kps,
                frames,
                ani_time: (frames as i32 * 1000) / kps,
                data,
            });
        }

        let mut sounds = Vec::with_capacity(sfx_count);
        for _ in 0..sfx_count {
            r.skip(32)?;
            let len = r.count("sound byte", 1 << 26)?;
            let b = r.bytes(len)?;
            sounds.push(
                b.chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect(),
            );
        }

        let mut anifx = [-1i32; 64];
        if r.remaining() >= 256 {
            for a in anifx.iter_mut() {
                *a = r.i32()?;
            }
        }

        Ok(CharacterInfo {
            name,
            model,
            animations,
            sounds,
            anifx,
        })
    }
}

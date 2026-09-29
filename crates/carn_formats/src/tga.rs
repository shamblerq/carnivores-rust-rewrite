//! Truevision TGA pictures: the menus, HUD pieces and map frame.
//!
//! The games wrote 16-bit 5-5-5 uncompressed files; mods sometimes bring
//! 24- or 32-bit or run-length ones, so those are read too.

use crate::color::rgb555;
use crate::reader::{FormatError, Reader, Result};

#[derive(Clone, Debug)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Top row first.
    pub rgba: Vec<u8>,
    /// The 16-bit pixels as stored, top row first, for pictures the game
    /// treats as 5-5-5 data (colour keys, region maps); empty otherwise.
    pub raw16: Vec<u16>,
}

impl Picture {
    pub fn load(path: &std::path::Path) -> Result<Picture> {
        let data =
            std::fs::read(path).map_err(|e| FormatError(format!("{}: {e}", path.display())))?;
        Self::parse(&data).map_err(|e| FormatError(format!("{}: {e}", path.display())))
    }

    pub fn parse(data: &[u8]) -> Result<Picture> {
        let mut r = Reader::new(data);
        let id_len = r.u8()? as usize;
        let cmap_type = r.u8()?;
        let img_type = r.u8()?;
        let _cmap_first = r.u16()?;
        let cmap_len = r.u16()? as usize;
        let cmap_bits = r.u8()? as usize;
        let _x0 = r.u16()?;
        let _y0 = r.u16()?;
        let w = r.u16()? as usize;
        let h = r.u16()? as usize;
        let bpp = r.u8()? as usize;
        let desc = r.u8()?;
        r.skip(id_len)?;
        if cmap_type != 0 {
            r.skip(cmap_len * cmap_bits.div_ceil(8))?;
        }
        let rle = match img_type {
            2 => false,
            10 => true,
            _ => return Err(FormatError(format!("unsupported TGA type {img_type}"))),
        };
        let px = bpp / 8;
        if !(2..=4).contains(&px) {
            return Err(FormatError(format!("unsupported TGA depth {bpp}")));
        }
        let n = w * h;
        let mut raw = Vec::with_capacity(n * px);
        if rle {
            while raw.len() < n * px {
                let hdr = r.u8()?;
                let count = (hdr & 0x7F) as usize + 1;
                if hdr & 0x80 != 0 {
                    let p = r.bytes(px)?;
                    for _ in 0..count {
                        raw.extend_from_slice(p);
                    }
                } else {
                    raw.extend_from_slice(r.bytes(count * px)?);
                }
            }
            raw.truncate(n * px);
        } else {
            raw.extend_from_slice(r.bytes(n * px)?);
        }

        // Rows are stored bottom-up unless descriptor bit 5 says otherwise.
        let top_down = desc & 0x20 != 0;
        let mut rgba = vec![0u8; n * 4];
        let mut raw16 = if px == 2 { vec![0u16; n] } else { Vec::new() };
        for y in 0..h {
            let sy = if top_down { y } else { h - 1 - y };
            for x in 0..w {
                let s = &raw[(sy * w + x) * px..];
                let d = &mut rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
                match px {
                    2 => {
                        let v = u16::from_le_bytes([s[0], s[1]]) & 0x7FFF;
                        raw16[y * w + x] = v;
                        let [cr, cg, cb] = rgb555(v);
                        d.copy_from_slice(&[cr, cg, cb, 255]);
                    }
                    3 => d.copy_from_slice(&[s[2], s[1], s[0], 255]),
                    _ => d.copy_from_slice(&[s[2], s[1], s[0], s[3]]),
                }
            }
        }
        Ok(Picture {
            width: w,
            height: h,
            rgba,
            raw16,
        })
    }
}

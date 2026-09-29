//! Reading Windows icons out of .ICO files and PE executables.
//!
//! Every Carnivores release and every mod carries its artwork the way Windows
//! expected: either as a loose .ICO beside the executable or as a resource
//! inside it. Reading those is what lets the launcher show a mod as itself
//! rather than as a generic entry.
//!
//! Both containers store an image as a DIB whose height is doubled - the top
//! half is the colour bitmap, the bottom half a 1-bit AND mask holding
//! transparency. Vista-era files may store a PNG instead.

/// A decoded icon, RGBA8888, top row first.
pub struct IconImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
const RT_ICON: u32 = 3;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn i32_at(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

// ------------------------------------------------------------------------
// DIB decoding

fn decode_dib(buf: &[u8]) -> Option<IconImage> {
    let hs = u32_at(buf, 0)? as usize;
    let w = i32_at(buf, 4)?;
    let h2 = i32_at(buf, 8)?;
    let bpp = u16_at(buf, 14)? as usize;
    let comp = u32_at(buf, 16)?;
    let mut ncol = u32_at(buf, 32)? as usize;

    let h = h2 / 2;
    if hs < 40 || !(1..=1024).contains(&w) || !(1..=1024).contains(&h) {
        return None;
    }
    if comp != 0 || ![1, 4, 8, 24, 32].contains(&bpp) {
        return None;
    }
    let (w, h) = (w as usize, h as usize);
    if ncol == 0 && bpp <= 8 {
        ncol = 1 << bpp;
    }
    if ncol > 256 {
        return None;
    }

    let pal_off = hs;
    let bits = pal_off + ncol * 4;
    let row = (w * bpp).div_ceil(32) * 4;
    let mrow = w.div_ceil(32) * 4;
    let mask = bits + row * h;
    if buf.len() < mask {
        return None;
    }
    let have_mask = buf.len() >= mask + mrow * h;

    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let ro = bits + row * y;
        let mo = mask + mrow * y;
        // DIB rows run bottom-up.
        let d = (h - 1 - y) * w * 4;
        for x in 0..w {
            let (r, g, b, mut a) = if bpp <= 8 {
                let mut idx = match bpp {
                    8 => buf[ro + x] as usize,
                    4 => ((buf[ro + (x >> 1)] >> if x & 1 == 1 { 0 } else { 4 }) & 0x0F) as usize,
                    _ => ((buf[ro + (x >> 3)] >> (7 - (x & 7))) & 1) as usize,
                };
                if idx >= ncol {
                    idx = 0;
                }
                let p = pal_off + idx * 4;
                (buf[p + 2], buf[p + 1], buf[p], 255)
            } else if bpp == 24 {
                let p = ro + x * 3;
                (buf[p + 2], buf[p + 1], buf[p], 255)
            } else {
                let p = ro + x * 4;
                (buf[p + 2], buf[p + 1], buf[p], buf[p + 3])
            };
            // A set mask bit means "leave the background showing".
            if have_mask && (buf[mo + (x >> 3)] >> (7 - (x & 7))) & 1 == 1 {
                a = 0;
            }
            out[d + x * 4..d + x * 4 + 4].copy_from_slice(&[r, g, b, a]);
        }
    }
    // 32-bit icons predating a real alpha channel leave it zero throughout.
    if bpp == 32 && out.iter().skip(3).step_by(4).all(|&a| a == 0) {
        out.iter_mut().skip(3).step_by(4).for_each(|a| *a = 255);
    }
    Some(IconImage {
        width: w,
        height: h,
        rgba: out,
    })
}

fn decode_entry(buf: &[u8]) -> Option<IconImage> {
    if buf.starts_with(PNG_SIG) {
        let img = image::load_from_memory_with_format(buf, image::ImageFormat::Png)
            .ok()?
            .to_rgba8();
        return Some(IconImage {
            width: img.width() as usize,
            height: img.height() as usize,
            rgba: img.into_raw(),
        });
    }
    decode_dib(buf)
}

// ------------------------------------------------------------------------
// .ICO container

pub fn read_ico(data: &[u8]) -> Option<IconImage> {
    if u16_at(data, 0)? != 0 || u16_at(data, 2)? != 1 {
        return None;
    }
    let count = u16_at(data, 4)? as usize;
    // Biggest wins so there is the most detail to scale from; on a tie the
    // deeper one wins, since a file often carries a 16- and a 256-colour copy
    // of the same artwork.
    let mut best: Option<((usize, u16), usize, usize)> = None;
    for i in 0..count {
        let e = 6 + i * 16;
        if e + 16 > data.len() {
            break;
        }
        let w = match data[e] {
            0 => 256,
            v => v as usize,
        };
        let h = match data[e + 1] {
            0 => 256,
            v => v as usize,
        };
        let bpp = u16_at(data, e + 6)?;
        let size = u32_at(data, e + 8)? as usize;
        let off = u32_at(data, e + 12)? as usize;
        let key = (w * h, bpp);
        if best.map(|b| key > b.0).unwrap_or(true) {
            best = Some((key, off, size));
        }
    }
    let (_, off, size) = best?;
    decode_entry(data.get(off..off.checked_add(size)?)?)
}

// ------------------------------------------------------------------------
// PE resource tree

fn read_pe(data: &[u8]) -> Option<IconImage> {
    if !data.starts_with(b"MZ") {
        return None;
    }
    let pe = u32_at(data, 0x3C)? as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let nsec = u16_at(data, pe + 6)? as usize;
    let optsz = u16_at(data, pe + 20)? as usize;
    let opt = pe + 24;
    let magic = u16_at(data, opt)?;
    if magic != 0x10B && magic != 0x20B {
        return None;
    }
    // Data directory entry 2 is the resource table.
    let dd = opt + if magic == 0x10B { 96 } else { 112 };
    let rsrc_rva = u32_at(data, dd + 16)?;
    if rsrc_rva == 0 {
        return None;
    }
    let mut sections = Vec::new();
    let mut so = opt + optsz;
    for _ in 0..nsec.min(96) {
        let (Some(vsize), Some(va), Some(rsize), Some(raw)) = (
            u32_at(data, so + 8),
            u32_at(data, so + 12),
            u32_at(data, so + 16),
            u32_at(data, so + 20),
        ) else {
            break;
        };
        sections.push((va, vsize.max(rsize), raw));
        so += 40;
    }
    let to_off = |rva: u32| -> Option<usize> {
        sections
            .iter()
            .find(|(va, span, _)| *va <= rva && rva < va + span)
            .map(|(va, _, raw)| (raw + (rva - va)) as usize)
    };
    let base = to_off(rsrc_rva)?;
    let entries = |dir: usize| -> Vec<(u32, u32)> {
        let (Some(named), Some(ids)) = (u16_at(data, dir + 12), u16_at(data, dir + 14)) else {
            return Vec::new();
        };
        (0..named as usize + ids as usize)
            .map_while(|i| {
                let e = dir + 16 + i * 8;
                Some((u32_at(data, e)?, u32_at(data, e + 4)?))
            })
            .collect()
    };

    // Level 0: the RT_ICON subtree.
    let icon_dir = entries(base)
        .into_iter()
        .find(|&(name, next)| name & 0x8000_0000 == 0 && name == RT_ICON && next & 0x8000_0000 != 0)
        .map(|(_, next)| base + (next & 0x7FFF_FFFF) as usize)?;

    // Levels 1 and 2: every leaf under RT_ICON is one image; the largest wins.
    let (mut best_off, mut best_size) = (0usize, 0usize);
    for (_, next) in entries(icon_dir) {
        if next & 0x8000_0000 == 0 {
            continue;
        }
        for (_, leaf) in entries(base + (next & 0x7FFF_FFFF) as usize) {
            if leaf & 0x8000_0000 != 0 {
                continue;
            }
            let d = base + leaf as usize;
            let (Some(rva), Some(size)) = (u32_at(data, d), u32_at(data, d + 4)) else {
                continue;
            };
            if let Some(off) = to_off(rva) {
                if size as usize > best_size {
                    (best_off, best_size) = (off, size as usize);
                }
            }
        }
    }
    if best_size == 0 {
        return None;
    }
    decode_entry(data.get(best_off..best_off + best_size)?)
}

/// The largest usable icon in a .ICO file or a PE executable.
pub fn read_icon(path: &std::path::Path) -> Option<IconImage> {
    let data = std::fs::read(path).ok()?;
    if data.starts_with(b"MZ") {
        read_pe(&data)
    } else {
        read_ico(&data)
    }
}

/// An entry's picture from whatever file holds it: an icon, a program
/// carrying one, or an ordinary image.
pub fn picture(source: &std::path::Path) -> Option<IconImage> {
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "exe" | "ico" | "dll") {
        return read_icon(source);
    }
    let img = image::open(source).ok()?.to_rgba8();
    Some(IconImage {
        width: img.width() as usize,
        height: img.height() as usize,
        rgba: img.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ico(rel: &str) -> IconImage {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        read_icon(&p).unwrap_or_else(|| panic!("{rel} did not decode"))
    }

    #[test]
    fn png_entries() {
        let i = ico("icon.ico");
        assert_eq!((i.width, i.height), (256, 256));
        assert_eq!(i.rgba.len(), 256 * 256 * 4);
    }

    /// An .ICO holding one 2x2 24-bit DIB: bottom row first, rows padded to
    /// four bytes, then the AND mask.
    fn dib_ico() -> Vec<u8> {
        let mut dib = Vec::new();
        for v in [40u32, 2, 4, 0x0018_0001, 0, 0, 0, 0, 0, 0] {
            dib.extend(v.to_le_bytes());
        }
        // Bottom row: blue, green. Top row: red, white.
        dib.extend([255, 0, 0, 0, 255, 0, 0, 0]);
        dib.extend([0, 0, 255, 255, 255, 255, 0, 0]);
        // Mask, bottom row first: the bottom-left pixel is see-through.
        dib.extend([0x80, 0, 0, 0, 0, 0, 0, 0]);
        let mut ico = vec![0, 0, 1, 0, 1, 0, 2, 2, 0, 0, 1, 0, 24, 0];
        ico.extend((dib.len() as u32).to_le_bytes());
        ico.extend(22u32.to_le_bytes());
        ico.extend(dib);
        ico
    }

    #[test]
    fn dib_entries() {
        let i = read_ico(&dib_ico()).expect("DIB did not decode");
        assert_eq!((i.width, i.height), (2, 2));
        let px: Vec<&[u8]> = i.rgba.chunks(4).collect();
        assert_eq!(
            px,
            [
                &[255, 0, 0, 255][..],
                &[255, 255, 255, 255],
                &[0, 0, 255, 0],
                &[0, 255, 0, 255],
            ]
        );
    }
}

//! The games' 16-bit colours.
//!
//! Textures, skies and models are 5-5-5 with red on top (0RRRRRGGGGGBBBBB).
//! Pixel value 0 doubles as the colour key on cut-out faces.

/// Expands a 5-bit channel to 8 bits the way the hardware did (replicating
/// the top bits into the bottom), so white stays 255.
#[inline]
pub fn expand5(c: u16) -> u8 {
    let c = (c & 31) as u8;
    (c << 3) | (c >> 2)
}

#[inline]
pub fn rgb555(w: u16) -> [u8; 3] {
    [expand5(w >> 10), expand5(w >> 5), expand5(w)]
}

/// Carnivores 2's brightness option, applied to every texture at load: each channel scaled by (brightness+128)/256. At night
/// (day_night == 2) the picture goes green, for the night-vision look.
pub fn brighten(pixels: &mut [u16], brightness: i32, day_night: i32) {
    let factor = brightness + 128;
    for w in pixels.iter_mut() {
        let mut b = ((*w & 31) as i32 * factor) >> 8;
        let g = (((*w >> 5) & 31) as i32 * factor) >> 8;
        let mut r = (((*w >> 10) & 31) as i32 * factor) >> 8;
        let g = g.min(31);
        b = b.min(31);
        r = r.min(31);
        if day_night == 2 {
            b = g >> 3;
            r = g >> 3;
        }
        *w = (b | (g << 5) | (r << 10)) as u16;
    }
}

/// Average colour of a texture, 0..255 per channel, used for
/// the colour of a body of water seen from inside.
pub fn mid_color(pixels: &[u16]) -> [u8; 3] {
    if pixels.is_empty() {
        return [0; 3];
    }
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for &w in pixels {
        b += ((w & 31) as u64) * 8;
        g += (((w >> 5) & 31) as u64) * 8;
        r += (((w >> 10) & 31) as u64) * 8;
    }
    let n = pixels.len() as u64;
    [(r / n) as u8, (g / n) as u8, (b / n) as u8]
}

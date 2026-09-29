//! Turning the games' 16-bit pictures into GPU images.
//!
//! Colours stay the stored values (a non-sRGB format), as the shaders do
//! their arithmetic on those and convert at the end. Mipmaps are made here,
//! averaging the stored values like the original's own mip levels did.

use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use carn_formats::color::rgb555;

/// A square power-of-two RGBA picture and its mip chain, top level first.
fn mip_chain(size: usize, rgba: Vec<u8>, keyed: bool) -> Vec<Vec<u8>> {
    let mut levels = vec![rgba];
    // Coverage of the cut-out at full size, which the smaller levels are
    // scaled to keep: otherwise foliage thins away into the distance.
    let coverage = |l: &[u8], scale: f32| {
        l.chunks_exact(4)
            .filter(|p| p[3] as f32 * scale >= 127.5)
            .count() as f32
            / (l.len() / 4) as f32
    };
    let base_cov = if keyed {
        coverage(&levels[0], 1.0)
    } else {
        0.0
    };
    let mut s = size;
    while s > 1 {
        let prev = levels.last().unwrap();
        let n = s / 2;
        let mut out = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0u32; 4];
                let mut wsum = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = &prev[((y * 2 + dy) * s + x * 2 + dx) * 4..][..4];
                    let w = if keyed { p[3] as u32 } else { 1 };
                    acc[0] += p[0] as u32 * w;
                    acc[1] += p[1] as u32 * w;
                    acc[2] += p[2] as u32 * w;
                    acc[3] += p[3] as u32;
                    wsum += w;
                }
                let o = &mut out[(y * n + x) * 4..][..4];
                if wsum > 0 {
                    o[0] = ((acc[0] + wsum / 2) / wsum) as u8;
                    o[1] = ((acc[1] + wsum / 2) / wsum) as u8;
                    o[2] = ((acc[2] + wsum / 2) / wsum) as u8;
                }
                o[3] = ((acc[3] + 2) / 4) as u8;
            }
        }
        if keyed && base_cov > 0.0 {
            // Find the alpha scale that gives this level the same coverage.
            let (mut lo, mut hi) = (0.25f32, 8.0f32);
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                if coverage(&out, mid) < base_cov {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let k = (lo + hi) / 2.0;
            for p in out.chunks_exact_mut(4) {
                p[3] = (p[3] as f32 * k).min(255.0) as u8;
            }
        }
        levels.push(out);
        s = n;
    }
    levels
}

/// Fills the colour of fully transparent texels from their neighbours, so
/// filtering at a cut-out's edge does not pull in the key colour (black).
fn bleed(size_w: usize, size_h: usize, rgba: &mut [u8]) {
    for _ in 0..4 {
        let src = rgba.to_vec();
        for y in 0..size_h {
            for x in 0..size_w {
                let i = (y * size_w + x) * 4;
                if src[i + 3] != 0 {
                    continue;
                }
                let mut acc = [0u32; 3];
                let mut n = 0;
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= size_w as i32 || ny >= size_h as i32 {
                        continue;
                    }
                    let j = (ny as usize * size_w + nx as usize) * 4;
                    if src[j + 3] != 0 || (src[j] | src[j + 1] | src[j + 2]) != 0 {
                        acc[0] += src[j] as u32;
                        acc[1] += src[j + 1] as u32;
                        acc[2] += src[j + 2] as u32;
                        n += 1;
                    }
                }
                if n > 0 {
                    rgba[i] = (acc[0] / n) as u8;
                    rgba[i + 1] = (acc[1] / n) as u8;
                    rgba[i + 2] = (acc[2] / n) as u8;
                }
            }
        }
    }
}

/// 5-5-5 pixels to RGBA; with `keyed`, pixel 0 becomes transparent.
pub fn to_rgba(px: &[u16], keyed: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for &w in px {
        let [r, g, b] = rgb555(w);
        let a = if keyed && w & 0x7FFF == 0 { 0 } else { 255 };
        out.extend_from_slice(&[r, g, b, a]);
    }
    out
}

pub fn sampler(quality: i32, repeat: bool) -> ImageSampler {
    let addr = if repeat {
        ImageAddressMode::Repeat
    } else {
        ImageAddressMode::ClampToEdge
    };
    let (filter, mip, aniso) = match quality {
        0 => (ImageFilterMode::Nearest, ImageFilterMode::Nearest, 1),
        1 => (ImageFilterMode::Linear, ImageFilterMode::Nearest, 1),
        2 => (ImageFilterMode::Linear, ImageFilterMode::Linear, 1),
        _ => (ImageFilterMode::Linear, ImageFilterMode::Linear, 8),
    };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: addr,
        address_mode_v: addr,
        address_mode_w: addr,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: mip,
        anisotropy_clamp: aniso,
        ..default()
    })
}

/// A stack of square textures (the ground's) as one array texture with mips.
pub fn texture_array(size: usize, layers: &[Vec<u16>], quality: i32) -> Image {
    let n = layers.len().max(1);
    let mut data = Vec::new();
    let mut mips = 0;
    for i in 0..n {
        let px = layers.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
        let rgba = if px.len() == size * size {
            to_rgba(px, false)
        } else {
            vec![128; size * size * 4]
        };
        let chain = mip_chain(size, rgba, false);
        mips = chain.len();
        for l in chain {
            data.extend_from_slice(&l);
        }
    }
    let mut img = Image::new_uninit(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: n as u32,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = mips as u32;
    img.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    img.sampler = sampler(quality, false);
    img
}

/// One square texture with mips. `keyed` makes pixel 0 transparent.
pub fn texture(size: usize, px: &[u16], keyed: bool, quality: i32, repeat: bool) -> Image {
    let mut rgba = to_rgba(px, keyed);
    rgba.resize(size * size * 4, 0);
    if keyed {
        bleed(size, size, &mut rgba);
    }
    let chain = mip_chain(size, rgba, keyed);
    let mips = chain.len() as u32;
    let mut img = Image::new_uninit(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(chain.concat());
    img.texture_descriptor.mip_level_count = mips;
    img.sampler = sampler(quality, repeat);
    img
}

/// A plain image without mips or filtering, for lookup tables.
pub fn table(w: u32, h: u32, format: TextureFormat, data: Vec<u8>) -> Image {
    let mut img = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    img
}

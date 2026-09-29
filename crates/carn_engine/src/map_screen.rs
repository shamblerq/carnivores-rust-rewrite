//! The area map (Tab): the frame picture with a small image of the area
//! made from the ground textures, where the hunter is, and how far he sees.

use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use carn_formats::color::rgb555;
use carn_formats::tga::Picture;
use carn_formats::Engine;

use crate::area::Area;
use crate::paths::DataRoot;
use crate::player::Player;

/// Where the area image sits inside the frame picture.
const X_SHIFT: usize = 11;
const Y_SHIFT: usize = 23;

#[derive(Resource)]
pub struct MapScreen {
    pub image: Handle<Image>,
    pub size: UVec2,
    pub open: bool,
}

#[derive(Component)]
pub struct MapRoot;

/// A texture's mip level of the given size, rounded.
fn mip(px: &[u16], size: usize) -> Vec<u16> {
    let mut cur: Vec<u16> = px.to_vec();
    let mut s = 128;
    while s > size {
        let n = s / 2;
        let mut out = vec![0u16; n * n];
        for y in 0..n {
            for x in 0..n {
                let c = [
                    cur[y * 2 * s + x * 2],
                    cur[y * 2 * s + x * 2 + 1],
                    cur[(y * 2 + 1) * s + x * 2],
                    cur[(y * 2 + 1) * s + x * 2 + 1],
                ];
                let ch = |sh: u16| {
                    ((c.iter().map(|v| ((v >> sh) & 31) as u32).sum::<u32>() + 2) >> 2) as u16
                };
                out[y * n + x] = (ch(10) << 10) | (ch(5) << 5) | ch(0);
            }
        }
        cur = out;
        s = n;
    }
    cur
}

pub fn build(root: &DataRoot, area: &Area, images: &mut Assets<Image>) -> Option<MapScreen> {
    let pic = Picture::load(&root.find("HUNTDAT/MENU/MAPFRAME.TGA")?).ok()?;
    let (w, h) = (pic.width, pic.height);
    let mut rgba = pic.rgba.clone();
    let tex = &area.rsc.textures;
    let small: Vec<Vec<u16>> = tex.iter().map(|t| mip(t, 32)).collect();
    let tiny: Vec<Vec<u16>> = tex.iter().map(|t| mip(t, 16)).collect();
    let step = area.size as usize / 256;
    for y in 0..256 {
        for x in 0..256 {
            let i = area.map.idx(x * step, y * step);
            let water_tex = match area.engine {
                Engine::C2 if area.map.deep_water(i) => area
                    .rsc
                    .waters
                    .get(area.map.wmap[i] as usize)
                    .map(|w| w.tindex.max(0) as usize),
                Engine::C1 if area.map.tmap1[i] == 0 => Some(0),
                _ => None,
            };
            let c = match water_tex {
                Some(t) => tiny.get(t).map(|d| d[(y & 15) * 16 + (x & 15)]),
                None => small
                    .get(area.map.tmap1[i] as usize)
                    .map(|d| d[(y & 31) * 32 + (x & 31)]),
            }
            .unwrap_or(0);
            let (px, py) = (x + X_SHIFT, y + Y_SHIFT);
            if px < w && py < h {
                let [r, g, b] = rgb555(c);
                rgba[(py * w + px) * 4..(py * w + px) * 4 + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    let mut img = Image::new(
        Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    Some(MapScreen {
        image: images.add(img),
        size: UVec2::new(w as u32, h as u32),
        open: false,
    })
}

/// What the map shows: the hunter's cell, the window height (and blip
/// style), the radar's blips.
type MapKey = (i32, i32, u32, Vec<(i32, i32)>);

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    map: Option<ResMut<MapScreen>>,
    area: Option<Res<Area>>,
    player: Option<Res<Player>>,
    (settings, radius): (Res<crate::settings::Settings>, Res<crate::ViewRadius>),
    view: Res<crate::ViewState>,
    windows: Query<&Window>,
    old: Query<Entity, With<MapRoot>>,
    sim: Option<Res<crate::sim::Sim>>,
    mut last: Local<Option<MapKey>>,
) {
    let (Some(mut map), Some(area), Some(player)) = (map, area, player) else {
        for e in old.iter() {
            commands.entity(e).despawn();
        }
        *last = None;
        return;
    };
    if keys.just_pressed(KeyCode::Tab) && !view.paused && !area.trophy {
        map.open = !map.open;
    }
    let Ok(win) = windows.single() else { return };
    let step = area.size / 256;
    let (cx, cz) = (
        (player.x / 256.0) as i32 / step,
        (player.z / 256.0) as i32 / step,
    );
    // With radar, the hunted kinds' positions (RenderHMap).
    let blips: Vec<(i32, i32)> = sim
        .as_deref()
        .filter(|s| s.radar)
        .map(|s| {
            s.chars
                .iter()
                .filter(|c| {
                    let bit = if s.c1 { c.ctype as i32 - 4 } else { c.ai };
                    c.health > 0
                        && !c.removed
                        && c.ai >= 10
                        && (0..32).contains(&bit)
                        && s.target_dino & (1 << bit) != 0
                })
                .map(|c| {
                    (
                        (c.pos.x / 256.0) as i32 / step,
                        (c.pos.z / 256.0) as i32 / step,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let key = (
        cx,
        cz,
        (win.height() as u32 & 0xFFFF)
            | ((radius.r as u32 & 0x7FFF) << 16)
            | ((settings.radar_clear as u32) << 31),
        blips.clone(),
    );
    let open = map.open && !view.paused;
    if !open {
        if last.is_some() {
            for e in old.iter() {
                commands.entity(e).despawn();
            }
            *last = None;
        }
        return;
    }
    if last.as_ref() == Some(&key) {
        return;
    }
    *last = Some(key);
    for e in old.iter() {
        commands.entity(e).despawn();
    }
    // Pixel for pixel at any resolution, as the games drew it.
    let s: f32 = 1.0;
    let (w, h) = (map.size.x as f32 * s, map.size.y as f32 * s);
    let left = (win.width() - w) / 2.0;
    let top = (win.height() - h) / 2.0 - 6.0 * s;
    let root = commands
        .spawn((
            ImageNode::new(map.image.clone()),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(left),
                top: Val::Px(top),
                width: Val::Px(w),
                height: Val::Px(h),
                ..default()
            },
            MapRoot,
        ))
        .id();
    let ox = X_SHIFT as f32 * s + cx as f32 * s;
    let oy = Y_SHIFT as f32 * s + cz as f32 * s;
    // How far the hunter can see: a ring of the view radius.
    let r = radius.r as f32 / step as f32 * s;
    for (d, col) in [
        (s, Color::srgb(0.0, 0.13, 0.0)),
        (0.0, Color::srgb(0.0, 0.58, 0.0)),
    ] {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(ox - r + d),
                top: Val::Px(oy - r + d),
                width: Val::Px(r * 2.0),
                height: Val::Px(r * 2.0),
                border: UiRect::all(Val::Px(s)),
                ..default()
            },
            BorderColor(col),
            BorderRadius::MAX,
            ChildOf(root),
        ));
    }
    // Each game's own 2x2 dot - green, Ice Age's blue - or the larger
    // marker: a pink core in a dark blue ring, its corners cut.
    let dot = match area.kind {
        crate::game::GameKind::IceAge => Color::srgb_u8(0, 0, 255),
        crate::game::GameKind::Carnivores => Color::srgb_u8(0, 247, 0),
        _ => Color::srgb_u8(0, 121, 0),
    };
    let mut px = |x: f32, y: f32, w: f32, h: f32, c: Color| {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                top: Val::Px(y),
                width: Val::Px(w),
                height: Val::Px(h),
                ..default()
            },
            BackgroundColor(c),
            ChildOf(root),
        ));
    };
    for (bx, bz) in blips {
        let (x, y) = (
            X_SHIFT as f32 * s + bx as f32 * s,
            Y_SHIFT as f32 * s + bz as f32 * s,
        );
        if settings.radar_clear {
            let edge = Color::srgb_u8(0, 4, 115);
            px(x - s, y - 2.0 * s, 3.0 * s, 5.0 * s, edge);
            px(x - 2.0 * s, y - s, 5.0 * s, 3.0 * s, edge);
            px(x - s, y - s, 3.0 * s, 3.0 * s, Color::srgb_u8(255, 24, 214));
        } else {
            px(x, y, 2.0 * s, 2.0 * s, dot);
        }
    }
    for (d, col) in [
        (s, Color::srgb(0.26, 0.0, 0.0)),
        (0.0, Color::srgb(0.97, 0.0, 0.0)),
    ] {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(ox - s + d),
                top: Val::Px(oy - s + d),
                width: Val::Px(2.0 * s),
                height: Val::Px(2.0 * s),
                ..default()
            },
            BackgroundColor(col),
            ChildOf(root),
        ));
    }
}

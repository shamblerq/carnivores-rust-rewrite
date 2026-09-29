//! Building the scene for a loaded area: the ground in chunks, the water
//! sheets, every object on the map and the sky, plus the systems that keep
//! them in step with the camera.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::NoFrustumCulling;
use carn_formats::map::{C1_FM_REVERSE, C2_FM_REVERSE};
use carn_formats::model::Model;
use carn_formats::rsc::{OF_ANIMATED, OF_GRND_LIGHT, OF_NO_BMP};
use carn_formats::Engine;

use crate::area::Area;
use crate::render::{
    textures, ObjectMaterial, SkyMaterial, SpriteMaterial, TerrainMaterial, WaterMaterial,
    ATTR_OBJECT, ATTR_SPRITE, ATTR_TERRAIN, ATTR_WATER, TAG_HAS_SPRITE,
};
use crate::settings::Settings;

/// Cells along a side of a ground chunk.
pub const CHUNK: i32 = 64;

/// Everything spawned for the current area, removed together.
#[derive(Component)]
pub struct AreaEntity;

/// A piece of the world that is hidden once it is wholly past the view
/// distance: its centre and radius on the ground plane.
#[derive(Component)]
pub struct Chunk {
    pub center: Vec2,
    pub radius: f32,
}

#[derive(Component)]
pub struct SkyPlane;

/// The ground and water materials, whose per-frame values change now and
/// then (see FrameValues).
#[derive(Resource, Clone)]
pub struct WorldMaterials {
    pub terrain: Handle<TerrainMaterial>,
    pub water: Handle<WaterMaterial>,
}

/// The objects on the map, and their sprites (Shift+M hides them).
#[derive(Component)]
pub struct MapObject;

/// Meshes of the animated objects, re-posed every frame.
#[derive(Resource, Default)]
pub struct AnimatedObjects(pub Vec<(usize, u32, Handle<Mesh>)>);

/// Shared images every world material binds.
#[derive(Resource, Clone)]
pub struct WorldImages {
    pub textures: Handle<Image>,
    pub cells: Handle<Image>,
    pub fog_map: Handle<Image>,
    pub fog_table: Handle<Image>,
    pub skymap: Handle<Image>,
    pub sky: Handle<Image>,
}

fn byte_swap_rgb(v: u32) -> u32 {
    ((v & 0xFF) << 16) | (v & 0xFF00) | ((v >> 16) & 0xFF)
}

/// Fog colour as 0..1 RGB, the way each engine stored it: Carnivores 2
/// files keep it blue-first (the loader swapped it for Direct3D), and at
/// night only the green survives.
pub fn fog_rgb(area: &Area, raw: u32) -> [f32; 3] {
    let mut v = match area.engine {
        Engine::C1 => raw,
        Engine::C2 => byte_swap_rgb(raw),
    };
    if area.engine == Engine::C2 && area.opt.day_night == 2 {
        v &= 0x00FF00;
    }
    [
        ((v >> 16) & 0xFF) as f32 / 255.0,
        ((v >> 8) & 0xFF) as f32 / 255.0,
        (v & 0xFF) as f32 / 255.0,
    ]
}

pub fn build_images(area: &Area, settings: &Settings, images: &mut Assets<Image>) -> WorldImages {
    let n = area.size as usize;
    let tex = textures::texture_array(128, &area.rsc.textures, settings.textures);

    let mut cells = Vec::with_capacity(n * n * 8);
    for i in 0..n * n {
        let t1 = area.map.tmap1[i];
        let t2 = if area.engine == Engine::C1 {
            area.map.tmap2[i]
        } else {
            t1
        };
        let w = area.map.wmap[i];
        let wt = if w != 255 {
            area.rsc
                .waters
                .get(w as usize)
                .map(|w| w.tindex.max(0) as u16)
                .unwrap_or(0xFFFF)
        } else {
            0xFFFF
        };
        for v in [t1, t2, area.map.fmap[i], wt] {
            cells.extend_from_slice(&v.to_le_bytes());
        }
    }
    let cells = textures::table(n as u32, n as u32, TextureFormat::Rgba16Uint, cells);

    let hn = (n / 2) as u32;
    let fog_map = textures::table(hn, hn, TextureFormat::R8Uint, area.map.fogsmap.clone());

    let mut ft = vec![0f32; 256 * 2 * 4];
    for (i, f) in area.rsc.fogs.iter().enumerate().take(256) {
        let [r, g, b] = fog_rgb(area, f.rgb);
        ft[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, f.y_begin * area.hs]);
        let j = (256 + i) * 4;
        ft[j..j + 4].copy_from_slice(&[f.transp, f.limit, if f.mortal { 1.0 } else { 0.0 }, 0.0]);
    }
    let ft: Vec<u8> = ft.iter().flat_map(|v| v.to_le_bytes()).collect();
    let fog_table = textures::table(256, 2, TextureFormat::Rgba32Float, ft);

    let mut skymap = textures::table(128, 128, TextureFormat::R8Unorm, area.rsc.skymap.clone());
    skymap.sampler = textures::sampler(2, true);

    let sky = textures::texture(256, area.rsc.sky(area.opt), false, 2, true);

    WorldImages {
        textures: images.add(tex),
        cells: images.add(cells),
        fog_map: images.add(fog_map),
        fog_table: images.add(fog_table),
        skymap: images.add(skymap),
        sky: images.add(sky),
    }
}

// ---------------------------------------------------------------------------
// Ground
// ---------------------------------------------------------------------------

fn terrain_chunk(area: &Area, x0: i32, z0: i32) -> Mesh {
    let n = area.size;
    let x1 = (x0 + CHUNK).min(n - 1);
    let z1 = (z0 + CHUNK).min(n - 1);
    let w = (x1 - x0 + 1) as usize;
    let mut pos = Vec::new();
    let mut attr = Vec::new();
    let c1 = area.engine == Engine::C1;
    let mut lowest = f32::MAX;
    for z in z0..=z1 {
        for x in x0..=x1 {
            let i = area.idx(x, z);
            let surface = area.surface_steps(x, z) as f32 * area.hs;
            pos.push([(x * 256) as f32, surface, (z * 256) as f32]);
            let water = c1 && area.map.deep_water(i);
            // Carnivores draws the ground at the bottom of its lakes while
            // the hunter is under water.
            let bottom = if c1 {
                (area.map.hmap2[i] as f32 - 48.0) * area.hs
            } else {
                surface
            };
            lowest = lowest.min(bottom);
            attr.push([
                area.map.lmap[i] as f32,
                area.random[((z & 31) * 32 + (x & 31)) as usize] as f32,
                if water { 1.0 } else { 0.0 },
                bottom,
            ]);
        }
    }
    // Not part of any triangle: it only takes the mesh's bounds down to the
    // lake bottom, so the view does not cull the chunk while it is drawn.
    if c1 {
        pos.push([(x0 * 256) as f32, lowest, (z0 * 256) as f32]);
        attr.push([0.0, 0.0, 0.0, lowest]);
    }
    let mut idx = Vec::new();
    for z in z0..z1 {
        for x in x0..x1 {
            let a = ((z - z0) as usize * w + (x - x0) as usize) as u32;
            let b = a + 1;
            let c = a + w as u32;
            let d = c + 1;
            if area.map.reverse(area.idx(x, z)) {
                idx.extend_from_slice(&[a, c, b, c, d, b]);
            } else {
                idx.extend_from_slice(&[a, d, b, a, c, d]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(ATTR_TERRAIN, attr)
    .with_inserted_indices(Indices::U32(idx))
}

fn water_chunk(area: &Area, x0: i32, z0: i32) -> Option<Mesh> {
    let n = area.size;
    let x1 = (x0 + CHUNK).min(n - 1);
    let z1 = (z0 + CHUNK).min(n - 1);
    let w = (x1 - x0 + 1) as usize;
    let m = &area.map;
    let water = |x: i32, z: i32| m.water(area.idx(x, z));
    let mut idx = Vec::new();
    for z in z0..z1 {
        for x in x0..x1 {
            if water(x, z) && water(x + 1, z) && water(x, z + 1) && water(x + 1, z + 1) {
                let a = ((z - z0) as usize * w + (x - x0) as usize) as u32;
                let b = a + 1;
                let c = a + w as u32;
                let d = c + 1;
                idx.extend_from_slice(&[a, d, b, a, c, d]);
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    let mut pos = Vec::new();
    let mut attr = Vec::new();
    for z in z0..=z1 {
        for x in x0..=x1 {
            let i = area.idx(x, z);
            let wi = m.wmap[i];
            let we = area
                .rsc
                .waters
                .get(wi as usize)
                .copied()
                .unwrap_or_default();
            pos.push([
                (x * 256) as f32,
                we.level as f32 * area.hs,
                (z * 256) as f32,
            ]);
            attr.push([
                (we.level - m.hmap[i] as i32) as f32,
                we.transp,
                area.random[((z & 31) * 32 + (x & 31)) as usize] as f32,
                if m.deep_water(i) { 1.0 } else { 0.0 },
            ]);
        }
    }
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(ATTR_WATER, attr)
        .with_inserted_indices(Indices::U32(idx)),
    )
}

/// Carnivores' water seen from below: the lakes' surface over a hunter who
/// has dived, drawn only then. The triangles are the ground's own wherever
/// they carry the water texture (0) and touch a lake vertex, one whose
/// surface is above its bottom.
fn c1_water_chunk(area: &Area, x0: i32, z0: i32) -> Option<Mesh> {
    let n = area.size;
    let x1 = (x0 + CHUNK).min(n - 1);
    let z1 = (z0 + CHUNK).min(n - 1);
    let w = (x1 - x0 + 1) as usize;
    let m = &area.map;
    let lake = |x: i32, z: i32| {
        let i = area.idx(x, z);
        m.hmap2[i] as i32 != m.hmap[i] as i32 + 48
    };
    let mut idx = Vec::new();
    for z in z0..z1 {
        for x in x0..x1 {
            if !(lake(x, z) || lake(x + 1, z) || lake(x, z + 1) || lake(x + 1, z + 1)) {
                continue;
            }
            let i = area.idx(x, z);
            let a = ((z - z0) as usize * w + (x - x0) as usize) as u32;
            let b = a + 1;
            let c = a + w as u32;
            let d = c + 1;
            let (first, second) = if m.reverse(i) {
                ([a, c, b], [c, d, b])
            } else {
                ([a, d, b], [a, c, d])
            };
            if m.tmap1[i] == 0 {
                idx.extend_from_slice(&first);
            }
            if m.tmap2[i] == 0 {
                idx.extend_from_slice(&second);
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    let mut pos = Vec::new();
    let mut attr = Vec::new();
    for z in z0..=z1 {
        for x in x0..=x1 {
            let i = area.idx(x, z);
            let on_lake = lake(x, z);
            pos.push([
                (x * 256) as f32,
                area.surface_steps(x, z) as f32 * area.hs,
                (z * 256) as f32,
            ]);
            attr.push([
                if on_lake { 1.0 } else { 0.0 },
                if on_lake && m.deep_water(i) { 1.0 } else { 0.0 },
                area.random[((z & 31) * 32 + (x & 31)) as usize] as f32,
                m.lmap[i] as f32,
            ]);
        }
    }
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(ATTR_WATER, attr)
        .with_inserted_indices(Indices::U32(idx)),
    )
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// A model as a mesh: three vertices per face (each face has its own
/// texture coordinates), with each vertex's light and the face's kind
/// (colour-keyed, one-sided).
pub fn model_mesh(
    model: &Model,
    positions: Option<&[[f32; 3]]>,
    vlight: &dyn Fn(usize) -> f32,
) -> Mesh {
    let mut pos = Vec::with_capacity(model.faces.len() * 3);
    let mut uv = Vec::with_capacity(model.faces.len() * 3);
    let mut attr = Vec::with_capacity(model.faces.len() * 3);
    for f in &model.faces {
        let kind = crate::render::face_kind(f);
        for k in 0..3 {
            let v = f.v[k] as usize;
            pos.push(positions.map(|p| p[v]).unwrap_or(model.vertices[v].pos));
            uv.push(f.uv[k]);
            attr.push([vlight(v), kind]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(ATTR_OBJECT, attr)
}

/// A model's texture: 256 wide, padded to square.
pub fn model_image(model: &Model, quality: i32) -> Image {
    let mut px = model.texture.clone();
    px.resize(256 * 256, 0);
    textures::texture(256, &px, model.has_cutouts(), quality, false)
}

// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn spawn_area(
    commands: &mut Commands,
    area: &Area,
    settings: &Settings,
    imgs: &WorldImages,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    terrain_mats: &mut Assets<TerrainMaterial>,
    object_mats: &mut Assets<ObjectMaterial>,
    water_mats: &mut Assets<WaterMaterial>,
    sky_mats: &mut Assets<SkyMaterial>,
    sprite_mats: &mut Assets<SpriteMaterial>,
) -> AnimatedObjects {
    let n = area.size;
    let rev_bit = match area.engine {
        Engine::C1 => C1_FM_REVERSE,
        Engine::C2 => C2_FM_REVERSE,
    } as f32;
    let c1 = if area.engine == Engine::C1 { 1.0 } else { 0.0 };
    let params = Vec4::new(area.rsc.textures.len().max(1) as f32, rev_bit, n as f32, c1);
    let frame = Vec4::new(if settings.clouds { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0);
    let terrain_mat = terrain_mats.add(TerrainMaterial {
        textures: imgs.textures.clone(),
        fog_map: imgs.fog_map.clone(),
        fog_table: imgs.fog_table.clone(),
        sky: imgs.sky.clone(),
        cells: imgs.cells.clone(),
        skymap: imgs.skymap.clone(),
        params,
        frame,
    });
    let water_mat = water_mats.add(WaterMaterial {
        textures: imgs.textures.clone(),
        fog_map: imgs.fog_map.clone(),
        fog_table: imgs.fog_table.clone(),
        sky: imgs.sky.clone(),
        cells: imgs.cells.clone(),
        params: Vec4::new(params.x, params.y, params.z, area.hs),
        frame: Vec4::new(frame.x, frame.y, c1, 0.0),
    });
    commands.insert_resource(WorldMaterials {
        terrain: terrain_mat.clone(),
        water: water_mat.clone(),
    });

    let chunk_radius = CHUNK as f32 * 256.0 * std::f32::consts::FRAC_1_SQRT_2;
    let mut z0 = 0;
    while z0 < n - 1 {
        let mut x0 = 0;
        while x0 < n - 1 {
            let center = Vec2::new(
                (x0 + CHUNK / 2) as f32 * 256.0,
                (z0 + CHUNK / 2) as f32 * 256.0,
            );
            commands.spawn((
                Mesh3d(meshes.add(terrain_chunk(area, x0, z0))),
                MeshMaterial3d(terrain_mat.clone()),
                Transform::IDENTITY,
                Chunk {
                    center,
                    radius: chunk_radius,
                },
                AreaEntity,
            ));
            let wm = match area.engine {
                Engine::C1 => c1_water_chunk(area, x0, z0),
                Engine::C2 => water_chunk(area, x0, z0),
            };
            if let Some(wm) = wm {
                commands.spawn((
                    Mesh3d(meshes.add(wm)),
                    MeshMaterial3d(water_mat.clone()),
                    Transform::IDENTITY,
                    Chunk {
                        center,
                        radius: chunk_radius,
                    },
                    AreaEntity,
                ));
            }
            x0 += CHUNK;
        }
        z0 += CHUNK;
    }

    // Objects: a material per kind, a mesh per kind and quarter-turn (the
    // sun light baked into the vertices depends on the turn).
    let lod = settings.object_lod(area.kind);
    let obj_mats: Vec<Handle<ObjectMaterial>> = area
        .rsc
        .objects
        .iter()
        .map(|o| {
            object_mats.add(ObjectMaterial {
                texture: images.add(model_image(&o.model, settings.textures)),
                fog_map: imgs.fog_map.clone(),
                fog_table: imgs.fog_table.clone(),
                sky: imgs.sky.clone(),
                lod: Vec4::new(lod, 0.0, 0.0, 0.0),
            })
        })
        .collect();
    // Past the object distance, the kinds with a sprite are drawn as it.
    let has_sprite = |o: &carn_formats::rsc::RscObject| {
        lod > 0.0 && o.bmp.is_some() && o.info.flags & OF_NO_BMP == 0
    };
    let sprite_mats: Vec<Option<Handle<SpriteMaterial>>> = area
        .rsc
        .objects
        .iter()
        .map(|o| {
            let b = o.bmp.as_ref().filter(|_| has_sprite(o))?;
            Some(sprite_mats.add(SpriteMaterial {
                texture: images.add(textures::texture(
                    128,
                    &b.texture,
                    true,
                    settings.textures,
                    false,
                )),
                fog_map: imgs.fog_map.clone(),
                fog_table: imgs.fog_table.clone(),
                sky: imgs.sky.clone(),
                lod: Vec4::new(lod, 0.0, 0.0, 0.0),
            }))
        })
        .collect();
    // Per chunk and kind: the feet and lights of its sprites.
    let mut sprites: std::collections::HashMap<(usize, usize), Vec<(Vec3, f32)>> =
        Default::default();
    let mut obj_meshes: Vec<[Option<Handle<Mesh>>; 4]> =
        vec![Default::default(); area.rsc.objects.len()];
    let mut animated = AnimatedObjects::default();

    let groups = (n + CHUNK - 1) / CHUNK;
    let mut group_entities = vec![Entity::PLACEHOLDER; (groups * groups) as usize];
    for gz in 0..groups {
        for gx in 0..groups {
            let center = Vec2::new(
                ((gx * CHUNK) + CHUNK / 2) as f32 * 256.0,
                ((gz * CHUNK) + CHUNK / 2) as f32 * 256.0,
            );
            group_entities[(gz * groups + gx) as usize] = commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::default(),
                    Chunk {
                        center,
                        radius: chunk_radius + 2048.0,
                    },
                    AreaEntity,
                ))
                .id();
        }
    }

    for z in 0..n {
        for x in 0..n {
            let i = area.idx(x, z);
            let ob = area.map.omap[i];
            if ob == 255 {
                continue;
            }
            let ob = ob as usize;
            let o = &area.rsc.objects[ob];
            let ground_lit = area.engine == Engine::C2 && o.info.flags & OF_GRND_LIGHT != 0;
            // Every object takes its cell's quarter-turn, the ground-lit
            // ones included.
            let turn = area.map.object_turn(i);
            let px = (x * 256 + 128) as f32;
            let pz = (z * 256 + 128) as f32;
            let rotation = Quat::from_rotation_y(turn as f32 * FRAC_PI_2);
            let transform = Transform::from_xyz(px, area.land_oh(x, z), pz).with_rotation(rotation);
            let mesh = if ground_lit {
                // Lit vertex by vertex from the ground under the turned
                // vertex, so every one is its own mesh.
                let m = model_mesh(&o.model, None, &|v| {
                    let p = rotation * Vec3::from(o.model.vertices[v].pos);
                    area.land_light(p.x + px, p.z + pz) - 128.0
                });
                meshes.add(m)
            } else {
                let vt = if area.engine == Engine::C1 {
                    0
                } else {
                    turn as usize
                };
                obj_meshes[ob][vt]
                    .get_or_insert_with(|| {
                        let vl = &area.extra[ob].vlight[vt];
                        let h = meshes.add(model_mesh(&o.model, None, &|v| vl[v]));
                        if o.info.flags & OF_ANIMATED != 0 && o.anim.is_some() {
                            animated.0.push((ob, vt as u32, h.clone()));
                        }
                        h
                    })
                    .clone()
            };
            let light = area.object_light(x, z);
            let gi = ((z / CHUNK) * groups + x / CHUNK) as usize;
            let g = group_entities[gi];
            let mut tag = light as u32;
            if has_sprite(o) {
                tag |= TAG_HAS_SPRITE;
                // Drawn a little darker than the model (mlight - 16).
                sprites
                    .entry((gi, ob))
                    .or_default()
                    .push((transform.translation, (light - 16.0).max(0.0) / 255.0));
            }
            let e = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(obj_mats[ob].clone()),
                    transform,
                    MeshTag(tag),
                    MapObject,
                ))
                .id();
            commands.entity(g).add_child(e);
        }
    }
    for ((gi, ob), list) in sprites {
        let (Some(b), Some(mat)) = (area.rsc.objects[ob].bmp.as_ref(), sprite_mats[ob].clone())
        else {
            continue;
        };
        let uv = [[0.0, 0.0], [0.995, 0.0], [0.995, 0.995], [0.0, 0.995]];
        let (mut pos, mut uvs, mut attr, mut idx) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (foot, light) in list {
            let base = pos.len() as u32;
            for (k, c) in b.quad.iter().enumerate() {
                pos.push(foot.to_array());
                uvs.push(uv[k]);
                attr.push([c[0], c[1], light]);
            }
            idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let m = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(ATTR_SPRITE, attr)
        .with_inserted_indices(Indices::U32(idx));
        let e = commands
            .spawn((
                Mesh3d(meshes.add(m)),
                MeshMaterial3d(mat),
                Transform::IDENTITY,
                NoFrustumCulling,
                MapObject,
            ))
            .id();
        commands.entity(group_entities[gi]).add_child(e);
    }

    // The sky plane, kept over the camera.
    let s = 1.0e6f32;
    let sky_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-s, 0.0, -s], [s, 0.0, -s], [s, 0.0, s], [-s, 0.0, s]],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    commands.spawn((
        Mesh3d(meshes.add(sky_mesh)),
        MeshMaterial3d(sky_mats.add(SkyMaterial {
            fog_map: imgs.fog_map.clone(),
            fog_table: imgs.fog_table.clone(),
            sky: imgs.sky.clone(),
        })),
        Transform::IDENTITY,
        NoFrustumCulling,
        SkyPlane,
        AreaEntity,
    ));

    animated
}

/// Hides the chunks past the view distance.
pub fn cull_chunks(
    cam: Query<&GlobalTransform, With<crate::MainCamera>>,
    view: Res<crate::ViewState>,
    mut chunks: Query<(&Chunk, &mut Visibility)>,
) {
    let Ok(cam) = cam.single() else { return };
    let c = cam.translation();
    let c = Vec2::new(c.x, c.z);
    let reach = view.globals.fade_end;
    for (ch, mut vis) in chunks.iter_mut() {
        let want = if ch.center.distance(c) - ch.radius <= reach {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}

pub fn follow_sky(
    cam: Query<&GlobalTransform, (With<crate::MainCamera>, Without<SkyPlane>)>,
    mut sky: Query<&mut Transform, With<SkyPlane>>,
) {
    let Ok(cam) = cam.single() else { return };
    let c = cam.translation();
    for mut t in sky.iter_mut() {
        t.translation = Vec3::new(c.x, c.y + 32768.0, c.z);
    }
}

/// Re-poses the animated objects' meshes.
pub fn animate_objects(
    area: Option<Res<Area>>,
    anim: Res<AnimatedObjects>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(area) = area else { return };
    if anim.0.is_empty() {
        return;
    }
    let t = (time.elapsed_secs_f64() * 1000.0) as i64;
    let mut buf = Vec::new();
    for (ob, vt, h) in &anim.0 {
        let o = &area.rsc.objects[*ob];
        let Some(a) = &o.anim else { continue };
        if a.ani_time <= 0 {
            continue;
        }
        a.sample(
            o.model.vertices.len(),
            (t % a.ani_time as i64) as i32,
            1.0,
            &mut buf,
        );
        if buf.len() != o.model.vertices.len() {
            continue;
        }
        let Some(mesh) = meshes.get_mut(h) else {
            continue;
        };
        let pos: Vec<[f32; 3]> = o
            .model
            .faces
            .iter()
            .flat_map(|f| f.v.map(|v| buf[v as usize]))
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        let _ = vt;
    }
}

/// Pushes the few per-frame values the vertex shaders need into the ground
/// and water materials, only when they change.
pub fn update_frame_values(
    mats: Option<Res<WorldMaterials>>,
    player: Option<Res<crate::player::Player>>,
    settings: Res<Settings>,
    mut terrain: ResMut<Assets<TerrainMaterial>>,
    mut water: ResMut<Assets<WaterMaterial>>,
) {
    let (Some(mats), Some(player)) = (mats, player) else {
        return;
    };
    // x and y change from frame to frame; z and w are set once.
    let xy = Vec2::new(
        if settings.clouds { 1.0 } else { 0.0 },
        if player.underwater { 1.0 } else { 0.0 },
    );
    if terrain
        .get(&mats.terrain)
        .map(|m| m.frame.xy() != xy)
        .unwrap_or(false)
    {
        if let Some(m) = terrain.get_mut(&mats.terrain) {
            m.frame = xy.extend(m.frame.z).extend(m.frame.w);
        }
    }
    if water
        .get(&mats.water)
        .map(|m| m.frame.xy() != xy)
        .unwrap_or(false)
    {
        if let Some(m) = water.get_mut(&mats.water) {
            m.frame = xy.extend(m.frame.z).extend(m.frame.w);
        }
    }
}

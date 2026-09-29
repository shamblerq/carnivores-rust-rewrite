//! Drawing the effects the simulation keeps (sim/fx.rs): particles, blood
//! spots and snow as small discs facing the view, rings on
//! the water, and the first game's flash where a shot lands.
//!
//! Each kind is one mesh in world space, rebuilt every frame.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::view::NoFrustumCulling;
use carn_formats::car::CharacterInfo;

use crate::game::GameKind;
use crate::paths::DataRoot;
use crate::render::{ObjectMaterial, ParticleMaterial, RingMaterial, ATTR_FX, ATTR_OBJECT};
use crate::settings::Settings;
use crate::sim::Sim;
use crate::world::{model_image, AreaEntity, WorldImages};
use crate::MainCamera;

/// The discs are R * 0.64 units across the games' radius R.
const DISC_K: f32 = 0.64;

#[derive(Resource)]
pub struct FxAssets {
    discs: (Entity, Handle<Mesh>),
    rings: Option<(Entity, Handle<Mesh>, Arc<CharacterInfo>)>,
    flash: Option<(Entity, Handle<Mesh>, Arc<CharacterInfo>)>,
}

fn empty_mesh(indexed: bool) -> Mesh {
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    if indexed {
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
        m.insert_indices(Indices::U32(vec![0, 1, 2]));
    }
    m
}

fn load(root: &DataRoot, rel: &str) -> Option<Arc<CharacterInfo>> {
    let p = root.find(rel)?;
    CharacterInfo::load(&p, None)
        .map_err(|e| warn!("{e}"))
        .ok()
        .map(Arc::new)
}

/// A model drawn many times in one mesh: its texture coordinates and face
/// flags repeated, positions filled in per frame.
fn model_instances(ci: &CharacterInfo, n: usize) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let mut uv = Vec::new();
    let mut attr = Vec::new();
    for _ in 0..n {
        for f in &ci.model.faces {
            let kind = crate::render::face_kind(f);
            for k in 0..3 {
                uv.push(f.uv[k]);
                attr.push([0.0, kind]);
            }
        }
    }
    (uv, attr)
}

/// One disc facing the view: a centre and eight points
/// around it, the diagonals a little in as the games had them.
#[allow(clippy::too_many_arguments)]
fn disc(
    pos: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    c: Vec3,
    r: f32,
    right: Vec3,
    up: Vec3,
    c1: [f32; 4],
    c2: [f32; 4],
) {
    let base = pos.len() as u32;
    let r2 = r * 0.65;
    pos.push(c.into());
    col.push(c1);
    for (dx, dy) in [
        (0.0, r),
        (r2, r2),
        (r, 0.0),
        (r2, -r2),
        (0.0, -r),
        (-r2, -r2),
        (-r, 0.0),
        (-r2, r2),
    ] {
        pos.push((c + right * dx + up * dy).into());
        col.push(c2);
    }
    for k in 0..8 {
        idx.extend([base, base + 1 + k, base + 1 + (k + 1) % 8]);
    }
}

fn rgba(rgb: [u8; 3], a: i32) -> [f32; 4] {
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        a.clamp(0, 255) as f32 / 255.0,
    ]
}

fn show(vis: &mut Query<&mut Visibility, With<AreaEntity>>, e: Entity, on: bool) {
    if let Ok(mut v) = vis.get_mut(e) {
        let want = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    sim: Option<ResMut<Sim>>,
    fx: Option<Res<FxAssets>>,
    root: Res<DataRoot>,
    kind: Res<GameKind>,
    settings: Res<Settings>,
    imgs: Option<Res<WorldImages>>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut particle_mats: ResMut<Assets<ParticleMaterial>>,
    mut ring_mats: ResMut<Assets<RingMaterial>>,
    mut object_mats: ResMut<Assets<ObjectMaterial>>,
    mut vis: Query<&mut Visibility, With<AreaEntity>>,
    mut buf: Local<Vec<[f32; 3]>>,
) {
    let Some(mut sim) = sim else { return };
    let Some(fx) = fx else {
        // The first frame of a hunt: load the models and make the entities.
        let Some(imgs) = imgs else { return };
        let mesh = meshes.add(empty_mesh(true));
        let e = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(particle_mats.add(ParticleMaterial::default())),
                Transform::IDENTITY,
                NoFrustumCulling,
                Visibility::Hidden,
                AreaEntity,
            ))
            .id();
        let discs = (e, mesh);
        let rings = if *kind == GameKind::Carnivores {
            None
        } else {
            load(&root, "HUNTDAT/WCIRCLE2.CAR")
        };
        let rings = rings.map(|ci| {
            let mesh = meshes.add(empty_mesh(false));
            let mat = ring_mats.add(RingMaterial {
                texture: images.add(model_image(&ci.model, settings.textures)),
            });
            let e = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::IDENTITY,
                    NoFrustumCulling,
                    Visibility::Hidden,
                    AreaEntity,
                ))
                .id();
            (e, mesh, ci)
        });
        let flash = if *kind == GameKind::Carnivores {
            load(&root, "HUNTDAT/EXPLO.CAR")
        } else {
            None
        };
        let flash = flash.map(|ci| {
            sim.fx.flash_time = ci.animations.first().map(|a| a.ani_time).unwrap_or(0);
            let mesh = meshes.add(empty_mesh(false));
            let mat = object_mats.add(ObjectMaterial {
                texture: images.add(model_image(&ci.model, settings.textures)),
                fog_map: imgs.fog_map.clone(),
                fog_table: imgs.fog_table.clone(),
                sky: imgs.sky.clone(),
                lod: Vec4::ZERO,
            });
            let e = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::IDENTITY,
                    // The first game drew it at full light.
                    MeshTag(255),
                    NoFrustumCulling,
                    Visibility::Hidden,
                    AreaEntity,
                ))
                .id();
            (e, mesh, ci)
        });
        commands.insert_resource(FxAssets {
            discs,
            rings,
            flash,
        });
        return;
    };
    let Ok(cam) = cam.single() else { return };
    let (_, rot, eye) = cam.to_scale_rotation_translation();
    let (right, up, fwd) = (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::NEG_Z);
    let far = 256.0 * sim.view_r as f32;
    // Too close to the eye, behind it, or past the view: not drawn.
    let seen = |p: Vec3| {
        let d = (p - eye).dot(fwd);
        d > 64.0 && d < far
    };

    // ---- discs
    let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for b in sim.fx.bursts.iter() {
        let (c1, c2) = (rgba(b.rgb, b.a1), rgba(b.rgb2, b.a2));
        for p in b.parts.iter().filter(|p| seen(p.pos)) {
            disc(
                &mut pos,
                &mut col,
                &mut idx,
                p.pos,
                p.r * DISC_K,
                right,
                up,
                c1,
                c2,
            );
        }
    }
    for s in sim.fx.blood.iter().filter(|s| seen(s.pos)) {
        let a1 = (0xE0 * s.ltime / 20000).min(0xE0);
        let a2 = (0x20 * s.ltime / 20000).min(0x20);
        let (c1, c2) = (rgba([0x70, 0, 0], a1), rgba([0x30, 0, 0], a2));
        disc(
            &mut pos,
            &mut col,
            &mut idx,
            s.pos,
            12.0 * DISC_K,
            right,
            up,
            c1,
            c2,
        );
    }
    for f in sim.fx.snow.iter().filter(|f| seen(f.pos)) {
        let (mut a1, mut a2) = (0xFF, 0x30);
        if f.ftime > 0 {
            a1 = a1 * (2000 - f.ftime).max(0) / 2000;
            a2 = a2 * (2000 - f.ftime).max(0) / 2000;
        }
        let (c1, c2) = (rgba([0xF0; 3], a1), rgba([0xB0; 3], a2));
        disc(
            &mut pos,
            &mut col,
            &mut idx,
            f.pos,
            8.0 * DISC_K,
            right,
            up,
            c1,
            c2,
        );
    }
    let (e, mesh) = &fx.discs;
    show(&mut vis, *e, !idx.is_empty());
    if !idx.is_empty() {
        if let Some(m) = meshes.get_mut(mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            m.insert_indices(Indices::U32(idx));
        }
    }

    // ---- rings on the water
    if let Some((e, mesh, ci)) = &fx.rings {
        let rings: Vec<_> = sim
            .fx
            .rings
            .iter()
            .filter(|r| (r.pos - eye).length() < far)
            .copied()
            .collect();
        show(&mut vis, *e, !rings.is_empty());
        if let (false, Some(an)) = (rings.is_empty(), ci.animations.first()) {
            let vc = ci.model.vertices.len();
            let mut pos: Vec<[f32; 3]> = Vec::with_capacity(rings.len() * ci.model.faces.len() * 3);
            let mut alpha = Vec::with_capacity(pos.capacity());
            for r in rings.iter() {
                an.sample(vc, r.ftime, r.scale, &mut buf);
                if buf.len() != vc {
                    buf.clear();
                    buf.extend(ci.model.vertices.iter().map(|v| v.pos));
                }
                // Fades out over its last 2000 ms.
                let a = ((2000 - r.ftime).max(0) / 38) as f32 / 255.0;
                for f in &ci.model.faces {
                    for v in f.v {
                        pos.push((r.pos + Vec3::from(buf[v as usize])).into());
                        alpha.push(a);
                    }
                }
            }
            let (uv, _) = model_instances(ci, rings.len());
            if let Some(m) = meshes.get_mut(mesh) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
                m.insert_attribute(ATTR_FX, alpha);
            }
        }
    }

    // ---- the first game's flash, square to the view
    if let Some((e, mesh, ci)) = &fx.flash {
        let flashes = sim.fx.flashes.clone();
        show(&mut vis, *e, !flashes.is_empty());
        if let (false, Some(an)) = (flashes.is_empty(), ci.animations.first()) {
            let vc = ci.model.vertices.len();
            let mut pos: Vec<[f32; 3]> = Vec::new();
            for f in flashes.iter() {
                an.sample(vc, f.ftime, 1.0, &mut buf);
                if buf.len() != vc {
                    buf.clear();
                    buf.extend(ci.model.vertices.iter().map(|v| v.pos));
                }
                for fc in &ci.model.faces {
                    for v in fc.v {
                        // The flash's own axes are the view's, as the
                        // game drew it (in camera
                        // space), so its faces wind the same way.
                        let [x, y, z] = buf[v as usize];
                        pos.push((f.pos + rot * Vec3::new(x, y, z)).into());
                    }
                }
            }
            let (uv, attr) = model_instances(ci, flashes.len());
            if let Some(m) = meshes.get_mut(mesh) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
                m.insert_attribute(ATTR_OBJECT, attr);
            }
        }
    }
}

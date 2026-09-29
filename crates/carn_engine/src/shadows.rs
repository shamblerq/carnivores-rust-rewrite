//! The creatures' shadows:
//! each posed model flattened onto the ground away from the sun, as a
//! translucent dark shape.
//!
//! Every shadow in view goes into one mesh in world space, rebuilt each
//! frame; render/shaders/shadow.wgsl explains how they are drawn.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::view::NoFrustumCulling;

use crate::area::Area;
use crate::render::{ShadowMaterial, ShadowResolveMaterial, ATTR_FX};
use crate::settings::Settings;
use crate::sim::Sim;
use crate::world::AreaEntity;

#[derive(Resource)]
pub struct Shadows {
    mesh: Handle<Mesh>,
    entity: Entity,
    /// The same shapes again, each in its shadow's colour, to darken the
    /// frame by what the first pass left (shadow_resolve.wgsl).
    resolve_mesh: Handle<Mesh>,
    resolve: Entity,
}

#[derive(Component)]
pub struct ShadowMesh;

fn triangle_mesh(pos: Vec<[f32; 3]>, light: Option<Vec<f32>>) -> Mesh {
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    if let Some(l) = light {
        m.insert_attribute(ATTR_FX, l);
    }
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m
}

/// A value as the games stored it (0..1, gamma) to linear.
fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A shadow's colour, linear. The first game's were black. Its successors
/// drew them in 0x222222 with the creature's own texture still bound and
/// every vertex at texture coordinate (0, 0), so the colour is that times
/// what the texture shows there: its four corners blended, the texture
/// repeating and filtered (the rows past its end are black). For almost
/// every creature that is black or within a few steps of it.
fn shadow_color(model: &carn_formats::model::Model, c1: bool) -> [f32; 4] {
    if c1 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let t = &model.texture;
    let texel = |x: usize, y: usize| t.get(y * 256 + x).copied().unwrap_or(0);
    let corners = [texel(0, 0), texel(255, 0), texel(0, 255), texel(255, 255)];
    let channel = |shift: u16| {
        corners
            .iter()
            .map(|&v| ((v >> shift) & 31) as f32 / 31.0)
            .sum::<f32>()
            / 4.0
    };
    let k = 34.0 / 255.0;
    [
        to_linear(channel(10) * k),
        to_linear(channel(5) * k),
        to_linear(channel(0) * k),
        1.0,
    ]
}

/// How far a shadow reaches from each vertex's height: the
/// first game's sun is fixed; the others' depends on the time of day, and
/// at night it throws them the other way.
fn shadow_k(c1: bool, day_night: i32) -> f32 {
    if c1 {
        return 0.5;
    }
    match day_night {
        0 => 0.7,
        2 => -0.7,
        _ => 0.5,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    sim: Option<Res<Sim>>,
    area: Option<Res<Area>>,
    settings: Res<Settings>,
    shadows: Option<Res<Shadows>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shadow_mats: ResMut<Assets<ShadowMaterial>>,
    mut resolve_mats: ResMut<Assets<ShadowResolveMaterial>>,
    mut vis: Query<&mut Visibility, With<ShadowMesh>>,
    mut buf: Local<Vec<[f32; 3]>>,
    mut flat: Local<Vec<[f32; 3]>>,
) {
    let (Some(sim), Some(area)) = (sim, area) else {
        return;
    };
    let Some(shadows) = shadows else {
        // The first frame of a hunt: make the two entities.
        let mesh = meshes.add(triangle_mesh(vec![[0.0; 3]; 3], Some(vec![1.0; 3])));
        let entity = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(shadow_mats.add(ShadowMaterial::default())),
                Transform::IDENTITY,
                NoFrustumCulling,
                Visibility::Hidden,
                ShadowMesh,
                AreaEntity,
            ))
            .id();
        let resolve_mesh = meshes.add(
            triangle_mesh(vec![[0.0; 3]; 3], None)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32, 0.0, 0.0, 1.0]; 3]),
        );
        let resolve = commands
            .spawn((
                Mesh3d(resolve_mesh.clone()),
                MeshMaterial3d(resolve_mats.add(ShadowResolveMaterial::default())),
                Transform::IDENTITY,
                NoFrustumCulling,
                Visibility::Hidden,
                ShadowMesh,
                AreaEntity,
            ))
            .id();
        commands.insert_resource(Shadows {
            mesh,
            entity,
            resolve_mesh,
            resolve,
        });
        return;
    };

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut light: Vec<f32> = Vec::new();
    let mut color: Vec<[f32; 4]> = Vec::new();
    if settings.shadows {
        let k = shadow_k(sim.c1, sim.day_night);
        // The blend happens on linear values; the games blended the stored
        // ones. For a black shadow - near enough every one - this power is
        // exactly the darkening they showed.
        let gamma = 2.2;
        let cam = sim.hunter.cam;
        let reach = 256.0 * (sim.view_r - 8) as f32;
        for c in sim.chars.iter() {
            if c.removed || (c.pos - cam).length() > reach {
                continue;
            }
            let Some(info) = sim.info(c) else { continue };
            let mut al = 0x60;
            if c.health == 0 {
                if sim.tranq || (sim.c1 && c.ctype == crate::sim::c1::HUNTER_CTYPE) {
                    continue;
                }
                // A body's shadow fades as it falls, and is gone after.
                let at = sim.ani_time(c, c.phase);
                if c.ftime == at - 1 {
                    continue;
                }
                al = al * (at - c.ftime).max(0) / at;
            }
            if !sim.c1 && c.ai == 0 {
                al = 0x50;
            }
            let left = (1.0 - al as f32 / 255.0).powf(gamma);
            let col = shadow_color(&info.model, sim.c1);
            sim.morph(c, &mut buf);
            if buf.len() != info.model.vertices.len() {
                continue;
            }
            let rot = Quat::from_rotation_y(FRAC_PI_2 - c.alpha);
            flat.clear();
            flat.extend(buf.iter().map(|v| {
                let w = rot * Vec3::from(*v);
                let x = c.pos.x + w.x + v[1] * k;
                let z = c.pos.z + w.z + v[1] * k;
                [x, area.land_h(x, z), z]
            }));
            for f in info.model.faces.iter() {
                for v in f.v {
                    pos.push(flat[v as usize]);
                    light.push(left);
                    color.push(col);
                }
            }
        }
    }
    let show = !pos.is_empty();
    for e in [shadows.entity, shadows.resolve] {
        if let Ok(mut v) = vis.get_mut(e) {
            let want = if show {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *v != want {
                *v = want;
            }
        }
    }
    if show {
        if let Some(m) = meshes.get_mut(&shadows.resolve_mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone());
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, color);
        }
        if let Some(m) = meshes.get_mut(&shadows.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(ATTR_FX, light);
        }
    }
}

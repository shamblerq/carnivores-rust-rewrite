//! The creatures on screen: loading their models, and each frame running
//! the simulation and posing every creature's mesh from it.

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::mesh::MeshTag;
use carn_formats::car::CharacterInfo;

use crate::area::Area;
use crate::game::GameKind;
use crate::paths::DataRoot;
use crate::render::ObjectMaterial;
use crate::settings::Settings;
use crate::sim::{script, DinoInfo, Sim};
use crate::world::{model_image, model_mesh, AreaEntity, WorldImages};

/// Brightness a creature is drawn at: 210 in
/// Carnivores 2; the first game's darkness 10, which is 255 - 10 * 4.
fn creature_light(kind: GameKind) -> u32 {
    if kind == GameKind::Carnivores {
        215
    } else {
        210
    }
}

#[derive(Component)]
pub struct CreatureMesh {
    pub index: usize,
}

/// Per creature type: its material; and the ship's.
#[derive(Resource, Default)]
pub struct CreatureAssets {
    pub materials: Vec<Option<Handle<ObjectMaterial>>>,
    pub ship: Option<(Entity, Handle<Mesh>)>,
}

#[derive(Component)]
pub struct ShipMesh;

/// The creature list for this game: Carnivores 2 and Ice Age read it from
/// _RES.TXT; the first game kept it in code.
pub fn load_dinos(
    root: &DataRoot,
    kind: GameKind,
) -> Result<(Vec<DinoInfo>, script::Script), String> {
    if kind == GameKind::Carnivores {
        return Ok((crate::sim::c1::dinos(), script::Script::default()));
    }
    let p = root.find_or_err("HUNTDAT/_RES.TXT")?;
    let text = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let text: String = text.iter().map(|&b| b as char).collect();
    let mut sc = script::parse(&text);
    if kind == GameKind::IceAge {
        for d in sc.dinos.iter_mut() {
            crate::sim::c2ai::ice_layout(d);
        }
    }
    if sc.dinos.is_empty() {
        return Err(format!("{}: no characters", p.display()));
    }
    Ok((sc.dinos.clone(), sc))
}

pub fn load_models(
    root: &DataRoot,
    kind: GameKind,
    dinos: &[DinoInfo],
    settings: &Settings,
) -> Vec<Option<Arc<CharacterInfo>>> {
    let bright = kind
        .has_time_of_day()
        .then_some((settings.brightness, settings.day_night));
    dinos
        .iter()
        .map(|d| {
            if d.file.is_empty() {
                return None;
            }
            let p = root.find(&format!("HUNTDAT/{}", d.file))?;
            match CharacterInfo::load(&p, bright) {
                Ok(c) => Some(Arc::new(c)),
                Err(e) => {
                    warn!("{e}");
                    None
                }
            }
        })
        .collect()
}

/// Sets up the creatures for a hunt: the simulation, and a material per
/// creature type.
#[allow(clippy::too_many_arguments)]
pub fn setup(
    commands: &mut Commands,
    root: &DataRoot,
    kind: GameKind,
    area: &Area,
    settings: &Settings,
    hunter: crate::sim::HunterView,
    spawn: Option<(usize, f32)>,
    no_dinos: bool,
    hunt: Option<&crate::menu::HuntSetup>,
    bodies: Option<&[crate::profile::TrophyItem]>,
    imgs: &WorldImages,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<ObjectMaterial>,
) -> Result<Sim, String> {
    let (dinos, _script) = load_dinos(root, kind)?;
    let models = load_models(root, kind, &dinos, settings);
    let mut sim = Sim::new(kind, dinos, models);
    let bright = kind
        .has_time_of_day()
        .then_some((settings.brightness, settings.day_night));
    sim.ship_info = root
        .find("HUNTDAT/SHIP2A.CAR")
        .and_then(|p| {
            CharacterInfo::load(&p, bright)
                .map_err(|e| warn!("{e}"))
                .ok()
        })
        .map(Arc::new);
    sim.hunter = hunter;
    // (The first game's ran 0-2.)
    let top = if sim.c1 { 2 } else { 255 };
    sim.opt_agres = settings.agres.clamp(0, top);
    sim.opt_dens = settings.dens.clamp(0, top);
    sim.opt_sens = settings.sens.clamp(0, top);
    sim.init_fx(area, settings.brightness, 0);
    sim.view_r = crate::ViewRadius::of(kind, settings, false, &Default::default()).r;
    sim.day_night = settings.day_night;
    sim.trophy = area.trophy;
    sim.wind.alpha = sim.rng.r(1024) as f32 * 2.0 * std::f32::consts::PI / 1024.0;
    sim.wind.speed = 10.0;
    // Without the menu's choice, every huntable creature is in the hunt.
    sim.target_dino = hunt.map(|h| h.target_dino).unwrap_or(!0);
    if let Some(h) = hunt {
        sim.tranq = h.tranq;
        sim.observer = h.observer;
        sim.camo = h.camo;
        sim.scent = h.scent;
        sim.radar = h.radar;
    }
    if area.trophy {
        if let Some(b) = bodies {
            sim.place_trophies(area, b);
        }
    } else if !no_dinos {
        if sim.c1 {
            sim.place_characters_c1(area);
        } else {
            sim.place_characters_c2(area);
        }
    }
    if let Some((ctype, d)) = spawn {
        if ctype < sim.dinos.len() && sim.chinfo[ctype].is_some() {
            let h = &sim.hunter;
            let (sa, ca) = h.cam_alpha.sin_cos();
            let (x, z) = (h.x + sa * d, h.z - ca * d);
            let mut c = crate::sim::Character {
                ctype,
                ..Default::default()
            };
            c.pos = Vec3::new(x, area.land_h(x, z), z);
            c.alpha = h.cam_alpha + std::f32::consts::FRAC_PI_2;
            c.tgx = x;
            c.tgz = z;
            sim.reset_character(&mut c);
            info!("placed {} {:.0} units ahead", sim.dinos[ctype].name, d);
            sim.chars.push(c);
        }
    }
    info!("{} creatures", sim.chars.len());

    let materials = sim
        .chinfo
        .iter()
        .map(|ci| {
            ci.as_ref().map(|ci| {
                mats.add(ObjectMaterial {
                    texture: images.add(model_image(&ci.model, settings.textures)),
                    fog_map: imgs.fog_map.clone(),
                    fog_table: imgs.fog_table.clone(),
                    sky: imgs.sky.clone(),
                    lod: Vec4::ZERO,
                })
            })
        })
        .collect();
    let ship = sim.ship_info.as_ref().map(|ci| {
        let mesh = meshes.add(model_mesh(&ci.model, None, &|_| 0.0));
        let mat = mats.add(ObjectMaterial {
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
                MeshTag(creature_light(kind)),
                bevy::render::view::NoFrustumCulling,
                Visibility::Hidden,
                ShipMesh,
                AreaEntity,
            ))
            .id();
        (e, mesh)
    });
    commands.insert_resource(CreatureAssets { materials, ship });
    Ok(sim)
}

/// Keeps one entity per creature, posed from the simulation.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    sim: Option<Res<Sim>>,
    assets: Option<Res<CreatureAssets>>,
    view: Res<crate::ViewState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<
        (
            Entity,
            &CreatureMesh,
            &Mesh3d,
            &mut Transform,
            &mut Visibility,
        ),
        Without<ShipMesh>,
    >,
    mut ship_q: Query<(&mut Transform, &mut Visibility), With<ShipMesh>>,
    mut buf: Local<Vec<[f32; 3]>>,
    mut have: Local<Vec<bool>>,
) {
    let (Some(sim), Some(assets)) = (sim, assets) else {
        return;
    };
    if let (Some((e, mesh)), Some(ci)) = (&assets.ship, sim.ship_info.as_ref()) {
        if let Ok((mut t, mut vis)) = ship_q.get_mut(*e) {
            let sh = &sim.ship;
            let want = if sh.state == -1 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            if *vis != want {
                *vis = want;
            }
            if sh.state != -1 {
                t.translation = sh.pos;
                t.rotation = Quat::from_rotation_y(-sh.alpha - FRAC_PI_2);
                if let Some(an) = ci.animations.first() {
                    an.sample(ci.model.vertices.len(), sh.ftime, 1.0, &mut buf);
                    if buf.len() == ci.model.vertices.len() {
                        if let Some(m) = meshes.get_mut(mesh) {
                            let pos: Vec<[f32; 3]> = ci
                                .model
                                .faces
                                .iter()
                                .flat_map(|f| f.v.map(|v| buf[v as usize]))
                                .collect();
                            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                        }
                    }
                }
            }
        }
    }
    have.clear();
    have.resize(sim.chars.len(), false);
    let cam = Vec3::new(sim.hunter.cam.x, sim.hunter.cam.y, sim.hunter.cam.z);
    let reach = view.globals.fade_end + 1024.0;
    for (e, cm, mesh, mut t, mut vis) in q.iter_mut() {
        let Some(c) = sim.chars.get(cm.index) else {
            commands.entity(e).despawn();
            continue;
        };
        have[cm.index] = true;
        let far = (c.pos - cam).length() > reach;
        if c.removed || far {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
        t.translation = c.pos;
        t.rotation = Quat::from_rotation_y(FRAC_PI_2 - c.alpha);
        sim.morph(c, &mut buf);
        let Some(info) = sim.info(c) else { continue };
        if buf.len() != info.model.vertices.len() {
            continue;
        }
        if let Some(m) = meshes.get_mut(&mesh.0) {
            let pos: Vec<[f32; 3]> = info
                .model
                .faces
                .iter()
                .flat_map(|f| f.v.map(|v| buf[v as usize]))
                .collect();
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        }
    }
    for (i, c) in sim.chars.iter().enumerate() {
        if have[i] {
            continue;
        }
        let (Some(info), Some(Some(mat))) = (sim.info(c), assets.materials.get(c.ctype)) else {
            continue;
        };
        let mesh = meshes.add(model_mesh(&info.model, None, &|_| 0.0));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(c.pos),
            MeshTag(creature_light(sim.kind)),
            // The bounds would be the model's rest pose; the animation
            // moves it well outside them.
            bevy::render::view::NoFrustumCulling,
            Visibility::Hidden,
            CreatureMesh { index: i },
            AreaEntity,
        ));
    }
}

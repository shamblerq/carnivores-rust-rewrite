//! The hunter's weapons and gear: drawing, firing and reloading, where
//! the shots go and what they do, and the binoculars, compass and wind
//! gauge.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::mesh::MeshTag;
use bevy::render::view::{NoFrustumCulling, RenderLayers};
use carn_formats::car::CharacterInfo;
use carn_formats::model::{Model, SF_DOUBLE_SIDE};
use carn_formats::tga::Picture;

use crate::area::Area;
use crate::audio::{Mixer, Pcm};
use crate::game::GameKind;
use crate::overlay::{Overlay, OverlayProjection, NEAR_K};
use crate::paths::DataRoot;
use crate::player::Player;
use crate::render::{ObjectMaterial, WeaponMaterial, ATTR_OBJECT, ATTR_WEAPON_FX};
use crate::sim::script::WeapInfo;
use crate::sim::{Hit, Sim};
use crate::world::{model_image, model_mesh, AreaEntity, WorldImages};

#[derive(Resource)]
pub struct Arsenal {
    pub info: Vec<WeapInfo>,
    pub cars: Vec<Option<Arc<CharacterInfo>>>,
    pub sounds: Vec<Vec<Pcm>>,
    /// 0 put away, 1 being drawn, 2 ready (firing while its clock runs), 3 being put
    /// away, 4 reloading.
    pub state: i32,
    pub ftime: i32,
    /// How much the aim wanders; grows while the weapon is held up.
    pub shakel: f32,
    pub current: usize,
    pub target: usize,
    pub shots_left: Vec<i32>,
    pub ammo_mag: Vec<i32>,
    /// Looking through a scope.
    pub optic: bool,
    pub binoculars: bool,
    pub bin_power: f32,
    pub dalpha: f32,
    pub dbeta: f32,
    /// The compass and wind gauge (Caps Lock hides them).
    pub gauges: bool,
    morph: Vec<[f32; 3]>,
    /// Where the scope's cross is on screen, 0..1, when it shows.
    pub cross: Option<Vec2>,
}

impl Arsenal {
    /// How much the view is magnified: the binoculars, or a scope.
    pub fn zoom(&self) -> f32 {
        if self.binoculars {
            self.bin_power
        } else if self.optic {
            3.0
        } else {
            1.0
        }
    }

    fn ani_time(&self, w: usize, a: usize) -> i32 {
        self.cars
            .get(w)
            .and_then(|c| c.as_ref())
            .and_then(|c| c.animations.get(a))
            .map(|a| a.ani_time)
            .unwrap_or(0)
    }

    fn sound(&self, w: usize, i: usize) -> Option<&Pcm> {
        self.sounds.get(w).and_then(|s| s.get(i))
    }
}

/// The overlay models' entities and the HUD's pictures.
#[derive(Resource, Default)]
pub struct Gear {
    weapons: Vec<Option<(Entity, Handle<Mesh>)>>,
    /// The weapons carry the highlights and the environment map (the
    /// second and third games, when FX/SPECULAR.TGA and FX/ENVMAP.TGA are
    /// there).
    shine: bool,
    compass: Option<Entity>,
    wind: Option<(Entity, Handle<Mesh>, Arc<CharacterInfo>)>,
    binocular: Option<Entity>,
    bullets: Vec<Option<(Handle<Image>, UVec2)>>,
}

#[derive(Component)]
pub struct HudRoot;

#[derive(Component)]
pub struct CrossLine;

fn camera_of(cams: &Query<(Entity, &Overlay)>, o: Overlay) -> Option<Entity> {
    cams.iter().find(|(_, x)| **x == o).map(|(e, _)| e)
}

#[allow(clippy::too_many_arguments)]
fn spawn_near(
    commands: &mut Commands,
    parent: Entity,
    layer: usize,
    model: &Model,
    light: u32,
    quality: i32,
    imgs: &WorldImages,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<ObjectMaterial>,
) -> (Entity, Handle<Mesh>) {
    let mesh = meshes.add(model_mesh(model, None, &|_| 0.0));
    let mat = mats.add(ObjectMaterial {
        texture: images.add(model_image(model, quality)),
        fog_map: imgs.fog_map.clone(),
        fog_table: imgs.fog_table.clone(),
        sky: imgs.sky.clone(),
        lod: Vec4::ZERO,
    });
    let e = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat),
            MeshTag(light),
            RenderLayers::layer(layer),
            NoFrustumCulling,
            Transform::IDENTITY,
            Visibility::Hidden,
            ChildOf(parent),
            AreaEntity,
        ))
        .id();
    (e, mesh)
}

/// One of the shine's pictures, its values kept as stored (the shader adds
/// them on the stored scale), filtered and repeating as the games had it.
fn fx_image(root: &DataRoot, rel: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    let p = Picture::load(&root.find(rel)?)
        .map_err(|e| warn!("{e}"))
        .ok()?;
    let mut img = Image::new(
        bevy::render::render_resource::Extent3d {
            width: p.width as u32,
            height: p.height as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        p.rgba,
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        bevy::render::render_asset::RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        ..bevy::image::ImageSamplerDescriptor::linear()
    });
    Some(images.add(img))
}

/// A weapon of the second or third game in the hands, with its shine
/// (render::WeaponMaterial).
#[allow(clippy::too_many_arguments)]
fn spawn_weapon(
    commands: &mut Commands,
    parent: Entity,
    model: &Model,
    quality: i32,
    imgs: &WorldImages,
    (specular, envmap, phong): (&Handle<Image>, &Handle<Image>, Vec4),
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<WeaponMaterial>,
) -> (Entity, Handle<Mesh>) {
    let n = model.faces.len() * 3;
    let mesh = meshes.add(
        model_mesh(model, None, &|_| 0.0)
            .with_inserted_attribute(ATTR_WEAPON_FX, vec![[0.5f32; 4]; n]),
    );
    let mat = mats.add(WeaponMaterial {
        texture: images.add(model_image(model, quality)),
        fog_map: imgs.fog_map.clone(),
        fog_table: imgs.fog_table.clone(),
        sky: imgs.sky.clone(),
        lod: Vec4::ZERO,
        specular: specular.clone(),
        envmap: envmap.clone(),
        phong,
    });
    let e = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat),
            MeshTag(200),
            RenderLayers::layer(1),
            NoFrustumCulling,
            Transform::IDENTITY,
            Visibility::Hidden,
            ChildOf(parent),
            AreaEntity,
        ))
        .id();
    (e, mesh)
}

/// Loads the weapons and gear for a hunt and puts their models on the
/// overlay cameras.
#[allow(clippy::too_many_arguments)]
pub fn setup(
    commands: &mut Commands,
    root: &DataRoot,
    kind: GameKind,
    quality: i32,
    brightness: Option<(i32, i32)>,
    imgs: &WorldImages,
    cams: &Query<(Entity, &Overlay)>,
    hunt: Option<&crate::menu::HuntSetup>,
    sky: [u8; 3],
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    (mats, weapon_mats): (&mut Assets<ObjectMaterial>, &mut Assets<WeaponMaterial>),
) {
    let info: Vec<WeapInfo> = match kind {
        GameKind::Carnivores => c1_weapons(),
        _ => root
            .find("HUNTDAT/_RES.TXT")
            .and_then(|p| std::fs::read(p).ok())
            .map(|t| {
                crate::sim::script::parse(&t.iter().map(|&b| b as char).collect::<String>()).weapons
            })
            .unwrap_or_default(),
    };
    let cars: Vec<Option<Arc<CharacterInfo>>> = info
        .iter()
        .map(|w| {
            let p = root
                .find(&format!("HUNTDAT/WEAPONS/{}", w.file))
                .or_else(|| root.find(&format!("HUNTDAT/{}", w.file)))?;
            CharacterInfo::load(&p, brightness)
                .map(Arc::new)
                .map_err(|e| warn!("{e}"))
                .ok()
        })
        .collect();
    let sounds = cars
        .iter()
        .map(|c| {
            c.as_ref()
                .map(|c| c.sounds.iter().map(|s| Pcm::from(s.as_slice())).collect())
                .unwrap_or_default()
        })
        .collect();
    let n = info.len();
    // The weapons taken on the hunt, or all of them; an
    // observer carries none. Double ammo is one magazine more of each.
    let taken = |w: usize| match hunt {
        Some(h) if h.observer || h.trophy => false,
        Some(h) => w < 32 && h.weapons & (1 << w) != 0,
        None => true,
    };
    let mut shots_left = vec![0; n];
    let mut ammo_mag = vec![0; n];
    for (w, wi) in info.iter().enumerate() {
        if cars[w].is_some() && taken(w) {
            shots_left[w] = wi.shots;
            if hunt.map(|h| h.double_ammo).unwrap_or(false) {
                ammo_mag[w] = 1;
            }
        }
    }
    let first = shots_left.iter().position(|&s| s > 0).unwrap_or(0);
    info!(
        "weapons: {}",
        info.iter()
            .zip(&cars)
            .map(|(w, c)| format!("{}{}", w.name, if c.is_some() { "" } else { " (missing)" }))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // The first game drew its near models at full light.
    let near_light = if kind == GameKind::Carnivores {
        255
    } else {
        192
    };
    let mut gear = Gear::default();
    // The second and third games' weapons shine: highlights in the sky's
    // colour raised by 64, and the environment map.
    let shine = if kind == GameKind::Carnivores {
        None
    } else {
        fx_image(root, "HUNTDAT/FX/SPECULAR.TGA", images).zip(fx_image(
            root,
            "HUNTDAT/FX/ENVMAP.TGA",
            images,
        ))
    };
    let phong = Vec4::new(
        (sky[0] as f32 + 64.0).min(255.0) / 255.0,
        (sky[1] as f32 + 64.0).min(255.0) / 255.0,
        (sky[2] as f32 + 64.0).min(255.0) / 255.0,
        1.0,
    );
    gear.shine = shine.is_some();
    if let Some(hands) = camera_of(cams, Overlay::Hands) {
        for c in &cars {
            gear.weapons.push(c.as_ref().map(|c| match &shine {
                Some((spec, env)) => spawn_weapon(
                    commands,
                    hands,
                    &c.model,
                    quality,
                    imgs,
                    (spec, env, phong),
                    images,
                    meshes,
                    weapon_mats,
                ),
                None => spawn_near(
                    commands, hands, 1, &c.model, 200, quality, imgs, images, meshes, mats,
                ),
            }));
        }
        if let Some(m) = root
            .find("HUNTDAT/BINOCUL.3DF")
            .and_then(|p| Model::load_3df(&p, brightness).ok())
        {
            gear.binocular = Some(
                spawn_near(
                    commands, hands, 1, &m, near_light, quality, imgs, images, meshes, mats,
                )
                .0,
            );
        }
    }
    if let Some(cam) = camera_of(cams, Overlay::Compass) {
        if let Some(m) = root
            .find("HUNTDAT/COMPAS.3DF")
            .and_then(|p| Model::load_3df(&p, brightness).ok())
        {
            gear.compass = Some(
                spawn_near(
                    commands, cam, 2, &m, near_light, quality, imgs, images, meshes, mats,
                )
                .0,
            );
        }
    }
    if let Some(cam) = camera_of(cams, Overlay::Wind) {
        if let Some(ci) = root
            .find("HUNTDAT/WIND.CAR")
            .and_then(|p| CharacterInfo::load(&p, brightness).ok())
        {
            let (e, h) = spawn_near(
                commands, cam, 3, &ci.model, near_light, quality, imgs, images, meshes, mats,
            );
            gear.wind = Some((e, h, Arc::new(ci)));
        }
    }
    gear.bullets = info
        .iter()
        .map(|w| {
            let p = root
                .find(&format!("HUNTDAT/WEAPONS/{}", w.pic))
                .or_else(|| root.find(&format!("HUNTDAT/MENU/WEPPIC/{}", w.pic)))?;
            let pic = Picture::load(&p).ok()?;
            let size = UVec2::new(pic.width as u32, pic.height as u32);
            let img = Image::new(
                bevy::render::render_resource::Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                pic.rgba,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::render::render_asset::RenderAssetUsages::RENDER_WORLD,
            );
            Some((images.add(img), size))
        })
        .collect();

    commands.insert_resource(gear);
    commands.insert_resource(Arsenal {
        info,
        cars,
        sounds,
        state: 0,
        ftime: 0,
        shakel: 0.2,
        current: first,
        target: first,
        shots_left,
        ammo_mag,
        optic: false,
        binoculars: false,
        bin_power: 2.5,
        dalpha: 0.0,
        dbeta: 0.0,
        gauges: true,
        morph: Vec::new(),
        cross: None,
    });
}

/// The first game's three weapons; it kept no script.
pub fn c1_weapons() -> Vec<WeapInfo> {
    let w =
        |name: &str, n: usize, power: f32, prec: f32, loud: f32, rate: f32, shots: i32| WeapInfo {
            name: name.into(),
            file: format!("WEAPON{n}.CAR"),
            pic: format!("BULLET{n}.TGA"),
            power,
            prec,
            loud,
            rate,
            shots,
            ..Default::default()
        };
    vec![
        w("Shotgun", 1, 1.5, 1.1, 0.3, 1.6, 6),
        w("X-Bow", 2, 1.1, 0.7, 1.9, 1.2, 8),
        w("Sniper Rifle", 3, 1.0, 1.8, 0.6, 1.0, 6),
    ]
}

/// Draws the weapon, or puts it away.
fn hide_weapon(ar: &mut Arsenal, mixer: &Mixer, underwater: bool, trophy: bool) {
    if (underwater && ar.state == 0) || trophy {
        return;
    }
    let cur = ar.current;
    if ar.state == 0 {
        if ar.shots_left.get(cur).copied().unwrap_or(0) == 0 {
            return;
        }
        if ar.info[cur].optic != 0 {
            ar.optic = true;
        }
        if let Some(s) = ar.sound(cur, 0) {
            mixer.play(s, None, 256);
        }
        ar.ftime = 0;
        ar.state = 1;
        ar.binoculars = false;
        ar.shakel = 0.2;
        return;
    }
    if ar.state != 2 || ar.ftime != 0 {
        return;
    }
    if let Some(s) = ar.sound(cur, 2) {
        mixer.play(s, None, 256);
    }
    ar.state = 3;
    ar.ftime = 0;
    ar.optic = false;
}

/// A shot's path: straight, or for a weapon whose shots fall, in
/// three dropping stretches.
fn make_shot(sim: &mut Sim, area: &Area, a: Vec3, b: Vec3, wi: &WeapInfo) {
    let (res, at) = if wi.fall == 0 {
        sim.trace_shot(area, a, b)
    } else {
        let dy = 40.0 * sim.view_r as f32 / 36.0;
        let dl = (b - a) / 3.0;
        let mut from = a;
        let mut out = (None, b);
        for k in [0.5f32, 3.0, 5.0] {
            let to = from + dl - Vec3::Y * dy * k;
            out = sim.trace_shot(area, from, to);
            if out.0.is_some() {
                break;
            }
            from = to;
        }
        out
    };
    debug!("shot hit {res:?}");
    if let Some(h) = &res {
        sim.shot_impact(area, h, at, wi.power);
    }
    if let Some(Hit::Char(ci, mortal)) = res {
        let killed = sim.shot_damage(area, ci, mortal, wi.power);
        if let Some(c) = sim.chars.get(ci) {
            info!(
                "hit {}{}: health {}{}",
                sim.dinos[c.ctype].name,
                if mortal { " (mortal)" } else { "" },
                c.health,
                if killed { ", killed" } else { "" }
            );
        }
    }
}

/// Firing.
fn shoot(ar: &mut Arsenal, sim: &mut Sim, area: &Area, player: &mut Player, mixer: &Mixer) {
    let cur = ar.current;
    if ar.shots_left.get(cur).copied().unwrap_or(0) == 0 {
        return;
    }
    if player.underwater {
        hide_weapon(ar, mixer, true, sim.trophy);
        return;
    }
    if ar.state != 2 || ar.ftime != 0 {
        return;
    }
    ar.ftime = 1;
    player.head_back = 64.0;
    sim.hunt_stats.shots_made += 1;
    sim.current_weapon = cur;
    if let Some(s) = ar.sound(cur, 1) {
        mixer.play(s, None, 256);
    }
    let wi = ar.info[cur].clone();
    let from = Vec3::new(player.x, player.y + player.head_y, player.z);
    for _ in 0..=wi.trace_c.max(0) {
        let ra = sim.rng.si(128) as f32 * 0.0001 * (2.0 - wi.prec);
        let rb = sim.rng.si(128) as f32 * 0.0001 * (2.0 - wi.prec);
        let (sa, ca) = (player.alpha + ar.dalpha + ra).sin_cos();
        let (sb, cb) = (player.beta + ar.dbeta + rb).sin_cos();
        let nv = Vec3::new(sa * cb, -sb, -ca * cb);
        let to = from + nv * 256.0 * sim.view_r as f32;
        make_shot(sim, area, from, to, &wi);
    }
    let pos = Vec3::new(player.x, player.y, player.z);
    // How far the shot is heard. Carnivores 1 divides by the weapon's
    // loudness (18*256 / Loud: the quieter number carries further); the
    // later games scale the view range by it.
    let range = if sim.c1 {
        18.0 * 256.0 / wi.loud.max(0.05)
    } else {
        sim.view_r as f32 * 200.0 * wi.loud
    };
    sim.make_noise(pos, range);
    ar.shots_left[cur] -= 1;
}

/// The posed model's vertex normals: the faces' own, summed
/// where they meet, double-sided faces left out.
fn normals(model: &Model, pos: &[[f32; 3]]) -> Vec<Vec3> {
    let mut n = vec![Vec3::ZERO; pos.len()];
    for f in &model.faces {
        if f.flags & SF_DOUBLE_SIDE != 0 {
            continue;
        }
        let [a, b, c] = f.v.map(|v| Vec3::from(pos[v as usize]));
        let nv = (b - a).cross(c - a).normalize_or_zero() * 1000.0;
        for &v in &f.v {
            n[v as usize] += nv;
        }
    }
    n.iter_mut().for_each(|v| *v = v.normalize_or_zero());
    n
}

/// Per-vertex light for the weapon: the sun, turned into the view, on its
/// normals.
fn gouraud(n: &[Vec3], sun_view: Vec3, out: &mut Vec<f32>) {
    let slight = -sun_view.normalize_or_zero();
    out.clear();
    out.extend(n.iter().map(|nv| {
        let mut c = nv.dot(slight);
        if c < 0.0 {
            c = 0.0;
        }
        c = (c - 0.5) * 2.0;
        c = c * c * c * 96.0;
        c.clamp(-64.0, 96.0)
    }));
}

/// Where each vertex takes the highlights (the sun's
/// half-way direction against the normal) and the environment map
/// (the normal-pushed direction against two axes that turn
/// as the hunter walks and looks), as texture coordinates.
fn shine_uv(
    pos: &[[f32; 3]],
    n: &[Vec3],
    sun_view: Vec3,
    (player_xz, view): (f32, Quat),
    out: &mut Vec<[f32; 4]>,
) {
    // A vector to a length, as the games scaled (the sign of
    // the length carried through).
    let norm = |v: Vec3, len: f32| v * (len / v.length_squared().max(1e-9).sqrt());
    let l = norm(-sun_view, 1.0);
    let tv = Vec3::new(1.0, 1.0, 0.0);
    let (ss, cc) = (player_xz / 500.0).sin_cos();
    let tx_env = view * Vec3::new(cc, 0.0, -ss);
    let ty_env = view * Vec3::new(ss, 0.0, cc);
    out.clear();
    out.extend(pos.iter().zip(n).map(|(p, &nv)| {
        let p = Vec3::from(*p);
        let v = norm(p, 1.0);
        let m = norm(l + v, 1.0);
        let tx = norm(if l.z < 0.0 { m.cross(tv) } else { m.cross(v) }, 1.0);
        let ty = norm(m.cross(tx), 1.0);
        let (px, py) = (tx.dot(nv), ty.dot(nv));
        let s = p.dot(nv);
        let me = norm(norm(nv, s * 2.0) + p, 1.0);
        let (ex, ey) = (tx_env.dot(me), ty_env.dot(me));
        [
            (128.0 + px * 127.0) / 256.0,
            (128.0 + py * 127.0) / 256.0,
            (128.0 + ex * 122.0) / 256.0,
            (128.0 + ey * 122.0) / 256.0,
        ]
    }));
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    (launch, debug): (Res<crate::Launch>, Res<crate::hud::DebugMode>),
    mut raised: Local<bool>,
    (keys, buttons): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    settings: Res<crate::settings::Settings>,
    time: Res<Time>,
    view: Res<crate::ViewState>,
    area: Res<Area>,
    mixer: Res<Mixer>,
    mut ar: ResMut<Arsenal>,
    gear: Res<Gear>,
    mut sim: Option<ResMut<Sim>>,
    mut player: ResMut<Player>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&mut Transform, &mut Visibility, &mut MeshTag), Without<Overlay>>,
    mut cams: Query<(&Overlay, &mut Projection)>,
    (mut vl, mut uv): (Local<Vec<f32>>, Local<Vec<[f32; 4]>>),
) {
    let dt = (time.delta_secs() * 1000.0).clamp(1.0, 1000.0) as i32;
    if let (Some(w), false) = (launch.weapon_up, *raised) {
        *raised = true;
        let w = w.saturating_sub(1).min(ar.info.len().saturating_sub(1));
        ar.current = w;
        ar.target = w;
        ar.state = 2;
        ar.ftime = 0;
        ar.optic = ar
            .info
            .get(ar.current)
            .map(|w| w.optic != 0)
            .unwrap_or(false);
    }
    let dead = sim.as_ref().map(|s| s.my_health == 0).unwrap_or(false);
    let trophy = area.trophy;
    let uw = player.underwater;

    if !view.paused && !dead {
        let digits = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ];
        for (w, k) in digits.iter().enumerate() {
            if keys.just_pressed(*k) && w < ar.info.len() && ar.ftime == 0 {
                if ar.shots_left[w] == 0 {
                    if let Some(s) = sim.as_mut() {
                        s.add_message("No weapon");
                    }
                    continue;
                }
                ar.target = w;
                if ar.state == 0 {
                    ar.current = w;
                }
                hide_weapon(&mut ar, &mixer, uw, trophy);
            }
        }
        let input = crate::keymap::Input {
            keys: &keys,
            mouse: &buttons,
        };
        let km = &settings.keys;
        if km.pressed(crate::keymap::Act::GetWeapon, &input) {
            hide_weapon(&mut ar, &mixer, uw, trophy);
        }
        if km.held(crate::keymap::Act::Fire, &input) {
            if let Some(s) = sim.as_mut() {
                shoot(&mut ar, s, &area, &mut player, &mixer);
            }
        }
        if km.pressed(crate::keymap::Act::Binoculars, &input) && ar.state == 0 && !uw {
            ar.binoculars = !ar.binoculars;
        }
        if ar.binoculars {
            let d = dt as f32 / 4000.0;
            if keys.pressed(KeyCode::NumpadAdd) || keys.pressed(KeyCode::Equal) {
                ar.bin_power += ar.bin_power * d;
            }
            if keys.pressed(KeyCode::NumpadSubtract) || keys.pressed(KeyCode::Minus) {
                ar.bin_power -= ar.bin_power * d;
            }
            ar.bin_power = ar.bin_power.clamp(1.5, 3.0);
        }
        if keys.just_pressed(KeyCode::CapsLock) {
            ar.gauges = !ar.gauges;
        }
    }
    if uw || dead {
        ar.binoculars = false;
    }

    // The weapon's own clock.
    let cur = ar.current;
    if ar.state != 0 && !view.paused {
        ar.shakel = (ar.shakel + dt as f32 / 10000.0).min(4.0);
        match ar.state {
            1 => {
                ar.ftime += dt;
                if ar.ftime >= ar.ani_time(cur, 0) {
                    ar.ftime = 0;
                    ar.state = 2;
                }
            }
            4 => {
                ar.ftime += dt;
                if ar.ftime >= ar.ani_time(cur, 3) {
                    ar.ftime = 0;
                    ar.state = 2;
                }
            }
            2 if ar.ftime > 0 => {
                ar.ftime += dt;
                if ar.ftime >= ar.ani_time(cur, 1) {
                    ar.ftime = 0;
                    let reload = ar.info[cur].reload;
                    let (sl, mag) = (ar.shots_left[cur], ar.ammo_mag[cur]);
                    if reload != 0 && sl % reload == 0 && (sl > 0 || mag > 0) {
                        ar.state = 4;
                        ar.ftime = 1;
                        if let Some(s) = ar.sound(cur, 3) {
                            mixer.play(s, None, 256);
                        }
                    }
                    if ar.shots_left[cur] == 0 && ar.ammo_mag[cur] > 0 {
                        ar.ammo_mag[cur] -= 1;
                        ar.shots_left[cur] = ar.info[cur].shots;
                        if ar.ani_time(cur, 3) > 0 {
                            ar.state = 4;
                            ar.ftime = 1;
                            if let Some(s) = ar.sound(cur, 3) {
                                mixer.play(s, None, 256);
                            }
                        }
                    }
                }
            }
            3 => {
                ar.ftime += dt;
                if ar.ftime >= ar.ani_time(cur, 2) {
                    ar.ftime = 0;
                    ar.state = 0;
                    if ar.current != ar.target {
                        ar.current = ar.target;
                        hide_weapon(&mut ar, &mixer, uw, trophy);
                    }
                }
            }
            _ => {}
        }
        if ar.state != 0 && ar.shots_left[ar.current] == 0 {
            hide_weapon(&mut ar, &mixer, uw, trophy);
            if let Some(w) = ar.shots_left.iter().position(|&s| s > 0) {
                ar.target = w;
            }
        }
    }
    // Debug mode never runs out: the weapon in hand is filled every frame.
    if debug.on {
        let cur = ar.current;
        let full = ar.info.get(cur).map(|w| w.shots);
        if let (Some(full), Some(s)) = (full, ar.shots_left.get_mut(cur)) {
            if *s != full {
                *s = full;
            }
        }
    }

    if ar.is_changed() {
        debug!(
            "weapon state {} ftime {} current {} shots {:?}",
            ar.state, ar.ftime, ar.current, ar.shots_left
        );
    }
    let rt = player.real_time;
    ar.dalpha = ar.shakel * (rt / 300.0 + std::f32::consts::FRAC_PI_2).sin() / 200.0;
    ar.dbeta = ar.shakel * (rt / 300.0).sin() / 400.0;

    // The hands camera: magnified with the view, and stretched to the
    // screen for the binocular frame and a scope's surround.
    let zoom = ar.zoom();
    for (o, mut p) in cams.iter_mut() {
        if *o != Overlay::Hands {
            continue;
        }
        if let Projection::Custom(c) = p.as_mut() {
            if let Some(op) = c.get_mut::<OverlayProjection>() {
                let stretch = ar.binoculars || ar.optic;
                let ys = NEAR_K * zoom;
                let xs = stretch.then_some(1.25 * zoom);
                if op.ys != ys || op.xs != xs {
                    op.ys = ys;
                    op.xs = xs;
                }
            }
        }
    }

    // The weapon in the hands.
    let show_weapon = ar.state != 0 && !dead;
    let wpnlight = match area.engine {
        carn_formats::Engine::C2 => 96 + area.land_light(player.x, player.z) as u32 / 4,
        carn_formats::Engine::C1 => 255,
    };
    for (w, g) in gear.weapons.iter().enumerate() {
        let Some((e, mesh)) = g else { continue };
        let Ok((mut t, mut vis, mut tag)) = q.get_mut(*e) else {
            continue;
        };
        let on = show_weapon && w == ar.current;
        let want = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        if !on {
            continue;
        }
        let Some(ci) = ar.cars[w].clone() else {
            continue;
        };
        let phase = (ar.state - 1).max(0) as usize;
        let mut morph = std::mem::take(&mut ar.morph);
        match ci.animations.get(phase) {
            Some(an) => an.sample(ci.model.vertices.len(), ar.ftime, 1.0, &mut morph),
            None => {
                morph.clear();
                morph.extend(ci.model.vertices.iter().map(|v| v.pos));
            }
        }
        if morph.len() == ci.model.vertices.len() {
            let sun = area.sun;
            let view = crate::player::view_rotation(player.cam_alpha, player.cam_beta).inverse();
            let sun_view = view * Vec3::new(sun[0], sun[1], sun[2]);
            let nv = normals(&ci.model, &morph);
            gouraud(&nv, sun_view, &mut vl);
            // The first game never lit it; the others unless GOUR is off.
            if area.engine == carn_formats::Engine::C1 || debug.no_gour {
                vl.iter_mut().for_each(|v| *v = 0.0);
            }
            if let Some(m) = meshes.get_mut(mesh) {
                let pos: Vec<[f32; 3]> = ci
                    .model
                    .faces
                    .iter()
                    .flat_map(|f| f.v.map(|v| morph[v as usize]))
                    .collect();
                // The shiny faces (face flags 0x30 and 0x50: a face with
                // either bit of one takes it) while the switches leave it on.
                let (phong, envmap) = (
                    gear.shine && !debug.no_phong,
                    gear.shine && !debug.no_envmap,
                );
                let attr: Vec<[f32; 2]> = ci
                    .model
                    .faces
                    .iter()
                    .flat_map(|f| {
                        let mut kind = crate::render::face_kind(f);
                        if phong && f.flags & 0x30 != 0 {
                            kind += 4.0;
                        }
                        if envmap && f.flags & 0x50 != 0 {
                            kind += 8.0;
                        }
                        f.v.map(|v| [vl[v as usize], kind])
                    })
                    .collect();
                if gear.shine {
                    shine_uv(&morph, &nv, sun_view, (player.x + player.z, view), &mut uv);
                    let fx: Vec<[f32; 4]> = ci
                        .model
                        .faces
                        .iter()
                        .flat_map(|f| f.v.map(|v| uv[v as usize]))
                        .collect();
                    m.insert_attribute(ATTR_WEAPON_FX, fx);
                }
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(ATTR_OBJECT, attr);
            }
            // The scope's cross sits where the model's last vertex lands.
            ar.cross = if ar.optic {
                morph.last().and_then(|p| {
                    let z = -p[2];
                    let c = (z > 1.0).then(|| {
                        let ys = NEAR_K * zoom;
                        Vec2::new(
                            0.5 + p[0] / z * 1.25 * zoom / 2.0,
                            0.5 - p[1] / z * ys / 2.0,
                        )
                    })?;
                    // The games leave it out when far off centre.
                    ((c.x - 0.5).abs() <= 0.5 && (c.y - 0.5).abs() <= 0.25).then_some(c)
                })
            } else {
                None
            };
        }
        if ar.ftime == 0 && ar.state == 2 && time.elapsed_secs() < 30.0 {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for p in &morph {
                lo = lo.min(Vec3::from(*p));
                hi = hi.max(Vec3::from(*p));
            }
            debug!("weapon {} bounds {lo} .. {hi}", ar.current);
        }
        ar.morph = morph;
        *t = Transform::from_rotation(
            Quat::from_rotation_x(-ar.dbeta) * Quat::from_rotation_y(-ar.dalpha),
        );
        if tag.0 != wpnlight {
            tag.0 = wpnlight;
        }
    }
    if !show_weapon {
        ar.cross = None;
    }

    // Binoculars.
    if let Some(e) = gear.binocular {
        if let Ok((mut t, mut vis, _)) = q.get_mut(e) {
            let want = if ar.binoculars {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
            t.translation = Vec3::new(0.0, 0.0, 2.0 * (216.0 - 72.0 * ar.bin_power));
        }
    }

    // Compass and wind gauge.
    let gauges = ar.gauges && !ar.binoculars && !ar.optic && !trophy && !dead;
    if let Some(e) = gear.compass {
        if let Ok((mut t, mut vis, _)) = q.get_mut(e) {
            let want = if gauges {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
            *t = Transform::from_xyz(8.0, -38.0, -96.0)
                .with_rotation(Quat::from_rotation_y(player.cam_alpha));
        }
    }
    if let (Some((e, mesh, ci)), Some(s)) = (&gear.wind, sim.as_ref()) {
        if let Ok((mut t, mut vis, _)) = q.get_mut(*e) {
            let want = if gauges {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
            *t = Transform::from_xyz(-10.0, -37.0, -96.0)
                .with_rotation(Quat::from_rotation_y(player.cam_alpha - s.wind.alpha));
            if let Some(an) = ci.animations.first() {
                let mut buf = Vec::new();
                an.sample(
                    ci.model.vertices.len(),
                    (s.wind.speed * 50.0) as i32 % an.ani_time.max(1),
                    1.0,
                    &mut buf,
                );
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

/// The rows of bullets in the top left, and the scope's cross.
#[allow(clippy::too_many_arguments)]
pub fn hud(
    mut commands: Commands,
    ar: Option<Res<Arsenal>>,
    gear: Option<Res<Gear>>,
    windows: Query<&Window>,
    old: Query<Entity, With<HudRoot>>,
    mut last: Local<Option<(usize, i32, i32, i32, u32, Option<(i32, i32)>)>>,
) {
    let (Some(ar), Some(gear)) = (ar, gear) else {
        if last.is_some() {
            for e in old.iter() {
                commands.entity(e).despawn();
            }
            *last = None;
        }
        return;
    };
    let Ok(win) = windows.single() else { return };
    let h = win.height();
    // Pixel for pixel at any resolution, as the games drew them.
    let scale: f32 = 1.0;
    let cross = ar
        .cross
        .map(|c| ((c.x * win.width()) as i32, (c.y * h) as i32));
    let key = (
        ar.current,
        if ar.state != 0 {
            ar.shots_left[ar.current]
        } else {
            -1
        },
        ar.ammo_mag.get(ar.current).copied().unwrap_or(0),
        ar.state.min(1),
        h as u32,
        cross,
    );
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    for e in old.iter() {
        commands.entity(e).despawn();
    }
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            HudRoot,
        ))
        .id();
    if ar.state != 0 {
        if let Some(Some((img, size))) = gear.bullets.get(ar.current) {
            let (w, hh) = (size.x as f32 * scale, size.y as f32 * scale);
            let mut y0 = 5.0 * scale;
            let row = |n: i32, y: f32, commands: &mut Commands| {
                for b in 0..n {
                    commands.spawn((
                        ImageNode::new(img.clone()),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(6.0 * scale + b as f32 * w),
                            top: Val::Px(y),
                            width: Val::Px(w),
                            height: Val::Px(hh),
                            ..default()
                        },
                        ChildOf(root),
                    ));
                }
            };
            if ar.ammo_mag[ar.current] > 0 {
                row(ar.info[ar.current].shots, y0, &mut commands);
                y0 += hh + 4.0 * scale;
            }
            row(ar.shots_left[ar.current], y0, &mut commands);
        }
    }
    if let Some((x, y)) = cross {
        // A thin cross with a gap in the middle.
        let len = 24.0 * scale;
        let gap = 4.0 * scale;
        let th = scale.round().max(1.0);
        let c = Color::srgb(0.0, 0.0, 0.0);
        for (l, t, w, hh) in [
            (x as f32 - gap - len, y as f32, len, th),
            (x as f32 + gap, y as f32, len, th),
            (x as f32, y as f32 - gap - len, th, len),
            (x as f32, y as f32 + gap, th, len),
        ] {
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(l),
                    top: Val::Px(t),
                    width: Val::Px(w),
                    height: Val::Px(hh),
                    ..default()
                },
                BackgroundColor(c),
                CrossLine,
                ChildOf(root),
            ));
        }
    }
}

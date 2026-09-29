//! The sun, or the moon at night, and the light it throws in the hunter's
//! eyes (and the washes laid over the frame), and the tint over everything
//! under water.
//!
//! The games measured two things off the framebuffer: how much the sky
//! round the sun differed from the clear sky's colour (clouds dim it), and
//! whether anything had been drawn over it (a hill or a tree hides it).
//! Here the first reads the same sky picture the sky shader draws, and the
//! second traces sight lines through the world as a shot would.

use bevy::prelude::*;
use bevy::render::camera::CameraProjection;
use carn_formats::color::rgb555;
use carn_formats::model::Model;
use carn_formats::Engine;

use crate::area::Area;
use crate::game::GameKind;
use crate::paths::DataRoot;
use crate::player::Player;
use crate::render::SunMaterial;
use crate::settings::Settings;
use crate::sim::{delta_func, Sim};
use crate::world::{model_image, model_mesh, AreaEntity};
use crate::{MainCamera, ViewState};

/// Where the games' pixel offsets were measured: their focal length at
/// 640 wide.
const FOCAL: f32 = 400.0;

#[derive(Resource)]
pub struct SunState {
    sun: Option<(Entity, Handle<SunMaterial>)>,
    wash: Entity,
    /// How clear the sky round the sun is, and how much of it
    /// is in plain view.
    sky_k: f32,
    trace_k: f32,
}

#[derive(Component)]
pub struct Wash;

/// Where the sun is, relative to the eye.
fn sun_pos(kind: GameKind, day_night: i32) -> Vec3 {
    if kind == GameKind::Carnivores {
        return Vec3::new(-2048.0, 4048.0, -2048.0);
    }
    match day_night {
        0 => Vec3::new(-4048.0, 2048.0, -4048.0),
        2 => Vec3::new(3048.0, 3048.0, 3048.0),
        _ => Vec3::new(-2048.0, 4048.0, -2048.0),
    }
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The games blended the stored values; the UI blends linear ones. This
/// is the alpha that gives the games' result over a mid-grey picture.
fn wash_alpha(c: [f32; 3], a: f32) -> f32 {
    let dst = 0.5f32;
    let lum = |v: [f32; 3]| (v[0] + v[1] + v[2]) / 3.0;
    let want = to_linear(dst * (1.0 - a) + lum(c) * a);
    let (d, s) = (to_linear(dst), to_linear(lum(c)));
    if (d - s).abs() < 1e-4 {
        return a;
    }
    ((d - want) / (d - s)).clamp(0.0, 1.0)
}

/// The sky's colour along a sight line, 0..255, as sky.wgsl works it out.
fn sky_at(area: &Area, cam: Vec3, dir: Vec3, t_ms: f32) -> Vec3 {
    let f = area.rsc.sky_fade(area.opt);
    let fade = Vec3::new(f[0] as f32, f[1] as f32, f[2] as f32);
    if dir.y <= 0.0005 {
        return fade;
    }
    let t = 32768.0 / dir.y;
    let hit = cam + dir * t;
    let dtt = (t_ms % 131072.0) / 512.0;
    let u = ((hit.x * 0.002 + dtt).floor() as i32).rem_euclid(256) as usize;
    let v = ((hit.z * 0.002 - dtt).floor() as i32).rem_euclid(256) as usize;
    let sky = area.rsc.sky(area.opt);
    let [r, g, b] = rgb555(sky.get(v * 256 + u).copied().unwrap_or(0));
    let tex = Vec3::new(r as f32, g as f32, b as f32);
    let d = t * Vec2::new(dir.x, dir.z).length();
    let a = 40240.0 / (40240.0 + (d - 100200.0).max(0.0));
    fade.lerp(tex, a)
}

type SunAssets<'w> = (
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<SunMaterial>>,
);
type SunQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    (With<AreaEntity>, Without<MainCamera>),
>;

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    state: Option<ResMut<SunState>>,
    root: Res<DataRoot>,
    (kind, radius): (Res<GameKind>, Res<crate::ViewRadius>),
    settings: Res<Settings>,
    area: Option<Res<Area>>,
    player: Option<Res<Player>>,
    sim: Option<Res<Sim>>,
    view: Res<ViewState>,
    time: Res<Time>,
    windows: Query<&Window>,
    cam: Query<(&Transform, &Projection), With<MainCamera>>,
    (mut meshes, mut images, mut mats): SunAssets,
    mut q: SunQuery,
    mut wash: Query<&mut BackgroundColor, With<Wash>>,
) {
    let (Some(area), Some(player)) = (area, player) else {
        return;
    };
    let night = kind.has_time_of_day() && settings.day_night == 2;
    let Some(mut st) = state else {
        // The first frame of a hunt.
        let file = if night {
            "HUNTDAT/MOON.3DF"
        } else {
            "HUNTDAT/SUN2.3DF"
        };
        let bright = kind
            .has_time_of_day()
            .then_some((settings.brightness, settings.day_night));
        let sun = root
            .find(file)
            .and_then(|p| Model::load_3df(&p, bright).map_err(|e| warn!("{e}")).ok())
            .map(|m| {
                let mat = mats.add(SunMaterial {
                    texture: images.add(model_image(&m, settings.textures)),
                    params: Vec4::ZERO,
                });
                let e = commands
                    .spawn((
                        Mesh3d(meshes.add(model_mesh(&m, None, &|_| 0.0))),
                        MeshMaterial3d(mat.clone()),
                        Transform::IDENTITY,
                        bevy::render::view::NoFrustumCulling,
                        Visibility::Hidden,
                        AreaEntity,
                    ))
                    .id();
                (e, mat)
            });
        let wash = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                // Under the HUD.
                GlobalZIndex(-1),
                Wash,
                AreaEntity,
            ))
            .id();
        commands.insert_resource(SunState {
            sun,
            wash,
            sky_k: 1.0,
            trace_k: 0.0,
        });
        return;
    };
    let Ok((ct, proj)) = cam.single() else { return };
    let Ok(win) = windows.single() else { return };
    let dt = time.delta_secs() * 1000.0;
    let rot = ct.rotation;
    let eye = ct.translation;
    // The sun in view space.
    let nv = rot.inverse() * sun_pos(*kind, settings.day_night);
    let mut light = 0.0f32;
    let mut shown = false;
    if nv.z < -2024.0 {
        // Where it is on the screen, and whether that is well inside it.
        let clip = proj.get_clip_from_view() * nv.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        let (w, h) = (win.width(), win.height());
        let (sx, sy) = ((ndc.x + 1.0) * 0.5 * w, (1.0 - ndc.y) * 0.5 * h);
        let tan = Vec2::new(nv.x / -nv.z, nv.y / -nv.z);
        let dir = |ox: f32, oy: f32| {
            (rot * Vec3::new(tan.x + ox / FOCAL, tan.y + oy / FOCAL, -1.0)).normalize()
        };

        // How far the sky round it is from the clear sky's colour.
        if sx >= 10.0 && sy >= 10.0 && sx <= w - 10.0 && sy <= h - 10.0 {
            let t_ms = time.elapsed_secs_wrapped() * 1000.0;
            let mut sum = Vec3::ZERO;
            for (ox, oy) in [
                (0.0, 0.0),
                (0.0, 6.0),
                (0.0, -6.0),
                (6.0, 0.0),
                (-6.0, 0.0),
                (4.0, 4.0),
                (-4.0, 4.0),
                (4.0, -4.0),
                (-4.0, -4.0),
            ] {
                sum += sky_at(&area, eye, dir(ox, oy), t_ms);
            }
            let tr = area.rsc.sky_trans(area.opt);
            sum -= Vec3::new(tr[0] as f32, tr[1] as f32, tr[2] as f32) * 9.0;
            let mut k = (sum.length() / 9.0).clamp(0.0, 80.0);
            k = (1.0 - k / 80.0).max(0.2);
            if night {
                k = 0.3 + k / 2.75;
            }
            let step = (0.07 + (k - st.sky_k).abs()) * (dt / 512.0);
            delta_func(&mut st.sky_k, k, step);
        }
        let d = Vec2::new(nv.x, nv.y).length();
        if d < 2048.0 {
            light = (220.0 - d * 220.0 / 2048.0).min(140.0) * st.sky_k;
        }
        let mut dd = (2048.0 + d.min(812.0)) / 3048.0 + (1.0 - st.sky_k) / 2.0;
        if night {
            dd = 1.5;
        }
        if let Some((e, mat)) = &st.sun {
            if let Ok((mut t, mut v)) = q.get_mut(*e) {
                t.translation = eye + rot * (nv * dd);
                t.rotation = rot;
                *v = Visibility::Inherited;
            }
            if let Some(m) = mats.get_mut(mat) {
                m.params.x = 200.0 * st.sky_k / 255.0;
            }
            shown = true;
        }

        // How much of it nothing stands in front of.
        if light > 1.0 {
            if sx < 8.0 || sy < 8.0 || sx > w - 8.0 || sy > h - 8.0 {
                light = 0.0;
            } else {
                let far = 256.0 * radius.r as f32;
                let mut k = 0.0;
                for (ox, oy) in [
                    (0.0, 0.0),
                    (0.0, 10.0),
                    (0.0, -10.0),
                    (10.0, 0.0),
                    (-10.0, 0.0),
                    (8.0, 8.0),
                    (-8.0, 8.0),
                    (8.0, -8.0),
                    (-8.0, -8.0),
                ] {
                    let to = eye + dir(ox, oy) * far;
                    let blocked = match sim.as_deref() {
                        Some(s) => s.trace_shot(&area, eye, to).0.is_some(),
                        None => {
                            let n = (far / 64.0) as i32;
                            (1..=n).any(|i| {
                                let p = eye.lerp(to, i as f32 / n as f32);
                                p.y < area.land_h(p.x, p.z)
                            })
                        }
                    };
                    if !blocked {
                        k += 1.0;
                    }
                }
                delta_func(&mut st.trace_k, k / 9.0, dt / 1024.0);
                light *= st.trace_k;
            }
        }
    }
    if !shown {
        if let Some((e, _)) = &st.sun {
            if let Ok((_, mut v)) = q.get_mut(*e) {
                *v = Visibility::Hidden;
            }
        }
    }

    // The washes over the frame: a tint under water, the glare otherwise.
    let wash_color = if player.underwater {
        let (c, a) = if area.engine == Engine::C1 {
            (
                [0.0, 0x40 as f32 / 255.0, 0x50 as f32 / 255.0],
                0x90 as f32 / 255.0,
            )
        } else {
            (view.globals.underwater_rgb, 0x70 as f32 / 255.0)
        };
        Color::srgba(c[0], c[1], c[2], wash_alpha(c, a))
    } else if !night && light > 1.0 {
        let c = [1.0, 1.0, 0xC0 as f32 / 255.0];
        let a = (light as i32) as f32 / 255.0;
        Color::srgba(c[0], c[1], c[2], wash_alpha(c, a))
    } else {
        Color::NONE
    };
    if let Ok(mut bg) = wash.get_mut(st.wash) {
        if bg.0 != wash_color {
            bg.0 = wash_color;
        }
    }
}

//! What a hunt shows over the picture: the line of
//! messages, the call's icon, the breath bar, what the binoculars make out,
//! the trophy card after a pick-up, the exit and pause pictures and the
//! countdown to evacuation - and the keys for those (Esc, Y, N, R, Pause).
//!
//! Pictures and text are drawn at a whole multiple of their size, one step
//! per 600 lines of the window, so they stay sharp.

use bevy::picking::Pickable;
use bevy::prelude::*;
use carn_formats::tga::Picture;
use std::collections::HashMap;

use crate::area::Area;
use crate::game::GameKind;
use crate::menu::CurrentProfile;
use crate::paths::DataRoot;
use crate::player::Player;
use crate::profile::TrophyItem;
use crate::settings::Settings;
use crate::sim::Sim;
use crate::ui::Screen;
use crate::weapons::Arsenal;
use crate::world::AreaEntity;
use crate::{AppState, LoadRequest, MainCamera, ViewState};

/// The exit prompt and the pause picture (EXITMODE, PAUSE).
#[derive(Resource, Default)]
pub struct HuntFlow {
    pub exit_mode: bool,
    pub pause: bool,
    /// The position readout (F10).
    pub showpos: bool,
    /// The hunt is being started over (R): what it gained is not kept.
    pub restart: bool,
}

/// The fonts the games' text used: Arial at 16 (semi-bold) and 14.
#[derive(Resource, Clone, Default)]
pub struct HudFonts {
    pub midd: Handle<Font>,
    pub small: Handle<Font>,
}

#[derive(Clone)]
struct Pic {
    image: Handle<Image>,
    size: UVec2,
}

#[derive(Resource)]
pub struct HudPics {
    exit: Option<Pic>,
    pause: Option<Pic>,
    card: Option<Pic>,
    trophy_exit: Option<Pic>,
    calls: HashMap<i32, Pic>,
}

#[derive(Component)]
pub struct HudRoot;

/// The games' debug mode (DEBUG) and the switches that go with it, for the
/// session as the games' globals were. Typing the game's code in a hunt
/// turns it on and off: the hunter cannot be hurt or run out of shots, the
/// creatures take no notice of him, Ctrl rushes him along, and the view
/// range, slow motion and timer keys work.
#[derive(Resource, Default)]
pub struct DebugMode {
    pub on: bool,
    /// Game time at a quarter of the real (SLOW).
    pub slow: bool,
    /// The frame time, polygon count and sound environment (TIMER).
    pub timer: bool,
    /// How much of the code has been typed.
    typed: usize,
    /// The view radii the keys have moved this hunt: the view range, the
    /// detail range within it and the sprite distance.
    pub view: Option<(i32, i32, i32)>,
    /// The weapon's environment mapping, lighting and highlights switched
    /// off (ENVMAP, GOUR, PHONG are on unless these are).
    pub no_envmap: bool,
    pub no_gour: bool,
    pub no_phong: bool,
    /// Triangles drawn last frame, and its milliseconds, for the timer.
    pub polys: usize,
    pub msc: i32,
}

impl DebugMode {
    /// What -debug starts with (the second and third games' command line
    /// set DEBUG, and the timer follows it).
    pub fn from_launch(kind: GameKind, debug: bool) -> DebugMode {
        let on = debug && kind != GameKind::Carnivores;
        DebugMode {
            on,
            timer: on,
            ..default()
        }
    }
}

/// One piece of text: where, and the runs of it in their colours.
#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    x: f32,
    y: f32,
    small: bool,
    runs: Vec<(String, [u8; 3])>,
}

/// Everything the HUD shows this frame; it is rebuilt when this changes.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct HudView {
    size: UVec2,
    lines: Vec<Line>,
    pics: Vec<(&'static str, i32, f32, f32)>,
    bar: Option<(f32, f32, f32, f32, [u8; 3])>,
}

const GREY: [u8; 3] = [0xBF, 0xBF, 0xBF];
const YELLOW: [u8; 3] = [0xBF, 0xBF, 0x00];
const MESSAGE: [u8; 3] = [0xA0, 0xA0, 0x20];
const EVAC: [u8; 3] = [0xD0, 0xC0, 0x60];
const LIFE: [u8; 3] = [0x00, 0xB0, 0x00];

/// Arial, or a face with its metrics, from the system.
pub fn load_fonts(fonts: &mut Assets<Font>) -> HudFonts {
    let mut find = |names: &[&str]| -> Option<Handle<Font>> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let mut dirs: Vec<std::path::PathBuf> = [
            "/usr/share/fonts/liberation",
            "/usr/share/fonts/liberation-fonts",
            "/usr/share/fonts/liberation-sans",
            "/usr/share/fonts/liberation-sans-fonts",
            "/usr/share/fonts/truetype/liberation",
            "/usr/share/fonts/truetype/liberation2",
            "/usr/share/fonts/TTF",
            "/usr/share/fonts/truetype/dejavu",
            "/usr/share/fonts/dejavu",
            "/usr/share/fonts/dejavu-sans-fonts",
            "/usr/local/share/fonts",
            "C:\\Windows\\Fonts",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
        if let Some(h) = home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
        if let Some(w) = std::env::var_os("WINDIR") {
            dirs.push(std::path::PathBuf::from(w).join("Fonts"));
        }
        for d in &dirs {
            for n in names {
                let p = d.join(n);
                if let Ok(bytes) = std::fs::read(&p) {
                    if let Ok(f) = Font::try_from_bytes(bytes) {
                        return Some(fonts.add(f));
                    }
                }
            }
        }
        None
    };
    let small =
        find(&["LiberationSans-Regular.ttf", "arial.ttf", "DejaVuSans.ttf"]).unwrap_or_default();
    let midd = find(&[
        "LiberationSans-Bold.ttf",
        "arialbd.ttf",
        "DejaVuSans-Bold.ttf",
    ])
    .unwrap_or_else(|| small.clone());
    HudFonts { midd, small }
}

fn load_pic(root: &DataRoot, rel: &str, images: &mut Assets<Image>) -> Option<Pic> {
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
        p.rgba.clone(),
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::nearest();
    Some(Pic {
        image: images.add(img),
        size: UVec2::new(p.width as u32, p.height as u32),
    })
}

/// The trophy card's lines, relative to its corner.
fn card_lines(sim: &Sim, t: &TrophyItem, ar: Option<&Arsenal>, imperial: bool) -> Vec<Line> {
    let Some(d) = sim.dinos.get(t.ctype.max(0) as usize) else {
        return Vec::new();
    };
    let (x0, y0) = (14.0, 18.0);
    let line = |dy: f32, runs: Vec<(String, [u8; 3])>| Line {
        x: x0,
        y: y0 + dy,
        small: true,
        runs,
    };
    let weight = d.mass * t.scale * t.scale;
    let weight = if imperial {
        format!("{:3.2}t ", weight / 0.907)
    } else {
        format!("{weight:3.2}T ")
    };
    let length = d.length * t.scale;
    let length = if imperial {
        format!("{:3.2}ft", length / 0.3)
    } else {
        format!("{length:3.2}m")
    };
    let wname = ar
        .and_then(|a| a.info.get(t.weapon.max(0) as usize))
        .map(|w| w.name.clone())
        .unwrap_or_default();
    let range = if imperial {
        format!("{:3.1}ft", t.range / 0.3)
    } else {
        format!("{:3.1}m", t.range)
    };
    let (day, mon, year) = (t.date & 255, (t.date >> 10) & 255, t.date >> 20);
    let date = if imperial {
        format!("{mon}.{day}.{year}   ")
    } else {
        format!("{day}.{mon}.{year}   ")
    };
    let time = format!("{}:{:02}", (t.time >> 10) & 255, t.time & 255);
    vec![
        line(0.0, vec![("Name: ".into(), GREY), (d.name.clone(), YELLOW)]),
        line(
            16.0,
            vec![
                ("Weight: ".into(), GREY),
                (weight, YELLOW),
                ("Length: ".into(), GREY),
                (length, YELLOW),
            ],
        ),
        line(
            32.0,
            vec![
                ("Weapon: ".into(), GREY),
                (format!("{wname}    "), YELLOW),
                ("Score: ".into(), GREY),
                (t.score.to_string(), YELLOW),
            ],
        ),
        line(
            48.0,
            vec![("Range of kill: ".into(), GREY), (range, YELLOW)],
        ),
        line(
            64.0,
            vec![
                ("Date: ".into(), GREY),
                (date, YELLOW),
                ("Time: ".into(), GREY),
                (time, YELLOW),
            ],
        ),
    ]
}

/// The nearest live creature near the middle of the binoculars' view that
/// can be seen.
fn scan_life(sim: &mut Sim, area: &Area, eye: Vec3, rot: Quat, view_r: i32) -> Option<usize> {
    let inv = rot.inverse();
    let mut best = None;
    let mut dm = (view_r + 2) as f32 * 256.0;
    for i in 0..sim.chars.len() {
        let c = &sim.chars[i];
        if c.health == 0 || c.removed {
            continue;
        }
        let rp = inv * (c.pos - eye);
        if rp.z > -512.0 {
            continue;
        }
        let d = rp.length();
        if d > view_r as f32 * 256.0 {
            continue;
        }
        if (rp.x.abs() + rp.y.abs()) / d > 0.15 {
            continue;
        }
        let from = c.pos + Vec3::Y * 220.0;
        if d < dm && !sim.trace_look(area, from, eye) {
            dm = d;
            best = Some(i);
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut flow: ResMut<HuntFlow>,
    pics: Option<Res<HudPics>>,
    fonts: Res<HudFonts>,
    (root, settings, radius, debug): (
        Res<DataRoot>,
        Res<Settings>,
        Res<crate::ViewRadius>,
        Res<DebugMode>,
    ),
    area: Option<Res<Area>>,
    player: Option<Res<Player>>,
    mut sim: Option<ResMut<Sim>>,
    (arsenal, mut profile): (Option<Res<Arsenal>>, ResMut<CurrentProfile>),
    (mut view, mut screen, mut next): (
        ResMut<ViewState>,
        ResMut<Screen>,
        ResMut<NextState<AppState>>,
    ),
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window>,
    cam: Query<&Transform, With<MainCamera>>,
    old: Query<Entity, With<HudRoot>>,
    mut last: Local<Option<HudView>>,
) {
    let (Some(area), Some(player)) = (area, player) else {
        return;
    };
    let Some(pics) = pics else {
        let mut calls = HashMap::new();
        if let Some(s) = sim.as_deref() {
            for t in s.callable() {
                if let Some(n) = s.call_number(t) {
                    if let Some(p) = load_pic(
                        &root,
                        &format!("HUNTDAT/MENU/PICS/CALL{n}.TGA"),
                        &mut images,
                    ) {
                        calls.insert(n, p);
                    }
                }
            }
        }
        let card = if area.trophy {
            "HUNTDAT/MENU/TROPHY.TGA"
        } else {
            "HUNTDAT/MENU/TROPHY_G.TGA"
        };
        commands.insert_resource(HudPics {
            exit: load_pic(&root, "HUNTDAT/MENU/EXIT.TGA", &mut images),
            pause: load_pic(&root, "HUNTDAT/MENU/PAUSE.TGA", &mut images),
            card: load_pic(&root, card, &mut images),
            trophy_exit: load_pic(&root, "HUNTDAT/MENU/TROPHY_E.TGA", &mut images),
            calls,
        });
        return;
    };

    // ---- keys
    let alive = sim.as_ref().map(|s| s.my_health > 0).unwrap_or(true);
    let dying = sim.as_ref().map(|s| s.demo.time > 0).unwrap_or(false);
    if *screen == Screen::None {
        if keys.just_pressed(KeyCode::Pause) && !area.trophy {
            flow.pause = !flow.pause;
            flow.exit_mode = false;
            view.paused = flow.pause;
        }
        if flow.exit_mode {
            if keys.just_pressed(KeyCode::KeyN) {
                flow.exit_mode = false;
            }
            if keys.any_just_pressed([KeyCode::KeyY, KeyCode::Enter, KeyCode::NumpadEnter]) {
                if let Some(s) = sim.as_mut() {
                    s.exit_time = if alive { 4000 } else { 1 };
                }
                flow.exit_mode = false;
            }
            if keys.just_pressed(KeyCode::KeyR) {
                // Start the same hunt over.
                flow.exit_mode = false;
                flow.restart = true;
                commands.insert_resource(LoadRequest {
                    project: area.name.clone(),
                    frames: 0,
                });
                *screen = Screen::Loading;
                next.set(AppState::Loading);
                return;
            }
        }
    }
    // In the trophy room, R takes down the mount in front.
    if area.trophy && *screen == Screen::None && keys.just_pressed(KeyCode::KeyR) {
        if let (Some(s), Some(p)) = (sim.as_mut(), profile.0.as_mut()) {
            if let Some(slot) = s.trophy_in_front() {
                if let Some(b) = p.bodies.get_mut(slot) {
                    b.ctype = 0;
                }
                for c in s.chars.iter_mut().filter(|c| c.state == slot as i32) {
                    c.removed = true;
                }
            }
        }
    }
    // The death camera brings the prompt up after six seconds.
    if dying
        && sim.as_ref().map(|s| s.demo.time > 6000).unwrap_or(false)
        && !flow.pause
        && !flow.exit_mode
    {
        flow.exit_mode = true;
    }
    if let Some(s) = sim.as_mut() {
        if s.exit_done {
            s.exit_done = false;
            flow.exit_mode = false;
            *screen = Screen::Main;
            next.set(AppState::Menu);
            return;
        }
    }

    // ---- what to show
    let Ok(win) = windows.single() else { return };
    let (w, h) = (win.width(), win.height());
    // The games drew their pictures and text pixel for pixel at any
    // resolution, and so does this.
    let k: f32 = 1.0;
    let mut v = HudView {
        size: UVec2::new(w as u32, h as u32),
        ..default()
    };
    let centred = |p: &Option<Pic>| {
        p.as_ref().map(|p| {
            (
                (w - p.size.x as f32 * k) / 2.0,
                (h - p.size.y as f32 * k) / 2.0,
            )
        })
    };

    if let Some(s) = sim.as_deref() {
        if let Some((m, _)) = &s.message {
            v.lines.push(Line {
                x: 10.0 * k,
                y: 10.0 * k,
                small: false,
                runs: vec![(m.clone(), MESSAGE)],
            });
        }
        // The timer: the frame's milliseconds and triangles at the top
        // right, and (not in the first game) the sound environment of the
        // spot under the message.
        if debug.timer {
            let text = |x: f32, y: f32, t: String| Line {
                x,
                y,
                small: false,
                runs: vec![(t, MESSAGE)],
            };
            v.lines
                .push(text(w - 1.0 - 81.0, 11.0, format!("msc: {}", debug.msc)));
            v.lines.push(text(
                w - 1.0 - 90.0,
                24.0,
                format!("polys: {}", debug.polys),
            ));
            if !s.c1 {
                let zone = area.ambient_zone(player.cam.x, player.cam.z);
                let env = area
                    .rsc
                    .ambients
                    .get(zone)
                    .and_then(|a| a.rdata.first())
                    .map(|r| r.envir as i32)
                    .unwrap_or(0);
                v.lines.push(text(10.0, 24.0, env.to_string()));
            }
        }
        if s.exit_time > 0 {
            let y = h / 3.0;
            v.lines.push(Line {
                x: -1.0,
                y,
                small: false,
                runs: vec![("Preparing for evacuation...".into(), EVAC)],
            });
            v.lines.push(Line {
                x: -1.0,
                y: y + 18.0 * k,
                small: false,
                runs: vec![(format!("{} seconds left.", 1 + s.exit_time / 1000), EVAC)],
            });
        }
        // The call picked, for a moment after choosing it.
        if s.call_shown > 0 {
            if let Some(p) = s
                .call_ctype
                .and_then(|t| s.call_number(t))
                .and_then(|n| pics.calls.get(&n))
            {
                v.pics.push((
                    "call",
                    s.call_ctype.and_then(|t| s.call_number(t)).unwrap_or(0),
                    w - 10.0 * k - p.size.x as f32 * k,
                    7.0 * k,
                ));
            }
        }
        // Breath running out under water.
        let mh = s.my_health;
        if mh > 0 && mh < 100000 {
            let l = w / 4.0;
            let x0 = w - w / 20.0 - l;
            let y0 = h / 40.0;
            let g = (mh * 240 / 100000).min(160) as u8;
            let r = ((100000 - mh) * 240 / 100000).min(160) as u8;
            let l0 = (l * mh as f32 / 100000.0).round();
            v.bar = Some((x0, y0, l0, k, [r, g, 0]));
        }
        // The trophy card: for a while after the ship takes a kill, or in
        // the trophy room for the mount in front of the hunter.
        let card = if area.trophy {
            s.trophy_in_front()
                .and_then(|slot| profile.0.as_ref().and_then(|p| p.bodies.get(slot).copied()))
                .filter(|t| t.ctype > 0)
        } else if s.trophy_time > 0 {
            s.trophies.last().map(|t| TrophyItem {
                ctype: t.ctype as i32,
                weapon: t.weapon as i32,
                score: t.score,
                date: t.date,
                time: t.time,
                scale: t.scale,
                range: t.range,
                ..Default::default()
            })
        } else {
            None
        };
        if let (Some(t), Some(p)) = (card, &pics.card) {
            let (x0, y0) = if area.trophy {
                (
                    w - (p.size.x as f32 + 16.0) * k,
                    h - (p.size.y as f32 + 12.0) * k,
                )
            } else {
                (
                    (w - p.size.x as f32 * k) / 2.0,
                    h - (p.size.y as f32 + 12.0) * k,
                )
            };
            v.pics.push(("card", 0, x0, y0));
            for mut l in card_lines(s, &t, arsenal.as_deref(), settings.imperial) {
                l.x = x0 + l.x * k;
                l.y = y0 + l.y * k;
                v.lines.push(l);
            }
        }
    }
    // What the binoculars make out.
    if let (Some(ar), Some(s), Ok(ct)) = (arsenal.as_deref(), sim.as_mut(), cam.single()) {
        if ar.binoculars && !dying {
            if let Some(i) = scan_life(s, &area, ct.translation, ct.rotation, radius.r) {
                let c = &s.chars[i];
                let d = &s.dinos[c.ctype];
                let (x, y) = (w / 2.0 + w / 64.0, h / 2.0 + h / 6.8);
                let weight = d.mass * c.scale * c.scale;
                let r = ((c.pos - Vec3::new(player.x, player.y, player.z)).length() * 3.0 / 64.0)
                    as i32;
                let (wt, dist) = if settings.imperial {
                    (
                        format!("Weight: {:3.2}t ", weight / 0.907),
                        format!("Distance: {r}ft "),
                    )
                } else {
                    (
                        format!("Weight: {weight:3.2}T "),
                        format!("Distance: {}m  ", r / 3),
                    )
                };
                for (n, t) in [d.name.clone(), wt, dist].into_iter().enumerate() {
                    v.lines.push(Line {
                        x,
                        y: y + n as f32 * 16.0 * k,
                        small: true,
                        runs: vec![(t, LIFE)],
                    });
                }
            }
        }
    }
    // Where the hunter is (F10), and the argument that puts him back there.
    if flow.showpos {
        let (px, py, pz) = (player.x, player.y, player.z);
        let tile = |v: f32| format!("{}.{:02}", (v / 256.0) as i32, (v / 2.56) as i32 % 100);
        let deg = |a: f32| (a.to_degrees() as i32).rem_euclid(360);
        let (yaw, pitch) = (deg(player.cam_alpha), player.cam_beta.to_degrees() as i32);
        let c = [0xE0, 0xE0, 0x20];
        let lines = [
            (
                format!("pos  {} {} {}  (world)", px as i32, py as i32, pz as i32),
                c,
            ),
            (format!("tile {} {}", tile(px), tile(pz)), c),
            (format!("yaw {yaw}  pitch {pitch}"), c),
            (
                format!("campos={},{},{}", tile(px), tile(pz), deg(player.alpha)),
                [0x40, 0xFF, 0xFF],
            ),
            (
                format!("viewR {} / {}   fov {}", radius.r, radius.r1, settings.fov),
                c,
            ),
        ];
        for (n, (t, c)) in lines.into_iter().enumerate() {
            v.lines.push(Line {
                x: 10.0 * k,
                y: (11.0 + 14.0 * n as f32) * k,
                small: true,
                runs: vec![(t, c)],
            });
        }
    }
    if area.trophy {
        if let Some(p) = &pics.trophy_exit {
            v.pics
                .push(("trophy_exit", 0, (w - p.size.x as f32 * k) / 2.0, 2.0 * k));
        }
    }
    if flow.exit_mode {
        if let Some((x, y)) = centred(&pics.exit) {
            v.pics.push(("exit", 0, x, y));
        }
    }
    if flow.pause {
        if let Some((x, y)) = centred(&pics.pause) {
            v.pics.push(("pause", 0, x, y));
        }
    }

    if last.as_ref() == Some(&v) && !old.is_empty() {
        return;
    }
    for e in old.iter() {
        commands.entity(e).despawn();
    }
    let root_e = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Pickable::IGNORE,
            HudRoot,
            AreaEntity,
        ))
        .id();
    let abs = |x: f32, y: f32, w: f32, h: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x),
        top: Val::Px(y),
        width: Val::Px(w),
        height: Val::Px(h),
        ..default()
    };
    for (name, n, x, y) in &v.pics {
        let p = match *name {
            "exit" => pics.exit.as_ref(),
            "pause" => pics.pause.as_ref(),
            "card" => pics.card.as_ref(),
            "trophy_exit" => pics.trophy_exit.as_ref(),
            _ => pics.calls.get(n),
        };
        if let Some(p) = p {
            commands.spawn((
                ImageNode::new(p.image.clone()),
                abs(*x, *y, p.size.x as f32 * k, p.size.y as f32 * k),
                ChildOf(root_e),
            ));
        }
    }
    if let Some((x0, y0, l0, k, [r, g, b])) = v.bar {
        // A dark edge, then the bar in its colour.
        commands.spawn((
            abs(x0 - k, y0, l0 + 2.0 * k, 4.0 * k),
            BackgroundColor(Color::srgba_u8(0, 0, 0x10, 0xF0)),
            ChildOf(root_e),
        ));
        commands.spawn((
            abs(x0, y0 + k, l0, 2.0 * k),
            BackgroundColor(Color::srgba_u8(r, g, b, 0xF0)),
            ChildOf(root_e),
        ));
    }
    for l in &v.lines {
        let font = if l.small {
            fonts.small.clone()
        } else {
            fonts.midd.clone()
        };
        let size = if l.small { 12.2 } else { 13.9 } * k;
        let mut node = Node {
            position_type: PositionType::Absolute,
            top: Val::Px(l.y),
            ..default()
        };
        if l.x < 0.0 {
            // Centred across the window.
            node.width = Val::Percent(100.0);
            node.justify_content = JustifyContent::Center;
        } else {
            node.left = Val::Px(l.x + k);
        }
        let holder = commands.spawn((node, ChildOf(root_e))).id();
        let (first, rest) = l.runs.split_first().expect("a line has text");
        let t = commands
            .spawn((
                Text::new(first.0.clone()),
                TextFont {
                    font: font.clone(),
                    font_size: size,
                    ..default()
                },
                TextColor(Color::srgb_u8(first.1[0], first.1[1], first.1[2])),
                TextShadow {
                    offset: Vec2::splat(k),
                    color: Color::srgb_u8(0x10, 0x10, 0x10),
                },
                ChildOf(holder),
            ))
            .id();
        for (s, c) in rest {
            commands.spawn((
                TextSpan::new(s.clone()),
                TextFont {
                    font: font.clone(),
                    font_size: size,
                    ..default()
                },
                TextColor(Color::srgb_u8(c[0], c[1], c[2])),
                ChildOf(t),
            ));
        }
    }
    *last = Some(v);
}

/// What the timer shows: the frame's milliseconds and the triangles of the
/// meshes drawn (last frame's; the count is only taken while it shows).
pub fn frame_stats(
    time: Res<Time>,
    meshes: Res<Assets<Mesh>>,
    drawn: Query<(&Mesh3d, &ViewVisibility)>,
    mut debug: ResMut<DebugMode>,
) {
    if !debug.timer {
        return;
    }
    let msc = (time.delta_secs() * 1000.0) as i32;
    let polys = drawn
        .iter()
        .filter(|(_, v)| v.get())
        .filter_map(|(m, _)| meshes.get(&m.0))
        .map(|m| {
            m.indices()
                .map(|i| i.len())
                .unwrap_or_else(|| m.count_vertices())
                / 3
        })
        .sum();
    if debug.msc != msc || debug.polys != polys {
        debug.msc = msc;
        debug.polys = polys;
    }
}

/// The letter a key types, for the debug code.
fn key_letter(k: KeyCode) -> Option<u8> {
    use KeyCode as K;
    const LETTERS: [KeyCode; 26] = [
        K::KeyA,
        K::KeyB,
        K::KeyC,
        K::KeyD,
        K::KeyE,
        K::KeyF,
        K::KeyG,
        K::KeyH,
        K::KeyI,
        K::KeyJ,
        K::KeyK,
        K::KeyL,
        K::KeyM,
        K::KeyN,
        K::KeyO,
        K::KeyP,
        K::KeyQ,
        K::KeyR,
        K::KeyS,
        K::KeyT,
        K::KeyU,
        K::KeyV,
        K::KeyW,
        K::KeyX,
        K::KeyY,
        K::KeyZ,
    ];
    LETTERS.iter().position(|&l| l == k).map(|i| b'A' + i as u8)
}

/// The developer keys, each game's own. The second and
/// third games: typing DEBUGUP turns debug mode on or off, and Shift with M,
/// F, L, C, E, G or P switches models, fog, flying, cloud shadows and the
/// weapon's environment mapping, lighting and highlights - all only with
/// Options -> Debug keys (or -debug) - while in debug mode U, I, O, P, [ and
/// ] move the view radii and Shift+S and Shift+T slow the game and show the
/// timer; F9 leaves the hunt and F10 shows the position. The first game:
/// DEBUGON, with or without the option; [ and ] (whose radius the next frame
/// puts back), Shift+S and Shift+T in debug mode; F9 quits.
#[allow(clippy::too_many_arguments)]
pub fn debug_keys(
    mut typed: EventReader<bevy::input::keyboard::KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    (kind, launch, radius): (Res<GameKind>, Res<crate::Launch>, Res<crate::ViewRadius>),
    mut settings: ResMut<Settings>,
    (mut flow, mut debug): (ResMut<HuntFlow>, ResMut<DebugMode>),
    screen: Res<Screen>,
    mut player: Option<ResMut<Player>>,
    mut sim: Option<ResMut<Sim>>,
    mut next: ResMut<NextState<AppState>>,
    mut objects: Query<&mut Visibility, With<crate::world::MapObject>>,
    (mut vtime, mut exit): (ResMut<Time<Virtual>>, EventWriter<AppExit>),
    mut models_off: Local<bool>,
    mut seeded: Local<bool>,
) {
    if !*seeded {
        *seeded = true;
        flow.showpos |= launch.showpos;
    }
    if *screen != Screen::None {
        typed.clear();
        return;
    }
    let c1 = *kind == GameKind::Carnivores;
    let option = settings.debug_keys || launch.debug;
    let say = |sim: &mut Option<ResMut<Sim>>, what: &str| {
        if let Some(s) = sim.as_mut() {
            s.add_message(what);
        }
    };
    let switch = |sim: &mut Option<ResMut<Sim>>, what: &str, on: bool| {
        say(sim, &format!("{what} is {}", if on { "ON" } else { "OFF" }));
    };

    // The code: every key either types its next letter or starts it over.
    let code: &[u8; 7] = if c1 { b"DEBUGON" } else { b"DEBUGUP" };
    for ev in typed.read() {
        if !ev.state.is_pressed() || !(c1 || option) {
            continue;
        }
        if key_letter(ev.key_code) == Some(code[debug.typed]) {
            debug.typed += 1;
            if debug.typed > 6 {
                debug.typed = 0;
                debug.on = !debug.on;
                switch(&mut sim, "Debug mode", debug.on);
            }
        } else {
            debug.typed = 0;
        }
    }

    let shift = option && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let slow_timer = |debug: &mut DebugMode, sim: &mut Option<ResMut<Sim>>| {
        if keys.just_pressed(KeyCode::KeyS) {
            debug.slow = !debug.slow;
            switch(sim, "Slow mode", debug.slow);
        }
        if keys.just_pressed(KeyCode::KeyT) {
            debug.timer = !debug.timer;
            switch(sim, "Timer", debug.timer);
        }
    };

    if c1 {
        if debug.on {
            for (key, d) in [(KeyCode::BracketLeft, -2), (KeyCode::BracketRight, 2)] {
                if keys.just_pressed(key) {
                    let r = (radius.r + d).clamp(10, 60);
                    say(&mut sim, &format!("ViewR = {r}"));
                }
            }
            if shift {
                slow_timer(&mut debug, &mut sim);
            }
        }
        if option && keys.just_pressed(KeyCode::F9) {
            exit.write(AppExit::Success);
        }
    } else {
        if keys.just_pressed(KeyCode::F10) {
            flow.showpos = !flow.showpos;
        }
        if option && keys.just_pressed(KeyCode::F9) {
            // Out at once, keeping nothing from the hunt.
            flow.restart = true;
            next.set(AppState::Menu);
            return;
        }
        let dbg = debug.on && option;
        if dbg {
            let mut moves = vec![
                (KeyCode::KeyU, (0, 0, -2)),
                (KeyCode::KeyI, (0, 0, 2)),
                (KeyCode::KeyO, (0, -2, 0)),
                (KeyCode::BracketLeft, (-2, 0, 0)),
                (KeyCode::BracketRight, (2, 0, 0)),
            ];
            if !shift {
                moves.push((KeyCode::KeyP, (0, 2, 0)));
            }
            for (key, (d1, d2, d3)) in moves {
                if !keys.just_pressed(key) {
                    continue;
                }
                let (r, r1, rm) = debug.view.unwrap_or((settings.view_radius(), 28, 24));
                let r = (r + d1).clamp(20, 122);
                let r1 = (r1 + d2).max(12).min(r - 10);
                let rm = (rm + d3).clamp(4, 60);
                debug.view = Some((r, r1, rm));
                say(
                    &mut sim,
                    &format!("ViewR = {r} ({r1} + {}) BMP at {rm}", r - r1),
                );
            }
            if shift {
                slow_timer(&mut debug, &mut sim);
            }
        }
        if shift {
            if keys.just_pressed(KeyCode::KeyM) {
                *models_off = !*models_off;
                for mut v in objects.iter_mut() {
                    *v = if *models_off {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    };
                }
                switch(&mut sim, "Draw 3D models", !*models_off);
            }
            if keys.just_pressed(KeyCode::KeyF) {
                settings.fog = !settings.fog;
                switch(&mut sim, "V.Fog", settings.fog);
            }
            if keys.just_pressed(KeyCode::KeyL) {
                if let Some(p) = player.as_mut() {
                    p.fly = !p.fly;
                    let on = p.fly;
                    switch(&mut sim, "Fly", on);
                }
            }
            if keys.just_pressed(KeyCode::KeyC) {
                settings.clouds = !settings.clouds;
                switch(&mut sim, "Clouds shadow", settings.clouds);
            }
            if keys.just_pressed(KeyCode::KeyE) {
                debug.no_envmap = !debug.no_envmap;
                switch(&mut sim, "Env.Mapping", !debug.no_envmap);
            }
            if keys.just_pressed(KeyCode::KeyG) {
                debug.no_gour = !debug.no_gour;
                switch(&mut sim, "Gour.Mapping", !debug.no_gour);
            }
            if keys.just_pressed(KeyCode::KeyP) {
                debug.no_phong = !debug.no_phong;
                switch(&mut sim, "Phong Mapping", !debug.no_phong);
            }
        }
    }
    // Slow motion: the game's clock at a quarter of the real one.
    let speed = if debug.slow { 0.25 } else { 1.0 };
    if vtime.relative_speed() != speed {
        vtime.set_relative_speed(speed);
    }
}

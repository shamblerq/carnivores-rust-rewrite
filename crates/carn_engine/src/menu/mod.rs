//! The games' own menus: pictures authored at 800x600, a second picture
//! with every button lit, and a half-size map saying which button each
//! spot belongs to. The screen is composed from those at 800x600 and shown
//! scaled into the window with bars at the sides or top; text is drawn over
//! it in the same coordinates, at the window's own resolution.
//!
//! Carnivores 2 and Ice Age share one set of screens (c2.rs), rebuilt from
//! the original artwork (after the Carn2-Menu project); the first
//! game has its own (c1.rs).

pub mod c1;
pub mod c2;

use std::sync::Arc;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseWheel;
use bevy::input::ButtonState;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use carn_formats::tga::Picture;
use carn_formats::wav::Wave;

use crate::audio::{wave_to_pcm, Mixer, Pcm};
use crate::game::GameKind;
use crate::hud::HudFonts;
use crate::paths::DataRoot;
use crate::profile::Profile;
use crate::settings::Settings;
use crate::{AppState, LoadRequest};

pub const W: i32 = 800;
pub const H: i32 = 600;

/// A picture, 8-bit RGBA.
#[derive(Clone)]
pub struct Pic {
    pub w: i32,
    pub h: i32,
    pub rgba: Arc<Vec<u8>>,
}

impl PartialEq for Pic {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.rgba, &o.rgba)
    }
}

impl std::fmt::Debug for Pic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pic({}x{})", self.w, self.h)
    }
}

pub fn load_pic(root: &DataRoot, rel: &str) -> Option<Pic> {
    let p = root.find(rel)?;
    match Picture::load(&p) {
        Ok(p) => Some(Pic {
            w: p.width as i32,
            h: p.height as i32,
            rgba: Arc::new(p.rgba),
        }),
        Err(e) => {
            warn!("{e}");
            None
        }
    }
}

/// A screen's pictures: plain, lit, and which button is where.
pub struct Art {
    pub bg: Pic,
    pub on: Option<Pic>,
    pub map: Option<Vec<u8>>,
}

impl Art {
    pub fn load(root: &DataRoot, bg: &str, on: Option<&str>, map: Option<&str>) -> Option<Art> {
        let bg = load_pic(root, bg)?;
        let on = on
            .and_then(|p| load_pic(root, p))
            .filter(|p| p.w == bg.w && p.h == bg.h);
        let map = map
            .and_then(|m| root.find(m))
            .and_then(|p| std::fs::read(p).ok())
            .filter(|m| m.len() >= (W * H / 4) as usize);
        Some(Art { bg, on, map })
    }

    /// The button at a spot of the 800x600 screen, 0 for none.
    pub fn id(&self, x: i32, y: i32) -> u8 {
        let Some(m) = &self.map else { return 0 };
        let (mx, my) = (x / 2, y / 2);
        if x < 0 || y < 0 || mx >= W / 2 || my >= H / 2 {
            return 0;
        }
        m[(my * (W / 2) + mx) as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Big,
    Midd,
    Small,
    Opt,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Right,
    Center,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Label {
    pub face: Face,
    pub x: i32,
    pub y: i32,
    pub s: String,
    pub rgb: [u8; 3],
    pub align: Align,
}

/// What a screen shows this frame.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Frame {
    /// Buttons drawn lit: the one under the pointer, and those switched on.
    pub lit: Vec<u8>,
    pub blits: Vec<(Pic, i32, i32)>,
    pub rects: Vec<(i32, i32, i32, i32, [u8; 3])>,
    pub texts: Vec<Label>,
}

impl Frame {
    pub fn text(
        &mut self,
        face: Face,
        x: i32,
        y: i32,
        s: impl Into<String>,
        rgb: [u8; 3],
        align: Align,
    ) {
        let s = s.into();
        if !s.is_empty() {
            self.texts.push(Label {
                face,
                x,
                y,
                s,
                rgb,
                align,
            });
        }
    }
}

/// The input of a frame, in the artwork's coordinates.
#[derive(Default)]
pub struct In {
    pub x: i32,
    pub y: i32,
    /// The left button went down this frame; is down.
    pub click: bool,
    pub held: bool,
    pub enter: bool,
    pub esc: bool,
    pub delete: bool,
    pub backspace: bool,
    pub chars: Vec<char>,
    pub wheel: i32,
    /// For rebinding: the key or button pressed this frame, if any.
    pub key: Option<KeyCode>,
    pub button: Option<MouseButton>,
    /// Milliseconds since the menu opened (the caret blinks by it).
    pub time_ms: u64,
}

/// What a screen asks for when it is done.
pub enum Outcome {
    Stay,
    Hunt(HuntSetup),
    Quit,
}

/// What the hunt screen chose.
#[derive(Resource, Clone, Debug, Default)]
pub struct HuntSetup {
    pub project: String,
    pub trophy: bool,
    /// Hunt kinds picked: a bit per AI id from 10 (the first game: per
    /// kind 0-6).
    pub target_dino: u32,
    /// Weapons taken, a bit each.
    pub weapons: u32,
    pub camo: bool,
    pub radar: bool,
    pub scent: bool,
    pub double_ammo: bool,
    pub tranq: bool,
    pub observer: bool,
}

/// The menu's own sounds (MENUAMB, MENUGO, MENUMOV).
#[derive(Default)]
struct Sounds {
    ambient: Option<Pcm>,
    go: Option<Pcm>,
    mv: Option<Pcm>,
}

pub enum Screens {
    C2(Box<c2::Menu>),
    C1(Box<c1::Menu>),
}

/// The menu while it is up.
#[derive(Resource)]
pub struct MenuCtl {
    screens: Screens,
    art: Option<Art>,
    art_key: String,
    sounds: Sounds,
    last_hover: u8,
    image: Handle<Image>,
    canvas: Vec<u8>,
    last: Option<(Frame, UVec2)>,
    start: std::time::Instant,
}

/// The current hunter.
#[derive(Resource, Clone, Default)]
pub struct CurrentProfile(pub Option<Profile>);

/// What the hunt screens had chosen when the menu last closed, for the
/// session: the games kept those choices in memory from one visit of the
/// menu to the next.
#[derive(Resource, Default)]
pub struct HuntChoices {
    c1: Option<c1::Choices>,
    c2: Option<c2::Choices>,
}

#[derive(Component)]
struct MenuRoot;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentProfile>()
            .init_resource::<HuntChoices>()
            .add_systems(OnEnter(AppState::Menu), open)
            .add_systems(OnExit(AppState::Menu), close)
            .add_systems(OnEnter(AppState::Loading), loading_on)
            .add_systems(OnExit(AppState::Loading), loading_off)
            .add_systems(Update, run.run_if(in_state(AppState::Menu)));
    }
}

/// The pictures a screen needs, by name, loaded when it comes up.
pub fn art_for(root: &DataRoot, key: &str) -> Option<Art> {
    let m = |n: &str| format!("HUNTDAT/MENU/{n}");
    let (bg, on, map) = match key {
        "register" => ("MENUR.TGA", Some("MENUR_ON.TGA"), Some("MR_MAP.RAW")),
        "delete" => ("MENUD.TGA", Some("MENUD_ON.TGA"), Some("MD_MAP.RAW")),
        "waiver" => ("MENUL.TGA", Some("MENUL_ON.TGA"), Some("ML_MAP.RAW")),
        "stats" => ("MENUS.TGA", None, None),
        "main" => ("MENUM.TGA", Some("MENUM_ON.TGA"), Some("MAIN_MAP.RAW")),
        "hunt" => ("MENU2.TGA", Some("MENU2_ON.TGA"), Some("M2_MAP.RAW")),
        "options" => ("OPT_OFF.TGA", Some("OPT_ON.TGA"), Some("OPT_MAP.RAW")),
        "credits" => ("CREDITS.TGA", None, None),
        "quit" => ("MENUQ.TGA", Some("MENUQ_ON.TGA"), Some("MQ_MAP.RAW")),
        // the first game's
        "location" => ("LOC_OFF.TGA", Some("LOC_ON.TGA"), Some("LOC_MAP.RAW")),
        "weapons" => ("WEP_OFF.TGA", Some("WEP_ON.TGA"), Some("WEP_MAP.RAW")),
        "menua" => ("MENUA.TGA", None, None),
        "menue" => ("MENUE.TGA", None, None),
        _ => return None,
    };
    Art::load(root, &m(bg), on.map(m).as_deref(), map.map(m).as_deref())
}

#[allow(clippy::too_many_arguments)]
fn open(
    mut commands: Commands,
    root: Res<DataRoot>,
    kind: Res<GameKind>,
    settings: Res<Settings>,
    profile: Res<CurrentProfile>,
    mixer: Res<Mixer>,
    mut images: ResMut<Assets<Image>>,
    existing: Option<Res<MenuCtl>>,
    choices: Res<HuntChoices>,
) {
    if existing.is_some() {
        return;
    }
    let load = |n: &str| {
        root.find(&format!("HUNTDAT/SOUNDFX/{n}"))
            .and_then(|p| Wave::load(&p).ok())
            .map(|w| wave_to_pcm(&w))
    };
    let sounds = Sounds {
        ambient: load("MENUAMB.WAV"),
        go: load("MENUGO.WAV"),
        mv: load("MENUMOV.WAV"),
    };
    mixer.stop_all();
    mixer.set_listener(Vec3::new(1024.0, 0.0, 0.0), 0.0);
    mixer.set_ambient(sounds.ambient.as_ref(), 200);
    let img = Image::new_fill(
        Extent3d {
            width: W as u32,
            height: H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let screens = if *kind == GameKind::Carnivores {
        let mut m = c1::Menu::new(&root, &settings, profile.0.clone());
        if let Some(c) = &choices.c1 {
            m.restore(c);
        }
        Screens::C1(Box::new(m))
    } else {
        let mut m = c2::Menu::new(&root, *kind, &settings, profile.0.clone());
        if let Some(c) = &choices.c2 {
            m.restore(c);
        }
        Screens::C2(Box::new(m))
    };
    commands.insert_resource(MenuCtl {
        screens,
        art: None,
        art_key: String::new(),
        sounds,
        last_hover: 0,
        image: images.add(img),
        canvas: vec![0; (W * H * 4) as usize],
        last: None,
        start: std::time::Instant::now(),
    });
}

#[derive(Component)]
struct LoadingRoot;

/// The loading picture: the empty bar, the top half
/// of LOADING.TGA, in the middle of the screen.
fn loading_on(
    mut commands: Commands,
    root: Res<DataRoot>,
    windows: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(p) = load_pic(&root, "HUNTDAT/MENU/LOADING.TGA") else {
        return;
    };
    let Ok(win) = windows.single() else { return };
    let (_, _, s) = board(win);
    let h = p.h / 2;
    let mut img = Image::new(
        Extent3d {
            width: p.w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        p.rgba[..(p.w * h * 4) as usize].to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::nearest();
    let r = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(25),
            LoadingRoot,
        ))
        .id();
    commands.spawn((
        ImageNode::new(images.add(img)),
        Node {
            width: Val::Px(p.w as f32 * s),
            height: Val::Px(h as f32 * s),
            ..default()
        },
        ChildOf(r),
    ));
}

fn loading_off(mut commands: Commands, q: Query<Entity, With<LoadingRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

fn close(
    mut commands: Commands,
    q: Query<Entity, With<MenuRoot>>,
    mixer: Res<Mixer>,
    ctl: Option<Res<MenuCtl>>,
    mut choices: ResMut<HuntChoices>,
) {
    match ctl.as_deref().map(|c| &c.screens) {
        Some(Screens::C1(m)) => choices.c1 = Some(m.choices()),
        Some(Screens::C2(m)) => choices.c2 = Some(m.choices()),
        None => {}
    }
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<MenuCtl>();
    mixer.set_ambient(None, 0);
}

/// Where the 800x600 screen sits in the window, and its scale.
fn board(win: &Window) -> (f32, f32, f32) {
    let (w, h) = (win.width(), win.height());
    let s = (w / W as f32).min(h / H as f32);
    ((w - W as f32 * s) / 2.0, (h - H as f32 * s) / 2.0, s)
}

fn compose(canvas: &mut [u8], art: Option<&Art>, f: &Frame) {
    match art {
        Some(a) => {
            canvas.copy_from_slice(&a.bg.rgba[..(W * H * 4) as usize]);
            if let (Some(on), Some(map)) = (&a.on, &a.map) {
                if !f.lit.is_empty() {
                    for my in 0..(H / 2) {
                        for mx in 0..(W / 2) {
                            let id = map[(my * (W / 2) + mx) as usize];
                            if id == 0 || !f.lit.contains(&id) {
                                continue;
                            }
                            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                                let i = (((my * 2 + dy) * W + mx * 2 + dx) * 4) as usize;
                                canvas[i..i + 4].copy_from_slice(&on.rgba[i..i + 4]);
                            }
                        }
                    }
                }
            }
        }
        None => canvas
            .chunks_mut(4)
            .for_each(|p| p.copy_from_slice(&[0, 0, 0, 255])),
    }
    for (p, x, y) in &f.blits {
        for row in 0..p.h {
            let ty = y + row;
            if !(0..H).contains(&ty) {
                continue;
            }
            for col in 0..p.w {
                let tx = x + col;
                if !(0..W).contains(&tx) {
                    continue;
                }
                let s = ((row * p.w + col) * 4) as usize;
                let d = ((ty * W + tx) * 4) as usize;
                canvas[d..d + 3].copy_from_slice(&p.rgba[s..s + 3]);
                canvas[d + 3] = 255;
            }
        }
    }
    for &(x, y, w, h, c) in &f.rects {
        for ty in y.max(0)..(y + h).min(H) {
            for tx in x.max(0)..(x + w).min(W) {
                let d = ((ty * W + tx) * 4) as usize;
                canvas[d..d + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    ctl: Option<ResMut<MenuCtl>>,
    root: Res<DataRoot>,
    kind: Res<GameKind>,
    mut settings: ResMut<Settings>,
    mut profile: ResMut<CurrentProfile>,
    fonts: Res<HudFonts>,
    mixer: Res<Mixer>,
    (keys, buttons): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    (mut kb, mut wheel): (EventReader<KeyboardInput>, EventReader<MouseWheel>),
    windows: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
    old: Query<Entity, With<MenuRoot>>,
    mut next: ResMut<NextState<AppState>>,
    mut screen: ResMut<crate::ui::Screen>,
    mut exit: EventWriter<AppExit>,
) {
    let Some(mut ctl) = ctl else { return };
    // An error from a hunt that would not load is up over the menu.
    if matches!(*screen, crate::ui::Screen::Error(_)) {
        return;
    }
    let Ok(win) = windows.single() else { return };
    let (ox, oy, s) = board(win);
    let cur = win.cursor_position().unwrap_or(Vec2::splat(-1.0));
    let mut inp = In {
        x: ((cur.x - ox) / s).floor() as i32,
        y: ((cur.y - oy) / s).floor() as i32,
        click: buttons.just_pressed(MouseButton::Left),
        held: buttons.pressed(MouseButton::Left),
        enter: keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
        esc: keys.just_pressed(KeyCode::Escape),
        delete: keys.just_pressed(KeyCode::Delete),
        backspace: keys.just_pressed(KeyCode::Backspace),
        time_ms: ctl.start.elapsed().as_millis() as u64,
        ..default()
    };
    for ev in kb.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if inp.key.is_none() {
            inp.key = Some(ev.key_code);
        }
        if let Key::Character(c) = &ev.logical_key {
            inp.chars.extend(c.chars().filter(|c| !c.is_control()));
        } else if ev.logical_key == Key::Space {
            inp.chars.push(' ');
        }
    }
    for ev in wheel.read() {
        inp.wheel += ev.y.signum() as i32;
    }
    for b in [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ] {
        if buttons.just_pressed(b) {
            inp.button = Some(b);
        }
    }

    // ---- the screen's logic and what it shows
    let ctl = &mut *ctl;
    let art_ref = ctl.art.as_ref();
    let (want_art, frame, outcome) = match &mut ctl.screens {
        Screens::C2(m) => m.frame(
            &inp,
            art_ref,
            &ctl.art_key,
            &root,
            *kind,
            &mut settings,
            &mut profile,
        ),
        Screens::C1(m) => m.frame(
            &inp,
            art_ref,
            &ctl.art_key,
            &root,
            &mut settings,
            &mut profile,
        ),
    };
    if want_art != ctl.art_key {
        ctl.art = art_for(&root, &want_art);
        ctl.art_key = want_art;
        ctl.last = None;
        // Draw the new screen next frame, once its logic has run on it.
        return;
    }

    // Moving onto a new button ticks; clicking one confirms.
    let hover = ctl.art.as_ref().map(|a| a.id(inp.x, inp.y)).unwrap_or(0);
    let pan = Some(Vec3::new(
        1024.0 + ((inp.x / 2) as f32 - 200.0) * 2.0,
        0.0,
        200.0,
    ));
    if hover != 0 && hover != ctl.last_hover {
        if let Some(p) = &ctl.sounds.mv {
            mixer.play(p, pan, 200);
        }
    }
    ctl.last_hover = hover;
    if inp.click && hover != 0 {
        if let Some(p) = &ctl.sounds.go {
            mixer.play(p, pan, 256);
        }
    }

    match outcome {
        Outcome::Stay => {}
        Outcome::Quit => {
            exit.write(AppExit::Success);
            return;
        }
        Outcome::Hunt(setup) => {
            commands.insert_resource(LoadRequest {
                project: setup.project.clone(),
                frames: 0,
            });
            commands.insert_resource(setup);
            *screen = crate::ui::Screen::Loading;
            next.set(AppState::Loading);
            return;
        }
    }

    // ---- drawing, when something changed
    let size = UVec2::new(win.width() as u32, win.height() as u32);
    let canvas_changed = ctl
        .last
        .as_ref()
        .map(|(f, _)| f.lit != frame.lit || f.blits != frame.blits || f.rects != frame.rects)
        .unwrap_or(true);
    let layout_changed = ctl
        .last
        .as_ref()
        .map(|(f, sz)| f.texts != frame.texts || *sz != size)
        .unwrap_or(true)
        || old.is_empty();
    if canvas_changed {
        let mut canvas = std::mem::take(&mut ctl.canvas);
        compose(&mut canvas, ctl.art.as_ref(), &frame);
        if let Some(img) = images.get_mut(&ctl.image) {
            img.data = Some(canvas.clone());
        }
        ctl.canvas = canvas;
    }
    if layout_changed {
        for e in old.iter() {
            commands.entity(e).despawn();
        }
        let rootn = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::BLACK),
                GlobalZIndex(20),
                Pickable::IGNORE,
                MenuRoot,
            ))
            .id();
        let boardn = commands
            .spawn((
                ImageNode::new(ctl.image.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(ox),
                    top: Val::Px(oy),
                    width: Val::Px(W as f32 * s),
                    height: Val::Px(H as f32 * s),
                    ..default()
                },
                ChildOf(rootn),
            ))
            .id();
        for t in &frame.texts {
            let (font, size) = match t.face {
                Face::Big => (fonts.midd.clone(), 20.0),
                Face::Midd => (fonts.midd.clone(), 13.9),
                Face::Small => (fonts.small.clone(), 12.2),
                Face::Opt => (fonts.small.clone(), 18.3),
            };
            let mut node = Node {
                position_type: PositionType::Absolute,
                top: Val::Px(t.y as f32 * s),
                ..default()
            };
            match t.align {
                Align::Left => node.left = Val::Px(t.x as f32 * s),
                Align::Right => node.right = Val::Px((W - t.x) as f32 * s),
                Align::Center => {
                    node.left = Val::Px((t.x - 400) as f32 * s);
                    node.width = Val::Px(800.0 * s);
                    node.justify_content = JustifyContent::Center;
                }
            }
            let holder = commands.spawn((node, ChildOf(boardn))).id();
            commands.spawn((
                Text::new(t.s.clone()),
                TextFont {
                    font,
                    font_size: size * s,
                    ..default()
                },
                TextColor(Color::srgb_u8(t.rgb[0], t.rgb[1], t.rgb[2])),
                TextShadow {
                    offset: Vec2::splat(s.max(1.0)),
                    color: Color::BLACK,
                },
                TextLayout::new_with_no_wrap(),
                ChildOf(holder),
            ));
        }
    }
    ctl.last = Some((frame, size));
}

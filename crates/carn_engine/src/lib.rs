//! The engine shared by the Carnivores, Carnivores 2 and Ice Age rewrites.
//!
//! Each game's binary calls [`run`] with its [`GameKind`]; everything else
//! (which files to expect, which rules apply) follows from that.

pub mod area;
pub mod audio;
pub mod creatures;
pub mod game;
pub mod hud;
pub mod keymap;
pub mod map_screen;
pub mod menu;
pub mod overlay;
pub mod particles;
pub mod paths;
pub mod player;
pub mod profile;
pub mod render;
pub mod settings;
pub mod shadows;
pub mod sim;
pub mod sounds;
pub mod sun;
pub mod ui;
pub mod weapons;
pub mod world;

use std::path::PathBuf;
use std::time::Duration;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::camera::ClearColorConfig;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorGrabMode, PresentMode, WindowMode, WindowResolution};
use bevy_framepace::{FramepacePlugin, FramepaceSettings, Limiter};
use carn_formats::rsc::LoadOptions;
use carn_formats::Engine;

use area::Area;
pub use game::GameKind;
use paths::DataRoot;
use player::Player;
use render::{Globals, ObjectMaterial, SkyMaterial, TerrainMaterial, WaterMaterial};
use settings::Settings;

#[derive(States, Default, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum AppState {
    #[default]
    Menu,
    Loading,
    Hunt,
}

/// What the command line asked for.
#[derive(Resource, Clone, Debug, Default)]
pub struct Launch {
    /// Start straight into this area ("prj=").
    pub project: Option<String>,
    /// Start position in cells ("x=", "y=") and heading in degrees ("a=").
    pub start: Option<(f32, f32, f32)>,
    /// Render this many frames of the hunt, save a screenshot and quit.
    pub shot: Option<u32>,
    pub shot_path: Option<PathBuf>,
    pub no_sound: bool,
    /// Debugging: start with this weapon (1-based) drawn.
    pub weapon_up: Option<usize>,
    /// Start pitch in degrees, positive looking down ("b=").
    pub pitch: Option<f32>,
    /// Developer keys: F7 kills the nearest creature.
    pub debug: bool,
    /// Debugging: put a creature of this type (its index in the creature
    /// list) in front of the hunter, this far away.
    pub spawn: Option<(usize, f32)>,
    /// -nodinos: an empty hunt, but for a spawn= one (for testing).
    pub no_dinos: bool,
    /// -freeze: creatures stand still until they are hit (for testing).
    pub freeze: bool,
    /// -showpos: the position readout from the start.
    pub showpos: bool,
}

/// The area to load next.
#[derive(Resource, Clone, Debug)]
pub struct LoadRequest {
    pub project: String,
    pub frames: u32,
}

/// Per-frame view values shared by the world systems.
#[derive(Resource, Default)]
pub struct ViewState {
    pub globals: Globals,
    pub paused: bool,
    /// Frames of mouse motion to ignore: grabbing the pointer can report
    /// the jump to the window's centre as motion.
    pub mouse_settle: u32,
}

#[derive(Component)]
pub struct MainCamera;

/// The view radius in cells this frame, with the detail radius
/// inside it. The first game fixed it at 36, 40 through the
/// binoculars or the scope; its successors take it from the view range
/// option, and debug mode's keys can move it for the hunt.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ViewRadius {
    pub r: i32,
    pub r1: i32,
}

impl ViewRadius {
    pub fn of(kind: GameKind, settings: &Settings, zoomed: bool, debug: &hud::DebugMode) -> Self {
        if kind == GameKind::Carnivores {
            return ViewRadius {
                r: if zoomed { 40 } else { 36 },
                r1: 0,
            };
        }
        match debug.view {
            Some((r, r1, _)) => ViewRadius { r, r1 },
            None => ViewRadius {
                r: settings.view_radius(),
                r1: 28,
            },
        }
    }
}

/// The window takes the program's own icon, resource 1 (games/
/// windows_icon.rs): the title bar and the taskbar show the window's icon,
/// not the file's.
#[cfg(windows)]
fn window_icon(
    windows: NonSend<bevy::winit::WinitWindows>,
    primary: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut done: Local<bool>,
) {
    use winit::platform::windows::IconExtWindows;
    if *done {
        return;
    }
    for e in &primary {
        if let Some(w) = windows.get_window(e) {
            if let Ok(icon) = winit::window::Icon::from_resource(1, None) {
                w.set_window_icon(Some(icon));
            }
            *done = true;
        }
    }
}

/// Where the game's files are when `data=` does not say: the working
/// directory if it holds them, else the folder the program is in (a copy
/// dropped into a game folder and started from elsewhere, by a shortcut or
/// from Explorer).
fn default_game_folder() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let has_data = |d: &std::path::Path| paths::find_ci(d, "HUNTDAT").is_some();
    if has_data(&cwd) {
        return cwd;
    }
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .filter(|d| has_data(d))
        .unwrap_or(cwd)
}

/// Stops with a message, as the games did: logged, shown in a box
/// of the system's own titled as theirs was, and the program ends.
fn fatal(message: &str) -> ! {
    eprintln!("ABNORMAL_HALT: {message}");
    show_error("Carnivores Termination", message);
    std::process::exit(1);
}

#[cfg(windows)]
fn show_error(title: &str, text: &str) {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    const MB_ICONEXCLAMATION: u32 = 0x30;
    const MB_SYSTEMMODAL: u32 = 0x1000;
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide(text).as_ptr(),
            wide(title).as_ptr(),
            MB_ICONEXCLAMATION | MB_SYSTEMMODAL,
        );
    }
}

/// The desktop's own dialog: zenity, or KDE's kdialog.
#[cfg(not(windows))]
fn show_error(title: &str, text: &str) {
    let zenity = std::process::Command::new("zenity")
        .args(["--error", "--no-markup", "--title", title, "--text", text])
        .status();
    if zenity.is_err() {
        let _ = std::process::Command::new("kdialog")
            .args(["--title", title, "--error", text])
            .status();
    }
}

/// Whether this is the Windows build running under Wine, whose ntdll has
/// an export real Windows does not.
#[cfg(windows)]
fn under_wine() -> bool {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleA(name: *const u8) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }
    // SAFETY: both take NUL-terminated names and only look things up.
    unsafe {
        let ntdll = GetModuleHandleA(b"ntdll.dll\0".as_ptr());
        !ntdll.is_null() && !GetProcAddress(ntdll, b"wine_get_version\0".as_ptr()).is_null()
    }
}

#[cfg(not(windows))]
fn under_wine() -> bool {
    false
}

pub fn run(kind: GameKind) {
    let mut settings = Settings::load(kind);
    let mut launch = Launch::default();
    let mut root = default_game_folder();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A first start takes its options from the game folder's own profile,
    // as the originals kept them there.
    if Settings::path(kind).map(|p| !p.exists()).unwrap_or(false) {
        let data = args
            .iter()
            .find_map(|a| a.strip_prefix("data="))
            .map(PathBuf::from)
            .unwrap_or_else(|| root.clone());
        if settings.import_save(kind, &paths::DataRoot(data)) {
            info!("options taken from the game folder's profile");
            settings.save(kind);
        }
    }
    for a in args {
        let (k, v) = a.split_once('=').unwrap_or((a.as_str(), ""));
        match k {
            "prj" => launch.project = Some(v.replace('\\', "/")),
            "data" => root = PathBuf::from(v),
            "x" | "y" | "a" => {
                let s = launch.start.get_or_insert((0.0, 0.0, 0.0));
                let f = v.parse().unwrap_or(0.0);
                match k {
                    "x" => s.0 = f,
                    "y" => s.1 = f,
                    _ => s.2 = f,
                }
            }
            "shot" => launch.shot = v.parse().ok(),
            "shotfile" => launch.shot_path = Some(PathBuf::from(v)),
            "spawn" => {
                let mut it = v.split(',');
                let t = it.next().and_then(|t| t.parse().ok());
                let d = it.next().and_then(|d| d.parse().ok()).unwrap_or(1500.0);
                launch.spawn = t.map(|t| (t, d));
            }
            "-nosound" => launch.no_sound = true,
            "-weapon" => launch.weapon_up = Some(1),
            "weapon" => launch.weapon_up = v.parse().ok(),
            "b" => launch.pitch = v.parse().ok(),
            "-debug" => launch.debug = true,
            "-nodinos" => launch.no_dinos = true,
            "-freeze" => launch.freeze = true,
            "-showpos" => launch.showpos = true,
            // campos=tileX,tileZ,yawDeg, as the position readout prints it.
            "campos" => {
                let v: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if v.len() >= 2 {
                    launch.start = Some((v[0], v[1], v.get(2).copied().unwrap_or(0.0)));
                }
            }
            "-window" => settings.fullscreen = false,
            "-fullscreen" => settings.fullscreen = true,
            "res" => {
                if let Some((w, h)) = v.split_once('x') {
                    settings.window_w = w.parse().unwrap_or(settings.window_w);
                    settings.window_h = h.parse().unwrap_or(settings.window_h);
                }
            }
            _ => {
                settings.set(k.trim_start_matches('-'), v);
            }
        }
    }
    settings.clamp();
    if paths::find_ci(&root, "HUNTDAT").is_none() {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|e| e.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "the program".into());
        fatal(&format!(
            "The game's files (HUNTDAT) are not in\n{}\n\nPut {exe} in the {} folder, \
             or start it with data=<the game's folder>.",
            root.display(),
            kind.title()
        ));
    }
    // Profiles earlier builds kept to themselves go back to the game folder.
    profile::migrate(kind, &paths::DataRoot(root.clone()));
    if launch.no_sound {
        settings.sound = false;
    }

    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    let window = Window {
        title: format!("{} (Rust)", kind.title()),
        name: Some(format!("{}-rs", kind.id())),
        resolution: WindowResolution::new(settings.window_w as f32, settings.window_h as f32),
        mode,
        present_mode: if settings.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        },
        ..default()
    };

    let mut wgpu = bevy::render::settings::WgpuSettings::default();
    // Under Wine, Direct3D 12 is a translation onto Vulkan with gaps that
    // stop the renderer; Vulkan itself goes straight to the host's driver.
    if under_wine() && std::env::var_os("WGPU_BACKEND").is_none() {
        wgpu.backends = Some(bevy::render::settings::Backends::VULKAN);
    }

    let debug = hud::DebugMode::from_launch(kind, launch.debug);
    let view_radius = ViewRadius::of(kind, &settings, false, &debug);

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(window),
                ..default()
            })
            .set(bevy::render::RenderPlugin {
                render_creation: bevy::render::settings::RenderCreation::Automatic(wgpu),
                ..default()
            })
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,naga=warn,bevy_render=warn,bevy_app=warn".into(),
                ..default()
            })
            // The mixer plays on the device itself (audio.rs).
            .disable::<bevy::audio::AudioPlugin>(),
    )
    .add_plugins(FramepacePlugin)
    .add_plugins(render::WorldRenderPlugin)
    .add_plugins(audio::AudioPlugin)
    .add_plugins(ui::UiPlugin)
    .add_plugins(menu::MenuPlugin)
    .insert_resource(kind)
    .insert_resource(DataRoot(root))
    .insert_resource(settings)
    .insert_resource(launch.clone())
    .insert_resource(ClearColor(Color::BLACK))
    .init_resource::<ViewState>()
    .init_resource::<sounds::AmbientTimers>()
    .init_resource::<world::AnimatedObjects>()
    .init_resource::<hud::HuntFlow>()
    .insert_resource(debug)
    .insert_resource(view_radius)
    .init_state::<AppState>()
    .add_systems(
        Startup,
        (setup_camera, overlay::setup, start_up, setup_fonts),
    )
    .add_systems(
        Update,
        (
            apply_settings,
            shot_mode,
            screenshot_key,
            weapons::hud,
            map_screen::update,
        ),
    )
    .add_systems(OnEnter(AppState::Loading), enter_loading)
    .add_systems(Update, do_loading.run_if(in_state(AppState::Loading)))
    .add_systems(
        Update,
        (
            hunt_update,
            weapons::update.run_if(resource_exists::<weapons::Arsenal>),
            sounds::update,
            update_camera,
            overlay::follow,
            creatures::sync,
            shadows::update,
            particles::update,
            sun::update,
            hud::frame_stats,
            hud::update,
            hud::debug_keys,
            world::cull_chunks,
            world::follow_sky,
            world::animate_objects,
            world::update_frame_values,
        )
            .chain()
            .run_if(in_state(AppState::Hunt)),
    )
    .add_systems(OnExit(AppState::Hunt), (save_hunt, leave_hunt).chain());
    #[cfg(windows)]
    app.add_systems(Update, window_icon);
    app.run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 62f32.to_radians(),
            near: 12.0,
            far: 4.0e6,
            ..default()
        }),
        Tonemapping::None,
        Msaa::Off,
        Globals::default().to_fog(),
        Transform::from_xyz(0.0, 0.0, 0.0),
        MainCamera,
    ));
}

fn start_up(launch: Res<Launch>, mut next: ResMut<NextState<AppState>>, mut commands: Commands) {
    if let Some(p) = &launch.project {
        commands.insert_resource(LoadRequest {
            project: p.clone(),
            frames: 0,
        });
        next.set(AppState::Loading);
    }
}

/// Keeps the window, frame pacing and camera in step with the options.
fn apply_settings(
    settings: Res<Settings>,
    mut windows: Query<&mut Window>,
    mut pace: ResMut<FramepaceSettings>,
    mut cams: Query<&mut Msaa, With<MainCamera>>,
    mut last: Local<Option<(bool, bool, u32, u32, u32, u32)>>,
) {
    let key = (
        settings.vsync,
        settings.fullscreen,
        settings.fps_limit,
        settings.msaa,
        settings.window_w,
        settings.window_h,
    );
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    if let Ok(mut w) = windows.single_mut() {
        let pm = if settings.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };
        if w.present_mode != pm {
            w.present_mode = pm;
        }
        let mode = if settings.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
        if w.mode != mode {
            w.mode = mode;
            if !settings.fullscreen {
                w.resolution
                    .set(settings.window_w as f32, settings.window_h as f32);
            }
        }
    }
    pace.limiter = if settings.fps_limit == 0 {
        Limiter::Off
    } else {
        Limiter::Manual(Duration::from_secs_f64(1.0 / settings.fps_limit as f64))
    };
    for mut m in cams.iter_mut() {
        *m = match settings.msaa {
            2 => Msaa::Sample2,
            4 => Msaa::Sample4,
            8 => Msaa::Sample8,
            _ => Msaa::Off,
        };
    }
}

fn enter_loading(mut commands: Commands, mut debug: ResMut<hud::DebugMode>) {
    // Each hunt starts from the option's view radii.
    debug.view = None;
    commands.insert_resource(ui::Screen::Loading);
}

type WorldMats<'w> = (
    ResMut<'w, Assets<TerrainMaterial>>,
    ResMut<'w, Assets<ObjectMaterial>>,
    ResMut<'w, Assets<WaterMaterial>>,
    ResMut<'w, Assets<SkyMaterial>>,
    ResMut<'w, Assets<render::SpriteMaterial>>,
    ResMut<'w, Assets<render::WeaponMaterial>>,
);

#[allow(clippy::too_many_arguments)]
fn do_loading(
    mut commands: Commands,
    mut req: ResMut<LoadRequest>,
    kind: Res<GameKind>,
    root: Res<DataRoot>,
    settings: Res<Settings>,
    launch: Res<Launch>,
    mut next: ResMut<NextState<AppState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    (
        mut terrain_mats,
        mut object_mats,
        mut water_mats,
        mut sky_mats,
        mut sprite_mats,
        mut weapon_mats,
    ): WorldMats,
    mut cams: Query<&mut Camera, With<MainCamera>>,
    overlays: Query<(Entity, &overlay::Overlay)>,
    (setup, profile): (Option<Res<menu::HuntSetup>>, Res<menu::CurrentProfile>),
) {
    // Let the loading screen reach the window before the work starts.
    req.frames += 1;
    if req.frames < 3 {
        return;
    }
    let opt = LoadOptions {
        brightness: settings.brightness,
        day_night: settings.day_night,
    };
    let t0 = std::time::Instant::now();
    let area = match Area::load(&root, *kind, &req.project, opt) {
        Ok(a) => a,
        Err(e) => {
            error!("{e}");
            commands.insert_resource(ui::Screen::Error(e));
            next.set(AppState::Menu);
            return;
        }
    };
    let imgs = world::build_images(&area, &settings, &mut images);
    let anim = world::spawn_area(
        &mut commands,
        &area,
        &settings,
        &imgs,
        &mut meshes,
        &mut images,
        &mut terrain_mats,
        &mut object_mats,
        &mut water_mats,
        &mut sky_mats,
        &mut sprite_mats,
    );
    info!(
        "loaded {} in {:.2}s",
        req.project,
        t0.elapsed().as_secs_f32()
    );
    let player = match launch.start {
        Some((x, y, a)) => Player::at(&area, x * 256.0, y * 256.0, a.to_radians()),
        None => Player::new(&area),
    };
    let mut player = player;
    if let Some(b) = launch.pitch {
        player.beta = b.to_radians();
    }
    let sim = match creatures::setup(
        &mut commands,
        &root,
        *kind,
        &area,
        &settings,
        hunter_view(&player),
        launch.spawn,
        launch.no_dinos,
        setup.as_deref(),
        profile.0.as_ref().map(|p| &p.bodies[..]),
        &imgs,
        &mut images,
        &mut meshes,
        &mut object_mats,
    ) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("no creatures: {e}");
            None
        }
    };
    weapons::setup(
        &mut commands,
        &root,
        *kind,
        settings.textures,
        kind.has_time_of_day()
            .then_some((settings.brightness, settings.day_night)),
        &imgs,
        &overlays,
        setup.as_deref(),
        area.rsc.sky_fade(area.opt),
        &mut images,
        &mut meshes,
        (&mut object_mats, &mut weapon_mats),
    );
    if let Some(m) = map_screen::build(&root, &area, &mut images) {
        commands.insert_resource(m);
    }
    let (bank, timers) = sounds::build(&root, &area, sim.as_ref());
    commands.insert_resource(bank);
    commands.insert_resource(timers);
    if let Some(mut sim) = sim {
        if launch.freeze {
            sim.hold_still = true;
            for c in sim.chars.iter_mut() {
                c.frozen = true;
            }
        }
        commands.insert_resource(sim);
    }
    info!(
        "hunter at cell {:.1},{:.1} facing {:.0} degrees",
        player.x / 256.0,
        player.z / 256.0,
        player.alpha.to_degrees()
    );
    let fade = area.rsc.sky_fade(opt);
    for mut c in cams.iter_mut() {
        c.clear_color = ClearColorConfig::Custom(Color::srgb_u8(fade[0], fade[1], fade[2]));
    }
    commands.insert_resource(anim);
    commands.insert_resource(imgs);
    commands.insert_resource(player);
    commands.insert_resource(area);
    commands.insert_resource(ui::Screen::None);
    next.set(AppState::Hunt);
}

fn hunter_view(p: &Player) -> sim::HunterView {
    sim::HunterView {
        x: p.x,
        y: p.y,
        z: p.z,
        head_y: p.head_y,
        cam: p.cam,
        cam_alpha: p.cam_alpha,
        underwater: p.underwater,
    }
}

#[allow(clippy::too_many_arguments)]
fn hunt_update(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    area: Res<Area>,
    settings: Res<Settings>,
    mut player: ResMut<Player>,
    mut view: ResMut<ViewState>,
    launch: Res<Launch>,
    mut sim: Option<ResMut<sim::Sim>>,
    mut next: ResMut<NextState<AppState>>,
    mut screen: ResMut<ui::Screen>,
    (kind, arsenal, debug, mut radius): (
        Res<GameKind>,
        Option<Res<weapons::Arsenal>>,
        Res<hud::DebugMode>,
        ResMut<ViewRadius>,
    ),
) {
    let zoomed = arsenal
        .as_deref()
        .map(|a| a.binoculars || a.optic)
        .unwrap_or(false);
    let r = ViewRadius::of(*kind, &settings, zoomed, &debug);
    if *radius != r {
        *radius = r;
    }
    if view.paused {
        return;
    }
    let dt_ms = (time.delta_secs() * 1000.0).clamp(1.0, 1000.0);
    if view.mouse_settle > 0 {
        view.mouse_settle -= 1;
    }
    let dying = sim.as_ref().map(|s| s.demo.time > 0).unwrap_or(false);
    let was_under = player.underwater;
    if !dying {
        let input = keymap::Input {
            keys: &keys,
            mouse: &buttons,
        };
        let mut c = player::read_controls(&input, &mouse, &mut player, &settings);
        // A jump the size of the window is the pointer being warped by a
        // grab (X11 does this as the window takes focus), not the hand.
        let jump = c.mouse.x.abs() > 600.0 || c.mouse.y.abs() > 600.0;
        if launch.shot.is_some() || view.mouse_settle > 0 || jump {
            c.mouse = Vec2::ZERO;
        }
        c.weapon_up = arsenal.as_deref().map(|a| a.state != 0).unwrap_or(false);
        if debug.on && keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
            c.boost = Some(if *kind == GameKind::Carnivores {
                4.0
            } else {
                8.0
            });
        }
        player::step(&mut player, &area, &c, &settings, dt_ms);
        if let Some(s) = sim.as_mut() {
            // The statistics' walk (backwards takes off) and time; an
            // observer, or anyone in debug mode, cannot be hurt.
            if !area.trophy {
                s.hunt_stats.path += dt_ms * player.vspeed / 128.0;
                s.hunt_stats.time += dt_ms / 1000.0;
            }
            if (s.observer || area.trophy || debug.on) && s.my_health > 0 {
                s.my_health = sim::MAX_HEALTH;
            }
        }
        // Under water the hunter runs out of breath.
        if let Some(s) = sim.as_mut() {
            if player.underwater && s.my_health > 0 {
                s.my_health -= (dt_ms * 12.0) as i32;
                if s.my_health <= 0 {
                    s.my_health = 1;
                    s.hunter = hunter_view(&player);
                    s.add_dead_body(&area, None, sim::HUNT_BREATH);
                }
            }
        }
    } else if let Some(s) = sim.as_mut() {
        let p = &mut *player;
        p.real_time += dt_ms;
        s.demo_camera(&area, &mut p.cam, &mut p.cam_alpha, &mut p.cam_beta);
        if s.demo.time > 12 * 1000 {
            *screen = ui::Screen::Main;
            next.set(AppState::Menu);
        }
    }
    if let Some(s) = sim.as_mut() {
        if !dying {
            let input = keymap::Input {
                keys: &keys,
                mouse: &buttons,
            };
            if settings.keys.pressed(keymap::Act::Call, &input) {
                s.make_call();
            }
            if settings.keys.pressed(keymap::Act::ChangeCall, &input) {
                s.change_call();
            }
        }
        if launch.debug && keys.just_pressed(KeyCode::F7) {
            let p = Vec3::new(player.x, player.y, player.z);
            let near = s
                .chars
                .iter()
                .enumerate()
                .filter(|(_, c)| c.health > 0 && !c.removed && c.ai > 0)
                .min_by(|a, b| (a.1.pos - p).length().total_cmp(&(b.1.pos - p).length()))
                .map(|(i, _)| i);
            if let Some(i) = near {
                s.shot_damage(&area, i, true, 0.0);
                info!("debug: killed {}", s.dinos[s.chars[i].ctype].name);
            }
        }
        if !dying {
            s.hunter = hunter_view(&player);
        } else {
            s.hunter.cam = player.cam;
            s.hunter.cam_alpha = player.cam_alpha;
        }
        if let Some(m) = player.message.take() {
            s.add_message(m);
        }
        s.view_r = radius.r;
        s.debug = debug.on;
        s.opt_sens = settings.sens.clamp(0, if s.c1 { 2 } else { 255 });
        s.update(&area, dt_ms as i32);
        if !dying {
            let wading = matches!(player.footstep, Some((true, _)));
            s.hunter_rings(&area, wading, was_under != player.underwater, player.swim);
        }
    }
}

fn update_camera(
    arsenal: Option<Res<weapons::Arsenal>>,
    player: Res<Player>,
    area: Res<Area>,
    settings: Res<Settings>,
    radius: Res<ViewRadius>,
    mut view: ResMut<ViewState>,
    mut cams: Query<
        (&mut Transform, &mut Projection, &mut bevy::pbr::DistanceFog),
        With<MainCamera>,
    >,
) {
    let r = radius.r as f32;
    let fade = area.rsc.sky_fade(area.opt);
    let w = if player.underwater {
        let wi = area.map.wmap[area.idx(player.cam.x as i32 / 256, player.cam.z as i32 / 256)];
        area.rsc.waters.get(wi as usize).copied()
    } else {
        None
    };
    let g = Globals {
        fade_rgb: [
            fade[0] as f32 / 255.0,
            fade[1] as f32 / 255.0,
            fade[2] as f32 / 255.0,
        ],
        fade_start: (r - 4.0) * 256.0,
        fade_end: (r - 1.0) * 256.0,
        // The water's fog reaches up to the body's level; Carnivores has no
        // bodies and takes the surface over the camera, eight steps higher.
        water_level: match (area.engine, w) {
            (Engine::C1, _) if player.underwater => {
                area.land_up_h(player.cam.x, player.cam.z) + 8.0 * area.hs
            }
            (_, Some(w)) => w.level as f32 * area.hs,
            _ => 0.0,
        },
        view_radius: r,
        clouds: settings.clouds,
        c1: area.engine == Engine::C1,
        height_scale: area.hs,
        underwater: player.underwater,
        fog: settings.fog,
        underwater_rgb: w
            .map(|w| {
                let v = w.fog_rgb;
                [
                    ((v >> 16) & 0xFF) as f32 / 255.0,
                    ((v >> 8) & 0xFF) as f32 / 255.0,
                    (v & 0xFF) as f32 / 255.0,
                ]
            })
            .unwrap_or(if area.engine == Engine::C1 {
                [0.376, 0.27, 0.0]
            } else {
                [0.31, 0.25, 0.0]
            }),
    };
    view.globals = g;
    for (mut t, mut proj, mut fog) in cams.iter_mut() {
        t.translation = player.cam;
        t.rotation = player::view_rotation(player.cam_alpha, player.cam_beta);
        if let Projection::Perspective(p) = proj.as_mut() {
            // The underwater sway stretches the picture: widen the vertical
            // field by the height factor, the width by the ratio.
            let base = (settings.fov as f32).to_radians();
            let zoom = arsenal.as_ref().map(|a| a.zoom()).unwrap_or(1.0);
            let half = (base * 0.5).tan() / player.zoom_h / zoom;
            p.fov = 2.0 * half.atan();
            p.near = 12.0;
            p.far = 4.0e6;
        }
        *fog = g.to_fog();
    }
}

fn setup_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    commands.insert_resource(hud::load_fonts(&mut fonts));
}

/// What a hunt leaves in the hunter's profile. Only a hunter who came out
/// alive keeps it: a death, or starting over, loses the hunt's credit and
/// trophies (the games reloaded the saved profile then).
fn save_hunt(
    kind: Res<GameKind>,
    root: Res<DataRoot>,
    sim: Option<Res<sim::Sim>>,
    area: Option<Res<Area>>,
    flow: Res<hud::HuntFlow>,
    mut profile: ResMut<menu::CurrentProfile>,
) {
    let (Some(s), Some(area), Some(p)) = (sim, area, profile.0.as_mut()) else {
        return;
    };
    if area.trophy {
        // Mounts taken down.
        profile::Store::new(*kind, &root).save(p);
        return;
    }
    if s.my_health == 0 || flow.restart {
        return;
    }
    p.score += s.score;
    for t in &s.trophies {
        p.add_trophy(profile::TrophyItem {
            ctype: t.ctype as i32,
            weapon: t.weapon as i32,
            phase: t.phase,
            score: t.score,
            date: t.date,
            time: t.time,
            scale: t.scale,
            range: t.range,
            ..Default::default()
        });
    }
    p.last = profile::Stats {
        shots_made: s.hunt_stats.shots_made,
        success: s.hunt_stats.success,
        path: s.hunt_stats.path,
        time: s.hunt_stats.time,
    };
    p.total.shots_made += p.last.shots_made;
    p.total.success += p.last.success;
    p.total.path += p.last.path;
    p.total.time += p.last.time;
    profile::Store::new(*kind, &root).save(p);
}

fn leave_hunt(
    mut commands: Commands,
    mixer: Res<audio::Mixer>,
    q: Query<Entity, With<world::AreaEntity>>,
    mut cams: Query<&mut Camera, With<MainCamera>>,
) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<Area>();
    commands.remove_resource::<Player>();
    commands.remove_resource::<sim::Sim>();
    commands.remove_resource::<creatures::CreatureAssets>();
    commands.remove_resource::<shadows::Shadows>();
    commands.remove_resource::<particles::FxAssets>();
    commands.remove_resource::<sun::SunState>();
    commands.remove_resource::<hud::HudPics>();
    commands.insert_resource(hud::HuntFlow::default());
    commands.remove_resource::<sounds::SoundBank>();
    commands.remove_resource::<weapons::Arsenal>();
    commands.remove_resource::<weapons::Gear>();
    commands.remove_resource::<map_screen::MapScreen>();
    mixer.stop_all();
    commands.insert_resource(world::AnimatedObjects::default());
    for mut c in cams.iter_mut() {
        c.clear_color = ClearColorConfig::Custom(Color::BLACK);
    }
}

/// shot=N: after N frames of the hunt, save a screenshot and quit. For
/// checking the renderer without anyone watching.
fn shot_mode(
    launch: Res<Launch>,
    state: Res<State<AppState>>,
    mut frames: Local<u32>,
    mut commands: Commands,
    mut exit: EventWriter<AppExit>,
) {
    let Some(n) = launch.shot else { return };
    // Counted from the start of the hunt when one was asked for, otherwise
    // from start-up (a shot of the menu).
    if launch.project.is_some() && *state.get() != AppState::Hunt {
        return;
    }
    *frames += 1;
    if *frames == n {
        let path = launch
            .shot_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("shot.png"));
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if *frames == n + 10 {
        exit.write(AppExit::Success);
    }
}

/// Grabs or frees the pointer; true when it has just been grabbed.
pub fn set_cursor_grab(windows: &mut Query<&mut Window>, grab: bool) -> bool {
    if let Ok(mut w) = windows.single_mut() {
        let mode = if grab {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        if w.cursor_options.grab_mode != mode {
            w.cursor_options.grab_mode = mode;
            w.cursor_options.visible = !grab;
            return grab;
        }
    }
    false
}

/// Where F12 puts screenshots: never the game folder. CARN_RS_SHOTDIR
/// overrides it.
pub fn screenshot_dir(kind: GameKind) -> PathBuf {
    if let Some(d) = std::env::var_os("CARN_RS_SHOTDIR") {
        return PathBuf::from(d);
    }
    let pics = std::env::var_os("XDG_PICTURES_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
            Some(PathBuf::from(home).join("Pictures"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    pics.join(format!("{} (Rust)", kind.title().replace(':', "")))
}

fn screenshot_key(
    keys: Res<ButtonInput<KeyCode>>,
    kind: Res<GameKind>,
    mut commands: Commands,
    mut count: Local<u32>,
) {
    if !keys.just_pressed(KeyCode::F12) {
        return;
    }
    let dir = screenshot_dir(*kind);
    let _ = std::fs::create_dir_all(&dir);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    *count += 1;
    let path = dir.join(format!("hunt-{stamp}-{:03}.png", *count));
    info!("screenshot {}", path.display());
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

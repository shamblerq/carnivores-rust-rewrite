//! Menus and on-screen text.
//!
//! For now these are simple text menus: pick an area, change the options,
//! pause a hunt. The games' own bitmap menus come later.

use bevy::app::AppExit;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::area::Area;
use crate::game::GameKind;
use crate::hud::HuntFlow;
use crate::settings::{Settings, FOV_MAX, FOV_MIN, FRAME_LIMITS};
use crate::sim::Sim;
use crate::{AppState, Launch, ViewState};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.insert_resource(Screen::Main)
            .insert_resource(Selected(0))
            .add_systems(Startup, setup_fps)
            .add_systems(
                Update,
                (keys, rebuild, clicks, highlight, fps_text, cursor).chain(),
            );
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub enum Screen {
    None,
    Main,
    Options { from_pause: bool },
    Pause,
    Loading,
    Error(String),
}

#[derive(Resource)]
pub struct Selected(pub usize);

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct FpsText;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opt {
    Fov,
    FpsLimit,
    VSync,
    Msaa,
    ViewRange,
    ShowFps,
    Fullscreen,
    Textures,
    Clouds,
    Shadows,
    MouseSens,
    MouseInvert,
    Brightness,
    RunHold,
    CrouchHold,
}

#[derive(Component, Clone, Debug, PartialEq)]
enum Action {
    Options,
    Quit,
    Resume,
    Leave,
    Back,
    Opt(Opt),
    Ok,
}

#[derive(Component)]
struct Item(usize);

const TEXT: Color = Color::srgb(0.85, 0.80, 0.62);
const TEXT_SEL: Color = Color::srgb(1.0, 0.93, 0.55);
const DIM: Color = Color::srgb(0.55, 0.52, 0.42);

fn opt_label(o: Opt, s: &Settings, kind: GameKind) -> String {
    let onoff = |b: bool| if b { "On" } else { "Off" };
    match o {
        Opt::Fov => format!("Field of view: {}\u{b0}", s.fov),
        Opt::FpsLimit => format!(
            "Frame limit: {}",
            if s.fps_limit == 0 {
                "Off".to_string()
            } else {
                s.fps_limit.to_string()
            }
        ),
        Opt::VSync => format!("V-Sync: {}", onoff(s.vsync)),
        Opt::Msaa => format!(
            "Anti-aliasing: {}",
            if s.msaa == 0 {
                "Off".to_string()
            } else {
                format!("{}x MSAA", s.msaa)
            }
        ),
        Opt::ViewRange => format!(
            "View distance: {} ({} cells)",
            s.view_range,
            s.view_radius()
        ),
        Opt::ShowFps => format!("Show FPS: {}", onoff(s.show_fps)),
        Opt::Fullscreen => format!("Full screen: {}", onoff(s.fullscreen)),
        Opt::Textures => format!(
            "Textures: {} (next hunt)",
            ["Low", "Normal", "High", "Best"][s.textures.clamp(0, 3) as usize]
        ),
        Opt::Clouds => format!("Cloud shadows: {}", onoff(s.clouds)),
        Opt::Shadows => format!("Creature shadows: {}", onoff(s.shadows)),
        Opt::MouseSens => format!("Mouse sensitivity: {}", s.mouse_sens),
        Opt::MouseInvert => format!("Invert mouse: {}", onoff(s.mouse_invert)),
        Opt::Brightness => {
            if kind.has_time_of_day() {
                format!("Brightness: {} (next hunt)", s.brightness)
            } else {
                "Brightness: as authored".to_string()
            }
        }
        Opt::RunHold => format!("Run: {}", if s.run_hold { "hold Q" } else { "toggle Q" }),
        Opt::CrouchHold => format!(
            "Crouch: {}",
            if s.crouch_hold {
                "hold Shift"
            } else {
                "toggle Shift"
            }
        ),
    }
}

fn change(o: Opt, s: &mut Settings, dir: i32) {
    match o {
        Opt::Fov => s.fov = (s.fov + dir * 2).clamp(FOV_MIN, FOV_MAX),
        Opt::FpsLimit => {
            let i = FRAME_LIMITS
                .iter()
                .position(|&f| f == s.fps_limit)
                .unwrap_or(0) as i32;
            let n = FRAME_LIMITS.len() as i32;
            s.fps_limit = FRAME_LIMITS[((i + dir).rem_euclid(n)) as usize];
        }
        Opt::VSync => s.vsync = !s.vsync,
        Opt::Msaa => {
            let steps = [0, 2, 4, 8];
            let i = steps.iter().position(|&m| m == s.msaa).unwrap_or(0) as i32;
            s.msaa = steps[((i + dir).rem_euclid(4)) as usize];
        }
        Opt::ViewRange => s.view_range = (s.view_range + dir * 16).clamp(0, 255),
        Opt::ShowFps => s.show_fps = !s.show_fps,
        Opt::Fullscreen => s.fullscreen = !s.fullscreen,
        Opt::Textures => s.textures = (s.textures + dir).rem_euclid(4),
        Opt::Clouds => s.clouds = !s.clouds,
        Opt::Shadows => s.shadows = !s.shadows,
        Opt::MouseSens => s.mouse_sens = (s.mouse_sens + dir * 16).clamp(0, 255),
        Opt::MouseInvert => s.mouse_invert = !s.mouse_invert,
        Opt::Brightness => s.brightness = (s.brightness + dir * 16).clamp(0, 255),
        Opt::RunHold => s.run_hold = !s.run_hold,
        Opt::CrouchHold => s.crouch_hold = !s.crouch_hold,
    }
}

fn setup_fps(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.25, 0.88, 0.25)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(12.0),
            ..default()
        },
        FpsText,
    ));
}

fn fps_text(
    settings: Res<Settings>,
    diag: Res<DiagnosticsStore>,
    mut q: Query<(&mut Text, &mut Node), With<FpsText>>,
    time: Res<Time>,
    debug: Res<crate::hud::DebugMode>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    for (mut t, mut node) in q.iter_mut() {
        // Under the timer's first line while that shows.
        let top = Val::Px(if debug.timer { 34.0 } else { 8.0 });
        if node.top != top {
            node.top = top;
        }
        if !settings.show_fps {
            if !t.0.is_empty() {
                t.0.clear();
            }
            continue;
        }
        if *acc < 0.5 && !t.0.is_empty() {
            continue;
        }
        *acc = 0.0;
        if let Some(fps) = diag
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
        {
            t.0 = format!("{:.0} fps", fps);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    screen: Res<Screen>,
    settings: Res<Settings>,
    kind: Res<GameKind>,
    old: Query<Entity, With<MenuRoot>>,
    mut sel: ResMut<Selected>,
    mut last: Local<Option<Screen>>,
) {
    let new_screen = last.as_ref() != Some(&*screen);
    if !new_screen && !settings.is_changed() {
        return;
    }
    if new_screen {
        sel.0 = 0;
    }
    *last = Some(screen.clone());
    for e in old.iter() {
        commands.entity(e).despawn();
    }

    let mut items: Vec<(String, Action)> = Vec::new();
    let title: String;
    let mut note: Option<String> = None;
    match &*screen {
        // The games' own menus and loading picture draw these (menu/).
        Screen::None | Screen::Loading | Screen::Main => return,
        Screen::Error(e) => {
            title = "Could not start the hunt".into();
            note = Some(e.clone());
            items.push(("OK".into(), Action::Ok));
        }
        Screen::Pause => {
            title = "Paused".into();
            items.push(("Resume".into(), Action::Resume));
            items.push(("Options".into(), Action::Options));
            items.push(("Leave the hunt".into(), Action::Leave));
            items.push(("Quit to desktop".into(), Action::Quit));
        }
        Screen::Options { .. } => {
            title = "Options".into();
            let mut opts = vec![
                Opt::Fov,
                Opt::FpsLimit,
                Opt::VSync,
                Opt::Msaa,
                Opt::ViewRange,
                Opt::ShowFps,
                Opt::Fullscreen,
                Opt::Textures,
                Opt::Clouds,
                Opt::Shadows,
                Opt::MouseSens,
                Opt::MouseInvert,
                Opt::RunHold,
                Opt::CrouchHold,
            ];
            if kind.has_time_of_day() {
                opts.push(Opt::Brightness);
            }
            for o in opts {
                items.push((opt_label(o, &settings, *kind), Action::Opt(o)));
            }
            items.push(("Back".into(), Action::Back));
            note = Some("Left/Right or click to change. Esc to go back.".into());
        }
    }
    let sel_i = sel.0.min(items.len().saturating_sub(1));
    spawn_panel(&mut commands, &title, note.as_deref(), &items, sel_i, false);
}

fn spawn_panel(
    commands: &mut Commands,
    title: &str,
    note: Option<&str>,
    items: &[(String, Action)],
    sel: usize,
    loading: bool,
) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(if loading {
                Color::BLACK
            } else {
                Color::srgba(0.0, 0.0, 0.0, 0.55)
            }),
            // Over the games' menus (an error comes up on top of them).
            GlobalZIndex(30),
            MenuRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(24.0)),
                    row_gap: Val::Px(6.0),
                    min_width: Val::Px(420.0),
                    max_width: Val::Px(760.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.05, 0.03, 0.85)),
                BorderRadius::all(Val::Px(6.0)),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(title),
                    TextFont {
                        font_size: 34.0,
                        ..default()
                    },
                    TextColor(TEXT_SEL),
                    Node {
                        margin: UiRect::bottom(Val::Px(12.0)),
                        ..default()
                    },
                ));
                for (i, (label, action)) in items.iter().enumerate() {
                    p.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        action.clone(),
                        Item(i),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(label.clone()),
                            TextFont {
                                font_size: 22.0,
                                ..default()
                            },
                            TextColor(if i == sel { TEXT_SEL } else { TEXT }),
                        ));
                    });
                }
                if let Some(n) = note {
                    p.spawn((
                        Text::new(n),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(DIM),
                        TextLayout::new_with_justify(JustifyText::Center),
                        Node {
                            margin: UiRect::top(Val::Px(14.0)),
                            ..default()
                        },
                    ));
                }
            });
        });
}

fn highlight(
    sel: Res<Selected>,
    items: Query<(&Item, &Children)>,
    mut texts: Query<&mut TextColor>,
) {
    if !sel.is_changed() {
        return;
    }
    for (item, children) in items.iter() {
        for c in children.iter() {
            if let Ok(mut t) = texts.get_mut(c) {
                t.0 = if item.0 == sel.0 { TEXT_SEL } else { TEXT };
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn act(
    action: &Action,
    dir: i32,
    screen: &mut Screen,
    settings: &mut Settings,
    kind: GameKind,
    state: &State<AppState>,
    next: &mut NextState<AppState>,
    view: &mut ViewState,
    exit: &mut EventWriter<AppExit>,
) {
    match action {
        Action::Options => {
            *screen = Screen::Options {
                from_pause: *state.get() == AppState::Hunt,
            };
        }
        Action::Quit => {
            exit.write(AppExit::Success);
        }
        Action::Resume => {
            *screen = Screen::None;
            view.paused = false;
        }
        Action::Leave => {
            view.paused = false;
            *screen = Screen::Main;
            next.set(AppState::Menu);
        }
        Action::Back => {
            *screen = match screen {
                Screen::Options { from_pause: true } => Screen::Pause,
                _ => Screen::Main,
            };
        }
        Action::Ok => *screen = Screen::Main,
        Action::Opt(o) => {
            change(*o, settings, dir);
            settings.save(kind);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn clicks(
    q: Query<(&Interaction, &Action, &Item), Changed<Interaction>>,
    mut screen: ResMut<Screen>,
    mut settings: ResMut<Settings>,
    kind: Res<GameKind>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut view: ResMut<ViewState>,
    mut exit: EventWriter<AppExit>,
    mut sel: ResMut<Selected>,
) {
    for (i, a, item) in q.iter() {
        match i {
            Interaction::Pressed => {
                sel.0 = item.0;
                let mut s = screen.clone();
                act(
                    a,
                    1,
                    &mut s,
                    &mut settings,
                    *kind,
                    &state,
                    &mut next,
                    &mut view,
                    &mut exit,
                );
                if s != *screen {
                    *screen = s;
                }
            }
            Interaction::Hovered => {
                if sel.0 != item.0 {
                    sel.0 = item.0;
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn keys(
    input: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<Screen>,
    mut sel: ResMut<Selected>,
    items: Query<(&Item, &Action)>,
    mut settings: ResMut<Settings>,
    kind: Res<GameKind>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut view: ResMut<ViewState>,
    mut exit: EventWriter<AppExit>,
    launch: Res<Launch>,
    (mut flow, sim, area): (ResMut<HuntFlow>, Option<Res<Sim>>, Option<Res<Area>>),
) {
    let hunting = *state.get() == AppState::Hunt && launch.shot.is_none();
    // F1: these options over the hunt, which waits meanwhile.
    if input.just_pressed(KeyCode::F1) && hunting && *screen == Screen::None && !flow.pause {
        view.paused = true;
        flow.exit_mode = false;
        *screen = Screen::Pause;
        return;
    }
    if input.just_pressed(KeyCode::Escape) {
        let s = match &*screen {
            // Esc in a hunt is the games' own: out of the pause picture, or
            // the exit prompt on and off (the trophy room just leaves).
            Screen::None if hunting => {
                if flow.pause {
                    flow.pause = false;
                    view.paused = false;
                } else if area.as_ref().map(|a| a.trophy).unwrap_or(false) {
                    next.set(AppState::Menu);
                    return;
                } else {
                    flow.exit_mode = !flow.exit_mode;
                    if sim.as_ref().map(|s| s.exit_time > 0).unwrap_or(false) {
                        flow.exit_mode = false;
                    }
                }
                Screen::None
            }
            Screen::Pause => {
                view.paused = false;
                Screen::None
            }
            Screen::Options { from_pause: true } => Screen::Pause,
            Screen::Options { from_pause: false } | Screen::Error(_) => Screen::Main,
            other => other.clone(),
        };
        if s != *screen {
            *screen = s;
        }
        return;
    }
    let n = items.iter().count();
    if n == 0 {
        return;
    }
    if input.just_pressed(KeyCode::ArrowDown) {
        sel.0 = (sel.0 + 1) % n;
    }
    if input.just_pressed(KeyCode::ArrowUp) {
        sel.0 = (sel.0 + n - 1) % n;
    }
    let dir = if input.just_pressed(KeyCode::ArrowLeft) {
        -1
    } else if input.just_pressed(KeyCode::ArrowRight)
        || input.just_pressed(KeyCode::Enter)
        || input.just_pressed(KeyCode::NumpadEnter)
    {
        1
    } else {
        return;
    };
    let Some((_, a)) = items.iter().find(|(i, _)| i.0 == sel.0) else {
        return;
    };
    let enter = input.just_pressed(KeyCode::Enter) || input.just_pressed(KeyCode::NumpadEnter);
    if !enter && !matches!(a, Action::Opt(_)) {
        return;
    }
    let a = a.clone();
    let mut s = screen.clone();
    act(
        &a,
        dir,
        &mut s,
        &mut settings,
        *kind,
        &state,
        &mut next,
        &mut view,
        &mut exit,
    );
    if s != *screen {
        *screen = s;
    }
}

fn cursor(
    screen: Res<Screen>,
    state: Res<State<AppState>>,
    mut windows: Query<&mut Window>,
    mut view: ResMut<ViewState>,
    mut focus: EventReader<bevy::window::WindowFocused>,
) {
    let grab = *screen == Screen::None && *state.get() == AppState::Hunt;
    if crate::set_cursor_grab(&mut windows, grab) {
        view.mouse_settle = 3;
    }
    if focus.read().any(|f| f.focused) {
        view.mouse_settle = 3;
    }
}

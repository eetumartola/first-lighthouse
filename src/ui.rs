//! Menus, HUD, pause, result screens. Text is sparse by design.

use crate::app::{AppState, Session, Settings};
use crate::sim::{self, world_weaver, Mode, Phase, Rules, Status};
use bevy::prelude::*;

const INK: Color = Color::srgb(0.88, 0.86, 0.8);
const DIM: Color = Color::srgb(0.55, 0.58, 0.62);
const WARM: Color = Color::srgb(1.0, 0.8, 0.5);
const PANEL: Color = Color::srgba(0.02, 0.03, 0.05, 0.78);

#[derive(Resource, Default)]
struct MenuState {
    selected: usize,
}

#[derive(Component)]
struct MenuEntry(usize);

#[derive(Component)]
struct HudTitle;
#[derive(Component)]
struct HudTimer;
#[derive(Component)]
struct HudScore;
#[derive(Component)]
struct HudWeaver;
#[derive(Component)]
struct HudHint;
#[derive(Component)]
struct RuleCard;
#[derive(Component)]
struct Toast {
    until: f32,
}
#[derive(Component)]
struct ToastList;
#[derive(Component)]
struct PauseSettingsText;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(AppState::Menu), spawn_menu)
            .add_systems(Update, menu_input.run_if(in_state(AppState::Menu)))
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(OnExit(AppState::Playing), despawn_hud)
            .add_systems(
                Update,
                (update_hud, update_rule_card, push_toasts, expire_toasts).run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnEnter(AppState::Paused), spawn_pause)
            .add_systems(Update, update_pause_text.run_if(in_state(AppState::Paused)))
            .add_systems(OnEnter(AppState::Result), spawn_result);
    }
}

fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

fn panel(width: Val) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(22)),
        row_gap: px(8),
        width,
        ..default()
    }
}

fn spawn_menu(mut commands: Commands, menu: Res<MenuState>) {
    commands
        .spawn((
            DespawnOnExit(AppState::Menu),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((Text::new("FIRST LIGHTHOUSE"), font(64.0), TextColor(WARM)));
            root.spawn((
                Text::new("to the saviour gods, on behalf of those who sail the seas"),
                font(20.0),
                TextColor(DIM),
                Node {
                    margin: UiRect::bottom(px(34)),
                    ..default()
                },
            ));
            for (i, mode) in Mode::MENU.iter().enumerate() {
                root.spawn((
                    MenuEntry(i),
                    Text::new(format!("{}   {}", mode.title(), mode.tagline())),
                    font(26.0),
                    TextColor(if i == menu.selected { INK } else { DIM }),
                    Node {
                        margin: UiRect::vertical(px(6)),
                        ..default()
                    },
                ));
            }
            root.spawn((
                Text::new("Up / Down choose    Enter begin    [ ] brightness    F4 constant-speed rotation    F3 debug overlay    F6 skip to dawn"),
                font(16.0),
                TextColor(DIM),
                Node {
                    margin: UiRect::top(px(40)),
                    ..default()
                },
            ));
        });
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuState>,
    mut entries: Query<(&MenuEntry, &mut TextColor)>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<AppState>>,
) {
    let n = Mode::MENU.len();
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        menu.selected = (menu.selected + 1) % n;
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        menu.selected = (menu.selected + n - 1) % n;
    }
    for (entry, mut color) in &mut entries {
        color.0 = if entry.0 == menu.selected { INK } else { DIM };
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        session.start(Mode::MENU[menu.selected]);
        next.set(AppState::Playing);
    }
}

#[derive(Component)]
struct HudRoot;

fn spawn_hud(mut commands: Commands, session: Res<Session>) {
    let Some(world) = session.world() else { return };
    let mode = world.mode;
    // Top-left: mode and timer. Top-right: rescues. Bottom-centre: controls hint.
    commands.spawn((
        HudRoot,
        HudTitle,
        Text::new(mode.title()),
        font(24.0),
        TextColor(WARM),
        Node {
            position_type: PositionType::Absolute,
            left: px(18),
            top: px(14),
            ..default()
        },
    ));
    commands.spawn((
        HudRoot,
        HudTimer,
        Text::new(""),
        font(20.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            left: px(18),
            top: px(44),
            ..default()
        },
    ));
    commands.spawn((
        HudRoot,
        Text::new(mode.objective()),
        font(15.0),
        TextColor(DIM),
        Node {
            position_type: PositionType::Absolute,
            left: px(18),
            top: px(70),
            ..default()
        },
    ));
    commands.spawn((
        HudRoot,
        HudScore,
        Text::new(""),
        font(20.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            right: px(18),
            top: px(14),
            ..default()
        },
    ));
    commands.spawn((
        HudRoot,
        HudWeaver,
        Text::new(""),
        font(20.0),
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            right: px(18),
            top: px(44),
            ..default()
        },
    ));
    commands.spawn((
        HudRoot,
        HudHint,
        Text::new(mode.controls()),
        font(15.0),
        TextColor(DIM),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(18),
            ..default()
        },
    ));
    commands.spawn((
        HudRoot,
        Text::new("N"),
        font(18.0),
        TextColor(DIM),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: percent(50),
            ..default()
        },
    ));
    commands
        .spawn((
            HudRoot,
            ToastList,
            Node {
                position_type: PositionType::Absolute,
                right: px(18),
                bottom: px(40),
                flex_direction: FlexDirection::ColumnReverse,
                row_gap: px(4),
                ..default()
            },
        ));
    // Rule card: mode summary shown during ignition and the first moments of the night.
    commands
        .spawn((
            HudRoot,
            RuleCard,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                bottom: px(40),
                width: px(720),
                margin: UiRect::left(px(-360)),
                ..panel(px(720))
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|card| {
            card.spawn((Text::new(mode.tagline()), font(22.0), TextColor(WARM)));
            card.spawn((Text::new(mode.rules_summary()), font(17.0), TextColor(INK)));
            card.spawn((Text::new(mode.controls()), font(15.0), TextColor(DIM)));
        });
}

fn despawn_hud(mut commands: Commands, hud: Query<Entity, With<HudRoot>>) {
    for e in &hud {
        commands.entity(e).despawn();
    }
}

fn update_hud(
    session: Res<Session>,
    mut timer: Query<&mut Text, (With<HudTimer>, Without<HudScore>, Without<HudWeaver>)>,
    mut score: Query<&mut Text, (With<HudScore>, Without<HudTimer>, Without<HudWeaver>)>,
    mut weaver: Query<&mut Text, (With<HudWeaver>, Without<HudTimer>, Without<HudScore>, Without<HudHint>)>,
    mut hint: Query<&mut Text, (With<HudHint>, Without<HudTimer>, Without<HudScore>, Without<HudWeaver>)>,
) {
    let Some(world) = session.world() else { return };
    let phase_text = match world.phase {
        Phase::Intro { .. } => "Dusk. The beacon is lit.".to_string(),
        Phase::Night => match world.night_remaining() {
            Some(r) => format!("Night remaining  {:01}:{:02}", (r / 60.0) as u32, (r % 60.0) as u32),
            None => "The voyage is under way.".to_string(),
        },
        Phase::Dawn { .. } => "First light.".into(),
        Phase::Playback => match &world.rules {
            Rules::WorldWeaver(ww) => format!("The ship sails  {:02}s", ww.playback.elapsed as u32),
            _ => String::new(),
        },
        Phase::Finished => String::new(),
    };
    for mut t in &mut timer {
        t.0 = phase_text.clone();
    }

    let score_text = match &world.rules {
        Rules::NightWatch(nw) => {
            let rescued = world.sea.entities.iter().filter(|e| e.status == Status::Secured).count();
            let lost = world.sea.entities.iter().filter(|e| e.status == Status::Sunk).count();
            format!("Rescued {rescued} / {}    Lost {lost}", nw.schedule.len())
        }
        Rules::MutableSea(ms) => {
            let secured = world.sea.entities.iter().filter(|e| e.status == Status::Secured).count();
            format!("Secured {secured} / {}  (need {})", ms.identities.len(), ms.target_rescues)
        }
        Rules::WorldWeaver(ww) => {
            let edited = ww.edited.iter().filter(|e| **e).count();
            format!("Sectors copied into World 1: {edited} / {}", ww.edited.len())
        }
        Rules::SpiralVoyage(sv) => format!("Harbor: World {}", sv.worlds.len()),
    };
    for mut t in &mut score {
        t.0 = score_text.clone();
    }

    let weaver_text = match &world.rules {
        Rules::WorldWeaver(ww) if world.phase == Phase::Night => {
            let layer = ww.layer_for(&world.sea) as usize;
            let sector = world.sea.beam.sector_index(world.tuning());
            let state = if layer == 0 {
                if ww.edited[sector] { "assembled, edited" } else { "assembled, baseline" }
            } else if ww.edited[sector] {
                "sector edited in World 1"
            } else {
                "sector unedited"
            };
            format!(
                "Inspecting {} ({})    Sector {}  ({state})",
                world_weaver::LAYER_NAMES[layer],
                world_weaver::LAYER_GLYPHS[layer],
                sector + 1
            )
        }
        Rules::SpiralVoyage(sv) if matches!(world.phase, Phase::Night | Phase::Intro { .. }) => {
            format!("World {} of {}", world.inspected_world() + 1, sv.worlds.len())
        }
        _ => String::new(),
    };
    for mut t in &mut weaver {
        t.0 = weaver_text.clone();
    }
    let hint_text = match world.phase {
        Phase::Intro { .. } | Phase::Night => world.mode.controls(),
        Phase::Dawn { .. } => "First light. The sea is revealed.",
        Phase::Playback => "The ship follows the passage it found through your sea.    Esc pause",
        Phase::Finished => "",
    };
    for mut t in &mut hint {
        if t.0 != hint_text {
            t.0 = hint_text.to_string();
        }
    }
}

fn update_rule_card(session: Res<Session>, mut cards: Query<(&mut Visibility, &mut BackgroundColor), With<RuleCard>>) {
    let Some(world) = session.world() else { return };
    // Visible through ignition and the first eight seconds of the night, then fades out.
    let alpha = match world.phase {
        Phase::Intro { .. } => 1.0,
        Phase::Night => (1.0 - (session.night_seconds - 14.0) / 3.0).clamp(0.0, 1.0),
        _ => 0.0,
    };
    for (mut vis, mut bg) in &mut cards {
        *vis = if alpha > 0.0 { Visibility::Visible } else { Visibility::Hidden };
        bg.0 = PANEL.with_alpha(0.78 * alpha);
    }
}

/// Short feedback lines for rescue, loss, transformation, capture.
fn push_toasts(
    mut commands: Commands,
    session: Res<Session>,
    time: Res<Time>,
    list: Query<Entity, With<ToastList>>,
) {
    let Some(world) = session.world() else { return };
    let Ok(list) = list.single() else { return };
    for ev in &session.events {
        let name = |id| world.sea.entity(id).map(|e| e.name).unwrap_or("A vessel");
        let text = match ev {
            sim::Event::Rescued { id, .. } => format!("{} made harbor.", name(*id)),
            sim::Event::Sunk { id, cause, .. } => match cause {
                sim::Cause::Rock => format!("{} struck the rocks.", name(*id)),
                sim::Cause::Creature => format!("{} was taken.", name(*id)),
            },
            sim::Event::Wrecked { id, .. } => format!("{} is wrecked.", name(*id)),
            sim::Event::Transformed { id, to, .. } => {
                let next = to.next();
                let dark = world.tuning().mutable_dark_durations[to.index()];
                format!(
                    "{} became {} {}. Left dark, it becomes {} {} in {:.0} s.",
                    name(*id),
                    article(*to),
                    to.name(),
                    article(next),
                    next.name(),
                    dark
                )
            }
            sim::Event::Captured { sector, layer } => format!(
                "Sector {} copied from {} into World 1.",
                sector + 1,
                world_weaver::LAYER_NAMES[*layer as usize]
            ),
            sim::Event::AssembledWorld => "Assembled world.".into(),
            sim::Event::LayerChanged { layer } => format!("Inspecting World {}", *layer as usize + 1),
            sim::Event::NoPassage => "No passage to harbor.".into(),
            sim::Event::ShipCrossed { world: w } => format!("The Wayfarer crossed into World {}.", *w as usize + 1),
            sim::Event::CreatureAppears { pos, .. } => format!("Something stirs in the {} dark. It follows the brightest light it can see.", sim::geom::compass_word(*pos)),
            sim::Event::VoyageBegins => "The ship enters from the shipping lane.".into(),
            _ => continue,
        };
        commands.spawn((
            Toast {
                until: time.elapsed_secs() + 5.0,
            },
            Text::new(text),
            font(17.0),
            TextColor(INK),
            ChildOf(list),
        ));
    }
}

fn article(form: sim::Form) -> &'static str {
    match form {
        sim::Form::Island => "an",
        _ => "a",
    }
}

fn expire_toasts(mut commands: Commands, time: Res<Time>, toasts: Query<(Entity, &Toast)>) {
    for (e, t) in &toasts {
        if time.elapsed_secs() > t.until {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_pause(mut commands: Commands) {
    commands
        .spawn((
            DespawnOnExit(AppState::Paused),
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(50),
                margin: UiRect::new(px(-220), px(0), px(-110), px(0)),
                ..panel(px(440))
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn((Text::new("Paused"), font(30.0), TextColor(WARM)));
            p.spawn((Text::new("Esc  resume\nR    restart this scenario\nM    return to menu"), font(18.0), TextColor(INK)));
            p.spawn((PauseSettingsText, Text::new(""), font(16.0), TextColor(DIM)));
        });
}

fn update_pause_text(settings: Res<Settings>, mut text: Query<&mut Text, With<PauseSettingsText>>) {
    for mut t in &mut text {
        t.0 = format!(
            "[ ]  brightness {:+.1} stops\nF4   constant-speed rotation: {}\nF3   debug overlay: {}\nF6   skip to dawn    F9   autopilot: {}",
            settings.brightness,
            if settings.constant_speed_rotation { "on" } else { "off" },
            if settings.debug_overlay { "on" } else { "off" },
            if settings.autopilot { "on" } else { "off" }
        );
    }
}

fn spawn_result(mut commands: Commands, session: Res<Session>) {
    let Some(world) = session.world() else { return };
    let Some(outcome) = &world.outcome else { return };
    commands
        .spawn((
            DespawnOnExit(AppState::Result),
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(50),
                margin: UiRect::new(px(-360), px(0), px(-160), px(0)),
                ..panel(px(720))
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(format!("{}: {}", world.mode.title(), if outcome.success { "success" } else { "not this night" })),
                font(20.0),
                TextColor(DIM),
            ));
            p.spawn((Text::new(outcome.headline.clone()), font(28.0), TextColor(WARM)));
            for line in &outcome.details {
                p.spawn((Text::new(line.clone()), font(17.0), TextColor(INK)));
            }
            p.spawn((
                Text::new("R / Enter  retry the same scenario        M / Esc  return to menu"),
                font(16.0),
                TextColor(DIM),
                Node {
                    margin: UiRect::top(px(16)),
                    ..default()
                },
            ));
        });
}

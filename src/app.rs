//! Session controller: app states, the fixed-step simulation driver, input mapping, settings.

use crate::sim::{self, Mode, Phase};
use bevy::prelude::*;

/// Top-level screens. The simulation's own phases (intro/night/dawn/playback) live inside
/// `Session` while the app is `Playing`.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Menu,
    Playing,
    Paused,
    Result,
}

/// Player-facing settings and developer toggles, kept across sessions.
#[derive(Resource, Debug, Clone)]
pub struct Settings {
    /// Added exposure stops; positive brightens the whole image for dim monitors.
    pub brightness: f32,
    /// Developer experiment: A/D select a rotation direction that continues after release.
    pub constant_speed_rotation: bool,
    pub debug_overlay: bool,
    /// Developer autopilot (F9): the scripted keeper plays the scenario.
    pub autopilot: bool,
    /// Desired-heading dial lines on observable ships (F5). On by default in prototype play.
    pub heading_lines: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            constant_speed_rotation: false,
            debug_overlay: false,
            autopilot: false,
            heading_lines: true,
        }
    }
}

/// Beam and entity poses before the latest fixed step, so presentation can interpolate between
/// steps instead of showing 60 Hz motion on a faster display.
#[derive(Default)]
struct Snapshot {
    beam_bearing: f32,
    beam_range: f32,
    entities: std::collections::HashMap<sim::EntityId, (glam::Vec2, f32)>,
}

impl Snapshot {
    fn of(world: &sim::World) -> Self {
        Self {
            beam_bearing: world.sea.beam.bearing(),
            beam_range: world.sea.beam.range,
            entities: world.sea.entities.iter().map(|e| (e.id, (e.pos, e.heading))).collect(),
        }
    }
}

#[derive(Resource, Default)]
pub struct Session {
    pub world: Option<sim::World>,
    /// Mode selected on the menu; also what Retry restarts.
    pub selected: Mode,
    /// Incremented every time a session starts so presentation can rebuild per-session state.
    pub generation: u32,
    /// Space edge-trigger gathered per frame and consumed by the first fixed step.
    capture_pending: bool,
    /// Events produced by the simulation this frame, for audio/UI. Cleared each `PreUpdate`.
    pub events: Vec<sim::Event>,
    /// Seconds since the night phase began (drives the fading rule card).
    pub night_seconds: f32,
    /// Scripted keeper for the current scenario (developer autopilot).
    keeper: Option<sim::autopilot::Keeper>,
    prev: Snapshot,
    /// Fraction of the way from the previous fixed step to the next, refreshed every frame.
    alpha: f32,
}

impl Session {
    pub fn world(&self) -> Option<&sim::World> {
        self.world.as_ref()
    }

    pub fn start(&mut self, mode: Mode) {
        self.selected = mode;
        self.world = Some(sim::World::new(mode, sim::Tuning::default()));
        self.keeper = Some(sim::autopilot::Keeper::for_mode(mode));
        self.generation += 1;
        self.capture_pending = false;
        self.events.clear();
        self.night_seconds = 0.0;
        self.prev = Snapshot::default();
        if let Some(world) = &self.world {
            self.prev = Snapshot::of(world);
        }
    }

    /// An entity's pose interpolated between the previous fixed step and the current one.
    pub fn view_pose(&self, e: &sim::Entity) -> (glam::Vec2, f32) {
        let alpha = self.alpha;
        match self.prev.entities.get(&e.id) {
            Some(&(pos, heading)) => {
                (pos.lerp(e.pos, alpha), heading + sim::geom::angle_delta(heading, e.heading) * alpha)
            }
            None => (e.pos, e.heading),
        }
    }

    /// The beam as it should be drawn this frame: bearing and range interpolated, winding kept on
    /// the current step's turn so revolution-based readings stay exact.
    pub fn view_beam(&self) -> Option<sim::Beam> {
        let world = self.world()?;
        let alpha = self.alpha;
        let mut beam = world.sea.beam.clone();
        beam.winding -= sim::geom::angle_delta(self.prev.beam_bearing, beam.bearing()) * (1.0 - alpha);
        beam.range = self.prev.beam_range + (beam.range - self.prev.beam_range) * alpha;
        Some(beam)
    }

    pub fn view_footprint(&self) -> Option<sim::Footprint> {
        Some(self.view_beam()?.footprint(self.world()?.tuning()))
    }
}

/// Simulation → render coordinates: the flat sea maps sim `y` (north) to `-z`.
pub fn to_world(p: glam::Vec2) -> Vec3 {
    Vec3::new(p.x, 0.0, -p.y)
}

pub fn to_world_h(p: glam::Vec2, height: f32) -> Vec3 {
    Vec3::new(p.x, height, -p.y)
}

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<Settings>()
            .init_resource::<Session>()
            .insert_resource(Time::<Fixed>::from_hz(60.0))
            .add_systems(Startup, autoplay_from_env)
            .add_systems(PreUpdate, clear_events)
            .add_systems(Update, global_hotkeys)
            .add_systems(Update, gather_capture.run_if(in_state(AppState::Playing)))
            .add_systems(FixedUpdate, step_simulation.run_if(in_state(AppState::Playing)))
            .add_systems(RunFixedMainLoop, refresh_alpha.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop))
            .add_systems(Update, (pause_input, finish_when_done).run_if(in_state(AppState::Playing)))
            .add_systems(Update, paused_input.run_if(in_state(AppState::Paused)))
            .add_systems(Update, result_input.run_if(in_state(AppState::Result)))
            .add_systems(OnEnter(AppState::Menu), clear_session);
    }
}

fn clear_events(mut session: ResMut<Session>) {
    session.events.clear();
}

/// Interpolation fraction for this frame; frozen at the current step while the simulation is
/// not running so paused scenes hold still.
fn refresh_alpha(state: Res<State<AppState>>, fixed: Res<Time<Fixed>>, mut session: ResMut<Session>) {
    session.alpha = if *state.get() == AppState::Playing { fixed.overstep_fraction() } else { 1.0 };
}

fn global_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>, mut session: ResMut<Session>) {
    if keys.just_pressed(KeyCode::F6) {
        if let Some(world) = session.world.as_mut() {
            world.skip_to_dawn();
        }
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        settings.brightness = (settings.brightness + 0.5).min(3.0);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        settings.brightness = (settings.brightness - 0.5).max(-1.0);
    }
    if keys.just_pressed(KeyCode::F3) {
        settings.debug_overlay = !settings.debug_overlay;
    }
    if keys.just_pressed(KeyCode::F4) {
        settings.constant_speed_rotation = !settings.constant_speed_rotation;
    }
    if keys.just_pressed(KeyCode::F9) {
        settings.autopilot = !settings.autopilot;
    }
    if keys.just_pressed(KeyCode::F5) {
        settings.heading_lines = !settings.heading_lines;
    }
}

/// `FIRST_LIGHT_AUTOPLAY=nightwatch|mutablesea|worldweaver|spiralvoyage` starts that mode on the autopilot.
fn autoplay_from_env(
    mut settings: ResMut<Settings>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Ok(value) = std::env::var("FIRST_LIGHT_AUTOPLAY") else { return };
    let mode = match value.to_ascii_lowercase().as_str() {
        "nightwatch" | "nw" => Mode::NightWatch,
        "mutablesea" | "ms" => Mode::MutableSea,
        "worldweaver" | "ww" => Mode::WorldWeaver,
        "spiralvoyage" | "sv" => Mode::SpiralVoyage,
        _ => return,
    };
    settings.autopilot = true;
    settings.debug_overlay = std::env::var_os("FIRST_LIGHT_DEBUG").is_some();
    session.start(mode);
    next.set(AppState::Playing);
}

fn gather_capture(keys: Res<ButtonInput<KeyCode>>, mut session: ResMut<Session>) {
    if keys.just_pressed(KeyCode::Space) {
        session.capture_pending = true;
    }
}

fn step_simulation(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<Settings>,
    time: Res<Time<Fixed>>,
    mut session: ResMut<Session>,
) {
    let capture = std::mem::take(&mut session.capture_pending);
    let dt = time.delta_secs();
    let Session { world, keeper, prev, .. } = &mut *session;
    let Some(world) = world.as_mut() else { return };
    *prev = Snapshot::of(world);
    let axis = |neg: &[KeyCode], pos: &[KeyCode]| -> f32 {
        let n = neg.iter().any(|k| keys.pressed(*k));
        let p = pos.iter().any(|k| keys.pressed(*k));
        (p as i32 - n as i32) as f32
    };
    let manual = sim::Input {
        rotate: axis(&[KeyCode::KeyA, KeyCode::ArrowLeft], &[KeyCode::KeyD, KeyCode::ArrowRight]),
        range: axis(&[KeyCode::KeyS, KeyCode::ArrowDown], &[KeyCode::KeyW, KeyCode::ArrowUp]),
        capture,
    };
    // A fresh beam control hands the lamp back to the player; the keeper never fights a hand on
    // it. Fresh presses only: the D that starts a demo from the menu is still held this step.
    const BEAM_KEYS: [KeyCode; 9] = [
        KeyCode::KeyA,
        KeyCode::KeyD,
        KeyCode::KeyW,
        KeyCode::KeyS,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::Space,
    ];
    if settings.autopilot && BEAM_KEYS.iter().any(|k| keys.just_pressed(*k)) {
        settings.autopilot = false;
    }
    let input = match keeper.as_mut() {
        Some(k) if settings.autopilot => k.input(world),
        _ => manual,
    };
    world.sea.beam.constant_speed = settings.constant_speed_rotation;
    world.step(input, dt);
    let in_night = world.phase == Phase::Night;
    let events = world.drain_events();
    if in_night {
        session.night_seconds += dt;
    }
    session.events.extend(events);
}

fn pause_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Paused);
    }
}

fn paused_input(keys: Res<ButtonInput<KeyCode>>, mut session: ResMut<Session>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Playing);
    } else if keys.just_pressed(KeyCode::KeyR) {
        let mode = session.selected;
        session.start(mode);
        next.set(AppState::Playing);
    } else if keys.just_pressed(KeyCode::KeyM) {
        next.set(AppState::Menu);
    }
}

fn finish_when_done(session: Res<Session>, mut next: ResMut<NextState<AppState>>) {
    if session.world().is_some_and(|w| w.phase == Phase::Finished) {
        next.set(AppState::Result);
    }
}

fn result_input(keys: Res<ButtonInput<KeyCode>>, mut session: ResMut<Session>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::KeyR) || keys.just_pressed(KeyCode::Enter) {
        let mode = session.selected;
        session.start(mode);
        next.set(AppState::Playing);
    } else if keys.just_pressed(KeyCode::KeyM) || keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Menu);
    }
}

fn clear_session(mut session: ResMut<Session>) {
    session.world = None;
    session.events.clear();
}

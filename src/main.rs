//! First Lighthouse — lighthouse prototype. Pure simulation in `sim`, Bevy presentation elsewhere.

mod app;
mod audio;
mod debug;
mod entities;
mod labels;
mod models;
mod scene;
mod sea;
mod sim;
mod spiral_widget;
mod ui;

use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

fn main() {
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut window = Window {
        title: "First Lighthouse".into(),
        resolution: WindowResolution::new(1600, 900),
        present_mode: PresentMode::AutoVsync,
        ..default()
    };
    #[cfg(target_arch = "wasm32")]
    {
        // Render into the page's canvas and follow the viewport; keys like Space must not scroll.
        window.canvas = Some("#game".into());
        window.fit_canvas_to_parent = true;
        window.prevent_default_event_handling = true;
    }
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(window), ..default() }))
        .add_plugins((
            app::AppPlugin,
            sea::SeaPlugin,
            scene::ScenePlugin,
            entities::EntitiesPlugin,
            labels::LabelsPlugin,
            ui::UiPlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
            spiral_widget::SpiralWidgetPlugin,
        ))
        .add_systems(Update, dispatch_audio)
        .run();
}

/// Feed simulation events to the audio layer and drive the mechanism loop.
/// Events are cleared in `PreUpdate` by the app plugin, so every Update consumer sees them.
fn dispatch_audio(
    mut commands: Commands,
    session: Res<app::Session>,
    sounds: Option<Res<audio::Sounds>>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<app::AppState>>,
    mut mechanism: Query<&mut AudioSink, With<audio::MechanismLoop>>,
) {
    let Some(sounds) = sounds.as_deref() else { return };
    for ev in &session.events {
        audio::play_event(&mut commands, sounds, ev);
    }
    let rotating = *state.get() == app::AppState::Playing
        && session.world().is_some_and(|w| w.beam_active())
        && [KeyCode::KeyA, KeyCode::KeyD, KeyCode::ArrowLeft, KeyCode::ArrowRight].iter().any(|k| keys.pressed(*k));
    audio::set_rotating(&mut mechanism, rotating);
}

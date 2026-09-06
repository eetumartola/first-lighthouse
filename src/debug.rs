//! Developer overlay (F3): reveals every entity, the footprint, charge grid, guidance targets,
//! transformation timers and World Weaver state without changing simulation behaviour.

use crate::app::{to_world, to_world_h, Session, Settings};
use crate::sim::entity::Target;
use crate::sim::{self, mutable_sea, Footprint, Form, Rules};
use bevy::color::palettes::css;
use bevy::prelude::*;

#[derive(Component)]
struct DebugText;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(KeyScript::from_env())
            .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_text)
            .add_systems(PreUpdate, scripted_keys.after(bevy::input::InputSystems))
            .add_systems(Update, (draw_overlay, update_text, screenshot_hotkey));
    }
}

/// `FIRST_LIGHT_KEYS="2:F12,3:ArrowDown,4:Enter"` presses keys at the given seconds after launch
/// through the normal `ButtonInput` path, so menus and screens can be exercised unattended.
#[derive(Resource, Default)]
struct KeyScript {
    pending: Vec<(f32, KeyCode)>,
    held: Vec<KeyCode>,
}

impl KeyScript {
    fn from_env() -> Self {
        let Ok(spec) = std::env::var("FIRST_LIGHT_KEYS") else { return Self::default() };
        let mut pending: Vec<(f32, KeyCode)> = spec
            .split(',')
            .filter_map(|item| {
                let (t, key) = item.trim().split_once(':')?;
                Some((t.parse().ok()?, parse_key(key)?))
            })
            .collect();
        pending.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { pending, held: Vec::new() }
    }
}

fn parse_key(name: &str) -> Option<KeyCode> {
    Some(match name {
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "Space" => KeyCode::Space,
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F6" => KeyCode::F6,
        "F9" => KeyCode::F9,
        "F12" => KeyCode::F12,
        "R" => KeyCode::KeyR,
        "M" => KeyCode::KeyM,
        "A" => KeyCode::KeyA,
        "D" => KeyCode::KeyD,
        "W" => KeyCode::KeyW,
        "S" => KeyCode::KeyS,
        "[" => KeyCode::BracketLeft,
        "]" => KeyCode::BracketRight,
        _ => return None,
    })
}

fn scripted_keys(mut script: ResMut<KeyScript>, time: Res<Time<Real>>, mut input: ResMut<ButtonInput<KeyCode>>) {
    for key in script.held.drain(..) {
        input.release(key);
    }
    let now = time.elapsed_secs();
    while script.pending.first().is_some_and(|(t, _)| *t <= now) {
        let (_, key) = script.pending.remove(0);
        input.press(key);
        script.held.push(key);
    }
}

/// F12 writes `screenshot-N.png` to the working directory. `FIRST_LIGHT_SHOTS=<seconds>` also
/// captures one every N seconds (used with the autoplay env var for unattended checks).
fn screenshot_hotkey(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut counter: Local<u32>,
    mut next_auto: Local<Option<f32>>,
) {
    let interval = std::env::var("FIRST_LIGHT_SHOTS").ok().and_then(|v| v.parse::<f32>().ok());
    let auto_due = match (interval, *next_auto) {
        (Some(_), None) => {
            *next_auto = Some(time.elapsed_secs() + 2.0);
            false
        }
        (Some(i), Some(due)) if time.elapsed_secs() >= due => {
            *next_auto = Some(due + i);
            true
        }
        _ => false,
    };
    if keys.just_pressed(KeyCode::F12) || auto_due {
        let path = format!("screenshot-{:03}.png", *counter);
        *counter += 1;
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(path));
    }
}

fn spawn_text(mut commands: Commands) {
    commands.spawn((
        DebugText,
        Text::new(""),
        TextFont::from_font_size(13.0),
        TextColor(Color::srgb(0.7, 1.0, 0.7)),
        Node { position_type: PositionType::Absolute, left: px(18), top: px(80), ..default() },
        Visibility::Hidden,
    ));
}

fn form_color(form: Form) -> Color {
    match form {
        Form::Ship => css::GOLD.into(),
        Form::Wreck => css::SADDLE_BROWN.into(),
        Form::Creature => css::LIME.into(),
        Form::Island => css::GRAY.into(),
    }
}

fn draw_overlay(settings: Res<Settings>, session: Res<Session>, mut gizmos: Gizmos) {
    if !settings.debug_overlay {
        return;
    }
    let Some(world) = session.world() else { return };
    let sea = &world.sea;
    let t = world.tuning();

    // Charge grid of the inspected world: cells with any charge.
    let field = world.view_charge();
    for (i, c) in field.charge.iter().enumerate() {
        if *c <= 0.0 {
            continue;
        }
        let p = field.cell_center(i);
        let k = (c / t.charge_cap).clamp(0.0, 1.0);
        let color = if *c >= t.strong_threshold {
            Color::srgba(0.2, 1.0, 1.0, 0.25 + 0.5 * k)
        } else {
            Color::srgba(0.2, 0.5, 0.8, 0.35)
        };
        gizmos.rect(
            Isometry3d::new(to_world_h(p, 0.3), Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            Vec2::splat(field.cell * 0.9),
            color,
        );
    }

    // Footprint outline.
    if world.beam_active() {
        match world.footprint() {
            Footprint::Spot { bearing, half_angle, r_min, r_max } => {
                let n = 12;
                let mut pts = Vec::with_capacity(n * 2 + 2);
                for i in 0..=n {
                    let a = bearing - half_angle + 2.0 * half_angle * i as f32 / n as f32;
                    pts.push(to_world_h(sim::geom::dir(a) * r_max, 0.4));
                }
                for i in (0..=n).rev() {
                    let a = bearing - half_angle + 2.0 * half_angle * i as f32 / n as f32;
                    pts.push(to_world_h(sim::geom::dir(a) * r_min, 0.4));
                }
                pts.push(pts[0]);
                gizmos.linestrip(pts, css::ORANGE);
            }
            Footprint::Sector { angle_start, angle_end, r_min, r_max, .. } => {
                let a = to_world_h(sim::geom::dir(angle_start) * r_min, 0.4);
                let b = to_world_h(sim::geom::dir(angle_start) * r_max, 0.4);
                let c = to_world_h(sim::geom::dir(angle_end) * r_max, 0.4);
                let d = to_world_h(sim::geom::dir(angle_end) * r_min, 0.4);
                gizmos.linestrip([a, b, c, d, a], css::ORANGE);
            }
        }
    }

    // Entities (every world), hull headings, desired headings, predator targets, contact radii.
    let inspected = world.inspected_world();
    for e in &sea.entities {
        let other_world = world.entity_world(e) != inspected;
        let color = if other_world { css::DIM_GRAY.into() } else { form_color(e.form) };
        let p = to_world_h(e.pos, 0.5);
        gizmos.circle(Isometry3d::new(p, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)), e.radius, color);
        gizmos.line(p, p + to_world(sim::geom::dir(e.heading)) * 4.0, color);
        if e.form == Form::Ship && !e.brain.desired.is_nan() {
            gizmos.line(p, p + to_world(sim::geom::dir(e.brain.desired)) * 7.0, css::WHITE);
        }
        if let Some(Target::Patch(tp)) = e.brain.target {
            gizmos.line(p, to_world_h(tp, 0.6), css::RED);
        }
        if e.form == Form::Creature {
            gizmos.circle(
                Isometry3d::new(p, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                t.creature_contact_radius,
                css::RED,
            );
            gizmos.circle(
                Isometry3d::new(p, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                t.creature_detect_radius,
                Color::srgba(1.0, 0.3, 0.3, 0.25),
            );
        }
    }

    // Land of the inspected world and the harbor.
    let rocks: &[sim::Circle] = match &world.rules {
        Rules::SpiralVoyage(sv) => &sv.worlds[inspected].rocks,
        _ => &sea.rocks,
    };
    for r in rocks {
        gizmos.circle(
            Isometry3d::new(to_world_h(r.center, 0.5), Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            r.radius,
            css::DIM_GRAY,
        );
    }
    let h = sea.harbor();
    gizmos.circle(
        Isometry3d::new(to_world_h(h.center, 0.5), Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        h.radius,
        css::GOLD,
    );

    if let Rules::WorldWeaver(ww) = &world.rules {
        gizmos.sphere(Isometry3d::from_translation(to_world_h(ww.lane_start, 1.0)), 1.2, css::YELLOW);
        if let Some(route) = &ww.playback.route {
            let pts: Vec<Vec3> = route.iter().map(|p| to_world_h(*p, 0.7)).collect();
            gizmos.linestrip(pts, css::YELLOW);
        }
        // Preview sector's land as circles even in darkness.
        let layer = ww.layer_for(sea);
        let sector = sea.beam.sector_index(t);
        for r in ww.slice_geometry(layer, sector, t) {
            gizmos.circle(
                Isometry3d::new(to_world_h(r.center, 0.5), Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                r.radius,
                css::VIOLET,
            );
        }
    }
}

fn update_text(
    settings: Res<Settings>,
    session: Res<Session>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut q: Query<(&mut Text, &mut Visibility), With<DebugText>>,
) {
    let fps = diagnostics
        .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    for (mut text, mut vis) in &mut q {
        if !settings.debug_overlay {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;
        let Some(world) = session.world() else {
            text.0 = format!("debug: no session  {fps:.0} fps {frame_ms:.1} ms");
            continue;
        };
        let sea = &world.sea;
        let mut lines = vec![format!(
            "{fps:.0} fps {frame_ms:.1} ms  phase {:?}  t={:.1}  beam bearing {:.1}° range {:.1} winding {:.2} rev {}",
            world.phase,
            sea.time,
            sea.beam.bearing().to_degrees(),
            sea.beam.range,
            sea.beam.winding,
            sea.beam.revolution()
        )];
        lines.push(format!(
            "inspected world {}  charged cells {}  dusk {:.2}",
            world.inspected_world() + 1,
            world.view_charge().charge.iter().filter(|c| **c > 0.0).count(),
            world.dusk()
        ));
        for e in &sea.entities {
            let timer = e
                .mutable
                .map(|m| {
                    format!(
                        "  timer {:.1}/{:.0}{}",
                        m.progress,
                        mutable_sea::dark_duration(e.form, sea),
                        if m.deferred { " deferred" } else { "" }
                    )
                })
                .unwrap_or_default();
            lines.push(format!(
                "{:<11} {:<8} {:?} ({:6.1},{:6.1}) hdg {:5.1}° want {:5.1}° score {:5.1} world {} {:?}{}",
                e.name,
                e.form.name(),
                e.status,
                e.pos.x,
                e.pos.y,
                e.heading.to_degrees(),
                e.brain.desired.to_degrees(),
                e.brain.desired_score,
                world.entity_world(e) + 1,
                e.brain.target,
                timer
            ));
        }
        match &world.rules {
            Rules::WorldWeaver(ww) => {
                let edited: Vec<usize> =
                    ww.edited.iter().enumerate().filter(|(_, e)| **e).map(|(s, _)| s + 1).collect();
                lines.push(format!(
                    "weaver inspecting world {} sector {}  edited sectors {:?}  route {:?} speed {:.1}",
                    ww.layer_for(sea) + 1,
                    sea.beam.sector_index(&sea.tuning) + 1,
                    edited,
                    ww.playback.route.as_ref().map(|r| r.len()),
                    ww.playback.speed
                ));
            }
            Rules::SpiralVoyage(sv) => {
                lines.push(format!(
                    "spiral beam winding {:.2} (world {})  ship world {:?}  end {:?}",
                    sea.beam.winding,
                    sv.beam_world(sea) + 1,
                    sv.ship_world(sea).map(|w| w + 1),
                    sv.end
                ));
            }
            _ => {}
        }
        text.0 = lines.join("\n");
    }
}

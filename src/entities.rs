//! Presentation of simulation entities: one silhouette family for ship, wreck, creature and
//! island, shown only where the simulation's visibility rules allow.

use crate::app::{to_world, to_world_h, Session, Settings};
use crate::scene::SessionScoped;
use crate::sim::{self, mutable_sea, EntityId, Form, Visibility as SimVis};
use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_4;

/// Silhouette clarity is quantised into this many alpha levels (one material each).
const SILHOUETTE_LEVELS: usize = 6;

#[derive(Resource)]
pub struct FormAssets {
    hull: Handle<Mesh>,
    prow: Handle<Mesh>,
    cabin: Handle<Mesh>,
    mast: Handle<Mesh>,
    broken_mast: Handle<Mesh>,
    lantern: Handle<Mesh>,
    body: Handle<Mesh>,
    crest: Handle<Mesh>,
    eye: Handle<Mesh>,
    mound: Handle<Mesh>,
    ridge: Handle<Mesh>,
    wood: Handle<StandardMaterial>,
    dark_wood: Handle<StandardMaterial>,
    lantern_glow: Handle<StandardMaterial>,
    creature: Handle<StandardMaterial>,
    eye_glow: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    /// Opaque dark shape at increasing clarity, indexed by glow strength.
    silhouettes: Vec<Handle<StandardMaterial>>,
}

/// Parent of a simulation entity's visual.
#[derive(Component)]
pub struct Visual {
    pub id: EntityId,

}

#[derive(Component)]
struct LitPart;

#[derive(Component)]
struct SilhouettePart;

#[derive(Resource, Default)]
struct VisualMap {
    generation: u32,
    map: HashMap<EntityId, (Entity, Form)>,
}

pub struct EntitiesPlugin;

impl Plugin for EntitiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisualMap>()
            .add_systems(Startup, setup_assets)
            .add_systems(Update, (sync_visuals, apply_visibility, draw_heading_lines).chain());
    }
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FormAssets {
        hull: meshes.add(Cuboid::new(1.4, 0.7, 3.0)),
        prow: meshes.add(Cuboid::new(1.0, 0.7, 1.0)),
        cabin: meshes.add(Cuboid::new(0.9, 0.55, 1.0)),
        mast: meshes.add(Cylinder::new(0.11, 3.6)),
        broken_mast: meshes.add(Cylinder::new(0.11, 1.5)),
        lantern: meshes.add(Sphere::new(0.32)),
        body: meshes.add(Capsule3d::new(0.7, 2.4)),
        crest: meshes.add(Cone {
            radius: 0.5,
            height: 1.6,
        }),
        eye: meshes.add(Sphere::new(0.11)),
        mound: meshes.add(Sphere::new(3.0)),
        ridge: meshes.add(Cuboid::new(1.6, 1.4, 1.6)),
        wood: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.28, 0.17),
            perceptual_roughness: 0.8,
            ..default()
        }),
        dark_wood: materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.11, 0.08),
            perceptual_roughness: 0.95,
            ..default()
        }),
        lantern_glow: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.75, 0.4),
            emissive: LinearRgba::new(9.0, 5.0, 1.6, 1.0),
            ..default()
        }),
        creature: materials.add(StandardMaterial {
            base_color: Color::srgb(0.07, 0.15, 0.15),
            // Faint bioluminescent sheen so a surfacing creature reads against black water.
            emissive: LinearRgba::new(0.12, 0.55, 0.5, 1.0),
            perceptual_roughness: 0.55,
            reflectance: 0.6,
            ..default()
        }),
        eye_glow: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 1.0, 0.7),
            emissive: LinearRgba::new(1.5, 5.0, 2.5, 1.0),
            ..default()
        }),
        stone: materials.add(StandardMaterial {
            base_color: Color::srgb(0.33, 0.32, 0.29),
            perceptual_roughness: 0.95,
            ..default()
        }),
        silhouettes: (0..SILHOUETTE_LEVELS)
            .map(|i| {
                let alpha = 0.35 + 0.65 * i as f32 / (SILHOUETTE_LEVELS - 1) as f32;
                materials.add(StandardMaterial {
                    base_color: Color::srgba(0.04, 0.06, 0.09, alpha),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })
            })
            .collect(),
    });
}

/// (mesh, lit material, local transform) for each part of a form. Parts reuse the same prow
/// wedge, mast/crest and hull proportions so the forms read as one myth.
fn parts(assets: &FormAssets, form: Form) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> {
    let wedge = Quat::from_rotation_y(FRAC_PI_4);
    match form {
        Form::Ship => vec![
            (assets.hull.clone(), assets.wood.clone(), Transform::from_xyz(0.0, 0.35, 0.0)),
            (assets.prow.clone(), assets.wood.clone(), Transform::from_xyz(0.0, 0.35, -1.75).with_rotation(wedge)),
            (assets.cabin.clone(), assets.dark_wood.clone(), Transform::from_xyz(0.0, 0.95, 0.75)),
            (assets.mast.clone(), assets.dark_wood.clone(), Transform::from_xyz(0.0, 2.3, -0.2)),
            (assets.lantern.clone(), assets.lantern_glow.clone(), Transform::from_xyz(0.0, 4.25, -0.2)),
        ],
        Form::Wreck => vec![
            (
                assets.hull.clone(),
                assets.dark_wood.clone(),
                Transform::from_xyz(0.0, -0.05, 0.2).with_rotation(Quat::from_rotation_z(0.42) * Quat::from_rotation_x(0.12)),
            ),
            (
                assets.prow.clone(),
                assets.dark_wood.clone(),
                Transform::from_xyz(0.35, 0.05, -1.6).with_rotation(Quat::from_rotation_z(0.5) * wedge),
            ),
            (
                assets.broken_mast.clone(),
                assets.dark_wood.clone(),
                Transform::from_xyz(0.4, 0.9, 0.1).with_rotation(Quat::from_rotation_z(1.05)),
            ),
        ],
        Form::Creature => vec![
            (
                assets.body.clone(),
                assets.creature.clone(),
                Transform::from_xyz(0.0, 0.05, 0.1).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ),
            (assets.prow.clone(), assets.creature.clone(), Transform::from_xyz(0.0, 0.3, -1.9).with_rotation(wedge).with_scale(Vec3::new(0.9, 0.8, 0.9))),
            (assets.crest.clone(), assets.creature.clone(), Transform::from_xyz(0.0, 1.15, 0.3)),
            (assets.eye.clone(), assets.eye_glow.clone(), Transform::from_xyz(-0.32, 0.6, -2.15)),
            (assets.eye.clone(), assets.eye_glow.clone(), Transform::from_xyz(0.32, 0.6, -2.15)),
        ],
        Form::Island => vec![
            (assets.mound.clone(), assets.stone.clone(), Transform::from_xyz(0.0, -0.9, 0.0).with_scale(Vec3::new(1.0, 0.55, 1.0))),
            (assets.ridge.clone(), assets.stone.clone(), Transform::from_xyz(0.0, 0.7, -0.8).with_rotation(wedge)),
            (assets.crest.clone(), assets.stone.clone(), Transform::from_xyz(0.0, 1.3, 0.6).with_scale(Vec3::new(1.3, 1.1, 1.3))),
        ],
    }
}

/// Spawn a form visual: lit parts and a silhouette copy under one parent.
fn spawn_form(commands: &mut Commands, assets: &FormAssets, form: Form, transform: Transform) -> Entity {
    let parent = commands.spawn((transform, Visibility::Hidden, SessionScoped)).id();
    for (mesh, material, local) in parts(assets, form) {
        commands.spawn((
            LitPart,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            local,
            ChildOf(parent),
        ));
        commands.spawn((
            SilhouettePart,
            Mesh3d(mesh),
            MeshMaterial3d(assets.silhouettes[SILHOUETTE_LEVELS - 1].clone()),
            local,
            Visibility::Hidden,
            ChildOf(parent),
        ));
    }
    parent
}

/// Visual-only enlargement so hull silhouettes read from the fixed camera; collision radii are
/// unchanged. Doubled from the first pass on playtest feedback.
const VISUAL_SCALE: f32 = 2.8;

fn heading_transform(pos: glam::Vec2, heading: f32, height: f32) -> Transform {
    let d = to_world(sim::geom::dir(heading));
    Transform::from_translation(to_world_h(pos, height))
        .looking_to(d, Vec3::Y)
        .with_scale(Vec3::splat(VISUAL_SCALE))
}

fn sync_visuals(
    mut commands: Commands,
    session: Res<Session>,
    assets: Res<FormAssets>,
    time: Res<Time>,
    mut visuals: ResMut<VisualMap>,
    mut transforms: Query<&mut Transform, With<Visual>>,
) {
    if session.generation != visuals.generation {
        // Session-scoped despawn is handled by the scene; just forget the mapping.
        visuals.map.clear();
        visuals.generation = session.generation;
    }
    let Some(world) = session.world() else { return };
    let t = time.elapsed_secs();

    for e in &world.sea.entities {
        let entry = visuals.map.get(&e.id).copied();
        let visual = match entry {
            Some((ent, form)) if form == e.form => ent,
            other => {
                if let Some((old, _)) = other {
                    commands.entity(old).despawn();
                }
                let ent = spawn_form(&mut commands, &assets, e.form, heading_transform(e.pos, e.heading, 0.0));
                commands.entity(ent).insert(Visual { id: e.id });
                visuals.map.insert(e.id, (ent, e.form));
                ent
            }
        };
        let Ok(mut tf) = transforms.get_mut(visual) else { continue };
        let phase = e.id as f32 * 1.7;
        let (bob, roll, yaw) = match e.form {
            Form::Ship => (0.1 * (t * 1.3 + phase).sin(), 0.05 * (t * 0.9 + phase).sin(), 0.0),
            Form::Creature => (0.15 * (t * 1.1 + phase).sin() - 0.1, 0.0, 0.12 * (t * 2.2 + phase).sin()),
            Form::Wreck => (0.0, 0.0, 0.0),
            Form::Island => (0.0, 0.0, 0.0),
        };
        let base = heading_transform(e.pos, e.heading + yaw, bob);
        let mut rotation = base.rotation * Quat::from_rotation_z(roll);
        let mut scale = Vec3::splat(VISUAL_SCALE);
        // Mutable Sea: a form becoming unstable shudders when inspected.
        let instability = mutable_sea::instability(e, &world.sea);
        if instability > 0.7 && e.is_active() {
            let k = (instability - 0.7) / 0.3;
            rotation *= Quat::from_rotation_y(0.18 * k * (t * 23.0 + phase).sin());
            scale = Vec3::splat(VISUAL_SCALE * (1.0 + 0.07 * k * (t * 17.0 + phase).sin()));
        }
        *tf = Transform {
            translation: base.translation,
            rotation,
            scale,
        };
    }
}

fn apply_visibility(
    session: Res<Session>,
    assets: Res<FormAssets>,
    mut parents: Query<(&Visual, &mut Visibility, &Children)>,
    mut parts: Query<(&mut Visibility, Has<LitPart>, Option<&mut MeshMaterial3d<StandardMaterial>>, Has<SilhouettePart>), Without<Visual>>,
) {
    let Some(world) = session.world() else { return };
    for (visual, mut vis, children) in &mut parents {
        let Some(e) = world.sea.entity(visual.id) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let mode_vis = world.entity_visibility(e);
        *vis = if mode_vis.is_visible() { Visibility::Visible } else { Visibility::Hidden };
        for child in children.iter() {
            if let Ok((mut cv, lit, material, sil)) = parts.get_mut(child) {
                // A creature's silhouette is its own dim glow (eyes, sheen); other forms are opaque
                // dark shapes whose clarity follows the glow around them.
                let show = match mode_vis {
                    SimVis::Lit => lit,
                    SimVis::Silhouette(_) if e.form == Form::Creature => lit,
                    SimVis::Silhouette(strength) => {
                        if sil {
                            if let Some(mut m) = material {
                                let level = ((strength * (SILHOUETTE_LEVELS - 1) as f32).round() as usize).min(SILHOUETTE_LEVELS - 1);
                                let wanted = &assets.silhouettes[level];
                                if m.0 != *wanted {
                                    m.0 = wanted.clone();
                                }
                            }
                        }
                        sil
                    }
                    SimVis::Hidden => false,
                };
                *cv = if show { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

/// Desired-heading dial line: a thin line from each observable ship toward its accepted intent.
/// It moves the instant intent changes, while the hull turns after it. Hidden ships show nothing.
fn draw_heading_lines(session: Res<Session>, settings: Res<Settings>, mut gizmos: Gizmos) {
    if !settings.heading_lines {
        return;
    }
    let Some(world) = session.world() else { return };
    let len = world.tuning().ship_length * 1.6 * VISUAL_SCALE;
    for e in world.sea.entities.iter().filter(|e| e.is_active_ship()) {
        if !world.entity_visibility(e).is_visible() || e.brain.desired.is_nan() {
            continue;
        }
        let from = to_world_h(e.pos, 0.9);
        let to = from + to_world(sim::geom::dir(e.brain.desired)) * len;
        gizmos.line(from, to, Color::srgba(1.0, 0.85, 0.55, 0.55));
        gizmos.sphere(Isometry3d::from_translation(to), 0.25, Color::srgba(1.0, 0.85, 0.55, 0.7));
    }
}

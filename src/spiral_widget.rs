//! Spiral Voyage: an abstract helicoid in the lower-left corner, one turn per world, drawn by a
//! second camera into its own viewport. The inspected world's turn glows; a warm bead marks the
//! beam's winding on the surface and a teal one the ship's, so "which world am I looking at"
//! and "where is the ship" are read at a glance.

use crate::app::Session;
use crate::sim::{level::SEAM, Rules, Tuning};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::f32::consts::TAU;

const LAYER: usize = 1;
const RADIUS: f32 = 1.0;
/// Height gained per turn; smaller than the radius so the turns read as stacked discs.
const PITCH: f32 = 0.85;
const SHAFT: f32 = 0.16;
/// Viewport size and its inset from the lower-left corner, in logical pixels.
const SIZE: Vec2 = Vec2::new(190.0, 320.0);
const INSET: Vec2 = Vec2::new(14.0, 84.0);

#[derive(Component)]
struct WidgetCamera;
#[derive(Component)]
struct Turn(usize);
#[derive(Component)]
struct BeamBead;
#[derive(Component)]
struct ShipBead;

#[derive(Resource)]
struct WidgetMaterials {
    dim: Handle<StandardMaterial>,
    lit: Handle<StandardMaterial>,
}

pub struct SpiralWidgetPlugin;

impl Plugin for SpiralWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, update);
    }
}

/// Point on the helicoid surface for an unwrapped winding and a radius.
fn on_surface(winding: f32, r: f32) -> Vec3 {
    Vec3::new(r * winding.sin(), winding / TAU * PITCH, r * winding.cos())
}

/// One full turn of the helicoid as a double-sided grid mesh.
fn turn_mesh(turn: usize) -> Mesh {
    const ALONG: usize = 48;
    const ACROSS: usize = 4;
    let (r0, r1) = (SHAFT * 0.9, RADIUS);
    let k = PITCH / TAU;
    let mut positions = Vec::with_capacity((ALONG + 1) * (ACROSS + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for i in 0..=ALONG {
        let u = SEAM + turn as f32 * TAU + i as f32 / ALONG as f32 * TAU;
        for j in 0..=ACROSS {
            let v = r0 + (r1 - r0) * j as f32 / ACROSS as f32;
            positions.push(on_surface(u, v));
            normals.push(Vec3::new(k * u.cos(), -v, -k * u.sin()).normalize());
            uvs.push(Vec2::new(i as f32 / ALONG as f32, j as f32 / ACROSS as f32));
        }
    }
    let mut indices = Vec::with_capacity(ALONG * ACROSS * 6);
    let row = (ACROSS + 1) as u32;
    for i in 0..ALONG as u32 {
        for j in 0..ACROSS as u32 {
            let a = i * row + j;
            indices.extend([a, a + 1, a + row, a + 1, a + row + 1, a + row]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let turns = Tuning::default().spiral_worlds;
    let layer = RenderLayers::layer(LAYER);
    let dim = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.45, 0.5),
        // Self-lit a little so the unvisited worlds still read against the dark sea.
        emissive: LinearRgba::new(0.18, 0.2, 0.26, 1.0),
        perceptual_roughness: 0.7,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let lit = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.8, 0.5),
        emissive: LinearRgba::new(2.2, 1.3, 0.5, 1.0),
        perceptual_roughness: 0.6,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let shaft = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.21, 0.24),
        metallic: 0.6,
        perceptual_roughness: 0.4,
        ..default()
    });
    let beam_bead = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.7, 0.3),
        emissive: LinearRgba::new(8.0, 4.0, 1.2, 1.0),
        ..default()
    });
    let ship_bead = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 1.0, 0.9),
        emissive: LinearRgba::new(1.5, 6.0, 5.0, 1.0),
        ..default()
    });

    for k in 0..turns {
        commands.spawn((Turn(k), Mesh3d(meshes.add(turn_mesh(k))), MeshMaterial3d(dim.clone()), layer.clone()));
    }
    let height = turns as f32 * PITCH;
    let mid_height = SEAM / TAU * PITCH + height * 0.5;
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(SHAFT, height + 0.7))),
        MeshMaterial3d(shaft),
        Transform::from_xyz(0.0, mid_height, 0.0),
        layer.clone(),
    ));
    let bead = meshes.add(Sphere::new(0.075));
    commands.spawn((BeamBead, Mesh3d(bead.clone()), MeshMaterial3d(beam_bead), Transform::default(), layer.clone()));
    commands.spawn((ShipBead, Mesh3d(bead), MeshMaterial3d(ship_bead), Transform::default(), layer.clone()));

    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(3.0, 6.0, 5.0).looking_at(Vec3::new(0.0, height * 0.5, 0.0), Vec3::Y),
        layer.clone(),
    ));
    commands.spawn((
        WidgetCamera,
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            is_active: false,
            // Alpha-blend onto the main image: the default write mode would replace the whole
            // viewport with this camera's (mostly empty) output as a black box.
            output_mode: bevy::camera::CameraOutputMode::Write {
                blend_state: Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
        Projection::from(PerspectiveProjection { fov: 20f32.to_radians(), ..default() }),
        // Looking down at the stack from a distance so each turn reads as an elliptical step.
        Transform::from_xyz(0.0, mid_height + 5.5, 15.0).looking_at(Vec3::new(0.0, mid_height, 0.0), Vec3::Y),
        Tonemapping::TonyMcMapface,
        // No MSAA: a multisampled second camera resolves over the main image instead of blending.
        Msaa::Off,
        layer,
    ));
    commands.insert_resource(WidgetMaterials { dim, lit });
}

fn update(
    session: Res<Session>,
    mats: Res<WidgetMaterials>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<&mut Camera, With<WidgetCamera>>,
    mut turns: Query<(&Turn, &mut MeshMaterial3d<StandardMaterial>)>,
    mut beam_bead: Query<&mut Transform, (With<BeamBead>, Without<ShipBead>)>,
    mut ship_bead: Query<(&mut Transform, &mut Visibility), (With<ShipBead>, Without<BeamBead>)>,
) {
    let Ok(mut cam) = camera.single_mut() else { return };
    let spiral = session.world().and_then(|w| match &w.rules {
        Rules::SpiralVoyage(sv) => Some((w, sv)),
        _ => None,
    });
    let Some((world, sv)) = spiral else {
        cam.is_active = false;
        return;
    };
    cam.is_active = true;

    // Viewport: lower-left corner, above the hint line.
    if let Ok(window) = windows.single() {
        let s = window.scale_factor();
        let full = window.physical_size();
        let size = (SIZE * s).as_uvec2().min(full);
        let pos = UVec2::new((INSET.x * s) as u32, full.y.saturating_sub(size.y + (INSET.y * s) as u32));
        cam.viewport = Some(Viewport { physical_position: pos, physical_size: size, ..default() });
    }

    let inspected = sv.beam_world(&world.sea);
    for (turn, mut material) in &mut turns {
        let want = if turn.0 == inspected { &mats.lit } else { &mats.dim };
        if material.0 != *want {
            material.0 = want.clone();
        }
    }
    for mut tf in &mut beam_bead {
        tf.translation = on_surface(session.view_beam().map_or(world.sea.beam.winding, |b| b.winding), RADIUS * 0.9);
    }
    let ship = sv.ship.and_then(|id| world.sea.entity(id));
    for (mut tf, mut vis) in &mut ship_bead {
        match ship {
            Some(e) => {
                tf.translation = on_surface(e.winding, RADIUS * 0.55) + Vec3::Y * 0.05;
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

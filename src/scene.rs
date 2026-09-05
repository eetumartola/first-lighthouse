//! Fixed scene: camera, fog, lighthouse, island, harbor, bearing reference, rocks, beam lights.

use crate::app::{to_world, to_world_h, Session, Settings};
use crate::sea::dawn_amount;
use crate::sim::{self, world_weaver, Footprint, Phase, Rules};
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{FogVolume, VolumetricFog, VolumetricLight};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

/// Baseline exposure for the night scene; `Settings::brightness` adds stops.
const BASE_EV100: f32 = 6.6;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
struct SkyLight;

/// Spawned per session and removed when a new session starts.
#[derive(Component)]
pub struct SessionScoped;

#[derive(Component)]
struct TowerBeam;

#[derive(Component)]
struct FootprintLight;

#[derive(Component)]
struct Flame;

#[derive(Component)]
struct FlameLight;

#[derive(Component)]
struct Reflector;

#[derive(Component)]
struct HarborLamp;

#[derive(Resource, Default)]
struct SceneState {
    generation: u32,
}

/// Shared materials for static geometry.
#[derive(Resource)]
pub struct SceneMaterials {
    pub stone: Handle<StandardMaterial>,
    pub dark_stone: Handle<StandardMaterial>,
    pub warm_glow: Handle<StandardMaterial>,
}


pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneState>()
            .insert_resource(ClearColor(Color::srgb(0.004, 0.006, 0.012)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.5, 0.65, 0.9),
                brightness: 4.0,
                ..default()
            })
            .add_systems(Startup, setup_scene)
            .add_systems(
                Update,
                (
                    sync_session_scene,
                    update_beam_lights,
                    update_lighthouse,
                    update_exposure,
                    update_weaver_markers,
                    update_world_rocks,
                ),
            );
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = sim::Tuning::default();

    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.34, 0.31),
        perceptual_roughness: 0.95,
        ..default()
    });
    let dark_stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.2, 0.19),
        perceptual_roughness: 0.98,
        ..default()
    });
    let bronze = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.38, 0.18),
        metallic: 0.9,
        perceptual_roughness: 0.35,
        ..default()
    });
    let warm_glow = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.7, 0.35),
        emissive: LinearRgba::new(6.0, 3.2, 1.0, 1.0),
        ..default()
    });

    // Camera: fixed, elevated, north up. Fog and bloom carry the atmosphere.
    commands.spawn((
        MainCamera,
        bevy::ui::IsDefaultUiCamera,
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: 47f32.to_radians(),
            ..default()
        }),
        // Framed so the full sea disc (radius 100) fits vertically with room for the bottom HUD.
        Transform::from_xyz(0.0, 225.0, 125.0).looking_at(Vec3::new(0.0, 0.0, 14.0), Vec3::Y),
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.22,
            ..Bloom::NATURAL
        },
        Exposure { ev100: BASE_EV100 },
        VolumetricFog {
            ambient_intensity: 0.0,
            step_count: 40,
            ..default()
        },
        Msaa::Sample4,
    ));

    // Fog volume covering the sea. Bevy attenuates in-scattered light over the volume's bounding
    // radius, so a scene this large needs a thin density and a strong artistic light multiplier.
    commands.spawn((
        FogVolume {
            density_factor: 0.008,
            absorption: 0.05,
            scattering: 0.9,
            scattering_asymmetry: 0.3,
            light_intensity: 26.0,
            fog_color: Color::srgb(0.75, 0.85, 1.0),
            ..default()
        },
        Transform::from_xyz(0.0, 12.0, 0.0).with_scale(Vec3::new(240.0, 32.0, 240.0)),
    ));

    // Sky light: moon by night, warming and brightening into the sun at first light.
    commands.spawn((
        SkyLight,
        DirectionalLight {
            illuminance: 1.2,
            color: Color::srgb(0.6, 0.7, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-60.0, 120.0, -40.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Central island and lighthouse: stone platform, brazier, bronze reflector.
    let island_mesh = meshes.add(Cone {
        radius: t.island_radius,
        height: 2.2,
    });
    commands.spawn((
        Mesh3d(island_mesh),
        MeshMaterial3d(dark_stone.clone()),
        Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(1.0, 1.0, 1.0)),
        Name::new("Island"),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(t.island_radius * 0.8, 1.6))),
        MeshMaterial3d(stone.clone()),
        Transform::from_xyz(0.0, 0.8, 0.0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(3.2, 1.2))),
        MeshMaterial3d(stone.clone()),
        Transform::from_xyz(0.0, 2.2, 0.0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(1.7, 10.0))),
        MeshMaterial3d(stone.clone()),
        Transform::from_xyz(0.0, 7.8, 0.0),
        Name::new("Tower"),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(2.4, 0.5))),
        MeshMaterial3d(bronze.clone()),
        Transform::from_xyz(0.0, 13.0, 0.0),
    ));
    // Brazier bowl.
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.5))),
        MeshMaterial3d(bronze.clone()),
        Transform::from_xyz(0.0, 13.6, 0.0).with_scale(Vec3::new(1.0, 0.45, 1.0)),
    ));
    // Flame: emissive sphere that grows during ignition.
    commands.spawn((
        Flame,
        Mesh3d(meshes.add(Sphere::new(0.9))),
        MeshMaterial3d(warm_glow.clone()),
        Transform::from_xyz(0.0, 14.4, 0.0).with_scale(Vec3::splat(0.01)),
    ));
    commands.spawn((
        FlameLight,
        PointLight {
            color: Color::srgb(1.0, 0.72, 0.4),
            intensity: 0.0,
            range: 60.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 14.8, 0.0),
    ));
    // Reflector: rotates with the beam.
    commands.spawn((
        Reflector,
        Mesh3d(meshes.add(Cuboid::new(3.0, 2.6, 0.25))),
        MeshMaterial3d(bronze.clone()),
        Transform::from_xyz(0.0, 14.6, 1.6),
    ));

    // Tower beam: the volumetric shaft from the brazier to the footprint.
    commands.spawn((
        TowerBeam,
        SpotLight {
            color: Color::srgb(1.0, 0.85, 0.6),
            intensity: 0.0,
            range: 140.0,
            radius: 0.5,
            shadow_maps_enabled: true,
            inner_angle: 0.02,
            outer_angle: 0.13,
            ..default()
        },
        VolumetricLight,
        Transform::from_xyz(0.0, 14.6, 0.0).looking_at(Vec3::new(0.0, 0.0, -50.0), Vec3::Y),
    ));
    // Footprint light: lights the water and whatever floats inside the patch.
    commands.spawn((
        FootprintLight,
        SpotLight {
            color: Color::srgb(1.0, 0.9, 0.7),
            intensity: 0.0,
            range: 120.0,
            shadow_maps_enabled: false,
            inner_angle: 0.1,
            outer_angle: 0.3,
            ..default()
        },
        Transform::from_xyz(0.0, 45.0, -50.0).looking_at(Vec3::new(0.0, 0.0, -50.0), Vec3::Z),
    ));

    // Harbor: ring of posts, entrance lamps, a faint mooring ring.
    let post = meshes.add(Cylinder::new(0.28, 1.8));
    let lamp = meshes.add(Sphere::new(0.45));
    let hc = t.harbor_center;
    for i in 0..9 {
        // Posts along the harbor circle, leaving the seaward (north) side open as the entrance.
        let a = 60f32.to_radians() + i as f32 * (240f32.to_radians() / 8.0);
        let p = hc + glam::Vec2::new(a.sin(), a.cos()) * t.harbor_radius;
        commands.spawn((
            Mesh3d(post.clone()),
            MeshMaterial3d(dark_stone.clone()),
            Transform::from_translation(to_world_h(p, 0.9)),
        ));
    }
    for side in [-1.0f32, 1.0] {
        let p = hc + glam::Vec2::new(side * t.harbor_radius * 0.75, t.harbor_radius * 0.75);
        commands.spawn((
            Mesh3d(post.clone()),
            MeshMaterial3d(dark_stone.clone()),
            Transform::from_translation(to_world_h(p, 1.2)).with_scale(Vec3::new(1.0, 1.5, 1.0)),
        ));
        commands.spawn((
            HarborLamp,
            Mesh3d(lamp.clone()),
            MeshMaterial3d(warm_glow.clone()),
            Transform::from_translation(to_world_h(p, 2.8)),
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(Torus {
            minor_radius: 0.12,
            major_radius: t.harbor_radius,
        }.mesh().major_resolution(64).minor_resolution(8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.75, 0.4),
            emissive: LinearRgba::new(0.8, 0.45, 0.15, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::from_translation(to_world_h(hc, 0.05)),
    ));

    // Playable boundary and bearing reference: a faint ring with four compass buoys, north brightest.
    commands.spawn((
        Mesh3d(meshes.add(Torus {
            minor_radius: 0.18,
            major_radius: t.sea_radius,
        }.mesh().major_resolution(192).minor_resolution(8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.45, 0.6),
            emissive: LinearRgba::new(0.12, 0.2, 0.32, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.05, 0.0),
    ));
    let buoy_body = meshes.add(Cylinder::new(0.6, 2.2));
    for (i, bearing) in [0.0f32, 90.0, 180.0, 270.0].into_iter().enumerate() {
        let p = sim::geom::dir(bearing.to_radians()) * (t.sea_radius + 3.0);
        let strength = if i == 0 { 5.0 } else { 1.2 };
        let color = if i == 0 {
            LinearRgba::new(0.9, 0.95, 1.0, 1.0)
        } else {
            LinearRgba::new(0.5, 0.6, 0.8, 1.0)
        };
        commands.spawn((
            Mesh3d(buoy_body.clone()),
            MeshMaterial3d(dark_stone.clone()),
            Transform::from_translation(to_world_h(p, 1.1)),
        ));
        commands.spawn((
            Mesh3d(lamp.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                emissive: color * strength,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(to_world_h(p, 2.7)).with_scale(Vec3::splat(if i == 0 { 1.4 } else { 0.9 })),
        ));
    }

    commands.insert_resource(SceneMaterials {
        stone,
        dark_stone,
        warm_glow,
    });
}

/// Rebuild the per-session geometry (rocks, World Weaver ring and buoys) when a session starts.
fn sync_session_scene(
    mut commands: Commands,
    session: Res<Session>,
    mut state: ResMut<SceneState>,
    mats: Res<SceneMaterials>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    scoped: Query<Entity, With<SessionScoped>>,
) {
    if session.generation == state.generation {
        return;
    }
    state.generation = session.generation;
    for e in &scoped {
        commands.entity(e).despawn();
    }
    let Some(world) = session.world() else { return };
    let t = world.tuning();

    // Fixed rocks (skip index 0: the central island is drawn by the fixed scene).
    for rock in world.sea.rocks.iter().skip(1) {
        let parent = spawn_rock(&mut commands, &mut meshes, &mats, *rock);
        commands.entity(parent).insert(SessionScoped);
    }

    if let Rules::WorldWeaver(ww) = &world.rules {
        // Every distinct piece a sector can hold exists as a hidden visual; the preview (or the
        // frozen composition after dawn) decides which one shows.
        for s in 0..t.weaver_sectors {
            let mut pieces: Vec<world_weaver::Piece> = ww.worlds.iter().map(|w| w[s]).collect();
            pieces.dedup();
            let mut unique = Vec::new();
            for p in pieces {
                if !unique.contains(&p) {
                    unique.push(p);
                }
            }
            for piece in unique {
                for rock in piece.geometry(s, t) {
                    let parent = spawn_rock(&mut commands, &mut meshes, &mats, rock);
                    commands.entity(parent).insert((SessionScoped, SlicePreview { sector: s, piece }, Visibility::Hidden));
                }
            }
        }
        // Outer-edge markers: one dot per sector, lit once that sector has been edited.
        let dot = meshes.add(Sphere::new(0.9));
        let a = t.sector_angle();
        for s in 0..t.weaver_sectors {
            let p = sim::geom::dir(s as f32 * a + a * 0.5) * (t.sea_radius + 3.5);
            commands.spawn((
                SessionScoped,
                EditMarker(s),
                Mesh3d(dot.clone()),
                MeshMaterial3d(mats.warm_glow.clone()),
                Transform::from_translation(to_world_h(p, 1.0)),
                Visibility::Hidden,
            ));
        }
        // The shipping-lane entrance is a fixed, marked scenario element.
        let lamp = meshes.add(Sphere::new(0.55));
        for side in [-1.0f32, 1.0] {
            let across = sim::geom::dir(sim::geom::bearing_of(ww.lane_start) + std::f32::consts::FRAC_PI_2);
            let p = ww.lane_start + across * side * 4.0;
            commands.spawn((
                SessionScoped,
                Mesh3d(lamp.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 0.8, 0.5),
                    emissive: LinearRgba::new(3.0, 2.0, 0.7, 1.0),
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(to_world_h(p, 1.4)),
            ));
        }
    }

    if let Rules::SpiralVoyage(sv) = &world.rules {
        // Each world's land exists as a hidden group; only the inspected world shows.
        for (w, sw) in sv.worlds.iter().enumerate() {
            for rock in sw.rocks.iter().skip(1) {
                let parent = spawn_rock(&mut commands, &mut meshes, &mats, *rock);
                commands.entity(parent).insert((SessionScoped, WorldRocks(w), Visibility::Hidden));
            }
        }
        // The north seam, where a circuit passes into the next world.
        commands.spawn((
            SessionScoped,
            Mesh3d(meshes.add(Cuboid::new(0.3, 0.1, t.sea_radius - t.island_radius - 4.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.9, 1.0),
                emissive: LinearRgba::new(0.4, 0.55, 0.9, 1.0),
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.08, -(t.island_radius + 2.0 + (t.sea_radius - t.island_radius - 4.0) * 0.5)),
        ));
    }
}

/// World Weaver: one sector's candidate land, shown when previewed or built.
#[derive(Component)]
struct SlicePreview {
    sector: usize,
    piece: world_weaver::Piece,
}

/// World Weaver: outer-edge dot marking an edited sector.
#[derive(Component)]
struct EditMarker(usize);

/// Spiral Voyage: land belonging to one world.
#[derive(Component)]
struct WorldRocks(usize);

/// A rock as a small connected cluster of boulders filling its collision circle. Deterministic
/// from the position so retries look identical.
fn spawn_rock(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &SceneMaterials,
    rock: sim::Circle,
) -> Entity {
    let r = rock.radius;
    let seed = ((rock.center.x * 12.9898 + rock.center.y * 78.233).sin() * 43758.5453).fract().abs();
    let yaw = seed * std::f32::consts::TAU;
    let parent = commands
        .spawn((
            Transform::from_translation(to_world_h(rock.center, 0.0)).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::Visible,
        ))
        .id();
    let boulders: [(f32, Vec3, Vec3, bool); 3] = [
        (0.72, Vec3::new(-0.22, -0.32, 0.10), Vec3::new(1.0, 0.72, 1.0), true),
        (0.55, Vec3::new(0.32, -0.28, -0.22), Vec3::new(1.05, 0.8, 0.95), true),
        (0.48, Vec3::new(0.05, 0.05, 0.26), Vec3::new(1.0, 1.0, 1.0), false),
    ];
    for (i, (size, offset, scale, round)) in boulders.into_iter().enumerate() {
        let mesh = if round {
            meshes.add(Sphere::new(r * size))
        } else {
            meshes.add(Cone {
                radius: r * size,
                height: r * 1.1,
            })
        };
        let material = if i == 1 { mats.stone.clone() } else { mats.dark_stone.clone() };
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(offset * r)
                .with_scale(scale)
                .with_rotation(Quat::from_rotation_z(0.18 * (seed - 0.5))),
            ChildOf(parent),
        ));
    }
    parent
}

/// How lit the beacon is: ramps during ignition, full at night, fades at dawn.
fn beacon_level(world: &sim::World) -> f32 {
    match world.phase {
        Phase::Intro { elapsed } => (elapsed / world.tuning().intro_seconds).clamp(0.0, 1.0).powi(2),
        Phase::Night => 1.0,
        Phase::Dawn { .. } | Phase::Playback | Phase::Finished => 1.0 - dawn_amount(world) * 0.85,
    }
}

fn update_beam_lights(
    session: Res<Session>,
    mut tower: Query<(&mut Transform, &mut SpotLight), (With<TowerBeam>, Without<FootprintLight>)>,
    mut patch: Query<(&mut Transform, &mut SpotLight), (With<FootprintLight>, Without<TowerBeam>)>,
) {
    let Some(world) = session.world() else {
        for (_, mut l) in &mut tower {
            l.intensity = 0.0;
        }
        for (_, mut l) in &mut patch {
            l.intensity = 0.0;
        }
        return;
    };
    let t = world.tuning();
    let fp = world.footprint();
    let center = fp.center();
    let level = beacon_level(world);
    let shaft_on = matches!(world.phase, Phase::Night | Phase::Intro { .. });

    for (mut tf, mut light) in &mut tower {
        let target = to_world_h(center, 0.0);
        *tf = Transform::from_xyz(0.0, 14.6, 0.0).looking_at(target, Vec3::Y);
        match fp {
            Footprint::Spot { half_angle, r_max, .. } => {
                light.outer_angle = half_angle;
                light.inner_angle = half_angle * 0.3;
                light.range = r_max + 6.0;
                light.intensity = if shaft_on { 90_000.0 * level } else { 0.0 };
            }
            Footprint::Sector { angle_start, angle_end, r_max, .. } => {
                let half = (angle_end - angle_start) * 0.5;
                light.outer_angle = half.min(1.2);
                light.inner_angle = half * 0.4;
                light.range = r_max + 6.0;
                light.intensity = if shaft_on { 90_000.0 * level } else { 0.0 };
            }
        }
    }

    for (mut tf, mut light) in &mut patch {
        match fp {
            Footprint::Spot { half_angle, r_min, r_max, .. } => {
                let height = 45.0;
                let half_len = (r_max - r_min) * 0.5;
                let half_w = r_max * half_angle.sin();
                let radius = half_len.max(half_w) + 1.5;
                *tf = Transform::from_translation(to_world_h(center, height)).looking_at(to_world(center), Vec3::Z);
                light.outer_angle = (radius / height).atan();
                light.inner_angle = light.outer_angle * 0.6;
                light.range = height + 10.0;
                light.intensity = if shaft_on { 260_000.0 * level } else { 0.0 };
            }
            Footprint::Sector { .. } => {
                let height = 80.0;
                let c = sim::geom::dir(fp.bearing()) * (t.sea_radius * 0.55);
                *tf = Transform::from_translation(to_world_h(c, height)).looking_at(to_world(c), Vec3::Z);
                light.outer_angle = (52.0f32 / height).atan();
                light.inner_angle = light.outer_angle * 0.7;
                light.range = height + 40.0;
                light.intensity = if shaft_on { 1_200_000.0 * level } else { 0.0 };
            }
        }
    }
}

fn update_lighthouse(
    session: Res<Session>,
    time: Res<Time>,
    mut flame: Query<&mut Transform, (With<Flame>, Without<Reflector>)>,
    mut flame_light: Query<&mut PointLight, With<FlameLight>>,
    mut reflector: Query<&mut Transform, (With<Reflector>, Without<Flame>)>,
) {
    let (level, bearing) = match session.world() {
        Some(w) => (beacon_level(w), w.sea.beam.bearing()),
        None => (0.0, time.elapsed_secs() * 0.15),
    };
    let flicker = 1.0 + 0.08 * (time.elapsed_secs() * 17.0).sin() + 0.05 * (time.elapsed_secs() * 29.0).sin();
    for mut tf in &mut flame {
        tf.scale = Vec3::splat((0.01 + level * 1.0) * flicker);
    }
    for mut l in &mut flame_light {
        l.intensity = 900_000.0 * level * flicker;
    }
    // Reflector sits behind the flame, opposite the beam direction; compass bearing → yaw.
    for mut tf in &mut reflector {
        let d = to_world(sim::geom::dir(bearing));
        tf.translation = Vec3::new(0.0, 14.6, 0.0) - d * 1.6;
        tf.look_to(d, Vec3::Y);
    }
}

/// Exposure follows the brightness setting, and daylight is pulled down so the sunrise reads as
/// a reveal rather than a white-out.
fn update_exposure(
    settings: Res<Settings>,
    session: Res<Session>,
    mut cams: Query<&mut Exposure, With<MainCamera>>,
    mut sky: Query<(&mut DirectionalLight, &mut Transform), With<SkyLight>>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let dawn = session.world().map(dawn_amount).unwrap_or(0.0);
    // Dusk: the sea starts visible in warm evening light and fades to darkness as the night begins.
    let flare = session.world().map(|w| w.dusk()).unwrap_or(0.0);
    for mut e in &mut cams {
        e.ev100 = BASE_EV100 - settings.brightness + dawn * 3.6;
    }
    let night = LinearRgba::new(0.6, 0.7, 1.0, 1.0);
    let day = LinearRgba::new(1.0, 0.82, 0.6, 1.0);
    for (mut light, mut tf) in &mut sky {
        light.illuminance = 1.2 + dawn * dawn * 4_000.0 + flare * 150.0;
        let flare_color = LinearRgba::new(1.0, 0.75, 0.45, 1.0);
        light.color = Color::LinearRgba(night.mix(&day, dawn).mix(&flare_color, flare));
        // The sun rises in the east, low and warm.
        let pos = Vec3::new(-60.0, 120.0, -40.0).lerp(Vec3::new(160.0, 45.0, -20.0), dawn);
        *tf = Transform::from_translation(pos).looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.brightness = 4.0 + dawn * 300.0 + flare * 60.0;
    ambient.color = Color::LinearRgba(LinearRgba::new(0.5, 0.65, 0.9, 1.0).mix(&LinearRgba::new(0.9, 0.85, 0.8, 1.0), dawn));
}

/// World Weaver: the lit sector previews its current piece (World 1 shows the assembled result);
/// after dawn the frozen composition shows everywhere. Edited sectors get an outer-edge dot.
fn update_weaver_markers(
    session: Res<Session>,
    mut slices: Query<(&SlicePreview, &mut Visibility), Without<EditMarker>>,
    mut markers: Query<(&EditMarker, &mut Visibility), Without<SlicePreview>>,
) {
    let Some(world) = session.world() else { return };
    let Rules::WorldWeaver(ww) = &world.rules else { return };
    let active = world.sea.beam.sector_index(world.tuning());
    let layer = ww.layer_for(&world.sea);
    for (slice, mut vis) in &mut slices {
        let show = match &ww.built {
            Some(built) => built[slice.sector] == slice.piece,
            // Dusk shows World 1 whole; the night shows only the lit sector's candidate.
            None => match world.phase {
                Phase::Intro { .. } => ww.assembled[slice.sector] == slice.piece,
                Phase::Night => slice.sector == active && ww.piece(layer, slice.sector) == slice.piece,
                _ => false,
            },
        };
        *vis = if show { Visibility::Visible } else { Visibility::Hidden };
    }
    for (marker, mut vis) in &mut markers {
        *vis = if ww.edited[marker.0] { Visibility::Visible } else { Visibility::Hidden };
    }
}

/// Spiral Voyage: only the inspected world's land is shown.
fn update_world_rocks(session: Res<Session>, mut rocks: Query<(&WorldRocks, &mut Visibility)>) {
    let Some(world) = session.world() else { return };
    let Rules::SpiralVoyage(_) = &world.rules else { return };
    let inspected = world.inspected_world();
    for (group, mut vis) in &mut rocks {
        *vis = if group.0 == inspected { Visibility::Visible } else { Visibility::Hidden };
    }
}

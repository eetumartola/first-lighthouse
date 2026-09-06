//! Fixed scene: camera, fog, lighthouse, island, harbor, bearing reference, rocks, beam lights.

use crate::app::{to_world, to_world_h, Session, Settings};
use crate::models;
use crate::sea::dawn_amount;
use crate::sim::{self, world_weaver, Footprint, Phase, Rules};
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{VolumetricFog, VolumetricLight};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use glam::Vec2;

/// Baseline exposure for the night scene; `Settings::brightness` adds stops.
const BASE_EV100: f32 = 6.6;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
struct SkyLight;
#[derive(Component)]
struct SkyDome;

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
struct FogBeam;
#[derive(Component)]
struct FlameLight;

#[derive(Component)]
struct Reflector;

#[derive(Component)]
struct HarborLamp;

/// Every piece of the harbor; hidden in Spiral Voyage while the ship's view of the harbor's
/// position is not World 4.
#[derive(Component)]
struct HarborPart;

#[derive(Resource, Default)]
struct SceneState {
    generation: u32,
}

/// Shared materials for static geometry.
#[derive(Resource)]
pub struct SceneMaterials {
    pub dark_stone: Handle<StandardMaterial>,
    /// Remembered land: a faint translucent outline of what the player already placed.
    pub ghost_stone: Handle<StandardMaterial>,
    pub warm_glow: Handle<StandardMaterial>,
}

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneState>()
            .insert_resource(GlobalAmbientLight { color: Color::srgb(0.5, 0.65, 0.9), brightness: 0.5, ..default() })
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
                    update_harbor,
                ),
            );
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let t = sim::Tuning::default();

    // Night sky: a dome of scattered stars over a faint horizon glow, unlit so exposure never
    // swallows it. Dawn brightens the world, not the sky texture.
    let sky_material = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(sky_image())),
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(Sphere::new(1400.0).mesh().uv(48, 24))),
        MeshMaterial3d(sky_material),
        Transform::from_xyz(0.0, -60.0, 0.0),
        Name::new("Sky"),
    ));

    // Rock takes its tone from vertex colours (dark wet base, pale dry tops); the tower is
    // plain slate.
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.31, 0.32),
        perceptual_roughness: 0.92,
        ..default()
    });
    let dark_stone =
        materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() });
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
        Projection::from(PerspectiveProjection { fov: 47f32.to_radians(), ..default() }),
        // Framed so the full sea disc (radius 100) fits vertically with room for the bottom HUD.
        // `FIRST_LIGHT_CAMERA=x,y,z,tx,ty,tz` (render coordinates) overrides it for model review.
        review_camera()
            .unwrap_or_else(|| Transform::from_xyz(0.0, 225.0, 125.0).looking_at(Vec3::new(0.0, 0.0, 14.0), Vec3::Y)),
        Tonemapping::TonyMcMapface,
        Bloom { intensity: 0.22, ..Bloom::NATURAL },
        Exposure { ev100: BASE_EV100 },
        VolumetricFog {
            // The fog carries its own faint moon-grey so it reads as mist even where no light
            // shaft crosses it; the beam then parts and lights it.
            ambient_color: Color::srgb(0.6, 0.7, 0.9),
            ambient_intensity: 0.0,
            ..default()
        },
        Msaa::Sample4,
    ));

    // Sky light: moon by night, warming and brightening into the sun at first light. It lights
    // the fog too, so the sea is a pale grey blanket rather than black.
    commands.spawn((
        SkyLight,
        DirectionalLight {
            illuminance: 1.2,
            color: Color::srgb(0.6, 0.7, 1.0),
            shadow_maps_enabled: true,
            ..default()
        },
        VolumetricLight,
        // The fog is only lit inside the shadow cascade, so it must cover the whole sea.
        bevy::light::CascadeShadowConfigBuilder {
            num_cascades: 2,
            maximum_distance: 900.0,
            first_cascade_far_bound: 300.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(-60.0, 120.0, -40.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Central island and lighthouse: a flat-topped crag, an octagonal stone tower, brazier and
    // bronze reflector.
    let island_mesh =
        meshes.add(models::island(&[sim::Circle::new(Vec2::ZERO, t.island_radius)], Vec2::ZERO, Some(2.0)));
    commands.spawn((Mesh3d(island_mesh), MeshMaterial3d(dark_stone.clone()), Transform::IDENTITY, Name::new("Island")));
    commands.spawn((
        Mesh3d(meshes.add(models::tower(&[(3.6, 1.4), (2.6, 0.8), (2.1, 4.2), (1.8, 5.0), (2.5, 0.5)], 8))),
        MeshMaterial3d(stone.clone()),
        Transform::from_xyz(0.0, 1.4, 0.0),
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
            // Short reach: the brazier must not reveal the reefs, only the island's own rocks.
            range: 22.0,
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
    // Fog beam: a wide, very strong spot that exists only to blaze through the mist along the
    // beam. It carries no shadows and is aimed at the footprint like the shaft.
    commands.spawn((
        FogBeam,
        SpotLight {
            color: Color::srgb(1.0, 0.9, 0.75),
            intensity: 0.0,
            range: 200.0,
            shadow_maps_enabled: true,
            inner_angle: 0.05,
            outer_angle: 0.35,
            ..default()
        },
        VolumetricLight,
        Transform::from_xyz(0.0, 14.6, 0.0).looking_at(Vec3::new(0.0, 0.0, -50.0), Vec3::Y),
    ));
    // Faint overhead fill keeps the fog readable without competing with the lighthouse beam.
    commands.spawn((
        SpotLight {
            color: Color::srgb(0.52, 0.62, 0.82),
            intensity: 100_000.0,
            range: 500.0,
            shadow_maps_enabled: true,
            inner_angle: 0.1,
            outer_angle: 0.42,
            ..default()
        },
        VolumetricLight,
        Transform::from_xyz(0.0, 380.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
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
            HarborPart,
            Mesh3d(post.clone()),
            MeshMaterial3d(dark_stone.clone()),
            Transform::from_translation(to_world_h(p, 0.9)),
        ));
    }
    for side in [-1.0f32, 1.0] {
        let p = hc + glam::Vec2::new(side * t.harbor_radius * 0.75, t.harbor_radius * 0.75);
        commands.spawn((
            HarborPart,
            Mesh3d(post.clone()),
            MeshMaterial3d(dark_stone.clone()),
            Transform::from_translation(to_world_h(p, 1.2)).with_scale(Vec3::new(1.0, 1.5, 1.0)),
        ));
        commands.spawn((
            HarborPart,
            HarborLamp,
            Mesh3d(lamp.clone()),
            MeshMaterial3d(warm_glow.clone()),
            Transform::from_translation(to_world_h(p, 2.8)),
        ));
    }
    commands.spawn((
        HarborPart,
        Mesh3d(meshes.add(
            Torus { minor_radius: 0.12, major_radius: t.harbor_radius }.mesh().major_resolution(64).minor_resolution(8),
        )),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.75, 0.4),
            emissive: LinearRgba::new(0.8, 0.45, 0.15, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::from_translation(to_world_h(hc, 0.05)),
    ));

    // Playable boundary and bearing reference: a faint ring, a dim compass rose on the water just
    // outside it, and four compass buoys, north brightest.
    commands.spawn((
        Mesh3d(meshes.add(
            Torus { minor_radius: 0.18, major_radius: t.sea_radius }.mesh().major_resolution(192).minor_resolution(8),
        )),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.45, 0.6),
            emissive: LinearRgba::new(0.12, 0.2, 0.32, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.05, 0.0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(models::compass_rose(t.sea_radius + 6.0, t.sea_radius + 16.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            // Unlit vertex colour, scaled down: it must never compete with the beam.
            base_color: Color::linear_rgb(0.2, 0.2, 0.2),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Name::new("Compass rose"),
    ));
    let buoy_body = meshes.add(Cylinder::new(0.6, 2.2));
    for (i, bearing) in [0.0f32, 90.0, 180.0, 270.0].into_iter().enumerate() {
        let p = sim::geom::dir(bearing.to_radians()) * (t.sea_radius + 3.0);
        let strength = if i == 0 { 5.0 } else { 1.2 };
        let color = if i == 0 { LinearRgba::new(0.9, 0.95, 1.0, 1.0) } else { LinearRgba::new(0.5, 0.6, 0.8, 1.0) };
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

    let ghost_stone = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.65, 0.8, 0.16),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.insert_resource(SceneMaterials { dark_stone, ghost_stone, warm_glow });
}

/// Equirectangular star field: dense faint stars, a few bright ones, and a cold glow band just
/// above the horizon.
fn sky_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let (w, h) = (2048usize, 1024usize);
    let mut data = vec![0u8; w * h * 4];
    let hash = |x: u32, y: u32| -> f32 {
        let mut n = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)) ^ 0x9E37_79B9;
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        ((n ^ (n >> 16)) & 0xFFFF) as f32 / 65535.0
    };
    for y in 0..h {
        // v: 0 at the top of the dome, 0.5 at the horizon.
        let v = y as f32 / h as f32;
        let above = (0.5 - v).max(0.0) * 2.0;
        let glow = (1.0 - above * 3.0).clamp(0.0, 1.0).powi(2);
        for x in 0..w {
            let mut c = [0.010 + 0.035 * glow, 0.014 + 0.045 * glow, 0.028 + 0.075 * glow];
            if above > 0.02 {
                let r = hash(x as u32, y as u32);
                if r > 0.9975 {
                    let b = 0.25 + 0.6 * hash(y as u32, x as u32);
                    c = [b, b * 0.98, b * 0.9];
                }
            }
            let i = (y * w + x) * 4;
            for k in 0..3 {
                data[i + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
            }
            data[i + 3] = 255;
        }
    }
    let mut image = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn review_camera() -> Option<Transform> {
    let spec = std::env::var("FIRST_LIGHT_CAMERA").ok()?;
    let v: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let [x, y, z, tx, ty, tz] = v[..] else { return None };
    Some(Transform::from_xyz(x, y, z).looking_at(Vec3::new(tx, ty, tz), Vec3::Y))
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

    // Fixed land (skip index 0: the central island is drawn by the fixed scene).
    for (island, _) in spawn_land(&mut commands, &mut meshes, &mats, &world.sea.rocks[1..]) {
        commands.entity(island).insert(SessionScoped);
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
                for (island, _) in spawn_land(&mut commands, &mut meshes, &mats, &piece.geometry(s, t)) {
                    commands.entity(island).insert((
                        SessionScoped,
                        SlicePreview { sector: s, piece },
                        Visibility::Hidden,
                    ));
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
        // Every world's land exists as a hidden group; each island shows when the ship's view of
        // the spiral resolves its position into its world.
        for (w, sw) in sv.worlds.iter().enumerate() {
            for (island, center) in spawn_land(&mut commands, &mut meshes, &mats, &sw.rocks[1..]) {
                commands.entity(island).insert((SessionScoped, WorldRocks { world: w, center }, Visibility::Hidden));
            }
        }
        // The south seam, where a clockwise circuit passes into the next world.
        let seam_length = t.sea_radius - t.island_radius - 4.0;
        let seam_center = sim::geom::dir(sim::level::SEAM) * (t.island_radius + 2.0 + seam_length * 0.5);
        commands.spawn((
            SessionScoped,
            Mesh3d(meshes.add(Cuboid::new(0.3, 0.1, seam_length))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.9, 1.0),
                emissive: LinearRgba::new(0.4, 0.55, 0.9, 1.0),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(to_world_h(seam_center, 0.08)),
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
struct WorldRocks {
    world: usize,
    center: Vec2,
}

/// Land as merged islands: overlapping collision circles become one faceted mesh. Returns each
/// island's entity with the cluster's centre (sim coordinates).
fn spawn_land(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &SceneMaterials,
    rocks: &[sim::Circle],
) -> Vec<(Entity, Vec2)> {
    models::clusters(rocks)
        .into_iter()
        .map(|cluster| {
            let center = models::cluster_center(&cluster);
            // Drawn a touch thinner than the hull-blocking circles: the water reads as passable
            // right up to the visible rock.
            let drawn: Vec<sim::Circle> = cluster.iter().map(|c| sim::Circle::new(c.center, c.radius * 0.9)).collect();
            let entity = commands
                .spawn((
                    Mesh3d(meshes.add(models::island(&drawn, center, None))),
                    MeshMaterial3d(mats.dark_stone.clone()),
                    Transform::from_translation(to_world_h(center, 0.0)),
                ))
                .id();
            (entity, center)
        })
        .collect()
}

/// How lit the beacon is: ramps during ignition, full at night, fades at dawn.
fn beacon_level(world: &sim::World) -> f32 {
    match world.phase {
        Phase::Intro { elapsed } => (elapsed / world.tuning().intro_seconds).clamp(0.0, 1.0).powi(2),
        Phase::Night => 1.0,
        Phase::Dawn { .. } | Phase::Playback | Phase::Finished => 1.0 - dawn_amount(world) * 0.85,
    }
}

/// Bearing and level of the idle beacon on the menu: a slow sweep so the title screen is alive.
fn idle_beacon(time: &Time) -> (f32, f32) {
    (time.elapsed_secs() * 0.15, 0.6)
}

fn update_beam_lights(
    session: Res<Session>,
    time: Res<Time>,
    mut tower: Query<(&mut Transform, &mut SpotLight), (With<TowerBeam>, Without<FootprintLight>, Without<FogBeam>)>,
    mut patch: Query<(&mut Transform, &mut SpotLight), (With<FootprintLight>, Without<TowerBeam>, Without<FogBeam>)>,
    mut fog_beam: Query<(&mut Transform, &mut SpotLight), (With<FogBeam>, Without<TowerBeam>, Without<FootprintLight>)>,
) {
    let Some(world) = session.world() else {
        let (bearing, level) = idle_beacon(&time);
        let target = to_world_h(sim::geom::dir(bearing) * 70.0, 0.0);
        for (mut tf, mut l) in &mut tower {
            *tf = Transform::from_xyz(0.0, 14.6, 0.0).looking_at(target, Vec3::Y);
            l.outer_angle = 0.13;
            l.inner_angle = 0.04;
            l.range = 120.0;
            l.intensity = 140_000.0 * level;
        }
        for (_, mut l) in &mut patch {
            l.intensity = 0.0;
        }
        for (_, mut l) in &mut fog_beam {
            l.intensity = 0.0;
        }
        return;
    };
    let t = world.tuning();
    let fp = session.view_footprint().unwrap_or_else(|| world.footprint());
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

    for (mut tf, mut light) in &mut fog_beam {
        *tf = Transform::from_xyz(0.0, 14.6, 0.0).looking_at(to_world_h(center, 0.0), Vec3::Y);
        let half = match fp {
            Footprint::Spot { half_angle, .. } => half_angle * 3.0,
            Footprint::Sector { angle_start, angle_end, .. } => (angle_end - angle_start) * 0.5,
        };
        light.outer_angle = half.min(1.2);
        light.inner_angle = light.outer_angle * 0.3;
        light.intensity = if shaft_on { 600_000.0 * level } else { 0.0 };
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
        Some(w) => (beacon_level(w), session.view_beam().map_or(w.sea.beam.bearing(), |b| b.bearing())),
        None => {
            let (bearing, level) = idle_beacon(&time);
            (level, bearing)
        }
    };
    let flicker = 1.0 + 0.08 * (time.elapsed_secs() * 17.0).sin() + 0.05 * (time.elapsed_secs() * 29.0).sin();
    for mut tf in &mut flame {
        tf.scale = Vec3::splat((0.01 + level * 1.0) * flicker);
    }
    for mut l in &mut flame_light {
        l.intensity = 250_000.0 * level * flicker;
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
    dome: Query<&MeshMaterial3d<StandardMaterial>, With<SkyDome>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let dawn = session.world().map(dawn_amount).unwrap_or(0.0);
    // Dusk: the sea starts in evening light and fades to darkness as the night begins.
    let flare = session.world().map(|w| w.dusk()).unwrap_or(0.0);
    let warmth = dawn.max(flare);
    for mut e in &mut cams {
        e.ev100 = BASE_EV100 - settings.brightness + warmth * 3.6;
    }
    // Both transitions pass through the same sunset: amber low on the horizon, the sky violet.
    // `warmth` is how far into daylight either transition is (dusk runs it backwards).
    let sun_color = |k: f32| {
        // Deep blood-red at the very edge of night, warming to amber, then pale gold high up.
        let blood = LinearRgba::new(1.0, 0.10, 0.03, 1.0);
        let amber = LinearRgba::new(1.0, 0.45, 0.16, 1.0);
        let gold = LinearRgba::new(1.0, 0.86, 0.66, 1.0);
        let night = LinearRgba::new(0.6, 0.7, 1.0, 1.0);
        night.mix(&blood, (k * 8.0).min(1.0)).mix(&amber, (k * 2.2).min(1.0)).mix(&gold, k * k)
    };
    for (mut light, mut tf) in &mut sky {
        light.illuminance = 1.2 + dawn * dawn * 5_000.0 + flare * flare * 3_500.0;
        light.color = Color::LinearRgba(sun_color(warmth));
        // The sun rises in the east, low and warm; at dusk it sets in the west.
        let (from, to) = if flare > 0.0 {
            (Vec3::new(-60.0, 120.0, -40.0), Vec3::new(-160.0, 30.0, 20.0))
        } else {
            (Vec3::new(-60.0, 120.0, -40.0), Vec3::new(160.0, 32.0, -20.0))
        };
        let pos = from.lerp(to, warmth);
        *tf = Transform::from_translation(pos).looking_at(Vec3::ZERO, Vec3::Y);
    }
    // Ambient: violet at the sunset edge, cool blue by night, so the warm sun reads as direction.
    let violet = LinearRgba::new(0.75, 0.45, 0.85, 1.0);
    let cool = LinearRgba::new(0.5, 0.65, 0.9, 1.0);
    ambient.brightness = 2.6 + warmth * 200.0;
    ambient.color = Color::LinearRgba(cool.mix(&violet, (warmth * 2.5).min(1.0) * (1.0 - warmth * 0.5)));
    // The star dome flushes with the sunset, then settles to a deep blue-grey in daylight so the
    // sea's rim still reads.
    for handle in &dome {
        if let Some(mut m) = materials.get_mut(&handle.0) {
            let k = 1.0 + warmth * 3.5;
            let blush = (warmth * 4.0).min(1.0) * (1.0 - warmth).powf(0.6);
            m.base_color = Color::linear_rgb(
                k * (1.0 + 0.3 * warmth + 3.5 * blush),
                k * (1.0 + 0.35 * warmth + 0.5 * blush),
                k * (1.0 + 0.6 * warmth + 0.4 * blush),
            );
        }
    }
}

/// World Weaver: the lit sector previews its current piece (World 1 shows the assembled result);
/// after dawn the frozen composition shows everywhere. Edited sectors get an outer-edge dot.
fn update_weaver_markers(
    session: Res<Session>,
    mats: Res<SceneMaterials>,
    mut slices: Query<(&SlicePreview, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), Without<EditMarker>>,
    mut markers: Query<(&EditMarker, &mut Visibility), Without<SlicePreview>>,
) {
    let Some(world) = session.world() else { return };
    let Rules::WorldWeaver(ww) = &world.rules else { return };
    let active = world.sea.beam.sector_index(world.tuning());
    let layer = ww.layer_for(&world.sea);
    for (slice, mut vis, mut material) in &mut slices {
        // Solid: the lit sector's candidate (or the whole built sea after dawn). Ghost: what the
        // player has already copied into World 1, so the composition stays in mind in the dark.
        let (show, ghost) = match &ww.built {
            Some(built) => (built[slice.sector] == slice.piece, false),
            None => match world.phase {
                Phase::Intro { .. } => (ww.assembled[slice.sector] == slice.piece, false),
                Phase::Night => {
                    if slice.sector == active {
                        (ww.piece(layer, slice.sector) == slice.piece, false)
                    } else {
                        (ww.edited[slice.sector] && ww.assembled[slice.sector] == slice.piece, true)
                    }
                }
                _ => (false, false),
            },
        };
        *vis = if show { Visibility::Visible } else { Visibility::Hidden };
        let want = if ghost { &mats.ghost_stone } else { &mats.dark_stone };
        if material.0 != *want {
            material.0 = want.clone();
        }
    }
    for (marker, mut vis) in &mut markers {
        *vis = if ww.edited[marker.0] { Visibility::Visible } else { Visibility::Hidden };
    }
}

/// Spiral Voyage: an island shows when, seen from the ship, its position lies in its world. The
/// seam therefore never changes appearance; the worlds only differ opposite the ship, where the
/// outgoing world's islands sink into the sea and the incoming world's rise over a short arc
/// instead of popping.
fn update_world_rocks(session: Res<Session>, mut rocks: Query<(&WorldRocks, &mut Visibility, &mut Transform)>) {
    let Some(world) = session.world() else { return };
    let Rules::SpiralVoyage(sv) = &world.rules else { return };
    let view = sv.perspective(&world.sea);
    let antipode = view.bearing + std::f32::consts::PI;
    const FADE_ARC: f32 = 12.0 * std::f32::consts::PI / 180.0;
    for (rock, mut vis, mut tf) in &mut rocks {
        let shown = view.world_at(rock.center) == rock.world;
        *vis = if shown { Visibility::Visible } else { Visibility::Hidden };
        if shown {
            let off = sim::geom::angle_delta(antipode, sim::geom::bearing_of(rock.center)).abs();
            let rise = (off / FADE_ARC).clamp(0.0, 1.0);
            tf.scale.y = (rise * rise * (3.0 - 2.0 * rise)).max(0.01);
        }
    }
}

/// The harbor exists only in the last world: in Spiral Voyage it shows when the ship's view of
/// the spiral resolves the harbor's position into World 4. Other modes always show it.
fn update_harbor(session: Res<Session>, mut parts: Query<&mut Visibility, With<HarborPart>>) {
    let show = match session.world() {
        Some(world) => match &world.rules {
            Rules::SpiralVoyage(sv) => {
                sv.perspective(&world.sea).world_at(world.tuning().harbor_center) == sv.worlds.len() - 1
            }
            _ => true,
        },
        None => true,
    };
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in &mut parts {
        if *vis != want {
            *vis = want;
        }
    }
}

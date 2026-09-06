//! Sea fog: a volumetric density field over the water that hides the sea until the beam parts
//! it. Density is 3D noise scrolled by the wind, multiplied by a clearance field the lighthouse
//! beam writes into every frame and that closes back over a few seconds. The beam itself lights
//! the fog (volumetric spot lights), so the parted lane reads as a glowing tunnel in the mist.
use crate::app::{Session, Settings};
use crate::scene::MainCamera;
use crate::sim::{Footprint, Phase};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::FogVolume;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Density texture resolution. Relative to the previous 64 × 8 × 64 grid, these dimensions make
/// each voxel approximately 1.5 times larger on every axis.
const N: usize = 43;
const H: usize = 5;
/// World extent the density texture spans (render coordinates); matches the fog volume.
const EXTENT: f32 = 240.0;
/// Seconds for parted fog to close back to full density.
const CLOSE_SECONDS: f32 = 7.0;

#[derive(Component)]
pub struct SeaFog;

#[derive(Resource)]
struct FogState {
    image: Handle<Image>,
    /// 1 = fully parted, 0 = fog at rest. Sim-plane grid over the same extent.
    clearance: Vec<f32>,
}

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, update);
    }
}

fn hash(x: f32, y: f32, z: f32) -> f32 {
    ((x * 127.1 + y * 311.7 + z * 74.7).sin() * 43758.5453).fract().abs()
}

fn value_noise(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let n = |dx: f32, dy: f32, dz: f32| hash(i.x + dx, i.y + dy, i.z + dz);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(n(0.0, 0.0, 0.0), n(1.0, 0.0, 0.0), u.x);
    let x10 = lerp(n(0.0, 1.0, 0.0), n(1.0, 1.0, 0.0), u.x);
    let x01 = lerp(n(0.0, 0.0, 1.0), n(1.0, 0.0, 1.0), u.x);
    let x11 = lerp(n(0.0, 1.0, 1.0), n(1.0, 1.0, 1.0), u.x);
    lerp(lerp(x00, x10, u.y), lerp(x01, x11, u.y), u.z)
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut image = Image::new_fill(
        Extent3d { width: N as u32, height: H as u32, depth_or_array_layers: N as u32 },
        TextureDimension::D3,
        &[0u8],
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    let image = images.add(image);

    // Keep ambient mist subordinate to the lighthouse; the dedicated beam light supplies the
    // dramatic in-scattering.
    commands.spawn((
        SeaFog,
        FogVolume {
            density_factor: 0.0,
            density_texture: Some(image.clone()),
            absorption: 0.05,
            scattering: 0.9,
            scattering_asymmetry: 0.3,
            light_intensity: 600.0,
            fog_color: Color::srgb(0.75, 0.85, 1.0),
            ..default()
        },
        Transform::from_xyz(0.0, 12.0, 0.0).with_scale(Vec3::new(240.0, 32.0, 240.0)),
    ));
    commands.insert_resource(FogState { image, clearance: vec![0.0; N * N] });
}

/// Texel (x, z) centre in sim coordinates (x east, y north).
fn cell_sim(x: usize, z: usize) -> glam::Vec2 {
    let u = (x as f32 + 0.5) / N as f32 - 0.5;
    let w = (z as f32 + 0.5) / N as f32 - 0.5;
    glam::Vec2::new(u * EXTENT, -w * EXTENT)
}

/// Select the fog-opening shoulder on the camera-near or camera-far side. Both shoulder
/// positions are expressed in simulation coordinates so review-camera overrides work too.
fn camera_side_half_width(bearing: f32, delta: f32, near_half: f32, far_half: f32, camera_sim: glam::Vec2) -> f32 {
    let positive_shoulder = crate::sim::geom::dir(bearing + far_half);
    let negative_shoulder = crate::sim::geom::dir(bearing - far_half);
    let positive_is_near =
        camera_sim.distance_squared(positive_shoulder) < camera_sim.distance_squared(negative_shoulder);
    if (delta >= 0.0) == positive_is_near {
        near_half
    } else {
        far_half
    }
}

/// Beam-relative clearance with a clear core wide enough to expose the original surface beam and
/// a smooth shoulder across the rest of the camera-relative opening.
fn clearance_profile(u: f32) -> f32 {
    let shoulder = ((u.clamp(0.0, 1.0) - 0.45) / 0.55).clamp(0.0, 1.0);
    1.0 - shoulder * shoulder * (3.0 - 2.0 * shoulder)
}

/// Fully parted fog reaches zero local density exactly on the beam centerline; untouched fog
/// remains at full density.
fn fog_density_multiplier(clearance: f32) -> f32 {
    1.0 - clearance.clamp(0.0, 1.0)
}

fn update(
    time: Res<Time>,
    session: Res<Session>,
    settings: Res<Settings>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut state: ResMut<FogState>,
    mut images: ResMut<Assets<Image>>,
    mut volumes: Query<&mut FogVolume, With<SeaFog>>,
) {
    let dt = time.delta_secs();
    let world = session.world();
    // Fog is a night thing: it rolls in with dusk and burns off at first light.
    let (presence, footprint) = match world {
        Some(w) => {
            let p = match w.phase {
                Phase::Intro { .. } => (1.0 - w.dusk()).powi(2),
                Phase::Night => 1.0,
                Phase::Dawn { .. } => 1.0 - crate::sea::dawn_amount(w),
                _ => 0.0,
            };
            let fp = w.beam_active().then(|| session.view_footprint().unwrap_or_else(|| w.footprint()));
            (p, fp)
        }
        None => (0.35, None),
    };
    let density = if settings.fog { presence } else { 0.0 };

    // Clearance: the beam's whole lane parts the fog where it lights the water; everywhere it does
    // not, the fog closes back.
    let (bearing, near_half, far_half) = match footprint {
        Some(Footprint::Spot { bearing, half_angle, .. }) => (bearing, half_angle * 4.5, half_angle * 9.0),
        Some(Footprint::Sector { angle_start, angle_end, .. }) => {
            let half = (angle_end - angle_start) * 0.5;
            ((angle_start + angle_end) * 0.5, half, half)
        }
        None => (0.0, -1.0, -1.0),
    };
    // Camera transforms use render coordinates (x east, z south), while beam geometry uses
    // simulation coordinates (x east, y north).
    let camera_sim = camera
        .single()
        .map(|transform| {
            let p = transform.translation();
            glam::Vec2::new(p.x, -p.z)
        })
        .unwrap_or(glam::Vec2::new(0.0, -1.0));
    let close = dt / CLOSE_SECONDS;
    for z in 0..N {
        for x in 0..N {
            let p = cell_sim(x, z);
            let r = p.length();
            let mut c = state.clearance[z * N + x] - close;
            if near_half > 0.0 && r > 6.0 && r < EXTENT * 0.5 {
                let point_bearing = crate::sim::geom::bearing_of(p);
                let d = crate::sim::geom::angle_delta(bearing, point_bearing);
                let half = camera_side_half_width(bearing, d, near_half, far_half, camera_sim);
                // Fade over the full camera-relative shoulder instead of cutting from a broad
                // empty core to full fog. The density conversion below retains mist at the center.
                let u = d.abs() / half;
                c = c.max(clearance_profile(u));
            }
            state.clearance[z * N + x] = c.clamp(0.0, 1.0);
        }
    }

    // Noise evolves slowly in place and drifts from the north-west toward the south-east. The
    // slab fades to nothing at its borders so the volume's box never shows.
    let t = time.elapsed_secs();
    let drift = Vec3::new(1.0, 0.0, 1.0) * (t * 0.09);
    let evolve = t * 0.03;
    if let Some(mut image) = images.get_mut(&state.image) {
        if let Some(data) = image.data.as_mut() {
            for z in 0..N {
                for y in 0..H {
                    // Dense near the water, thinning to nothing at the top of the slab.
                    let height = 1.0 - ((y as f32 + 0.5) / H as f32).powi(2);
                    for x in 0..N {
                        let p = Vec3::new(x as f32, y as f32 * 2.0, z as f32) - drift;
                        let a = value_noise(p / 9.0 + Vec3::new(0.0, evolve, 0.0));
                        let b = value_noise(p / 4.0 + Vec3::new(17.3, evolve * 1.7, 5.1));
                        let n = ((0.65 * a + 0.35 * b - 0.2) * 1.8).clamp(0.0, 1.0) * height;
                        // Fade out beyond the playable sea so the slab's box never shows.
                        let radius = cell_sim(x, z).length();
                        let edge = 1.0 - ((radius - 78.0) / 32.0).clamp(0.0, 1.0);
                        let d = n * edge * fog_density_multiplier(state.clearance[z * N + x]);
                        data[(z * H + y) * N + x] = (d * 255.0) as u8;
                    }
                }
            }
        }
    }
    for mut volume in &mut volumes {
        volume.density_factor = 0.35 * density;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    #[test]
    fn fog_opening_uses_the_camera_near_shoulder() {
        let beam_half = 0.1;
        let near = beam_half * 4.5;
        let far = beam_half * 9.0;

        // The default camera is south in simulation coordinates. Facing east, the clockwise
        // (positive) shoulder points south; facing west, the counter-clockwise shoulder does.
        let south = glam::Vec2::new(0.0, -100.0);
        assert_eq!(camera_side_half_width(FRAC_PI_2, 0.1, near, far, south), near);
        assert_eq!(camera_side_half_width(FRAC_PI_2, -0.1, near, far, south), far);
        assert_eq!(camera_side_half_width(3.0 * FRAC_PI_2, -0.1, near, far, south), near);
        assert_eq!(camera_side_half_width(3.0 * FRAC_PI_2, 0.1, near, far, south), far);

        // An east-side review camera sees the clockwise shoulder of a north-facing beam first.
        let east = glam::Vec2::new(100.0, 0.0);
        assert_eq!(camera_side_half_width(0.0, 0.1, near, far, east), near);
        assert_eq!(camera_side_half_width(0.0, -0.1, near, far, east), far);

        // A diagonal FIRST_LIGHT_CAMERA override still follows geometric proximity.
        let diagonal = glam::Vec2::new(100.0, -100.0);
        assert_eq!(camera_side_half_width(FRAC_PI_4, 0.1, near, far, diagonal), near);
        assert_eq!(camera_side_half_width(FRAC_PI_4, -0.1, near, far, diagonal), far);

        // The shoulder multipliers remain 4.5x and 9x the beam half-angle.
        assert!((near / (beam_half * 9.0) - 0.5).abs() < f32::EPSILON);
        assert!((far / beam_half - 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn fog_opening_fades_gradually_from_a_clear_center() {
        let center = fog_density_multiplier(clearance_profile(0.0));
        let middle = fog_density_multiplier(clearance_profile(0.5));
        let edge = fog_density_multiplier(clearance_profile(1.0));

        assert!(center.abs() < f32::EPSILON);
        assert!(fog_density_multiplier(clearance_profile(0.4)).abs() < f32::EPSILON);
        assert!(center < middle && middle < edge);
        assert!((edge - 1.0).abs() < f32::EPSILON);

        let mut previous = center;
        for step in 1..=100 {
            let density = fog_density_multiplier(clearance_profile(step as f32 / 100.0));
            assert!(density >= previous);
            previous = density;
        }
    }
}

//! Sea fog: a volumetric density field over the water that hides the sea until the beam parts
//! it. Density is 3D noise scrolled by the wind, multiplied by a clearance field the lighthouse
//! beam writes into every frame and that closes back over a few seconds. The beam itself lights
//! the fog (volumetric spot lights), so the parted lane reads as a glowing tunnel in the mist.

use crate::app::{Session, Settings};
use crate::sim::{Footprint, Phase};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::FogVolume;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Density texture resolution: `N` cells across the sea in x/z, `H` vertically.
const N: usize = 64;
const H: usize = 8;
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

    // Bevy attenuates in-scattered light over the volume's bounding radius, so a scene this large
    // needs a strong artistic light multiplier.
    commands.spawn((
        SeaFog,
        FogVolume {
            density_factor: 0.0,
            density_texture: Some(image.clone()),
            absorption: 0.05,
            scattering: 0.9,
            scattering_asymmetry: 0.3,
            light_intensity: 2500.0,
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

fn update(
    time: Res<Time>,
    session: Res<Session>,
    settings: Res<Settings>,
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
                Phase::Intro { .. } => 1.0 - w.dusk(),
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
    let (bearing, half_angle) = match footprint {
        Some(Footprint::Spot { bearing, half_angle, .. }) => (bearing, half_angle * 8.0),
        Some(Footprint::Sector { angle_start, angle_end, .. }) => {
            ((angle_start + angle_end) * 0.5, (angle_end - angle_start) * 0.5)
        }
        None => (0.0, -1.0),
    };
    let close = dt / CLOSE_SECONDS;
    for z in 0..N {
        for x in 0..N {
            let p = cell_sim(x, z);
            let r = p.length();
            let mut c = state.clearance[z * N + x] - close;
            if half_angle > 0.0 && r > 6.0 && r < EXTENT * 0.5 {
                let d = crate::sim::geom::angle_delta(bearing, crate::sim::geom::bearing_of(p)).abs();
                // Soft-edged wedge along the centreline; the lane keeps a quarter of the fog.
                let part = 1.0 - (d / half_angle).clamp(0.0, 1.0);
                c = c.max(0.75 * part * part);
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
                        let d = n * edge * (1.0 - state.clearance[z * N + x]);
                        data[(z * H + y) * N + x] = (d * 255.0) as u8;
                    }
                }
            }
        }
    }
    for mut volume in &mut volumes {
        volume.density_factor = 0.8 * density;
    }
}

//! Sea surface material: PBR water extended with the plankton charge texture and beam footprint.

use crate::app::Session;
use crate::sim::{Footprint, Phase};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

pub type SeaMaterial = ExtendedMaterial<StandardMaterial, SeaExtension>;

/// Ships whose wakes the water shows.
pub const WAKE_SHIPS: usize = 8;

#[derive(ShaderType, Reflect, Debug, Clone, Copy, Default)]
pub struct SeaParams {
    pub time: f32,
    pub sea_radius: f32,
    pub charge_cap: f32,
    pub fp_kind: u32,
    pub fp_bearing: f32,
    pub fp_half_angle: f32,
    pub fp_r_min: f32,
    pub fp_r_max: f32,
    pub dawn: f32,
    pub strong_threshold: f32,
    /// 1 when the beam's whole length dimly lights the water (Spiral Voyage); 0 otherwise.
    pub beam_lane: f32,
    pub _pad1: f32,
    /// Per ship: sim x, sim y, heading, 1 when active (0 = unused slot).
    pub ships: [Vec4; WAKE_SHIPS],
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct SeaExtension {
    #[uniform(100)]
    pub params: SeaParams,
    #[texture(101)]
    #[sampler(102)]
    pub charge: Handle<Image>,
}

impl MaterialExtension for SeaExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/sea.wgsl".into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/sea.wgsl".into()
    }
}

/// Handles the sea systems need every frame.
#[derive(Resource)]
pub struct SeaHandles {
    pub material: Handle<SeaMaterial>,
    pub charge_image: Handle<Image>,
}

pub struct SeaPlugin;

impl Plugin for SeaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SeaMaterial>::default())
            .add_systems(Startup, spawn_sea)
            .add_systems(Update, update_sea);
    }
}

fn spawn_sea(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let tuning = crate::sim::Tuning::default();
    let grid_size = (2.0 * tuning.sea_radius / tuning.cell_size).ceil() as usize;
    // R: plankton charge; G: shore proximity for foam. Both rewritten every frame.
    let mut image = Image::new_fill(
        Extent3d { width: grid_size as u32, height: grid_size as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0u8, 0u8],
        TextureFormat::Rg8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::linear();
    let charge_image = images.add(image);

    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.015, 0.035, 0.06),
            perceptual_roughness: 0.22,
            metallic: 0.0,
            reflectance: 0.55,
            ..default()
        },
        extension: SeaExtension {
            params: SeaParams {
                sea_radius: tuning.sea_radius,
                charge_cap: tuning.charge_cap,
                strong_threshold: tuning.strong_threshold,
                ..default()
            },
            charge: charge_image.clone(),
        },
    });

    commands.spawn((
        Mesh3d(meshes.add(Circle::new(tuning.sea_radius * 1.6).mesh().resolution(160))),
        MeshMaterial3d(material.clone()),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Name::new("Sea"),
    ));

    commands.insert_resource(SeaHandles { material, charge_image });
}

fn update_sea(
    time: Res<Time>,
    session: Res<Session>,
    handles: Res<SeaHandles>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let Some(mut material) = materials.get_mut(&handles.material) else { return };
    let params = &mut material.extension.params;
    params.time = time.elapsed_secs();

    let Some(world) = session.world() else {
        params.fp_kind = 0;
        params.dawn = 0.0;
        if let Some(mut image) = images.get_mut(&handles.charge_image) {
            if let Some(data) = image.data.as_mut() {
                data.iter_mut().for_each(|b| *b = 0);
            }
        }
        return;
    };

    // Footprint parameters, only while the beam lights the water. The spiral shows the beam's
    // whole length dimly; the bright footprint alone charges plankton.
    params.beam_lane = if matches!(world.rules, crate::sim::Rules::SpiralVoyage(_)) { 1.0 } else { 0.0 };
    if world.beam_active() {
        match session.view_footprint().unwrap_or_else(|| world.footprint()) {
            Footprint::Spot { bearing, half_angle, r_min, r_max } => {
                params.fp_kind = 1;
                params.fp_bearing = bearing;
                params.fp_half_angle = half_angle;
                params.fp_r_min = r_min;
                params.fp_r_max = r_max;
            }
            Footprint::Sector { angle_start, angle_end, r_min, r_max, .. } => {
                params.fp_kind = 2;
                params.fp_bearing = (angle_start + angle_end) * 0.5;
                params.fp_half_angle = (angle_end - angle_start) * 0.5;
                params.fp_r_min = r_min;
                params.fp_r_max = r_max;
            }
        }
    } else {
        params.fp_kind = 0;
    }
    params.dawn = dawn_amount(world);

    // Wakes: active ships at their interpolated poses.
    params.ships = [Vec4::ZERO; WAKE_SHIPS];
    let mut slot = 0;
    for e in &world.sea.entities {
        if slot == WAKE_SHIPS {
            break;
        }
        if e.is_active_ship() {
            let (pos, heading) = session.view_pose(e);
            params.ships[slot] = Vec4::new(pos.x, pos.y, heading, 1.0);
            slot += 1;
        }
    }

    // Upload the charge grid on view (the spiral composites one from the ship's perspective) and
    // a shore mask: how much land lies within two cells. Row j = sim y from -R upward.
    if let Some(mut image) = images.get_mut(&handles.charge_image) {
        if let Some(data) = image.data.as_mut() {
            let field = world.view_charge();
            let cap = world.tuning().charge_cap;
            let n = field.size;
            debug_assert_eq!(data.len(), field.charge.len() * 2);
            let land = |i: usize, j: usize| -> bool {
                let idx = j * n + i;
                !field.sea[idx] && field.cell_center(idx).length() <= field.sea_radius - field.cell
            };
            for j in 0..n {
                for i in 0..n {
                    let idx = j * n + i;
                    data[idx * 2] = ((field.charge[idx] / cap).clamp(0.0, 1.0) * 255.0) as u8;
                    let mut near = 0.0f32;
                    for dj in -2i32..=2 {
                        for di in -2i32..=2 {
                            let (ii, jj) = (i as i32 + di, j as i32 + dj);
                            if ii < 0 || jj < 0 || ii >= n as i32 || jj >= n as i32 {
                                continue;
                            }
                            if land(ii as usize, jj as usize) {
                                // Nearer land counts more, so the mask ramps toward the coast.
                                near += 1.0 / (1.0 + (di * di + dj * dj) as f32);
                            }
                        }
                    }
                    data[idx * 2 + 1] = ((near / 3.0).clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
    }
}

/// 0 during the night, ramping to 1 through the sunrise transition and staying there.
pub fn dawn_amount(world: &crate::sim::World) -> f32 {
    match world.phase {
        Phase::Intro { .. } | Phase::Night => 0.0,
        Phase::Dawn { elapsed } => (elapsed / world.tuning().dawn_seconds).clamp(0.0, 1.0),
        Phase::Playback | Phase::Finished => 1.0,
    }
}

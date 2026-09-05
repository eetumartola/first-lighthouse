//! Every provisional number from GDD §10 lives here so playtests change one place.

use glam::Vec2;

#[derive(Clone, Debug)]
pub struct Tuning {
    // --- Scene geometry (world units; the sea is a flat plane) ---
    pub sea_radius: f32,
    pub island_radius: f32,
    pub harbor_center: Vec2,
    pub harbor_radius: f32,
    /// Charge grid cell edge length.
    pub cell_size: f32,

    // --- Session ---
    pub night_watch_night: f32,
    pub mutable_sea_night: f32,
    pub world_weaver_night: f32,
    pub world_weaver_playback_limit: f32,
    pub intro_seconds: f32,
    pub dawn_seconds: f32,

    // --- Beam ---
    /// Seconds for one full revolution under held input.
    pub beam_turn_seconds: f32,
    /// Seconds per revolution in the optional constant-speed experiment.
    pub auto_turn_seconds: f32,
    pub beam_width_deg: f32,
    /// Radial length of the spotlight footprint (about one eighth of the sea radius).
    pub beam_length: f32,
    /// Footprint range change per second of held input.
    pub beam_range_speed: f32,

    // --- Plankton ---
    /// Seconds of afterglow gained per second of illumination.
    pub charge_rate: f32,
    /// Maximum stored afterglow in seconds.
    pub charge_cap: f32,
    /// Remaining glow (seconds) that counts as strong afterglow.
    pub strong_threshold: f32,
    /// Minimum charge for a guidance sample to be selectable by ships.
    pub usable_sample_threshold: f32,
    /// Minimum spacing between guidance samples.
    pub sample_spacing: f32,

    // --- Ships ---
    pub ship_speed: f32,
    pub ship_turn_rate_deg: f32,
    pub ship_look_distance: f32,
    pub ship_reach_radius: f32,
    /// Seconds before a passed sample may be selected again.
    pub sample_revisit_delay: f32,
    /// Attraction strength of a ship lantern, expressed in charge seconds.
    pub lantern_brightness: f32,
    pub ship_radius: f32,
    pub wreck_radius: f32,
    pub creature_radius: f32,
    pub island_form_radius: f32,

    // --- Creature ---
    pub creature_speed: f32,
    pub creature_turn_rate_deg: f32,
    pub creature_detect_radius: f32,
    pub creature_contact_radius: f32,
    /// A new target must beat the current one by this factor to steal attention.
    pub creature_stickiness: f32,
    /// Seconds between creature calls; each call surfaces it briefly.
    pub creature_call_period: f32,
    /// How long a calling creature stays visible as a silhouette.
    pub creature_surface_seconds: f32,

    // --- Night Watch ---
    pub night_watch_max_active_ships: usize,
    pub night_watch_creature_activation: f32,

    // --- Mutable Sea ---
    /// Dark seconds to leave each form: ship, wreck, creature, island.
    pub mutable_dark_durations: [f32; 4],

    // --- World Weaver ---
    pub weaver_sectors: usize,
    pub weaver_layers: usize,
    pub weaver_default_layer: u8,
    pub weaver_wreck_delay: f32,
    pub weaver_voyage_speed: f32,
    pub weaver_join_radius: f32,
    pub weaver_wreck_radius: f32,
    pub weaver_creature_speed: f32,
    pub weaver_creature_detect_radius: f32,
    /// Afterglow (seconds) stamped on a captured sector as its visual record.
    pub weaver_capture_glow: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        let sea_radius = 100.0;
        Self {
            sea_radius,
            island_radius: 8.0,
            harbor_center: Vec2::new(0.0, -14.0),
            harbor_radius: 6.0,
            cell_size: 2.0,

            night_watch_night: 180.0,
            mutable_sea_night: 180.0,
            world_weaver_night: 180.0,
            world_weaver_playback_limit: 45.0,
            intro_seconds: 4.0,
            dawn_seconds: 3.5,

            beam_turn_seconds: 8.0,
            auto_turn_seconds: 12.0,
            beam_width_deg: 15.0,
            beam_length: sea_radius / 8.0,
            beam_range_speed: 28.0,

            charge_rate: 5.0,
            charge_cap: 30.0,
            strong_threshold: 5.0,
            usable_sample_threshold: 3.0,
            sample_spacing: 3.0,

            ship_speed: 2.0,
            ship_turn_rate_deg: 40.0,
            ship_look_distance: 28.0,
            ship_reach_radius: 2.5,
            sample_revisit_delay: 20.0,
            lantern_brightness: 4.0,
            ship_radius: 1.6,
            wreck_radius: 1.8,
            creature_radius: 1.8,
            island_form_radius: 3.0,

            creature_speed: 2.0,
            creature_turn_rate_deg: 70.0,
            creature_detect_radius: 60.0,
            creature_contact_radius: 2.6,
            creature_stickiness: 1.3,
            creature_call_period: 8.0,
            creature_surface_seconds: 2.5,

            night_watch_max_active_ships: 3,
            night_watch_creature_activation: 50.0,

            mutable_dark_durations: [16.0, 10.0, 12.0, 8.0],

            weaver_sectors: 12,
            weaver_layers: 4,
            weaver_default_layer: 0,
            weaver_wreck_delay: 4.0,
            weaver_voyage_speed: 5.0,
            weaver_join_radius: 8.0,
            weaver_wreck_radius: 5.0,
            weaver_creature_speed: 3.2,
            weaver_creature_detect_radius: 30.0,
            weaver_capture_glow: 9.0,
        }
    }
}

impl Tuning {
    pub fn beam_min_range(&self) -> f32 {
        self.island_radius + self.beam_length * 0.5
    }
    pub fn beam_max_range(&self) -> f32 {
        self.sea_radius - self.beam_length * 0.5
    }
    pub fn sector_angle(&self) -> f32 {
        std::f32::consts::TAU / self.weaver_sectors as f32
    }
}

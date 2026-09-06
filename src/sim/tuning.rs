//! Every provisional number from the GDD and its addendum lives here so playtests change one place.

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
    /// Dusk fade: the scene starts visible and darkens over this many seconds.
    pub intro_seconds: f32,
    pub dawn_seconds: f32,

    // --- Beam ---
    /// Seconds for one full revolution at the capped angular speed.
    pub beam_turn_seconds: f32,
    /// Seconds to reach full angular speed from rest under held input.
    pub beam_accel_seconds: f32,
    /// Seconds to come to rest after release.
    pub beam_stop_seconds: f32,
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
    /// Remaining glow (seconds) that counts as strong afterglow (full silhouette clarity).
    pub strong_threshold: f32,
    /// Glow below which surrounding water shows no silhouette at all.
    pub silhouette_min_glow: f32,

    // --- Ships and light-reading guidance ---
    pub ship_speed: f32,
    pub ship_length: f32,
    pub ship_radius: f32,
    /// Maximum hull turn rate; inertia belongs to the ship.
    pub ship_turn_rate_deg: f32,
    /// Forward arc scanned for candidate headings.
    pub guidance_arc_deg: f32,
    /// Corridor length inspected per candidate, in ship lengths.
    pub guidance_lookahead_lengths: f32,
    /// How often intent is reconsidered.
    pub guidance_hz: f32,
    /// A competitor must beat the incumbent by this fraction to displace it.
    pub guidance_switch_advantage: f32,
    /// Seconds a better direction must keep winning before it displaces a live incumbent.
    pub guidance_dwell: f32,
    /// Fraction of a corridor's score lost at the edge of the arc (quadratic toward the bow):
    /// turning costs way, so light abeam is worth less than the same light ahead.
    pub guidance_turn_penalty: f32,
    /// Corridor score below which a direction is not considered useful illumination.
    pub guidance_min_score: f32,
    /// Light that ends closer than this many ship lengths ahead is a patch being passed, not a
    /// direction: a corridor counts only if its lit water reaches at least this far.
    pub guidance_min_reach_lengths: f32,
    /// Small immediate-obstacle rejection: candidates hitting land within this many ship
    /// lengths are discarded. Not a route repair.
    pub guidance_obstacle_rejection: bool,
    pub guidance_obstacle_lengths: f32,
    pub wreck_radius: f32,
    pub creature_radius: f32,
    pub island_form_radius: f32,

    // --- Predator (Night Watch) ---
    pub night_watch_monster: bool,
    pub night_watch_creature_activation: f32,
    pub creature_speed: f32,
    pub creature_turn_rate_deg: f32,
    /// Radius around the predator within which it eats plankton, beyond its own hull radius.
    pub creature_mouth: f32,
    /// Finite detection radius for stored plankton charge.
    pub creature_detect_radius: f32,
    pub creature_contact_radius: f32,
    /// A new patch must beat the current target by this factor to steal attention.
    pub creature_stickiness: f32,
    /// Charge seconds removed per second from cells under the predator.
    pub creature_consume_rate: f32,
    /// Seconds between creature calls (heard, not seen: in the dark only its eyes show).
    pub creature_call_period: f32,
    /// How long a newly arrived ship shows at the world's edge as a silhouette.
    pub ship_arrival_reveal_seconds: f32,

    // --- Night Watch ---
    pub night_watch_max_active_ships: usize,

    // --- Mutable Sea (suspended; kept so the retained implementation still runs) ---
    /// Dark seconds to leave each form: ship, wreck, creature, island.
    pub mutable_dark_durations: [f32; 4],

    // --- World Weaver ---
    pub weaver_sectors: usize,
    pub weaver_layers: usize,
    /// Route-finder grid cell.
    pub weaver_route_cell: f32,
    /// Extra clearance beyond the hull radius when finding a route.
    pub weaver_route_margin: f32,
    /// Seconds the found route should take; playback speed is derived from it.
    pub weaver_playback_target: f32,
    pub weaver_playback_min_speed: f32,
    /// Afterglow (seconds) stamped on a copied sector as its visual record.
    pub weaver_capture_glow: f32,

    // --- Spiral Voyage ---
    pub spiral_worlds: usize,
    /// Ship speed multiplier for the four-world voyage; its broad authored routes support a
    /// slightly faster crossing while retaining the same guidance and inertia rules.
    pub spiral_ship_speed_factor: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        let sea_radius = 100.0;
        Self {
            sea_radius,
            island_radius: 8.0,
            harbor_center: Vec2::new(0.0, 14.0),
            harbor_radius: 6.0,
            cell_size: 2.0,

            night_watch_night: 180.0,
            mutable_sea_night: 180.0,
            world_weaver_night: 180.0,
            world_weaver_playback_limit: 45.0,
            intro_seconds: 6.0,
            dawn_seconds: 6.0,

            beam_turn_seconds: 8.0,
            beam_accel_seconds: 0.25,
            beam_stop_seconds: 0.12,
            auto_turn_seconds: 12.0,
            // Oval spot: axes widened ~13% over the old wedge so the lit area is unchanged.
            beam_width_deg: 17.0,
            beam_length: sea_radius / 7.0,
            beam_range_speed: 28.0,

            charge_rate: 5.0,
            charge_cap: 30.0,
            strong_threshold: 5.0,
            silhouette_min_glow: 1.0,

            ship_speed: 4.0,
            ship_length: 3.0,
            ship_radius: 1.6,
            ship_turn_rate_deg: 40.0,
            guidance_arc_deg: 150.0,
            guidance_lookahead_lengths: 6.0,
            guidance_hz: 15.0,
            guidance_dwell: 0.5,
            guidance_switch_advantage: 0.2,
            guidance_min_score: 4.0,
            guidance_turn_penalty: 0.6,
            guidance_min_reach_lengths: 2.0,
            guidance_obstacle_rejection: true,
            guidance_obstacle_lengths: 1.5,
            wreck_radius: 1.8,
            creature_radius: 1.8,
            island_form_radius: 3.0,

            night_watch_monster: true,
            night_watch_creature_activation: 50.0,
            creature_speed: 3.0,
            creature_turn_rate_deg: 40.0,
            creature_mouth: 2.0,
            creature_detect_radius: 40.0,
            creature_contact_radius: 2.4,
            creature_stickiness: 1.3,
            creature_consume_rate: 6.0,
            creature_call_period: 8.0,
            ship_arrival_reveal_seconds: 2.0,

            night_watch_max_active_ships: 3,

            mutable_dark_durations: [16.0, 10.0, 12.0, 8.0],

            weaver_sectors: 12,
            weaver_layers: 4,
            weaver_route_cell: 2.0,
            weaver_route_margin: 0.6,
            weaver_playback_target: 36.0,
            weaver_playback_min_speed: 5.0,
            weaver_capture_glow: 9.0,

            spiral_worlds: 4,
            spiral_ship_speed_factor: 1.2,
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
    pub fn guidance_lookahead(&self) -> f32 {
        self.guidance_lookahead_lengths * self.ship_length
    }
}

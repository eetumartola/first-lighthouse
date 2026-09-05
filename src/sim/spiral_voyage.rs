//! Variant 4 — Spiral Voyage: one ship, four persistent worlds arranged along the beam's
//! winding. A clockwise circuit past the south seam enters the next world. The beam and the ship
//! each keep their own winding, so the player can scout ahead while the vessel sails on.

use super::beam::Footprint;
use super::charge::ChargeField;
use super::entity::{EntityId, Form, Status};
use super::geom::{angle_delta, bearing_of, compass_word, turn_toward, Circle};
use super::level::SEAM;
use super::islands;
use super::steering::{self, Waters};
use super::tuning::Tuning;
use super::{Cause, Event, Outcome, Sea};
use glam::Vec2;
use std::f32::consts::TAU;

#[derive(Clone, Debug)]
pub struct SpiralWorld {
    /// Land of this world, including the central island.
    pub rocks: Vec<Circle>,
    /// This world's stored plankton charge; each world keeps and decays its own.
    pub charge: ChargeField,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VoyageEnd {
    Arrived,
    Grounded(Vec2),
}

/// Angular half-width of a seam: the ship's world only changes once its winding is this far past
/// the seam, so a hull straddling it while turning cannot flip worlds twice.
pub const SEAM_BAND: f32 = 1.5 * TAU / super::level::COLUMNS as f32;

#[derive(Clone, Debug)]
pub struct SpiralVoyage {
    pub worlds: Vec<SpiralWorld>,
    pub ship: Option<EntityId>,
    /// The world the ship is in: follows its winding with `SEAM_BAND` hysteresis.
    pub ship_world: usize,
    pub start: Vec2,
    pub start_heading: f32,
    pub last_beam_world: usize,
    pub end: Option<VoyageEnd>,
}

/// The seam sits in the south; column 0 of an authored level block is south. A world's windings
/// are `[SEAM + k·TAU, SEAM + (k+1)·TAU)`.
pub fn world_of(winding: f32, worlds: usize) -> usize {
    (((winding - SEAM) / TAU).floor().max(0.0) as usize).min(worlds - 1)
}

/// A bearing as an absolute winding inside world `k`: `world_of` of the result is `k`.
pub fn winding_in_world(k: usize, bearing: f32) -> f32 {
    bearing + TAU * (k as f32 + if bearing < SEAM { 1.0 } else { 0.0 })
}
/// Light and land as seen from a ship at a given winding: positions near the seam resolve into
/// the neighbouring world instead of leaking charge between unrelated layers.
struct SpiralWaters<'a> {
    worlds: &'a [SpiralWorld],
    ship_winding: f32,
    ship_bearing: f32,
    sea_radius: f32,
}

impl SpiralWaters<'_> {
    fn world_at(&self, p: Vec2) -> usize {
        let w = self.ship_winding + angle_delta(self.ship_bearing, bearing_of(p));
        world_of(w, self.worlds.len())
    }
}

impl Waters for SpiralWaters<'_> {
    fn charge_at(&self, p: Vec2) -> f32 {
        self.worlds[self.world_at(p)].charge.charge_at(p)
    }
    fn clearance_at(&self, center: Vec2) -> f32 {
        self.worlds[self.world_at(center)]
            .rocks
            .iter()
            .map(|rock| center.distance(rock.center) - rock.radius)
            .fold(self.sea_radius - center.length(), f32::min)
    }
    fn hull_is_clear(&self, center: Vec2, radius: f32) -> bool {
        let world = self.world_at(center);
        let clear_in = |index: usize| {
            self.worlds[index]
                .rocks
                .iter()
                .all(|rock| center.distance(rock.center) >= radius + rock.radius)
        };
        if center.length() > self.sea_radius - radius || !clear_in(world) {
            return false;
        }
        if center.y < 0.0 && center.x.abs() <= radius {
            (world == 0 || clear_in(world - 1))
                && (world + 1 == self.worlds.len() || clear_in(world + 1))
        } else {
            true
        }
    }
}

impl SpiralVoyage {
    pub fn scenario(t: &Tuning) -> Self {
        let island = Circle::new(Vec2::ZERO, t.island_radius);
        // The level is authored as a polar ASCII map; column 0 of each 30-column block is the
        // seam bearing (south) and the bottom row is the island itself.
        let layouts = super::level::parse(super::level::MODE4_LEVEL1, t.island_radius, t.sea_radius);
        let worlds = layouts
            .into_iter()
            .map(|mut rocks| {
                rocks.insert(0, island);
                let charge = ChargeField::new(t, &rocks);
                SpiralWorld { rocks, charge }
            })
            .collect();
        Self {
            worlds,
            ship: None,
            ship_world: 0,
            start: islands::polar(300.0, 40.0),
            start_heading: 60f32.to_radians(),
            last_beam_world: 0,
            end: None,
        }
    }

    pub fn beam_world(&self, sea: &Sea) -> usize {
        world_of(sea.beam.winding, self.worlds.len())
    }

    pub fn ship_world(&self, sea: &Sea) -> Option<usize> {
        sea.entity(self.ship?).map(|_| self.ship_world)
    }
}

/// Spawn the ship in World 1 and point the beam at it; the beam winds finitely over the voyage.
pub fn populate(sv: &mut SpiralVoyage, sea: &mut Sea) {
    let id = sea.spawn("Wayfarer", Form::Ship, sv.start, sv.start_heading);
    sv.ship = Some(id);
    // Worlds are measured from the seam (south): the start bearing unwrapped into world 1.
    let start_winding = winding_in_world(0, bearing_of(sv.start));
    sv.ship_world = 0;
    if let Some(e) = sea.entity_mut(id) {
        e.winding = start_winding;
    }
    let worlds = sea.tuning.spiral_worlds as f32;
    sea.beam.winding_limits = Some((SEAM, SEAM + worlds * TAU - 1e-3));
    sea.beam.winding = start_winding;
    sea.beam.range = sv.start.length().clamp(sea.tuning.beam_min_range(), sea.tuning.beam_max_range());
}

/// Advance one step. Returns true when the voyage has ended.
pub fn step(sv: &mut SpiralVoyage, sea: &mut Sea, footprint: &Footprint, dt: f32) -> bool {
    let t = sea.tuning.clone();
    let n = sv.worlds.len();

    // Only the inspected world is charged; every world decays.
    let beam_world = sv.beam_world(sea);
    for (i, w) in sv.worlds.iter_mut().enumerate() {
        w.charge.step((i == beam_world).then_some(footprint), &t, dt);
    }
    if beam_world != sv.last_beam_world {
        sv.last_beam_world = beam_world;
        sea.events.push(Event::LayerChanged { layer: beam_world as u8 });
    }

    let Some(ship_id) = sv.ship.or_else(|| sea.entities.iter().find(|e| e.form == Form::Ship).map(|e| e.id)) else {
        return true;
    };
    sv.ship = Some(ship_id);
    let Some(idx) = sea.entities.iter().position(|e| e.id == ship_id) else { return true };
    if !sea.entities[idx].is_active() {
        return true;
    }

    let old_pos = sea.entities[idx].pos;
    let old_winding = sea.entities[idx].winding;
    let old_heading = sea.entities[idx].heading;
    {
        let waters = SpiralWaters {
            worlds: &sv.worlds,
            ship_winding: old_winding,
            ship_bearing: bearing_of(old_pos),
            sea_radius: t.sea_radius,
        };
        let mut voyage_tuning = t.clone();
        voyage_tuning.ship_speed *= t.spiral_ship_speed_factor;
        steering::steer_ship(&mut sea.entities[idx], &waters, &voyage_tuning, dt);
    }
    let ship = &mut sea.entities[idx];
    let new_winding = old_winding + angle_delta(bearing_of(old_pos), bearing_of(ship.pos));
    let max_winding = SEAM + n as f32 * TAU;
    if new_winding < SEAM || new_winding >= max_winding {
        // The voyage is finite: the seam before World 1 and after World 4 is a wall. Hold position
        // and turn inward so the hull does not sit against it.
        ship.pos = old_pos;
        ship.heading = turn_toward(old_heading, bearing_of(-old_pos), t.ship_turn_rate_deg.to_radians() * dt);
        ship.brain.desired = ship.heading;
    } else {
        ship.winding = new_winding;
        // Cross a seam only once the hull is clearly past it (a seam has a width).
        let upper = SEAM + (sv.ship_world + 1) as f32 * TAU + SEAM_BAND;
        let lower = SEAM + sv.ship_world as f32 * TAU - SEAM_BAND;
        if new_winding >= upper && sv.ship_world + 1 < n {
            sv.ship_world += 1;
            sea.events.push(Event::ShipCrossed { world: sv.ship_world as u8 });
        } else if new_winding < lower && sv.ship_world > 0 {
            sv.ship_world -= 1;
            sea.events.push(Event::ShipCrossed { world: sv.ship_world as u8 });
        }
    }

    let ship_world = sv.ship_world;
    let harbor = sea.harbor();
    let hull = sea.entities[idx].circle();
    if ship_world == n - 1 && harbor.contains(hull.center) {
        sv.end = Some(VoyageEnd::Arrived);
        sea.secure(ship_id);
        sea.events.push(Event::VoyageArrived);
        return true;
    }
    if let Some(rock) = sv.worlds[ship_world].rocks.iter().find(|r| r.overlaps(&hull)) {
        sea.entities[idx].status = Status::Sunk;
        sv.end = Some(VoyageEnd::Grounded(rock.center));
        sea.events.push(Event::Sunk { id: ship_id, pos: hull.center, cause: Cause::Rock });
        return true;
    }
    false
}

pub fn outcome(sv: &SpiralVoyage, sea: &Sea) -> Outcome {
    let n = sv.worlds.len();
    let reached = sv.ship_world(sea).map(|w| w + 1).unwrap_or(1);
    let (success, headline) = match sv.end {
        Some(VoyageEnd::Arrived) => (true, "The Wayfarer came through all four worlds to harbor.".to_string()),
        Some(VoyageEnd::Grounded(at)) => (
            false,
            format!("The Wayfarer struck the {} rocks in World {reached}.", compass_word(at)),
        ),
        None => (false, "The voyage did not resolve.".to_string()),
    };
    Outcome {
        success,
        headline,
        details: vec![format!("Reached World {reached} of {n}.")],
        rescued: success as usize,
        total: 1,
    }
}

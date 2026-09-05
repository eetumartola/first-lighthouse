//! Variant 4 — Spiral Voyage: one ship, four persistent worlds arranged along the beam's
//! winding. A clockwise circuit past the north seam enters the next world. The beam and the ship
//! each keep their own winding, so the player can scout ahead while the vessel sails on.

use super::beam::Footprint;
use super::charge::ChargeField;
use super::entity::{EntityId, Form, Status};
use super::geom::{angle_delta, bearing_of, compass_word, dir, turn_toward, Circle};
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

#[derive(Clone, Debug)]
pub struct SpiralVoyage {
    pub worlds: Vec<SpiralWorld>,
    pub ship: Option<EntityId>,
    pub start: Vec2,
    pub start_heading: f32,
    pub last_beam_world: usize,
    pub end: Option<VoyageEnd>,
}

/// World index of an unwrapped angle, clamped to the finite voyage.
pub fn world_of(winding: f32, worlds: usize) -> usize {
    ((winding / TAU).floor().max(0.0) as usize).min(worlds - 1)
}

/// Light and land as seen from a ship at a given winding: positions near the seam resolve into
/// the neighbouring world instead of leaking charge between unrelated layers.
struct SpiralWaters<'a> {
    worlds: &'a [SpiralWorld],
    ship_winding: f32,
    ship_bearing: f32,
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
    fn is_land(&self, p: Vec2) -> bool {
        self.worlds[self.world_at(p)].charge.is_land(p)
    }
}

impl SpiralVoyage {
    pub fn scenario(t: &Tuning) -> Self {
        let p = islands::polar;
        let island = Circle::new(Vec2::ZERO, t.island_radius);
        // Every world keeps the seam approach (within ~15° of north, r 15–60) clear.
        let layouts: Vec<Vec<Circle>> = vec![
            // World 1: a short leg to the seam. An outer reef pushes the approach inward; an islet
            // splits it into an inner lagoon channel and a middle channel.
            islands::land(vec![
                islands::arc(56.0, 318.0, 348.0, 3.4),
                islands::islet(p(330.0, 26.0), 5.0),
                islands::islet(p(275.0, 48.0), 4.5),
            ]),
            // World 2: a full circuit. Outer reef in the north-east, islet at 60°, skerries in the
            // south-east, an inner reef in the south-west forcing the outer passage there.
            islands::land(vec![
                islands::arc(62.0, 20.0, 80.0, 3.2),
                islands::islet(p(60.0, 36.0), 6.0),
                islands::chain(p(150.0, 46.0), dir(240f32.to_radians()) * 4.4, 4, 3.0),
                islands::arc(30.0, 200.0, 250.0, 2.8),
                islands::islet(p(320.0, 48.0), 5.0),
            ]),
            // World 3: outer reef north-east, inner reef east, islet south-west, a radial wall
            // near the seam that forces the inner approach.
            islands::land(vec![
                islands::arc(70.0, 12.0, 60.0, 3.2),
                islands::arc(25.0, 90.0, 150.0, 2.8),
                islands::islet(p(230.0, 42.0), 6.0),
                islands::radial(300.0, 44.0, 6, 4.4, 3.0),
            ]),
            // World 4: the harbor world. An islet east, a reef band south-east; the harbor's
            // southern approach stays open.
            islands::land(vec![
                islands::islet(p(60.0, 30.0), 5.0),
                islands::arc(46.0, 100.0, 140.0, 3.0),
                vec![Circle::new(p(250.0, 30.0), 3.5)],
            ]),
        ];
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
            start: p(300.0, 40.0),
            start_heading: 30f32.to_radians(),
            last_beam_world: 0,
            end: None,
        }
    }

    pub fn beam_world(&self, sea: &Sea) -> usize {
        world_of(sea.beam.winding, self.worlds.len())
    }

    pub fn ship_world(&self, sea: &Sea) -> Option<usize> {
        let e = sea.entity(self.ship?)?;
        Some(world_of(e.winding, self.worlds.len()))
    }
}

/// Spawn the ship in World 1 and point the beam at it; the beam winds finitely over the voyage.
pub fn populate(sv: &mut SpiralVoyage, sea: &mut Sea) {
    let id = sea.spawn("Wayfarer", Form::Ship, sv.start, sv.start_heading);
    sv.ship = Some(id);
    let start_winding = bearing_of(sv.start);
    if let Some(e) = sea.entity_mut(id) {
        e.winding = start_winding;
    }
    let worlds = sea.tuning.spiral_worlds as f32;
    sea.beam.winding_limits = Some((0.0, worlds * TAU - 1e-3));
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
        };
        let mut voyage_tuning = t.clone();
        voyage_tuning.ship_speed *= t.spiral_ship_speed_factor;
        steering::steer_ship(&mut sea.entities[idx], &waters, &voyage_tuning, dt);
    }
    let ship = &mut sea.entities[idx];
    let new_winding = old_winding + angle_delta(bearing_of(old_pos), bearing_of(ship.pos));
    let max_winding = n as f32 * TAU;
    if new_winding < 0.0 || new_winding >= max_winding {
        // The voyage is finite: the seam before World 1 and after World 4 is a wall. Hold position
        // and turn inward so the hull does not sit against it.
        ship.pos = old_pos;
        ship.heading = turn_toward(old_heading, bearing_of(-old_pos), t.ship_turn_rate_deg.to_radians() * dt);
        ship.brain.desired = ship.heading;
    } else {
        let before = world_of(old_winding, n);
        ship.winding = new_winding;
        let after = world_of(new_winding, n);
        if after != before {
            sea.events.push(Event::ShipCrossed { world: after as u8 });
        }
    }

    let ship_world = world_of(sea.entities[idx].winding, n);
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

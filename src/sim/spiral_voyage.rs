//! Variant 4 — Spiral Voyage: one ship, four persistent worlds arranged along a spiral that
//! passes the south seam once per world. Everything is seen from the ship: a position ahead of
//! it past the seam already belongs to the next world, one behind it before the seam to the
//! previous. Land, light, the beam and grounding all resolve this way, so the seam never changes
//! appearance as the ship approaches it; the two worlds only differ at the ship's antipode.

use super::beam::Footprint;
use super::charge::ChargeField;
use super::entity::{EntityId, Form, Status};
use super::geom::{angle_delta, bearing_of, compass_word, turn_toward, Circle};
use super::islands;
use super::level::SEAM;
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
    Taken(Vec2),
}

/// Where a spiral position resolves for an observer at `winding`/`bearing`: the world of the
/// unwrapped angle `winding + angle_delta(bearing, bearing_of(p))`, so the half circle ahead of
/// the observer runs into the next world and the half circle behind into the previous.
#[derive(Clone, Copy, Debug)]
pub struct Perspective {
    pub winding: f32,
    pub bearing: f32,
    pub worlds: usize,
}

impl Perspective {
    pub fn world_at(&self, p: Vec2) -> usize {
        world_of(self.winding + angle_delta(self.bearing, bearing_of(p)), self.worlds)
    }
}

/// Angular half-width of the seam for `SpiralVoyage::ship_world`.
pub const SEAM_BAND: f32 = 1.5 * TAU / super::level::COLUMNS as f32;

#[derive(Clone, Debug)]
pub struct SpiralVoyage {
    pub worlds: Vec<SpiralWorld>,
    /// The composite the ship sees (and the presentation draws): each cell taken from the world
    /// it resolves to from the ship's perspective. Rebuilt every step.
    pub view: ChargeField,
    cell_bearings: Vec<f32>,
    pub ship: Option<EntityId>,
    /// The world the ship is announced to be in: follows its winding with `SEAM_BAND` hysteresis
    /// so a hull working radially along the seam does not cross back and forth every step. The
    /// view of the spiral uses the ship's exact winding and needs no such band.
    pub ship_world: usize,
    pub start: Vec2,
    pub start_heading: f32,
    /// One predator per world, in the same order as `worlds`; `None` where a world has no open
    /// water for one.
    pub creatures: Vec<Option<EntityId>>,
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
/// Light and land as seen from the ship.
struct SpiralWaters<'a> {
    worlds: &'a [SpiralWorld],
    view: Perspective,
    sea_radius: f32,
}

impl SpiralWaters<'_> {
    fn world_at(&self, p: Vec2) -> usize {
        self.view.world_at(p)
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
            self.worlds[index].rocks.iter().all(|rock| center.distance(rock.center) >= radius + rock.radius)
        };
        if center.length() > self.sea_radius - radius || !clear_in(world) {
            return false;
        }
        if center.y < 0.0 && center.x.abs() <= radius {
            (world == 0 || clear_in(world - 1)) && (world + 1 == self.worlds.len() || clear_in(world + 1))
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
        let worlds: Vec<SpiralWorld> = layouts
            .into_iter()
            .map(|mut rocks| {
                rocks.insert(0, island);
                let charge = ChargeField::new(t, &rocks);
                SpiralWorld { rocks, charge }
            })
            .collect();
        let view = worlds[0].charge.clone();
        let cell_bearings = (0..view.charge.len()).map(|i| bearing_of(view.cell_center(i))).collect();
        Self {
            worlds,
            view,
            cell_bearings,
            ship: None,
            ship_world: 0,
            start: islands::polar(300.0, 40.0),
            start_heading: 60f32.to_radians(),
            creatures: Vec::new(),
            end: None,
        }
    }

    /// The ship's view of the spiral; before the ship exists, the start position's.
    pub fn perspective(&self, sea: &Sea) -> Perspective {
        let worlds = self.worlds.len();
        match self.ship.and_then(|id| sea.entity(id)) {
            Some(e) => Perspective { winding: e.winding, bearing: bearing_of(e.pos), worlds },
            None => Perspective {
                winding: winding_in_world(0, bearing_of(self.start)),
                bearing: bearing_of(self.start),
                worlds,
            },
        }
    }

    /// World the beam footprint centre resolves to from the ship.
    pub fn beam_world(&self, sea: &Sea) -> usize {
        world_of(sea.beam.winding, self.worlds.len())
    }

    pub fn ship_world(&self, sea: &Sea) -> Option<usize> {
        sea.entity(self.ship?).map(|_| self.ship_world)
    }

    /// Rebuild `view` from the ship's perspective.
    fn refresh_view(&mut self, view: Perspective) {
        for (i, &bearing) in self.cell_bearings.iter().enumerate() {
            let world = &self.worlds[world_of(view.winding + angle_delta(view.bearing, bearing), view.worlds)];
            self.view.charge[i] = world.charge.charge[i];
            self.view.sea[i] = world.charge.sea[i];
        }
    }
}

/// Names of the predators, one per world.
const CREATURE_NAMES: [&str; 4] = ["Leviathan", "Hafgufa", "Nixie", "Kraken"];

/// Open water for a world's predator: a deterministic clockwise scan from the world's own
/// offset, so each world's monster starts in different water and never inside its own rocks.
fn creature_spawn(rocks: &[Circle], world: usize, t: &Tuning) -> Option<(Vec2, f32)> {
    let clear = t.creature_radius + t.cell_size;
    for step in 0..24 {
        let deg = (world as f32 * 90.0 + step as f32 * 15.0) % 360.0;
        for fraction in [0.62, 0.78, 0.46] {
            let pos = islands::polar(deg, t.sea_radius * fraction);
            if rocks.iter().all(|rock| pos.distance(rock.center) > rock.radius + clear) {
                // Facing the lighthouse: the water it hunts lies inward along the beam.
                return Some((pos, (deg + 180.0).rem_euclid(360.0).to_radians()));
            }
        }
    }
    None
}

/// Surface the predator of the ship's current world briefly, so entering a world announces the
/// thing that lives in it instead of revealing it only on contact.
fn announce_creature(sv: &SpiralVoyage, sea: &mut Sea) {
    let Some(id) = sv.creatures.get(sv.ship_world).copied().flatten() else { return };
    let seconds = sea.tuning.ship_arrival_reveal_seconds;
    let now = sea.time;
    let Some(pos) = sea.entity_mut(id).map(|e| {
        e.surface(now, seconds);
        e.pos
    }) else {
        return;
    };
    sea.events.push(Event::CreatureAppears { id, pos });
}

/// Spawn the ship in World 1 and point the beam at it.
pub fn populate(sv: &mut SpiralVoyage, sea: &mut Sea) {
    let id = sea.spawn("Wayfarer", Form::Ship, sv.start, sv.start_heading);
    sv.ship = Some(id);
    // Worlds are measured from the seam (south): the start bearing unwrapped into world 1.
    let start_winding = winding_in_world(0, bearing_of(sv.start));
    sv.ship_world = 0;
    if let Some(e) = sea.entity_mut(id) {
        e.winding = start_winding;
    }
    sea.beam.winding = start_winding;
    sea.beam.range = sv.start.length().clamp(sea.tuning.beam_min_range(), sea.tuning.beam_max_range());

    // Every world keeps its own predator: hunting its own plankton, dangerous only where the
    // ship's view of the spiral resolves it into the world it belongs to.
    sv.creatures = vec![None; sv.worlds.len()];
    if sea.tuning.spiral_monsters {
        let t = sea.tuning.clone();
        for world in 0..sv.worlds.len() {
            let Some((pos, heading)) = creature_spawn(&sv.worlds[world].rocks, world, &t) else { continue };
            let name = CREATURE_NAMES[world % CREATURE_NAMES.len()];
            let creature = sea.spawn(name, Form::Creature, pos, heading);
            if let Some(e) = sea.entity_mut(creature) {
                e.winding = winding_in_world(world, bearing_of(pos));
            }
            sv.creatures[world] = Some(creature);
        }
        announce_creature(sv, sea);
    }
    let view = sv.perspective(sea);
    sv.refresh_view(view);
}

/// The beam lives on the ship's spiral neighbourhood: same bearing, winding within half a turn of
/// the ship, so the world it reports is the one its light lands in.
pub fn rebase_beam(view: Perspective, sea: &mut Sea) {
    sea.beam.winding = view.winding + angle_delta(view.bearing, sea.beam.bearing());
}

/// Advance one step. Returns true when the voyage has ended.
pub fn step(sv: &mut SpiralVoyage, sea: &mut Sea, footprint: &Footprint, dt: f32) -> bool {
    let t = sea.tuning.clone();
    let n = sv.worlds.len();

    // The beam charges whichever world each footprint cell belongs to from the ship; every
    // world decays.
    let view = sv.perspective(sea);
    for (i, w) in sv.worlds.iter_mut().enumerate() {
        w.charge.step_where(Some(footprint), &t, dt, |p| view.world_at(p) == i);
    }

    let Some(ship_id) = sv.ship.or_else(|| sea.entities.iter().find(|e| e.form == Form::Ship).map(|e| e.id)) else {
        sv.refresh_view(view);
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
        let waters = SpiralWaters { worlds: &sv.worlds, view, sea_radius: t.sea_radius };
        let mut voyage_tuning = t.clone();
        voyage_tuning.ship_speed *= t.spiral_ship_speed_factor;
        steering::steer_ship(&mut sea.entities[idx], &waters, &voyage_tuning, dt);
    }
    let mut crossed = false;
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
        // Announce a crossing only once the hull is clearly past the seam.
        let upper = SEAM + (sv.ship_world + 1) as f32 * TAU + SEAM_BAND;
        let lower = SEAM + sv.ship_world as f32 * TAU - SEAM_BAND;
        if new_winding >= upper && sv.ship_world + 1 < n {
            sv.ship_world += 1;
            sea.events.push(Event::ShipCrossed { world: sv.ship_world as u8 });
            crossed = true;
        } else if new_winding < lower && sv.ship_world > 0 {
            sv.ship_world -= 1;
            sea.events.push(Event::ShipCrossed { world: sv.ship_world as u8 });
            crossed = true;
        }
    }
    let view = Perspective { winding: ship.winding, bearing: bearing_of(ship.pos), worlds: n };
    if crossed {
        announce_creature(sv, sea);
    }
    rebase_beam(view, sea);
    sv.refresh_view(view);

    let harbor = sea.harbor();
    let hull = sea.entities[idx].circle();
    if sv.ship_world == n - 1 && harbor.contains(hull.center) {
        sv.end = Some(VoyageEnd::Arrived);
        sea.secure(ship_id);
        sea.events.push(Event::VoyageArrived);
        return true;
    }
    // Grounding is against the land the ship sees: each rock in the world it resolves to.
    let struck = sv
        .worlds
        .iter()
        .enumerate()
        .flat_map(|(i, w)| w.rocks.iter().map(move |r| (i, r)))
        .find(|(i, r)| r.overlaps(&hull) && view.world_at(r.center) == *i);
    if let Some((_, rock)) = struck {
        sea.entities[idx].status = Status::Sunk;
        sv.end = Some(VoyageEnd::Grounded(rock.center));
        sea.events.push(Event::Sunk { id: ship_id, pos: hull.center, cause: Cause::Rock });
        return true;
    }
    // Predators: each hunts the plankton of its own world and takes the ship only while the ship
    // sees it in that world, exactly as grounding resolves land.
    for world in 0..sv.worlds.len() {
        let Some(creature_id) = sv.creatures[world] else { continue };
        let Some(ci) = sea.entities.iter().position(|e| e.id == creature_id) else { continue };
        let spiral_world = &mut sv.worlds[world];
        let creature = &mut sea.entities[ci];
        steering::steer_predator(creature, &mut spiral_world.charge, &spiral_world.rocks, &t, dt);
        // A predator belongs to one world: its winding follows its bearing inside that world.
        creature.winding = winding_in_world(world, bearing_of(creature.pos));
        let reach = Circle::new(creature.pos, t.creature_contact_radius);
        if view.world_at(creature.pos) == world && reach.overlaps(&hull) {
            let at = creature.pos;
            sea.entities[idx].status = Status::Sunk;
            sv.end = Some(VoyageEnd::Taken(at));
            sea.events.push(Event::Sunk { id: ship_id, pos: hull.center, cause: Cause::Creature });
            return true;
        }
    }
    false
}

pub fn outcome(sv: &SpiralVoyage, sea: &Sea) -> Outcome {
    let n = sv.worlds.len();
    let reached = sv.ship_world(sea).map(|w| w + 1).unwrap_or(1);
    let (success, headline) = match sv.end {
        Some(VoyageEnd::Arrived) => (true, "The Wayfarer came through all four worlds to harbor.".to_string()),
        Some(VoyageEnd::Grounded(at)) => {
            (false, format!("The Wayfarer struck the {} rocks in World {reached}.", compass_word(at)))
        }
        Some(VoyageEnd::Taken(at)) => {
            (false, format!("Something took the Wayfarer in the {} water of World {reached}.", compass_word(at)))
        }
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

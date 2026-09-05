//! Variant 1 — Night Watch: guide five ships home past small islands; one ship-sized predator
//! that eats the glow it finds.

use super::entity::{Form, Status};
use super::geom::Circle;
use super::islands;
use super::steering;
use super::tuning::Tuning;
use super::{Cause, Event, Outcome, Sea};
use glam::Vec2;

#[derive(Clone, Debug)]
pub struct Arrival {
    pub time: f32,
    pub name: &'static str,
    pub pos: Vec2,
    pub heading_deg: f32,
}

#[derive(Clone, Debug)]
pub struct NightWatch {
    pub schedule: Vec<Arrival>,
    pub next_arrival: usize,
    pub creature_spawn: (Vec2, f32),
    pub creature_id: Option<u32>,
    pub target_rescues: usize,
}

impl NightWatch {
    /// Fixed scenario: overlapping arrivals; a north reef, an eastern islet with a tail, a line of
    /// south-western skerries and a south-eastern rock pair shape the approaches. The harbor sits
    /// north of the lighthouse; the reef makes ships from the north round it either way.
    pub fn scenario(_t: &Tuning) -> (Self, Vec<Circle>) {
        let rocks = islands::land(vec![
            islands::arc(46.0, -20.0, 20.0, 3.5),
            islands::islet(Vec2::new(40.0, 18.0), 6.0),
            islands::chain(Vec2::new(46.0, 13.0), Vec2::new(4.0, -2.5), 3, 3.0),
            islands::chain(Vec2::new(-48.0, -34.0), Vec2::new(4.5, -4.5), 5, 3.2),
            islands::islet(Vec2::new(24.0, -46.0), 4.0),
            vec![Circle::new(Vec2::new(32.0, -40.0), 3.0), Circle::new(Vec2::new(-55.0, 14.0), 4.0)],
        ]);
        let schedule = vec![
            Arrival { time: 0.0, name: "Alder", pos: Vec2::new(0.0, 95.0), heading_deg: 180.0 },
            Arrival { time: 0.0, name: "Brant", pos: Vec2::new(95.0, 8.0), heading_deg: 270.0 },
            Arrival { time: 45.0, name: "Cormorant", pos: Vec2::new(-68.0, -66.0), heading_deg: 45.0 },
            Arrival { time: 80.0, name: "Dunlin", pos: Vec2::new(66.0, 68.0), heading_deg: 225.0 },
            Arrival { time: 108.0, name: "Eider", pos: Vec2::new(78.0, -54.0), heading_deg: 290.0 },
        ];
        (
            Self {
                schedule,
                next_arrival: 0,
                creature_spawn: (Vec2::new(-76.0, 28.0), 105.0),
                creature_id: None,
                target_rescues: 3,
            },
            rocks,
        )
    }
}

fn spawn_due(nw: &mut NightWatch, sea: &mut Sea, night_time: f32) {
    let max_active = sea.tuning.night_watch_max_active_ships;
    while let Some(a) = nw.schedule.get(nw.next_arrival) {
        let active = sea.entities.iter().filter(|e| e.is_active_ship()).count();
        if a.time > night_time || active >= max_active {
            break;
        }
        let id = sea.spawn(a.name, Form::Ship, a.pos, a.heading_deg.to_radians());
        // A new arrival is seen at the world's edge for a moment, so the player knows where to look.
        let (now, seconds) = (sea.time, sea.tuning.ship_arrival_reveal_seconds);
        if let Some(e) = sea.entity_mut(id) {
            e.surface(now, seconds);
        }
        sea.events.push(Event::ShipArrived { id, pos: a.pos });
        nw.next_arrival += 1;
    }
}

/// Spawn the dusk boats at session start (called by `World::new`).
pub fn dusk_boats(nw: &mut NightWatch, sea: &mut Sea) {
    spawn_due(nw, sea, 0.0);
}

pub fn step(nw: &mut NightWatch, sea: &mut Sea, dt: f32) {
    let t = sea.tuning.clone();
    let night_time = sea.time - t.intro_seconds;
    spawn_due(nw, sea, night_time);

    if t.night_watch_monster && nw.creature_id.is_none() && night_time >= t.night_watch_creature_activation {
        let (pos, heading) = nw.creature_spawn;
        let id = sea.spawn("Leviathan", Form::Creature, pos, heading.to_radians());
        // Its eyes are the sighting the "something stirs" cue points at; the body stays dark.
        sea.events.push(Event::CreatureAppears { id, pos });
        nw.creature_id = Some(id);
    }

    // Ships read the light and sail; groundings are lost rescues.
    for idx in 0..sea.entities.len() {
        if !sea.entities[idx].is_active_ship() {
            continue;
        }
        if let Some(cause) = sea.move_ship(idx, dt) {
            let e = &mut sea.entities[idx];
            e.status = Status::Sunk;
            sea.events.push(Event::Sunk { id: e.id, pos: e.pos, cause });
        }
    }

    // Predator: eats glow, sinks what it touches.
    if let Some(cid) = nw.creature_id {
        let land = sea.land_for(cid);
        if let Some(idx) = sea.entities.iter().position(|e| e.id == cid) {
            let Sea { entities, charge, .. } = sea;
            let (before, rest) = entities.split_at_mut(idx);
            let (creature, after) = rest.split_first_mut().unwrap();
            steering::steer_predator(creature, charge, &land, &t, dt);
            let reach = Circle::new(creature.pos, t.creature_contact_radius);
            for ship in before.iter_mut().chain(after.iter_mut()) {
                if ship.is_active_ship() && reach.overlaps(&ship.circle()) {
                    ship.status = Status::Sunk;
                    sea.events.push(Event::Sunk { id: ship.id, pos: ship.pos, cause: Cause::Creature });
                }
            }
        }
    }
}

pub fn outcome(nw: &NightWatch, sea: &Sea) -> Outcome {
    let ships: Vec<_> = sea.entities.iter().filter(|e| e.form == Form::Ship || e.status != Status::Active).collect();
    let rescued = ships.iter().filter(|e| e.status == Status::Secured).count();
    let sunk = ships.iter().filter(|e| e.status == Status::Sunk).count();
    let offshore = ships.iter().filter(|e| e.is_active_ship()).count();
    let never_arrived = nw.schedule.len().saturating_sub(nw.next_arrival);
    let total = nw.schedule.len();
    let mut details = vec![format!("{rescued} of {total} vessels rescued.")];
    if sunk > 0 {
        details.push(format!("{sunk} lost to rocks or the predator."));
    }
    if offshore + never_arrived > 0 {
        details.push(format!("{} still offshore at first light.", offshore + never_arrived));
    }
    for e in &ships {
        let state = match e.status {
            Status::Secured => "moored in harbor",
            Status::Sunk => "lost",
            Status::Active => "still at sea",
        };
        details.push(format!("{}: {state}", e.name));
    }
    let success = rescued >= nw.target_rescues;
    Outcome {
        success,
        headline: if success {
            "The night ends. The harbor is fuller than it was.".into()
        } else {
            "The night ends. Too few made it home.".into()
        },
        details,
        rescued,
        total,
    }
}

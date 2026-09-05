//! Variant 2 — Mutable Sea: three persistent identities cycle ship → wreck → creature → island
//! in darkness. Light pauses the timer; damage turns ships into wrecks.

use super::entity::{Entity, Form, Mutable, Status};
use super::geom::Circle;
use super::steering;
use super::{Cause, Event, Outcome, Sea};
use glam::Vec2;

#[derive(Clone, Debug)]
pub struct Identity {
    pub name: &'static str,
    pub form: Form,
    pub pos: Vec2,
    pub heading_deg: f32,
    /// Seconds of darkness already accumulated in the starting form.
    pub progress: f32,
}

#[derive(Clone, Debug)]
pub struct MutableSea {
    pub identities: Vec<Identity>,
    pub target_rescues: usize,
}

impl MutableSea {
    pub fn scenario() -> (Self, Vec<Circle>) {
        let rocks = vec![
            Circle::new(Vec2::new(30.0, 40.0), 5.0),
            Circle::new(Vec2::new(-40.0, 20.0), 4.5),
            Circle::new(Vec2::new(24.0, -52.0), 4.0),
            Circle::new(Vec2::new(-14.0, 62.0), 4.0),
            Circle::new(Vec2::new(56.0, 32.0), 4.5),
        ];
        let identities = vec![
            Identity { name: "Kestrel", form: Form::Ship, pos: Vec2::new(-62.0, 52.0), heading_deg: 150.0, progress: 0.0 },
            Identity { name: "Merlin", form: Form::Ship, pos: Vec2::new(72.0, -18.0), heading_deg: 262.0, progress: 3.0 },
            Identity { name: "Osprey", form: Form::Creature, pos: Vec2::new(-24.0, -58.0), heading_deg: 20.0, progress: 6.0 },
        ];
        (Self { identities, target_rescues: 2 }, rocks)
    }
}

pub fn populate(ms: &MutableSea, sea: &mut Sea) {
    for i in &ms.identities {
        let id = sea.spawn(i.name, i.form, i.pos, i.heading_deg.to_radians());
        if let Some(e) = sea.entity_mut(id) {
            e.mutable = Some(Mutable { progress: i.progress, deferred: false });
        }
    }
}

pub fn dark_duration(form: Form, sea: &Sea) -> f32 {
    sea.tuning.mutable_dark_durations[form.index()]
}

/// Fraction of the way to the next form; presentation uses this for the instability cue.
pub fn instability(e: &Entity, sea: &Sea) -> f32 {
    e.mutable
        .map(|m| (m.progress / dark_duration(e.form, sea)).clamp(0.0, 1.0))
        .unwrap_or(0.0)
}

fn wreck(sea: &mut Sea, idx: usize, cause: Cause) {
    let t = sea.tuning.clone();
    let e = &mut sea.entities[idx];
    e.set_form(Form::Wreck, &t);
    if let Some(m) = &mut e.mutable {
        *m = Mutable::default();
    }
    sea.events.push(Event::Wrecked { id: e.id, pos: e.pos, cause });
}

pub fn step(_ms: &mut MutableSea, sea: &mut Sea, dt: f32) {
    let t = sea.tuning.clone();

    // Transformation timers: darkness advances, preserving light pauses.
    for idx in 0..sea.entities.len() {
        let e = &sea.entities[idx];
        if !e.is_active() || e.mutable.is_none() {
            continue;
        }
        let preserved = sea.is_preserved(e.pos);
        let duration = dark_duration(e.form, sea);
        let e = &mut sea.entities[idx];
        let m = e.mutable.as_mut().unwrap();
        if !preserved {
            m.progress = (m.progress + dt).min(duration);
        }
        if m.progress < duration {
            continue;
        }
        // Due: defer while the new footprint would appear through another entity.
        let next = e.form.next();
        let placement = Circle::new(e.pos, next.radius(&t));
        let id = e.id;
        let blocked = sea
            .entities
            .iter()
            .any(|o| o.id != id && o.is_active() && o.circle().overlaps(&placement));
        let e = &mut sea.entities[idx];
        let m = e.mutable.as_mut().unwrap();
        if blocked {
            m.deferred = true;
            continue;
        }
        let from = e.form;
        e.set_form(next, &t);
        e.mutable = Some(Mutable::default());
        sea.events.push(Event::Transformed { id, pos: e.pos, from, to: next });
    }

    // Ship motion, rescue, groundings.
    for idx in 0..sea.entities.len() {
        if !sea.entities[idx].is_active_ship() {
            continue;
        }
        if let Some(cause) = sea.move_ship(idx, dt) {
            // Back the hull off the obstruction so the wreck sits beside it, not inside it.
            let e = &mut sea.entities[idx];
            e.pos -= super::geom::dir(e.heading) * t.ship_speed * dt * 2.0;
            wreck(sea, idx, cause);
        }
    }

    // Creatures: follow light, threaten ships by contact.
    let creature_ids: Vec<u32> = sea
        .entities
        .iter()
        .filter(|e| e.is_active() && e.form == Form::Creature)
        .map(|e| e.id)
        .collect();
    for cid in creature_ids {
        let lights = steering::collect_lights(&sea.guidance, &sea.charge, &sea.entities, &t);
        let land = sea.land_for(cid);
        let Some(idx) = sea.entities.iter().position(|e| e.id == cid) else { continue };
        let creature = &mut sea.entities[idx];
        steering::steer_creature(creature, &lights, &land, &t, t.creature_speed, t.creature_detect_radius, dt);
        let reach = Circle::new(creature.pos, t.creature_contact_radius);
        let victims: Vec<usize> = sea
            .entities
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_active_ship() && reach.overlaps(&s.circle()))
            .map(|(i, _)| i)
            .collect();
        for v in victims {
            wreck(sea, v, Cause::Creature);
        }
    }
}

pub fn outcome(ms: &MutableSea, sea: &Sea) -> Outcome {
    let secured = sea.entities.iter().filter(|e| e.status == Status::Secured).count();
    let total = ms.identities.len();
    let mut details = vec![format!("{secured} of {total} identities secured as ships.")];
    for e in &sea.entities {
        details.push(match e.status {
            Status::Secured => format!("{}: moored in harbor", e.name),
            _ => format!("{}: fixed at dawn as {} {}", e.name, article(e.form), e.form.name()),
        });
    }
    let success = secured >= ms.target_rescues;
    Outcome {
        success,
        headline: if success {
            "First light fixes the sea. Enough came home as themselves.".into()
        } else {
            "First light fixes the sea. What remained offshore stays as it is.".into()
        },
        details,
        rescued: secured,
        total,
    }
}

fn article(form: Form) -> &'static str {
    match form {
        Form::Island => "an",
        _ => "a",
    }
}

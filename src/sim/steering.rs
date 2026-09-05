//! Ship guidance along painted samples and the single creature behaviour model.

use super::charge::ChargeField;
use super::entity::{Entity, EntityId, Target};
use super::geom::{bearing_of, dir, resolve_circle, turn_toward, Circle};
use super::guidance::Guidance;
use super::tuning::Tuning;
use glam::Vec2;

/// Ahead-of-vessel acceptance: samples up to 75° off the bow can be turned toward.
const AHEAD_COS: f32 = 0.26;

/// Choose the next painted sample for a ship, or keep its heading when none is usable.
pub fn steer_ship(
    e: &mut Entity,
    guidance: &Guidance,
    field: &ChargeField,
    t: &Tuning,
    time: f32,
    dt: f32,
) {
    e.brain.visited.retain(|(_, until)| *until > time);
    let fwd = dir(e.heading);
    let pass_sq = (t.ship_reach_radius * 1.5) * (t.ship_reach_radius * 1.5);
    let spacing_sq = t.sample_spacing * t.sample_spacing;

    // Every sample the hull passes counts as reached, so a route is consumed, never orbited.
    for s in &guidance.samples {
        let to = s.pos - e.pos;
        let passed = to.length_squared() <= pass_sq || (to.dot(fwd) < 0.0 && to.length_squared() < pass_sq * 4.0);
        if passed && !e.brain.visited.iter().any(|(v, _)| v.distance_squared(s.pos) < spacing_sq) {
            e.brain.visited.push((s.pos, time + t.sample_revisit_delay));
        }
    }

    if let Some(Target::Sample(p)) = e.brain.target {
        let visited = e.brain.visited.iter().any(|(v, _)| v.distance_squared(p) < spacing_sq);
        let usable = field.charge_at(p) >= t.usable_sample_threshold;
        if visited || !usable {
            e.brain.target = None;
        }
    }

    if e.brain.target.is_none() {
        let mut best: Option<(f32, Vec2)> = None;
        for (s, charge) in guidance.usable(field, t) {
            let to = s.pos - e.pos;
            let dist = to.length();
            if dist > t.ship_look_distance || dist < 1e-3 {
                continue;
            }
            let ahead = to.dot(fwd) / dist;
            if ahead < AHEAD_COS {
                continue;
            }
            if e.brain.visited.iter().any(|(v, _)| v.distance_squared(s.pos) < spacing_sq) {
                continue;
            }
            // Nearest sample ahead wins; brightness breaks ties at intersections.
            let score = (1.0 - dist / t.ship_look_distance) * 1.5 + ahead * 0.4 + (charge / t.charge_cap) * 0.6;
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((score, s.pos));
            }
        }
        e.brain.target = best.map(|(_, p)| Target::Sample(p));
    }

    if let Some(Target::Sample(p)) = e.brain.target {
        let want = bearing_of(p - e.pos);
        e.heading = turn_toward(e.heading, want, t.ship_turn_rate_deg.to_radians() * dt);
    }
    e.pos += dir(e.heading) * t.ship_speed * dt;
}

/// Follow an authored waypoint list (World Weaver voyage). Returns true when the final waypoint is reached.
pub fn steer_along_route(e: &mut Entity, route: &[Vec2], speed: f32, turn_rate_deg: f32, dt: f32) -> bool {
    let idx = match e.brain.target {
        Some(Target::Waypoint(i)) => i,
        _ => 0,
    };
    if idx >= route.len() {
        return true;
    }
    let goal = route[idx];
    let to = goal - e.pos;
    if to.length() <= speed * dt * 1.5 + 0.5 {
        if idx + 1 >= route.len() {
            e.pos = goal;
            e.brain.target = Some(Target::Waypoint(route.len()));
            return true;
        }
        e.brain.target = Some(Target::Waypoint(idx + 1));
    } else {
        e.brain.target = Some(Target::Waypoint(idx));
    }
    let want = bearing_of(to);
    e.heading = turn_toward(e.heading, want, turn_rate_deg.to_radians() * dt);
    e.pos += dir(e.heading) * speed * dt;
    false
}

/// A light the creature can perceive.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub pos: Vec2,
    pub brightness: f32,
    pub lantern: Option<EntityId>,
}

/// Collect every light source the creature may respond to. Plankton counts only when strong.
pub fn collect_lights(guidance: &Guidance, field: &ChargeField, ships: &[Entity], t: &Tuning) -> Vec<Light> {
    let mut lights: Vec<Light> = guidance
        .samples
        .iter()
        .filter_map(|s| {
            let c = field.charge[s.cell];
            (c >= t.strong_threshold).then_some(Light {
                pos: s.pos,
                brightness: c,
                lantern: None,
            })
        })
        .collect();
    lights.extend(ships.iter().filter(|s| s.is_active() && s.lantern).map(|s| Light {
        pos: s.pos,
        brightness: t.lantern_brightness,
        lantern: Some(s.id),
    }));
    lights
}

fn light_score(from: Vec2, l: &Light) -> f32 {
    l.brightness / (1.0 + from.distance(l.pos) / 20.0)
}

/// Move a creature toward the strongest detectable light with target hysteresis.
pub fn steer_creature(e: &mut Entity, lights: &[Light], land: &[Circle], t: &Tuning, speed: f32, detect: f32, dt: f32) {
    let detect_sq = detect * detect;
    let current = match e.brain.target {
        Some(Target::Sample(p)) => lights
            .iter()
            .find(|l| l.lantern.is_none() && l.pos.distance_squared(p) < 0.25),
        Some(Target::Lantern(id)) => lights.iter().find(|l| l.lantern == Some(id)),
        _ => None,
    }
    .filter(|l| l.pos.distance_squared(e.pos) <= detect_sq)
    .copied();

    let best = lights
        .iter()
        .filter(|l| l.pos.distance_squared(e.pos) <= detect_sq)
        .max_by(|a, b| light_score(e.pos, a).total_cmp(&light_score(e.pos, b)))
        .copied();

    let chosen = match (current, best) {
        (Some(c), Some(b)) => {
            if light_score(e.pos, &b) > light_score(e.pos, &c) * t.creature_stickiness {
                Some(b)
            } else {
                Some(c)
            }
        }
        (None, b) => b,
        (c, None) => c,
    };

    if let Some(l) = chosen {
        e.brain.target = Some(match l.lantern {
            Some(id) => Target::Lantern(id),
            None => Target::Sample(l.pos),
        });
        let to = l.pos - e.pos;
        let dist = to.length();
        e.heading = turn_toward(e.heading, bearing_of(to), t.creature_turn_rate_deg.to_radians() * dt);
        // Hover on top of a reached patch instead of overshooting and oscillating.
        let step = (speed * dt).min(dist.max(0.0));
        if dist > 0.8 {
            e.pos += dir(e.heading) * step;
        }
    } else {
        // Nothing detectable: drift slowly inward from the rim, then prowl a wide circle around
        // the island so it stays in the world the player is watching.
        let r = e.pos.length();
        let inward = bearing_of(-e.pos);
        let want = if r > 55.0 { inward } else { inward + std::f32::consts::FRAC_PI_2 };
        e.heading = turn_toward(e.heading, want, t.creature_turn_rate_deg.to_radians() * 0.4 * dt);
        e.pos += dir(e.heading) * speed * 0.4 * dt;
    }

    slide_off_land(e, land, speed * dt);
}

/// Land blocks the creature: push out along the contact normal, then slide tangentially so a
/// head-on approach walks around the obstruction instead of sticking to it.
pub fn slide_off_land(e: &mut Entity, land: &[Circle], step: f32) -> bool {
    let mut touched = false;
    for c in land {
        let (p, hit) = resolve_circle(e.pos, e.radius, c);
        if !hit {
            continue;
        }
        touched = true;
        let n = (p - c.center).normalize_or_zero();
        let d = dir(e.heading);
        // Tangent closest to the intended direction; on a perfectly head-on hit pick clockwise.
        let mut tangent = d - n * d.dot(n);
        if tangent.length_squared() < 1e-4 {
            tangent = Vec2::new(n.y, -n.x);
        }
        e.pos = p + tangent.normalize() * step;
        // Re-project so the slide never tunnels into the circle.
        e.pos = resolve_circle(e.pos, e.radius, c).0;
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::beam::{Beam, FootprintKind};
    use crate::sim::entity::Form;

    fn paint_line(g: &mut Guidance, f: &mut ChargeField, t: &Tuning, from: Vec2, to: Vec2, secs: f32) {
        // Emulate dwelling the footprint along a line by stamping charge and samples.
        let n = ((to - from).length() / 2.0).ceil() as usize;
        for i in 0..=n {
            let p = from.lerp(to, i as f32 / n as f32);
            let idx = f.index_of(p).unwrap();
            f.charge[idx] = secs.min(t.charge_cap);
            g.paint(p, f, t);
        }
    }

    #[test]
    fn ship_follows_a_painted_turn_and_does_not_circle() {
        let t = Tuning::default();
        let mut field = ChargeField::new(&t, &[]);
        let mut g = Guidance::default();
        // Ship heading south from the north; route turns it west.
        let mut ship = Entity::new(1, "Test", Form::Ship, Vec2::new(0.0, 60.0), std::f32::consts::PI, &t);
        paint_line(&mut g, &mut field, &t, Vec2::new(0.0, 50.0), Vec2::new(0.0, 35.0), 25.0);
        paint_line(&mut g, &mut field, &t, Vec2::new(0.0, 35.0), Vec2::new(-30.0, 35.0), 25.0);
        let dt = 1.0 / 60.0;
        let mut time = 0.0;
        let mut min_x = f32::MAX;
        for _ in 0..(60.0 * 40.0) as usize {
            steer_ship(&mut ship, &g, &field, &t, time, dt);
            time += dt;
            min_x = min_x.min(ship.pos.x);
        }
        // It turned west along the route and then kept going past the end.
        assert!(min_x < -35.0, "ship never followed the westward leg: {:?}", ship.pos);
        assert!((ship.pos.y - 35.0).abs() < 6.0, "ship left the route line: {:?}", ship.pos);
        assert!(ship.pos.x < -35.0, "ship circled back instead of continuing: {:?}", ship.pos);
    }

    #[test]
    fn stationary_patch_is_passed_not_orbited() {
        let t = Tuning::default();
        let mut field = ChargeField::new(&t, &[]);
        let mut g = Guidance::default();
        let mut ship = Entity::new(1, "Test", Form::Ship, Vec2::new(0.0, 60.0), std::f32::consts::PI, &t);
        let beacon = Vec2::new(6.0, 40.0);
        let beacon_cell = field.index_of(beacon).unwrap();
        field.charge[beacon_cell] = t.charge_cap;
        g.paint(beacon, &field, &t);
        let dt = 1.0 / 60.0;
        let mut time = 0.0;
        let mut passed = false;
        for _ in 0..(60.0 * 30.0) as usize {
            steer_ship(&mut ship, &g, &field, &t, time, dt);
            time += dt;
            if ship.pos.distance(beacon) < t.ship_reach_radius {
                passed = true;
            }
        }
        assert!(passed);
        assert!(ship.pos.distance(beacon) > 20.0, "ship stayed near the beacon: {:?}", ship.pos);
    }

    #[test]
    fn creature_prefers_bright_lure_over_faint_lantern_and_sticks() {
        let t = Tuning::default();
        let mut field = ChargeField::new(&t, &[]);
        let mut g = Guidance::default();
        let lure = Vec2::new(30.0, 0.0);
        let lure_cell = field.index_of(lure).unwrap();
        field.charge[lure_cell] = t.charge_cap;
        g.paint(lure, &field, &t);
        let ship = Entity::new(2, "S", Form::Ship, Vec2::new(-30.0, 0.0), 0.0, &t);
        let mut c = Entity::new(1, "C", Form::Creature, Vec2::new(0.0, 0.0), 0.0, &t);
        let lights = collect_lights(&g, &field, &[ship], &t);
        for _ in 0..900 {
            steer_creature(&mut c, &lights, &[], &t, t.creature_speed, t.creature_detect_radius, 1.0 / 60.0);
        }
        assert!(c.pos.x > 20.0, "creature ignored the lure: {:?}", c.pos);
        let ship = Entity::new(2, "S", Form::Ship, Vec2::new(-30.0, 0.0), 0.0, &t);
        let mut c = Entity::new(1, "C", Form::Creature, Vec2::new(0.0, 0.0), 0.0, &t);
        let dark_field = ChargeField::new(&t, &[]);
        let lights = collect_lights(&g, &dark_field, &[ship], &t);
        for _ in 0..900 {
            steer_creature(&mut c, &lights, &[], &t, t.creature_speed, t.creature_detect_radius, 1.0 / 60.0);
        }
        assert!(c.pos.x < -20.0, "creature ignored the lantern: {:?}", c.pos);
    }

    #[test]
    fn creature_slides_along_land() {
        let t = Tuning::default();
        let rock = Circle::new(Vec2::new(0.0, 15.0), 5.0);
        let mut c = Entity::new(1, "C", Form::Creature, Vec2::new(0.0, 0.0), 0.0, &t);
        let lights = [Light {
            pos: Vec2::new(0.0, 40.0),
            brightness: 30.0,
            lantern: None,
        }];
        for _ in 0..(60 * 30) {
            steer_creature(&mut c, &lights, &[rock], &t, t.creature_speed, t.creature_detect_radius, 1.0 / 60.0);
            assert!(!rock.overlaps(&Circle::new(c.pos, c.radius - 0.05)), "creature entered land at {:?}", c.pos);
        }
        assert!(c.pos.y > 30.0, "creature stuck on rock: {:?}", c.pos);
    }

    #[test]
    fn beam_footprint_charge_makes_samples_usable() {
        let t = Tuning::default();
        let mut field = ChargeField::new(&t, &[]);
        let mut g = Guidance::default();
        let b = Beam::new(FootprintKind::Spot, &t);
        let fp = b.footprint(&t);
        for _ in 0..60 {
            field.step(Some(&fp), &t, 1.0 / 60.0);
            g.paint(fp.center(), &field, &t);
        }
        assert_eq!(g.samples.len(), 1);
        assert_eq!(g.usable(&field, &t).count(), 1);
    }
}

//! Light-reading ship guidance (Modes 1 and 4), the plankton-eating predator, and route following.
//!
//! Ships scan a forward arc for the most promising *corridor* of charged water several ship
//! lengths ahead. Intent (`Brain::desired`) is chosen with hysteresis; the hull turns toward it
//! gradually. No maze solving: a dark sea means the ship keeps its last accepted heading.

use super::charge::ChargeField;
use super::entity::{Entity, Target};
use super::geom::{angle_delta, bearing_of, dir, resolve_circle, turn_toward, Circle};
use super::tuning::Tuning;
use glam::Vec2;

/// What a vessel can perceive of the water around it. One charge field in Modes 1 and 3; the
/// spiral resolves positions near the seam into the neighbouring world.
pub trait Waters {
    fn charge_at(&self, p: Vec2) -> f32;
    fn is_land(&self, p: Vec2) -> bool;
}

impl Waters for ChargeField {
    fn charge_at(&self, p: Vec2) -> f32 {
        ChargeField::charge_at(self, p)
    }
    fn is_land(&self, p: Vec2) -> bool {
        ChargeField::is_land(self, p)
    }
}

/// Number of candidate headings across the forward arc.
const CANDIDATES: usize = 15;

/// One candidate corridor: summed weighted charge and the number of lit samples.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corridor {
    pub score: f32,
    pub lit: u32,
}

impl Corridor {
    /// Score of one sample's worth of this corridor's light: a difference smaller than this is
    /// sampling noise (a clipped cell edge, the cosine-shortening of an angled corridor at a
    /// trail's end), never a meaningfully better direction.
    fn sample_noise(&self) -> f32 {
        if self.lit == 0 {
            0.0
        } else {
            1.5 * self.score / self.lit as f32
        }
    }
}

/// Corridor for one heading: summed charge along a lookahead corridor that starts *beyond the
/// hull* (the cell the ship sits in is never "ahead"), slightly favouring the near end so a turn
/// can still be executed. A corridor counts as illumination only if it is lit at two or more
/// samples (a clipped cell corner is noise) and its light reaches the tuned minimum distance (a
/// patch ending under the bow is being passed, not pointing anywhere). `None` when the corridor
/// runs into land within the immediate-obstacle distance (a rejection, not a detour).
pub fn corridor(from: Vec2, heading: f32, waters: &impl Waters, t: &Tuning) -> Option<Corridor> {
    let d = dir(heading);
    let step = t.ship_length / 3.0;
    let first = (t.ship_length / step).round() as usize;
    let count = (t.guidance_lookahead() / step).ceil() as usize;
    let reject_within = t.guidance_obstacle_lengths * t.ship_length + t.ship_radius;
    let mut score = 0.0;
    let mut lit = 0;
    let mut reach = 0.0;
    for k in first..=count {
        let dist = k as f32 * step;
        let p = from + d * dist;
        if t.guidance_obstacle_rejection && dist <= reject_within && waters.is_land(p) {
            return None;
        }
        let c = waters.charge_at(p);
        if c >= t.silhouette_min_glow {
            lit += 1;
            reach = dist;
        }
        let near_weight = 1.0 + 0.5 * (1.0 - k as f32 / count as f32);
        score += c * near_weight;
    }
    let useful = lit >= 2 && reach >= t.guidance_min_reach_lengths * t.ship_length;
    Some(if useful { Corridor { score, lit } } else { Corridor::default() })
}

/// Reconsider intent at the tuned rate, then turn the hull toward it and move forward.
pub fn steer_ship(e: &mut Entity, waters: &impl Waters, t: &Tuning, dt: f32) {
    if e.brain.desired.is_nan() {
        e.brain.desired = e.heading;
    }
    e.brain.since_eval += dt;
    if e.brain.since_eval >= 1.0 / t.guidance_hz {
        e.brain.since_eval = 0.0;
        let half_arc = t.guidance_arc_deg.to_radians() * 0.5;
        let step = 2.0 * half_arc / (CANDIDATES - 1) as f32;
        let mut best: Option<(Corridor, f32)> = None;
        for i in 0..CANDIDATES {
            let offset = -half_arc + step * i as f32;
            let heading = (e.heading + offset).rem_euclid(std::f32::consts::TAU);
            let Some(c) = corridor(e.pos, heading, waters, t) else { continue };
            if best.is_none_or(|(b, _)| c.score > b.score) {
                best = Some((c, heading));
            }
        }
        // The incumbent is the current intent judged against the *current* light: a dead trail
        // loses its priority; a live one keeps it until a competitor is meaningfully better.
        let incumbent = if angle_delta(e.heading, e.brain.desired).abs() <= half_arc {
            corridor(e.pos, e.brain.desired, waters, t).unwrap_or_default()
        } else {
            Corridor::default()
        };
        let incumbent_live = incumbent.score >= t.guidance_min_score;
        e.brain.desired_score = if incumbent_live { incumbent.score } else { 0.0 };
        let mut challenger_persists = false;
        if let Some((c, heading)) = best {
            let better = c.score >= t.guidance_min_score
                && (!incumbent_live
                    || (c.score > incumbent.score * (1.0 + t.guidance_switch_advantage)
                        && c.score - incumbent.score > incumbent.sample_noise()));
            if better {
                // A dead incumbent is replaced at once; a live one only by a challenger that
                // keeps winning for the dwell (crossing corridors jitter as the ship moves).
                let same = !e.brain.challenger.is_nan() && angle_delta(e.brain.challenger, heading).abs() <= step * 1.01;
                e.brain.challenger_for = if same { e.brain.challenger_for + 1.0 / t.guidance_hz } else { 0.0 };
                e.brain.challenger = heading;
                challenger_persists = true;
                if !incumbent_live || e.brain.challenger_for >= t.guidance_dwell {
                    e.brain.desired = heading;
                    e.brain.desired_score = c.score;
                    challenger_persists = false;
                }
            }
        }
        if !challenger_persists {
            e.brain.challenger = f32::NAN;
            e.brain.challenger_for = 0.0;
        }
        // Nothing useful anywhere ahead: hold the last accepted heading.
    }
    e.heading = turn_toward(e.heading, e.brain.desired, t.ship_turn_rate_deg.to_radians() * dt);
    e.pos += dir(e.heading) * t.ship_speed * dt;
}

/// Follow a computed waypoint list (World Weaver playback). Returns true at the final waypoint.
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
    e.brain.desired = bearing_of(to);
    e.heading = turn_toward(e.heading, e.brain.desired, turn_rate_deg.to_radians() * dt);
    e.pos += dir(e.heading) * speed * dt;
    false
}

/// Ship-sized plankton eater: heads for the strongest stored glow within a finite radius, with
/// target persistence, and eats the charge it passes over. Nothing else attracts it.
pub fn steer_predator(e: &mut Entity, field: &mut ChargeField, land: &[Circle], t: &Tuning, dt: f32) {
    let min = t.silhouette_min_glow * 2.0;
    let score = |p: Vec2, c: f32| c / (1.0 + p.distance(e.pos) / 20.0);
    let current = match e.brain.target {
        Some(Target::Patch(p)) => {
            let c = field.charge_at(p);
            (c >= min && p.distance(e.pos) <= t.creature_detect_radius).then_some((p, c))
        }
        _ => None,
    };
    let best = field.strongest_within(e.pos, t.creature_detect_radius, min);
    let chosen = match (current, best) {
        (Some((cp, cc)), Some((bp, bc))) if score(bp, bc) > score(cp, cc) * t.creature_stickiness => Some(bp),
        (Some((cp, _)), _) => Some(cp),
        (None, Some((bp, _))) => Some(bp),
        (None, None) => None,
    };

    let turn = t.creature_turn_rate_deg.to_radians() * dt;
    match chosen {
        Some(p) => {
            e.brain.target = Some(Target::Patch(p));
            let to = p - e.pos;
            let dist = to.length();
            e.heading = turn_toward(e.heading, bearing_of(to), turn);
            if dist > 0.8 {
                e.pos += dir(e.heading) * (t.creature_speed * dt).min(dist);
            }
        }
        None => {
            // No detectable glow: cruise slowly and predictably, staying in the playable sea.
            e.brain.target = None;
            if e.pos.length() > t.sea_radius - 6.0 {
                e.heading = turn_toward(e.heading, bearing_of(-e.pos) + std::f32::consts::FRAC_PI_4, turn * 0.5);
            }
            e.pos += dir(e.heading) * t.creature_speed * 0.5 * dt;
        }
    }
    slide_off_land(e, land, t.creature_speed * dt);
    field.consume(e.pos, e.radius + 0.6, t.creature_consume_rate * dt);
}

/// Land blocks movement: push out along the contact normal, then slide tangentially so a head-on
/// approach walks around the obstruction instead of sticking to it.
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
        let mut tangent = d - n * d.dot(n);
        if tangent.length_squared() < 1e-4 {
            tangent = Vec2::new(n.y, -n.x);
        }
        e.pos = p + tangent.normalize() * step;
        e.pos = resolve_circle(e.pos, e.radius, c).0;
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::entity::Form;
    use std::f32::consts::PI;

    fn paint(f: &mut ChargeField, from: Vec2, to: Vec2, charge: f32) {
        let n = ((to - from).length() / 1.0).ceil() as usize;
        for i in 0..=n {
            let p = from.lerp(to, i as f32 / n as f32);
            if let Some(idx) = f.index_of(p) {
                f.charge[idx] = f.charge[idx].max(charge);
            }
        }
    }

    fn run(ship: &mut Entity, f: &ChargeField, t: &Tuning, seconds: f32, mut each: impl FnMut(&Entity)) {
        for _ in 0..(seconds * 60.0) as usize {
            steer_ship(ship, f, t, 1.0 / 60.0);
            each(ship);
        }
    }

    #[test]
    fn ship_notices_a_trail_several_lengths_ahead_and_turns_onto_it() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        // Ship heading south; a moderately bright trail starts 5 ship lengths ahead-left and runs west.
        let mut ship = Entity::new(1, "S", Form::Ship, Vec2::new(0.0, 60.0), PI, &t);
        let start = Vec2::new(-3.0, 60.0 - 5.0 * t.ship_length);
        paint(&mut f, start, start + Vec2::new(-40.0, 0.0), 10.0);
        // Measure line-keeping while still on the trail (x = -40); past its end the sea is dark.
        let mut min_x = f32::MAX;
        let mut y_near_end = f32::NAN;
        run(&mut ship, &f, &t, 40.0, |s| {
            min_x = min_x.min(s.pos.x);
            if y_near_end.is_nan() && s.pos.x < -40.0 {
                y_near_end = s.pos.y;
            }
        });
        assert!(min_x < -30.0, "ship did not follow the trail west: {:?}", ship.pos);
        assert!((y_near_end - start.y).abs() < 3.0, "ship left the trail line: y = {y_near_end}");
    }

    #[test]
    fn sustained_trail_beats_isolated_bright_dot() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        let mut ship = Entity::new(1, "S", Form::Ship, Vec2::new(0.0, 60.0), PI, &t);
        // Saturated dot to the right, moderate continuous trail to the left.
        let dot = Vec2::new(8.0, 48.0);
        let idx = f.index_of(dot).unwrap();
        f.charge[idx] = t.charge_cap;
        paint(&mut f, Vec2::new(-4.0, 52.0), Vec2::new(-30.0, 30.0), 9.0);
        run(&mut ship, &f, &t, 25.0, |_| {});
        assert!(ship.pos.x < -10.0, "ship chased the dot instead of the trail: {:?}", ship.pos);
    }

    #[test]
    fn nearly_equal_competitor_does_not_displace_incumbent() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        // Ship and fork sit on a cell centre so neither branch is favoured by grid alignment.
        let mut ship = Entity::new(1, "S", Form::Ship, Vec2::new(1.0, 60.0), PI, &t);
        // Two trails diverge ahead: left slightly brighter, right almost equal.
        paint(&mut f, Vec2::new(1.0, 52.0), Vec2::new(-24.0, 25.0), 10.0);
        paint(&mut f, Vec2::new(1.0, 52.0), Vec2::new(26.0, 25.0), 9.0);
        // Before the fork both branches are only seen through corridors that cross them, so the
        // decision is still being made; once the hull is past the fork point the accepted branch
        // must hold against its near-equal neighbour.
        let mut switches = 0;
        let mut last = f32::NAN;
        run(&mut ship, &f, &t, 20.0, |s| {
            if s.pos.y < 52.0 && !last.is_nan() && angle_delta(last, s.brain.desired).abs() > 0.4 {
                switches += 1;
            }
            if s.pos.y < 52.0 {
                last = s.brain.desired;
            }
        });
        assert_eq!(switches, 0, "intent switched branches after the fork");
        assert!(ship.pos.x < -7.0, "ship should have committed to the brighter left trail: {:?}", ship.pos);
    }

    #[test]
    fn passed_bright_spot_does_not_pull_the_ship_back() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        let mut ship = Entity::new(1, "S", Form::Ship, Vec2::new(0.0, 60.0), PI, &t);
        paint(&mut f, Vec2::new(0.0, 50.0), Vec2::new(0.0, 44.0), t.charge_cap);
        let mut passed = false;
        run(&mut ship, &f, &t, 30.0, |s| {
            if s.pos.y < 44.0 {
                passed = true;
            }
        });
        assert!(passed);
        // 30 s at ship speed is exactly the 60 units from start to y = 0: any lingering shows.
        assert!(ship.pos.y < 1.0, "ship lingered near the patch: {:?}", ship.pos);
        assert!(angle_delta(ship.heading, PI).abs() < 0.3, "ship turned around: {}", ship.heading.to_degrees());
    }

    #[test]
    fn dark_sea_keeps_last_accepted_heading() {
        let t = Tuning::default();
        let f = ChargeField::new(&t, &[]);
        let mut ship = Entity::new(1, "S", Form::Ship, Vec2::new(0.0, 60.0), 2.0, &t);
        run(&mut ship, &f, &t, 10.0, |_| {});
        assert_eq!(ship.heading, 2.0);
        assert_eq!(ship.brain.desired, 2.0);
    }

    #[test]
    fn corridor_into_land_is_rejected_not_detoured() {
        let t = Tuning::default();
        let rock = Circle::new(Vec2::new(0.0, 50.0), 4.0);
        let mut f = ChargeField::new(&t, &[rock]);
        // Bright trail straight through the rock: the heading into it is rejected; the ship
        // holds course rather than inventing a detour when nothing else is lit.
        paint(&mut f, Vec2::new(0.0, 58.0), Vec2::new(0.0, 40.0), 20.0);
        let mut ship = Entity::new(1, "S", Form::Ship, Vec2::new(0.0, 56.0), PI, &t);
        assert!(corridor(ship.pos, PI, &f, &t).is_none());
        run(&mut ship, &f, &t, 1.0, |_| {});
        assert!(angle_delta(ship.heading, PI).abs() < 0.5);
    }

    #[test]
    fn predator_goes_for_the_brighter_decoy_and_eats_it() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        let route = Vec2::new(20.0, 0.0);
        let decoy = Vec2::new(-20.0, 0.0);
        let (ri, di) = (f.index_of(route).unwrap(), f.index_of(decoy).unwrap());
        f.charge[ri] = 10.0;
        f.charge[di] = 28.0;
        let mut p = Entity::new(1, "P", Form::Creature, Vec2::ZERO, 0.0, &t);
        for _ in 0..(60 * 15) {
            steer_predator(&mut p, &mut f, &[], &t, 1.0 / 60.0);
        }
        assert!(p.pos.x < -15.0, "predator ignored the decoy: {:?}", p.pos);
        assert!(f.charge_at(decoy) < 28.0 * 0.5, "decoy was not eaten: {}", f.charge_at(decoy));
        assert_eq!(f.charge_at(route), 10.0, "untouched route lost charge");
    }

    #[test]
    fn predator_consumes_a_road_locally_as_it_travels() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        paint(&mut f, Vec2::new(-30.0, 0.0), Vec2::new(30.0, 0.0), 20.0);
        let mut p = Entity::new(1, "P", Form::Creature, Vec2::new(-30.0, 6.0), 0.0, &t);
        for _ in 0..(60 * 20) {
            steer_predator(&mut p, &mut f, &[], &t, 1.0 / 60.0);
        }
        let eaten = f.charge_at(Vec2::new(-30.0, 0.0));
        let far = f.charge_at(Vec2::new(30.0, 0.0));
        assert!(eaten < 20.0, "start of the road not consumed");
        assert!(far > eaten, "consumption should be local, not the whole road: {eaten} vs {far}");
    }

    #[test]
    fn predator_slides_along_land() {
        let t = Tuning::default();
        let rock = Circle::new(Vec2::new(0.0, 15.0), 5.0);
        let mut f = ChargeField::new(&t, &[rock]);
        let lure = f.index_of(Vec2::new(0.0, 32.0)).unwrap();
        f.charge[lure] = t.charge_cap;
        let mut p = Entity::new(1, "P", Form::Creature, Vec2::ZERO, 0.0, &t);
        for _ in 0..(60 * 30) {
            steer_predator(&mut p, &mut f, &[rock], &t, 1.0 / 60.0);
            assert!(!rock.overlaps(&Circle::new(p.pos, p.radius - 0.05)), "predator entered land at {:?}", p.pos);
        }
        assert!(p.pos.y > 25.0, "predator stuck on rock: {:?}", p.pos);
    }
}

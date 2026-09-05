//! Scripted keeper that plays each authored scenario through the real beam controls.
//! Used by the scenario tests as the "demonstrated successful run" and by the developer
//! autopilot (F9) for demos and visual checks. It knows the authored routes, nothing hidden.

use super::beam::Input;
use super::entity::{Entity, EntityId};
use super::geom::{angle_delta, bearing_of, dir};
use super::islands::polar;
use super::{Mode, Phase, Rules, World};
use glam::Vec2;
use std::collections::HashMap;
use std::f32::consts::TAU;

#[derive(Debug)]
pub struct Keeper {
    /// Per-vessel route points from the approach to the harbor (Night Watch, Mutable Sea).
    pub routes: HashMap<&'static str, Vec<Vec2>>,
    /// Spiral Voyage: route points per world.
    pub world_routes: Vec<Vec<Vec2>>,
    /// Charge the keeper considers "painted enough" before moving on.
    pub target_charge: f32,
    /// Ship currently being tended; switching costs attention, so it needs a clear reason.
    focus: Option<EntityId>,
    /// World Weaver plan: (sector, world) copies in order.
    pub plan: Vec<(usize, u8)>,
    plan_index: usize,
}

fn pts(list: &[(f32, f32)]) -> Vec<Vec2> {
    list.iter().map(|&(x, y)| Vec2::new(x, y)).collect()
}

fn polar_pts(list: &[(f32, f32)]) -> Vec<Vec2> {
    list.iter().map(|&(b, r)| polar(b, r)).collect()
}

impl Keeper {
    pub fn for_mode(mode: Mode) -> Self {
        match mode {
            // Ships cover a route point in a few seconds now, so the keeper paints ahead sooner.
            Mode::NightWatch => Self::new(night_watch_routes(), 8.0),
            Mode::MutableSea => Self::new(mutable_sea_routes(), 8.0),
            Mode::WorldWeaver => Self::weaver(world_weaver_solution()),
            Mode::SpiralVoyage => Self {
                routes: HashMap::new(),
                world_routes: spiral_routes(),
                // Soft-saturating charge reaches 12 s a third slower than it used to; at 4 u/s
                // the ship outruns a keeper that waits for that, so it moves on at 8 s.
                target_charge: 8.0,
                focus: None,
                plan: Vec::new(),
                plan_index: 0,
            },
        }
    }

    pub fn new(routes: HashMap<&'static str, Vec<Vec2>>, target_charge: f32) -> Self {
        Self {
            routes,
            world_routes: Vec::new(),
            target_charge,
            focus: None,
            plan: Vec::new(),
            plan_index: 0,
        }
    }

    pub fn weaver(plan: Vec<(usize, u8)>) -> Self {
        Self {
            routes: HashMap::new(),
            world_routes: Vec::new(),
            target_charge: 0.0,
            focus: None,
            plan,
            plan_index: 0,
        }
    }

    /// Route point a ship can actually see and that still needs light.
    fn next_point(&self, w: &World, ship: &Entity, route: &[Vec2], charge_at: &dyn Fn(Vec2) -> f32) -> Option<Vec2> {
        let _ = w;
        let fwd = dir(ship.heading);
        let visible = |p: &Vec2| {
            let to = *p - ship.pos;
            let d = to.length();
            d > 0.5 && to.dot(fwd) / d > 0.3
        };
        let start = route
            .iter()
            .enumerate()
            .filter(|(_, p)| visible(p))
            .min_by(|a, b| a.1.distance(ship.pos).total_cmp(&b.1.distance(ship.pos)))
            .map(|(i, _)| i)
            .or_else(|| {
                // Ship has wandered off: paint the route point nearest to where it is heading.
                let probe = ship.pos + fwd * 8.0;
                route
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.distance(probe).total_cmp(&b.1.distance(probe)))
                    .map(|(i, _)| i)
            })?;
        route[start..]
            .iter()
            .find(|p| charge_at(**p) < self.target_charge)
            .or_else(|| route[start..].first())
            .copied()
    }

    /// Where the footprint should be right now (Night Watch / Mutable Sea).
    pub fn aim_point(&mut self, w: &World) -> Option<Vec2> {
        let sea = &w.sea;
        let t = &sea.tuning;
        let mut best: Option<(f32, EntityId, Vec2)> = None;
        for ship in sea.entities.iter().filter(|e| e.is_active_ship()) {
            let Some(route) = self.routes.get(ship.name) else { continue };
            let charge_at = |p: Vec2| sea.charge.charge_at(p);
            let Some(point) = self.next_point(w, ship, route, &charge_at) else { continue };
            let dist = point.distance(ship.pos);
            let to_harbor = ship.pos.distance(t.harbor_center);
            let mut urgency = 1.0 / (1.0 + dist / 10.0) + 0.6 / (1.0 + to_harbor / 20.0);
            if let Some(m) = ship.mutable {
                urgency += m.progress / t.mutable_dark_durations[ship.form.index()] * 2.0;
            }
            if self.focus == Some(ship.id) {
                urgency *= 1.6;
            }
            if best.is_none_or(|(u, _, _)| urgency > u) {
                best = Some((urgency, ship.id, point));
            }
        }
        self.focus = best.map(|(_, id, _)| id);
        best.map(|(_, _, p)| p)
    }

    pub fn input(&mut self, w: &World) -> Input {
        if w.phase != Phase::Night {
            // Dusk: aim at the first thing worth painting so the night starts on target.
            if let Phase::Intro { .. } = w.phase {
                if let Some(aim) = self.aim_point(w) {
                    return aim_at(w, aim);
                }
            }
            return Input::default();
        }
        match &w.rules {
            Rules::WorldWeaver(ww) => {
                let Some(&(sector, layer)) = self.plan.get(self.plan_index) else {
                    return Input::default();
                };
                let t = &w.sea.tuning;
                let target = layer as f32 * TAU + (sector as f32 + 0.5) * t.sector_angle();
                let d = target - w.sea.beam.winding;
                let here = w.sea.beam.sector_index(t) == sector && ww.layer_for(&w.sea) == layer;
                if here && d.abs() < t.sector_angle() * 0.35 {
                    self.plan_index += 1;
                    return Input {
                        capture: true,
                        ..Default::default()
                    };
                }
                Input {
                    rotate: d.signum(),
                    ..Default::default()
                }
            }
            Rules::SpiralVoyage(sv) => {
                let n = sv.worlds.len();
                let Some(ship) = sv.ship.and_then(|id| w.sea.entity(id)) else { return Input::default() };
                // One route through all worlds, ordered by absolute winding, so the point after
                // a seam is painted in the next world *before* the ship gets there.
                let flat = self
                    .world_routes
                    .iter()
                    .enumerate()
                    .flat_map(|(k, r)| r.iter().map(move |p| (k as f32 * TAU + bearing_of(*p), *p, k)));
                let ahead: Vec<(f32, Vec2, usize)> = flat.filter(|(wind, _, _)| *wind > ship.winding - 0.05).collect();
                let Some(&(target_winding, point, _)) = ahead
                    .iter()
                    .find(|(_, p, k)| *k < n && sv.worlds[*k].charge.charge_at(*p) < self.target_charge)
                    .or_else(|| ahead.first())
                else {
                    return Input::default();
                };
                let d = target_winding - w.sea.beam.winding;
                let d_range = point.length() - w.sea.beam.range;
                Input {
                    rotate: if d.abs() > 0.02 { d.signum() } else { 0.0 },
                    range: if d_range.abs() > 1.0 { d_range.signum() } else { 0.0 },
                    capture: false,
                }
            }
            _ => {
                let Some(aim) = self.aim_point(w) else { return Input::default() };
                aim_at(w, aim)
            }
        }
    }
}

/// Aim the beam footprint at a world position through the real controls.
pub fn aim_at(w: &World, aim: Vec2) -> Input {
    let beam = &w.sea.beam;
    let d_angle = angle_delta(beam.bearing(), bearing_of(aim));
    let d_range = aim.length() - beam.range;
    Input {
        rotate: if d_angle.abs() > 0.02 { d_angle.signum() } else { 0.0 },
        range: if d_range.abs() > 1.0 { d_range.signum() } else { 0.0 },
        capture: false,
    }
}

pub fn night_watch_routes() -> HashMap<&'static str, Vec<Vec2>> {
    HashMap::from([
        ("Alder", pts(&[(0.0, 85.0), (-10.0, 70.0), (-24.0, 56.0), (-30.0, 40.0), (-30.0, 24.0), (-26.0, 8.0), (-22.0, -8.0), (-14.0, -18.0), (-4.0, -22.0), (0.0, -16.0)])),
        ("Brant", pts(&[(85.0, 8.0), (72.0, 0.0), (60.0, -8.0), (46.0, -12.0), (32.0, -14.0), (18.0, -16.0), (6.0, -16.0), (0.0, -15.0)])),
        // South-east of the skerry chain, which runs from (-48,-34) to (-30,-52).
        ("Cormorant", pts(&[(-58.0, -64.0), (-44.0, -64.0), (-30.0, -62.0), (-20.0, -50.0), (-14.0, -36.0), (-8.0, -22.0), (-2.0, -14.0)])),
        ("Dunlin", pts(&[(56.0, 58.0), (46.0, 48.0), (38.0, 36.0), (30.0, 26.0), (26.0, 14.0), (22.0, 2.0), (20.0, -10.0), (14.0, -16.0), (6.0, -15.0), (2.0, -14.0)])),
        ("Eider", pts(&[(66.0, -50.0), (54.0, -46.0), (44.0, -40.0), (36.0, -32.0), (28.0, -24.0), (18.0, -20.0), (10.0, -16.0), (4.0, -14.0)])),
    ])
}

pub fn mutable_sea_routes() -> HashMap<&'static str, Vec<Vec2>> {
    HashMap::from([
        ("Kestrel", pts(&[(-54.0, 38.0), (-46.0, 26.0), (-34.0, 14.0), (-28.0, 2.0), (-22.0, -8.0), (-14.0, -15.0), (-6.0, -16.0), (0.0, -15.0)])),
        ("Merlin", pts(&[(62.0, -20.0), (50.0, -22.0), (38.0, -22.0), (26.0, -20.0), (16.0, -18.0), (8.0, -16.0), (2.0, -14.0)])),
        ("Osprey", pts(&[(-20.0, -50.0), (-14.0, -40.0), (-8.0, -30.0), (-4.0, -22.0), (0.0, -16.0)])),
    ])
}

/// One validated World Weaver assembly: open the east with sector 3 from World 2.
pub fn world_weaver_solution() -> Vec<(usize, u8)> {
    vec![(3, 1)]
}

/// A second validated assembly: open the west with sector 10 from World 3.
#[cfg(test)]
pub fn world_weaver_alternative() -> Vec<(usize, u8)> {
    vec![(10, 2)]
}

/// Spiral Voyage route points per world (compass degrees, radius).
pub fn spiral_routes() -> Vec<Vec<Vec2>> {
    vec![
        polar_pts(&[(315.0, 40.0), (330.0, 40.0), (345.0, 40.0), (358.0, 40.0)]),
        polar_pts(&[
            (10.0, 40.0), (25.0, 48.0), (45.0, 50.0), (65.0, 50.0), (85.0, 46.0), (110.0, 40.0), (135.0, 34.0),
            // Clear of the inner reef arc (r 30, 200°-250°) with hull margin.
            (160.0, 32.0), (185.0, 38.0), (215.0, 46.0), (240.0, 46.0), (265.0, 40.0), (290.0, 38.0),
            (315.0, 36.0), (340.0, 36.0), (358.0, 36.0),
        ]),
        polar_pts(&[
            (15.0, 40.0), (40.0, 45.0), (70.0, 45.0), (100.0, 42.0), (130.0, 42.0), (160.0, 38.0), (190.0, 32.0),
            (215.0, 26.0), (240.0, 24.0), (265.0, 30.0), (290.0, 34.0), (310.0, 34.0), (335.0, 36.0), (358.0, 38.0),
        ]),
        {
            let mut r = polar_pts(&[(15.0, 36.0), (40.0, 42.0), (65.0, 42.0), (90.0, 40.0), (110.0, 34.0), (135.0, 30.0), (160.0, 24.0), (178.0, 18.0)]);
            r.push(Vec2::new(0.0, -16.0));
            r
        },
    ]
}

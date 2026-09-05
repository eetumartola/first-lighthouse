//! Scripted keeper that plays each scenario through the real beam controls. Used by the scenario
//! tests as the "demonstrated successful run" and by the developer autopilot (F9). It plans with
//! the same clearance-aware route finder as World Weaver over the *authored level* land, so it
//! works on any map and knows nothing hidden about the ships.

use super::beam::Input;
use super::entity::{Entity, EntityId, Form};
use super::geom::{angle_delta, bearing_of, dir};
use super::islands::polar;
use super::level;
use super::route;
use super::spiral_voyage::winding_in_world;
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


impl Keeper {
    pub fn for_mode(mode: Mode) -> Self {
        // Ships cover a route point in a few seconds now, so the keeper paints ahead sooner:
        // soft-saturating charge reaches 12 s a third slower than linear charging used to.
        Self {
            routes: HashMap::new(),
            world_routes: match mode {
                Mode::SpiralVoyage => spiral_world_routes(),
                _ => Vec::new(),
            },
            target_charge: if mode == Mode::SpiralVoyage { 2.0 } else { 4.0 },
            focus: None,
            plan: match mode {
                Mode::WorldWeaver => world_weaver_solution(),
                _ => Vec::new(),
            },
            plan_index: 0,
        }
    }


    /// Route point a ship can actually see and that still needs light.
    fn next_point(
        target_charge: f32,
        ship: &Entity,
        route: &[Vec2],
        charge_at: &dyn Fn(Vec2) -> f32,
    ) -> Option<Vec2> {
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
            .find(|p| charge_at(**p) < target_charge)
            .or_else(|| route[start..].first())
            .copied()
    }

    /// Where the footprint should be right now (Night Watch / Mutable Sea). Routes are planned
    /// on first sight with the clearance-aware finder over the level's land; a ship with no
    /// passage is left to its own lights.
    pub fn aim_point(&mut self, w: &World) -> Option<Vec2> {
        let sea = &w.sea;
        let t = &sea.tuning;
        let mut best: Option<(f32, EntityId, Vec2)> = None;
        for ship in sea.entities.iter().filter(|e| e.is_active_ship()) {
            if !self.routes.contains_key(ship.name) {
                let planned = plan_route(&sea.rocks, t, ship.pos, t.harbor_center).unwrap_or_default();
                self.routes.insert(ship.name, planned);
            }
            let route = &self.routes[ship.name];
            let charge_at = |p: Vec2| sea.charge.charge_at(p);
            let Some(point) = Self::next_point(self.target_charge, ship, route, &charge_at) else { continue };
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
            match (&w.phase, &w.rules) {
                // Spiral uses the same authored sequence at dusk, positioning the beam at the
                // first useful point before the ship starts moving.
                (Phase::Intro { .. }, Rules::SpiralVoyage(_)) => {}
                (Phase::Intro { .. }, _) => {
                    if let Some(aim) = self.aim_point(w) {
                        return aim_at(w, aim);
                    }
                    return Input::default();
                }
                _ => return Input::default(),
            }
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
                let Some(ship) = sv.ship.and_then(|id| w.sea.entity(id)) else { return Input::default() };
                // Keep the beam within the ship's readable lookahead. Scouting farther is legal,
                // but an unattended keeper must not abandon the live trail while the ship catches up.
                let mut target = None;
                let max_ahead = w.sea.tuning.ship_length * w.sea.tuning.guidance_lookahead_lengths * 0.8;
                for world in sv.ship_world..self.world_routes.len() {
                    let route = &self.world_routes[world];
                    let start = if world == sv.ship_world {
                        // Nearest route point that is not clearly astern: a ship that has lost
                        // the trail at a bend is drawn back onto it rather than led further away.
                        route
                            .iter()
                            .enumerate()
                            .filter(|(_, point)| winding_in_world(world, bearing_of(**point)) > ship.winding - 0.15)
                            .min_by(|a, b| a.1.distance(ship.pos).total_cmp(&b.1.distance(ship.pos)))
                            .map(|(index, _)| index)
                            .unwrap_or(route.len())
                    } else {
                        0
                    };
                    if start == route.len() {
                        continue;
                    }
                    let mut end = start + 1;
                    let mut distance = 0.0;
                    while end < route.len() {
                        distance += route[end - 1].distance(route[end]);
                        if distance > max_ahead {
                            break;
                        }
                        end += 1;
                    }
                    // Nearest dim point first: the trail grows continuously from the ship, so a
                    // rounded footprint never leaves an unlit gap the corridor search would miss.
                    let window = &route[start..end];
                    let point = window
                        .iter()
                        .find(|point| sv.worlds[world].charge.charge_at(**point) < self.target_charge)
                        .copied();
                    if point.is_none() && end == route.len() {
                        continue;
                    }
                    let point = point.or_else(|| window.last().copied()).unwrap();
                    target = Some((winding_in_world(world, bearing_of(point)), point));
                    break;
                }
                let Some((target_winding, point)) = target else {
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
            Rules::NightWatch(_) => {
                if let Some(decoy) = predator_decoy(w) {
                    aim_at(w, decoy)
                } else {
                    let Some(aim) = self.aim_point(w) else { return Input::default() };
                    aim_at(w, aim)
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
/// Pull a nearby predator away from the vessel it is about to intercept. The keeper still uses
/// only visible entity positions and the real beam; this is the attentive counterplay taught by
/// the mode rather than immunity or hidden control.
fn predator_decoy(w: &World) -> Option<Vec2> {
    let creature = w.sea.entities.iter().find(|entity| entity.form == Form::Creature && entity.is_active())?;
    let ship = w
        .sea
        .entities
        .iter()
        .filter(|entity| entity.is_active_ship())
        .min_by(|a, b| a.pos.distance(creature.pos).total_cmp(&b.pos.distance(creature.pos)))?;
    if ship.pos.distance(creature.pos) > 18.0 {
        return None;
    }
    let away = (creature.pos - ship.pos).normalize_or(Vec2::X);
    let mut decoy = creature.pos + away * 18.0;
    let limit = w.sea.tuning.sea_radius - w.sea.tuning.beam_length;
    if decoy.length() > limit {
        decoy = decoy.normalize_or(Vec2::X) * limit;
    }
    Some(decoy)
}

/// A clearance-aware route from `from` to `goal` over the given land. The finder simplifies only
/// across safe lines of sight; points are then restored at guidance-scale spacing so ships see a
/// continuous trail rather than isolated corners.
pub fn plan_route(
    land: &[super::geom::Circle],
    t: &super::tuning::Tuning,
    from: Vec2,
    goal: Vec2,
) -> Option<Vec<Vec2>> {
    plan_route_with_buffer(land, t, from, goal, 3.0)
}

fn plan_route_with_buffer(
    land: &[super::geom::Circle],
    t: &super::tuning::Tuning,
    from: Vec2,
    goal: Vec2,
    steering_buffer: f32,
) -> Option<Vec<Vec2>> {
    let sparse = route::find_route(
        land,
        t.sea_radius,
        t.weaver_route_cell,
        t.ship_radius + steering_buffer,
        from,
        goal,
    )?;
    let mut route = vec![sparse[0]];
    for segment in sparse.windows(2) {
        let steps = (segment[0].distance(segment[1]) / 6.0).ceil().max(1.0) as usize;
        for step in 1..=steps {
            route.push(segment[0].lerp(segment[1], step as f32 / steps as f32));
        }
    }
    Some(route)
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

/// Spiral Voyage: one ordered route per world. Intermediate columns force clockwise progress
/// around each authored block instead of allowing the Cartesian route finder to shortcut backward
/// across the south seam.
pub fn spiral_world_routes() -> Vec<Vec<Vec2>> {
    let t = super::tuning::Tuning::default();
    // A ship following point guidance can deviate by roughly its minimum turning radius; retain
    // the route finder's normal margin beyond that dynamic envelope.
    let steering_buffer = t.ship_speed * t.spiral_ship_speed_factor
        / t.ship_turn_rate_deg.to_radians()
        + t.weaver_route_margin;
    let worlds = level::parse(level::MODE4_LEVEL1, t.island_radius, t.sea_radius);
    let mut land = vec![super::geom::Circle::new(Vec2::ZERO, t.island_radius)];
    let mut routes = Vec::with_capacity(worlds.len());
    let mut entry = polar(300.0, 40.0);

    for (world, rocks) in worlds.iter().enumerate() {
        land.truncate(1);
        land.extend(rocks.iter().copied());
        let mut route = vec![entry];
        let columns: &[usize] = if world == 0 { &[15, 22] } else { &[7, 15, 22] };
        for &col in columns {
            append_column_leg(&mut route, &land, &t, world, col, steering_buffer)
                .unwrap_or_else(|| panic!("World {} has no clockwise passage through column {col}", world + 1));
        }

        if world + 1 == worlds.len() {
            let from = *route.last().unwrap();
            let leg = plan_route_with_buffer(&land, &t, from, t.harbor_center, steering_buffer)
                .unwrap_or_else(|| panic!("World {} has no passage to the harbor", world + 1));
            extend_route(&mut route, leg);
        } else {
            let row =
                append_seam_leg(&mut route, &land, &worlds[world + 1], &t, world, steering_buffer)
                    .unwrap_or_else(|| panic!("World {} has no hull-clear south-seam exit", world + 1));
            entry = level::cell_center(level::MODE4_LEVEL1, 0, row, t.island_radius, t.sea_radius);
        }
        routes.push(straighten(route, &land, &t, t.ship_radius + steering_buffer));
    }
    routes
}

/// Append a safe leg to a free cell in `col`, preferring the middle radii. Returns its row.
fn append_column_leg(
    route: &mut Vec<Vec2>,
    land: &[super::geom::Circle],
    t: &super::tuning::Tuning,
    world: usize,
    col: usize,
    steering_buffer: f32,
) -> Option<usize> {
    let rows = level::rows(level::MODE4_LEVEL1);
    let middle = rows / 2;
    let mut row_candidates: Vec<_> = (1..rows - 1).collect();
    row_candidates.sort_by_key(|row| row.abs_diff(middle));
    let column_candidates = [
        col,
        col - 1,
        col + 1,
        col - 2,
        col + 2,
        col - 3,
        col + 3,
        col - 4,
        col + 4,
        col - 5,
        col + 5,
        col - 6,
        col + 6,
    ];
    for candidate_col in column_candidates {
        for &row in &row_candidates {
            if !level::is_free(level::MODE4_LEVEL1, world, candidate_col, row) {
                continue;
            }
            let goal =
                level::cell_center(level::MODE4_LEVEL1, candidate_col, row, t.island_radius, t.sea_radius);
            if hull_clearance(goal, land, t) < steering_buffer {
                continue;
            }
            if let Some(leg) =
                plan_route_with_buffer(land, t, *route.last().unwrap(), goal, steering_buffer)
            {
                extend_route(route, leg);
                return Some(row);
            }
        }
    }
    None
}

/// Pick a seam row by actual hull clearance in both adjacent worlds, not merely empty ASCII cells.
fn append_seam_leg(
    route: &mut Vec<Vec2>,
    land: &[super::geom::Circle],
    next_rocks: &[super::geom::Circle],
    t: &super::tuning::Tuning,
    world: usize,
    steering_buffer: f32,
) -> Option<usize> {
    let mut candidates: Vec<_> = (1..level::rows(level::MODE4_LEVEL1) - 1)
        .filter(|&row| {
            level::is_free(level::MODE4_LEVEL1, world, level::COLUMNS - 1, row)
                && level::is_free(level::MODE4_LEVEL1, world + 1, 0, row)
        })
        .collect();
    let clearance = |row: usize| {
        let exit = level::cell_center(level::MODE4_LEVEL1, level::COLUMNS - 1, row, t.island_radius, t.sea_radius);
        let entry = level::cell_center(level::MODE4_LEVEL1, 0, row, t.island_radius, t.sea_radius);
        let current = land.iter().map(|r| exit.distance(r.center) - r.radius);
        let next = next_rocks
            .iter()
            .map(|r| entry.distance(r.center) - r.radius)
            .chain(std::iter::once(entry.length() - t.island_radius));
        current.chain(next).fold(f32::INFINITY, f32::min) - t.ship_radius
    };
    candidates.sort_by(|a, b| clearance(*b).total_cmp(&clearance(*a)));
    for row in candidates {
        if clearance(row) < steering_buffer {
            continue;
        }
        let goal =
            level::cell_center(level::MODE4_LEVEL1, level::COLUMNS - 1, row, t.island_radius, t.sea_radius);
        if let Some(leg) =
            plan_route_with_buffer(land, t, *route.last().unwrap(), goal, steering_buffer)
        {
            extend_route(route, leg);
            return Some(row);
        }
    }
    None
}
fn hull_clearance(
    point: Vec2,
    land: &[super::geom::Circle],
    t: &super::tuning::Tuning,
) -> f32 {
    land.iter()
        .map(|rock| point.distance(rock.center) - rock.radius)
        .fold(f32::INFINITY, f32::min)
        - t.ship_radius
}

fn extend_route(route: &mut Vec<Vec2>, leg: Vec<Vec2>) {
    route.extend(leg.into_iter().skip(1));
}

/// Greedy line-of-sight shortcutting over a finished route, then re-spacing at trail scale. The
/// forced column waypoints can send a leg into a pocket and straight back out; a ship reading
/// light cannot follow such a hairpin, so it is cut where the hull can pass directly. Shortcuts
/// only ever advance clockwise by less than a quarter turn, so the circuit itself survives.
fn straighten(route: Vec<Vec2>, land: &[super::geom::Circle], t: &super::tuning::Tuning, clearance: f32) -> Vec<Vec2> {
    let allowed = |a: Vec2, b: Vec2| {
        let d = angle_delta(bearing_of(a), bearing_of(b));
        (-0.05..=std::f32::consts::FRAC_PI_2).contains(&d) && route::segment_clear(a, b, land, t.sea_radius, clearance)
    };
    let mut sparse = vec![route[0]];
    let mut i = 0;
    while i + 1 < route.len() {
        let mut j = route.len() - 1;
        while j > i + 1 && !allowed(route[i], route[j]) {
            j -= 1;
        }
        sparse.push(route[j]);
        i = j;
    }
    let mut out = vec![sparse[0]];
    for segment in sparse.windows(2) {
        let steps = (segment[0].distance(segment[1]) / 6.0).ceil().max(1.0) as usize;
        for step in 1..=steps {
            out.push(segment[0].lerp(segment[1], step as f32 / steps as f32));
        }
    }
    out
}

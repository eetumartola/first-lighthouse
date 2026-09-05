//! Variant 3 — World Weaver: browse four authored layers by winding the beam, capture sectors,
//! then watch the assembled world tested by a fixed voyage at first light.

use super::beam::Input;
use super::entity::{EntityId, Form, Status, Target};
use super::geom::{bearing_of, compass_word, dir, turn_toward, Circle};
use super::steering;
use super::tuning::Tuning;
use super::{Cause, Event, Outcome, Sea};
use glam::Vec2;

#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub name: &'static str,
    pub pos: Vec2,
    pub sector: usize,
    /// Cycle offset: form in layer `l` is `CYCLE[(phase + l) % 4]`.
    pub phase: u8,
}

impl Anchor {
    pub fn form_in(&self, layer: u8) -> Form {
        Form::CYCLE[(self.phase as usize + layer as usize) % 4]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Voyage {
    pub elapsed: f32,
    pub expedition: Option<EntityId>,
    pub followers: Vec<EntityId>,
    pub delay_remaining: f32,
    pub handled_wrecks: Vec<EntityId>,
    pub failure: Option<String>,
    pub arrived: bool,
}

/// A connected chain of rocks that exists in one sector of one layer only.
#[derive(Clone, Debug)]
pub struct Reef {
    pub sector: usize,
    pub layer: u8,
    pub rocks: Vec<Circle>,
}

impl Reef {
    /// `count` rocks of `radius` starting at `start`, each `step` further along; overlapping
    /// edges make the chain read as one connected reef.
    fn chain(sector: usize, layer: u8, start: Vec2, step: Vec2, count: usize, radius: f32) -> Self {
        Self {
            sector,
            layer,
            rocks: (0..count).map(|i| Circle::new(start + step * i as f32, radius)).collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorldWeaver {
    pub anchors: Vec<Anchor>,
    /// Layer-specific rock geometry, so each layer of a sector is a different sea.
    pub reefs: Vec<Reef>,
    /// Committed layer per sector; `None` uses the stated default at dawn.
    pub committed: Vec<Option<u8>>,
    /// Authored sea lane, ending at the harbor.
    pub route: Vec<Vec2>,
    pub last_layer: u8,
    pub voyage: Voyage,
    /// Composition frozen at dawn: layer used per sector.
    pub built: Option<Vec<u8>>,
}

pub const LAYER_NAMES: [&str; 4] = ["Calm", "Tide", "Deep", "Storm"];
pub const LAYER_GLYPHS: [&str; 4] = ["I", "II", "III", "IV"];

impl WorldWeaver {
    pub fn scenario(t: &Tuning) -> (Self, Vec<Circle>) {
        let d = |deg: f32, r: f32| dir(deg.to_radians()) * r;
        let rocks = vec![
            Circle::new(d(315.0, 56.0), 5.0),
            Circle::new(d(250.0, 58.0), 4.5),
            Circle::new(d(15.0, 77.0), 4.0),
        ];
        // The lane crosses sectors 1–6 clockwise and enters the harbor from the south.
        let route = vec![
            d(40.0, 92.0),
            d(45.0, 60.0),
            d(75.0, 48.0),
            d(105.0, 45.0),
            d(135.0, 42.0),
            d(165.0, 38.0),
            Vec2::new(0.0, -28.0),
            t.harbor_center + Vec2::new(0.0, -1.0),
        ];
        let anchors = vec![
            Anchor { name: "Gull", pos: d(45.0, 60.0), sector: 1, phase: 3 },
            Anchor { name: "Tern", pos: d(75.0, 48.0), sector: 2, phase: 1 },
            Anchor { name: "Skua", pos: d(100.0, 53.0), sector: 3, phase: 2 },
            Anchor { name: "Petrel", pos: d(105.0, 45.0), sector: 3, phase: 3 },
            Anchor { name: "Fulmar", pos: d(135.0, 42.0), sector: 4, phase: 0 },
            Anchor { name: "Gannet", pos: d(140.0, 50.0), sector: 4, phase: 2 },
            Anchor { name: "Shearwater", pos: d(165.0, 38.0), sector: 5, phase: 1 },
            Anchor { name: "Puffin", pos: d(195.0, 30.0), sector: 6, phase: 2 },
        ];
        // Reefs. Each lane sector has one layer whose reef lies across the lane (authored in
        // Cartesian coordinates, perpendicular to the lane); every other reef keeps clear of it.
        // Decorative reefs run radially near a sector's middle so they never straddle a seam.
        // Outer sectors change geometry too, so the layers read as different seas everywhere.
        let radial = |sector: usize, layer: u8, bearing: f32, r0: f32, count: usize, spacing: f32, radius: f32| {
            Reef::chain(sector, layer, d(bearing, r0), dir(bearing.to_radians()) * spacing, count, radius)
        };
        let reefs = vec![
            radial(0, 0, 10.0, 62.0, 3, 5.0, 3.0),
            radial(0, 1, 20.0, 40.0, 4, 4.5, 2.5),
            radial(0, 2, 8.0, 80.0, 3, 4.5, 2.6),
            radial(0, 3, 22.0, 50.0, 3, 5.0, 3.0),
            // Sector 1: layer III reef across the lane near r 75.
            radial(1, 0, 38.0, 22.0, 3, 4.5, 2.5),
            radial(1, 1, 52.0, 24.0, 3, 4.5, 2.5),
            Reef::chain(1, 2, Vec2::new(41.6, 60.8), Vec2::new(4.3, -2.55), 5, 3.0),
            radial(1, 3, 40.0, 20.0, 3, 4.5, 2.5),
            // Sector 2: layer I reef across the lane near r 49.
            Reef::chain(2, 0, Vec2::new(38.2, 17.4), Vec2::new(4.95, 0.65), 4, 3.0),
            radial(2, 1, 70.0, 22.0, 3, 4.5, 2.5),
            radial(2, 2, 82.0, 78.0, 3, 5.0, 3.0),
            radial(2, 3, 68.0, 66.0, 3, 4.5, 2.5),
            // Sector 3: layer IV reef across the lane near r 45.
            radial(3, 0, 98.0, 22.0, 3, 4.5, 2.5),
            radial(3, 1, 112.0, 76.0, 3, 5.0, 3.0),
            radial(3, 2, 116.0, 60.0, 3, 4.5, 2.5),
            Reef::chain(3, 3, Vec2::new(36.5, -7.1), Vec2::new(4.95, -0.6), 4, 3.0),
            // Sector 4: layer III reef across the lane near r 42.
            radial(4, 0, 128.0, 20.0, 3, 4.5, 2.5),
            radial(4, 1, 142.0, 72.0, 3, 5.0, 3.0),
            Reef::chain(4, 2, Vec2::new(26.6, -21.5), Vec2::new(3.95, -3.05), 4, 3.0),
            radial(4, 3, 127.0, 62.0, 3, 5.0, 3.0),
            // Sector 5: layer II reef across the lane near r 38.
            radial(5, 0, 158.0, 58.0, 3, 4.5, 2.5),
            Reef::chain(5, 1, Vec2::new(12.2, -30.6), Vec2::new(1.65, -4.7), 3, 3.0),
            radial(5, 2, 172.0, 56.0, 2, 4.5, 2.5),
            radial(5, 3, 165.0, 72.0, 3, 5.0, 3.0),
            // Sector 6: layer IV reef across the harbor approach.
            radial(6, 0, 192.0, 40.0, 3, 4.5, 2.5),
            radial(6, 1, 202.0, 62.0, 3, 5.0, 3.0),
            radial(6, 2, 188.0, 78.0, 3, 4.5, 2.6),
            Reef::chain(6, 3, Vec2::new(-3.6, -22.5), Vec2::new(-0.8, -4.5), 3, 2.8),
            // Sectors 7–11: decoration only.
            radial(7, 0, 220.0, 62.0, 4, 5.0, 3.0),
            radial(7, 1, 232.0, 36.0, 3, 4.5, 2.5),
            radial(7, 2, 218.0, 48.0, 3, 5.0, 3.0),
            radial(7, 3, 236.0, 80.0, 3, 4.5, 2.6),
            radial(8, 0, 262.0, 30.0, 3, 4.5, 2.5),
            radial(8, 1, 256.0, 72.0, 4, 5.0, 3.0),
            radial(8, 2, 248.0, 38.0, 3, 4.5, 2.5),
            radial(8, 3, 264.0, 52.0, 3, 5.0, 3.0),
            radial(9, 0, 285.0, 50.0, 3, 5.0, 3.0),
            radial(9, 1, 292.0, 76.0, 4, 5.0, 3.0),
            radial(9, 2, 278.0, 28.0, 3, 4.5, 2.5),
            radial(9, 3, 294.0, 40.0, 3, 4.5, 2.5),
            radial(10, 0, 322.0, 74.0, 3, 5.0, 3.0),
            radial(10, 1, 308.0, 34.0, 3, 4.5, 2.5),
            radial(10, 2, 318.0, 24.0, 3, 4.5, 2.5),
            radial(10, 3, 326.0, 66.0, 4, 5.0, 3.0),
            radial(11, 0, 345.0, 46.0, 3, 5.0, 3.0),
            radial(11, 1, 338.0, 70.0, 3, 5.0, 3.0),
            radial(11, 2, 352.0, 30.0, 3, 4.5, 2.5),
            radial(11, 3, 342.0, 84.0, 3, 4.5, 2.6),
        ];
        (
            Self {
                anchors,
                reefs,
                committed: vec![None; t.weaver_sectors],
                route,
                last_layer: 0,
                voyage: Voyage::default(),
                built: None,
            },
            rocks,
        )
    }

    /// Layer index for the beam's current total winding (cyclic over the authored layers).
    pub fn layer_for(&self, sea: &Sea) -> u8 {
        sea.beam.revolution().rem_euclid(sea.tuning.weaver_layers as i32) as u8
    }

    /// Candidate forms shown in the active preview sector.
    pub fn preview(&self, sector: usize, layer: u8) -> impl Iterator<Item = (&Anchor, Form)> + '_ {
        self.anchors
            .iter()
            .filter(move |a| a.sector == sector)
            .map(move |a| (a, a.form_in(layer)))
    }

    pub fn layer_used(&self, sector: usize, t: &Tuning) -> u8 {
        self.committed[sector].unwrap_or(t.weaver_default_layer)
    }
}

pub fn step_night(ww: &mut WorldWeaver, sea: &mut Sea, input: Input, _dt: f32) {
    let layer = ww.layer_for(sea);
    if layer != ww.last_layer {
        ww.last_layer = layer;
        sea.events.push(Event::LayerChanged { layer });
    }
    if input.capture {
        let sector = sea.beam.sector_index(&sea.tuning);
        ww.committed[sector] = Some(layer);
        let fp = sea.beam.footprint(&sea.tuning);
        sea.charge.stamp(&fp, sea.tuning.weaver_capture_glow);
        sea.events.push(Event::Captured { sector, layer });
    }
}

/// Dawn: freeze commitments, instantiate exactly one entity per anchor, and lay the chosen reefs.
pub fn freeze_and_build(ww: &mut WorldWeaver, sea: &mut Sea) {
    let t = sea.tuning.clone();
    let built: Vec<u8> = (0..t.weaver_sectors).map(|s| ww.layer_used(s, &t)).collect();
    for a in ww.anchors.clone() {
        let form = a.form_in(built[a.sector]);
        // Idle candidate ships point along the lane so joining looks natural.
        let heading = bearing_of(-a.pos) + std::f32::consts::FRAC_PI_2;
        sea.spawn(a.name, form, a.pos, heading);
    }
    let chosen: Vec<Circle> = ww
        .reefs
        .iter()
        .filter(|r| built[r.sector] == r.layer)
        .flat_map(|r| r.rocks.iter().copied())
        .collect();
    sea.charge.add_land(&chosen);
    sea.rocks.extend(chosen);
    ww.built = Some(built);
    let start = ww.route[0];
    let heading = bearing_of(ww.route[1] - start);
    let id = sea.spawn("Expedition", Form::Ship, start, heading);
    if let Some(e) = sea.entity_mut(id) {
        e.brain.target = Some(Target::Waypoint(1));
    }
    ww.voyage = Voyage {
        expedition: Some(id),
        ..Default::default()
    };
}

pub fn begin_voyage(_ww: &mut WorldWeaver, sea: &mut Sea) {
    sea.events.push(Event::VoyageBegins);
}

/// Returns true when the voyage has resolved.
pub fn step_playback(ww: &mut WorldWeaver, sea: &mut Sea, dt: f32) -> bool {
    let t = sea.tuning.clone();
    let v = &mut ww.voyage;
    let Some(ex_id) = v.expedition else { return true };
    v.elapsed += dt;
    if v.elapsed > t.world_weaver_playback_limit {
        v.failure = Some("First light faded before the expedition reached harbor.".into());
        return true;
    }

    let Some(ex_idx) = sea.entities.iter().position(|e| e.id == ex_id) else { return true };

    if v.delay_remaining > 0.0 {
        v.delay_remaining -= dt;
    } else {
        // Wrecks on the lane cost a salvage delay once each.
        let ex_pos = sea.entities[ex_idx].pos;
        let wreck = sea.entities.iter().find(|o| {
            o.is_active()
                && o.form == Form::Wreck
                && !v.handled_wrecks.contains(&o.id)
                && o.pos.distance(ex_pos) <= t.weaver_wreck_radius
        });
        if let Some(w) = wreck {
            v.handled_wrecks.push(w.id);
            v.delay_remaining = t.weaver_wreck_delay;
            sea.events.push(Event::VoyageDelay { pos: w.pos });
        } else {
            let route = ww.route.clone();
            let harbor = sea.harbor();
            let ex = &mut sea.entities[ex_idx];
            let arrived = steering::steer_along_route(ex, &route, t.weaver_voyage_speed, 90.0, dt);
            let hull = ex.circle();
            if arrived || harbor.contains(ex.pos) {
                v.arrived = true;
                sea.events.push(Event::VoyageArrived);
                sea.secure(ex_id);
                for f in v.followers.clone() {
                    if sea.entity(f).is_some_and(|e| e.is_active()) {
                        sea.secure(f);
                    }
                }
                return true;
            }
            // Grounding: an island from the composition or a reef across the lane ends the voyage.
            let island = sea
                .entities
                .iter()
                .find(|o| o.id != ex_id && o.is_active() && o.form == Form::Island && o.circle().overlaps(&hull))
                .map(|o| (o.pos, "island"));
            let reef = sea
                .rocks
                .iter()
                .find(|r| r.overlaps(&hull))
                .map(|r| (r.center, "rocks"));
            if let Some((pos, what)) = island.or(reef) {
                v.failure = Some(format!("The {} {what} blocked the route.", compass_word(pos)));
                sea.events.push(Event::VoyageBlocked { pos });
                return true;
            }
        }
    }

    // Candidate ships join when the expedition passes nearby.
    let ex_pos = sea.entities[ex_idx].pos;
    let joiners: Vec<(EntityId, Vec2)> = sea
        .entities
        .iter()
        .filter(|o| {
            o.id != ex_id
                && o.is_active_ship()
                && !v.followers.contains(&o.id)
                && o.pos.distance(ex_pos) <= t.weaver_join_radius
        })
        .map(|o| (o.id, o.pos))
        .collect();
    for (id, pos) in joiners {
        v.followers.push(id);
        sea.events.push(Event::VoyageJoined { id, pos });
    }

    // Followers trail the expedition in a line.
    let ex_heading = sea.entities[ex_idx].heading;
    for (k, fid) in v.followers.clone().into_iter().enumerate() {
        let Some(f) = sea.entity_mut(fid) else { continue };
        if !f.is_active() {
            continue;
        }
        let slot = ex_pos - dir(ex_heading) * (4.0 * (k as f32 + 1.0));
        let to = slot - f.pos;
        if to.length() > 0.5 {
            f.heading = turn_toward(f.heading, bearing_of(to), 120f32.to_radians() * dt);
            let step = (t.weaver_voyage_speed * 1.15 * dt).min(to.length());
            f.pos += dir(f.heading) * step;
        }
    }

    // Creatures follow vessel lights and sink what they touch.
    let creature_ids: Vec<EntityId> = sea
        .entities
        .iter()
        .filter(|e| e.is_active() && e.form == Form::Creature)
        .map(|e| e.id)
        .collect();
    for cid in creature_ids {
        let lights: Vec<steering::Light> = sea
            .entities
            .iter()
            .filter(|s| s.is_active() && s.lantern)
            .map(|s| steering::Light {
                pos: s.pos,
                brightness: t.lantern_brightness,
                lantern: Some(s.id),
            })
            .collect();
        let land = sea.land_for(cid);
        let Some(idx) = sea.entities.iter().position(|e| e.id == cid) else { continue };
        let creature = &mut sea.entities[idx];
        steering::steer_creature(
            creature,
            &lights,
            &land,
            &t,
            t.weaver_creature_speed,
            t.weaver_creature_detect_radius,
            dt,
        );
        let reach = Circle::new(creature.pos, t.creature_contact_radius);
        let creature_pos = creature.pos;
        let victims: Vec<EntityId> = sea
            .entities
            .iter()
            .filter(|s| s.is_active_ship() && reach.overlaps(&s.circle()))
            .map(|s| s.id)
            .collect();
        for vid in victims {
            if let Some(s) = sea.entity_mut(vid) {
                s.status = Status::Sunk;
                let pos = s.pos;
                sea.events.push(Event::Sunk { id: vid, pos, cause: Cause::Creature });
            }
            if vid == ex_id {
                v.failure = Some(format!("The {} creature took the expedition.", compass_word(creature_pos)));
                return true;
            }
        }
    }
    false
}

pub fn outcome(ww: &WorldWeaver, sea: &Sea) -> Outcome {
    let v = &ww.voyage;
    let bonus = v
        .followers
        .iter()
        .filter(|id| sea.entity(**id).is_some_and(|e| e.status == Status::Secured))
        .count();
    let rescued = if v.arrived { 1 + bonus } else { 0 };
    let captured = ww.committed.iter().filter(|c| c.is_some()).count();
    let mut details = vec![format!("{captured} of {} sectors captured; the rest used {}.", ww.committed.len(), LAYER_NAMES[sea.tuning.weaver_default_layer as usize])];
    if let Some(built) = &ww.built {
        let summary: Vec<String> = ww
            .anchors
            .iter()
            .map(|a| format!("{} ({}): {}", a.name, LAYER_GLYPHS[built[a.sector] as usize], a.form_in(built[a.sector]).name()))
            .collect();
        details.push(summary.join(", "));
    }
    let headline = match (&v.failure, v.arrived) {
        (Some(reason), _) => reason.clone(),
        (None, true) if bonus > 0 => format!("The expedition reached harbor with {bonus} vessel{} joining it.", if bonus == 1 { "" } else { "s" }),
        (None, true) => "The expedition reached harbor alone.".into(),
        (None, false) => "The voyage did not resolve.".into(),
    };
    Outcome {
        success: v.arrived,
        headline,
        details,
        rescued,
        total: 1 + ww.anchors.len(),
    }
}

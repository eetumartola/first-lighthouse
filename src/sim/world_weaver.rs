//! Variant 3 — World Weaver as maze assembler. World 1 is the sea the ship will sail; Worlds 2–4
//! hold stable alternative sectors. Space copies the previewed sector's land into World 1. At
//! dawn a route finder looks for any hull-clearance passage from the shipping lane to the harbor.

use super::beam::Input;
use super::entity::{EntityId, Form, Status, Target};
use super::geom::{bearing_of, compass_word, Circle};
use super::islands;
use super::route;
use super::steering;
use super::tuning::Tuning;
use super::{Cause, Event, Outcome, Sea};
use glam::Vec2;

pub const LAYER_NAMES: [&str; 4] = ["World 1", "World 2", "World 3", "World 4"];
pub const LAYER_GLYPHS: [&str; 4] = ["I", "II", "III", "IV"];

/// One sector's land vocabulary: an outer reef band, an inner reef band, and an optional radial
/// wall between them. Gaps in the bands are the passages; walls block the ring channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub outer_gap: bool,
    pub inner_gap: bool,
    pub wall: bool,
}

pub const OUTER_R: f32 = 72.0;
pub const INNER_R: f32 = 30.0;

impl Piece {
    const fn new(outer_gap: bool, inner_gap: bool, wall: bool) -> Self {
        Self { outer_gap, inner_gap, wall }
    }

    /// Rock circles for this piece in `sector`. Centres stay inside the sector; bands meet their
    /// neighbours at the seams so coastlines read as continuous.
    pub fn geometry(self, sector: usize, t: &Tuning) -> Vec<Circle> {
        let a = t.sector_angle().to_degrees();
        let mid = sector as f32 * a + a * 0.5;
        let half = a * 0.5 - 1.0;
        let mut rocks = Vec::new();
        let band = |r: f32, rock: f32, gap: Option<f32>| -> Vec<Circle> {
            islands::arc(r, mid - half, mid + half, rock)
                .into_iter()
                .filter(|c| {
                    let deg = bearing_of(c.center).to_degrees();
                    let off = ((deg - mid + 540.0) % 360.0) - 180.0;
                    gap.is_none_or(|g| off.abs() > g)
                })
                .collect()
        };
        rocks.extend(band(OUTER_R, 3.0, self.outer_gap.then_some(10.0)));
        rocks.extend(band(INNER_R, 2.6, self.inner_gap.then_some(14.0)));
        if self.wall {
            rocks.extend(islands::radial(mid, INNER_R + 3.0, 9, 4.0, 2.6));
        }
        rocks
    }
}

#[derive(Clone, Debug, Default)]
pub struct Playback {
    pub route: Option<Vec<Vec2>>,
    pub speed: f32,
    pub elapsed: f32,
    pub ship: Option<EntityId>,
    pub arrived: bool,
    pub failure: Option<String>,
    /// Short beat after a failure before the result shows.
    pub beat: f32,
}

#[derive(Clone, Debug)]
pub struct WorldWeaver {
    /// Sector vocabulary per world; index 0 is World 1's immutable baseline.
    pub worlds: Vec<Vec<Piece>>,
    /// World 1 as currently assembled (mutable; restart rebuilds it from the baseline).
    pub assembled: Vec<Piece>,
    /// Which sectors have been edited (outer-edge markers).
    pub edited: Vec<bool>,
    /// Marked start inside the shipping-lane entrance, and the heading into the sea.
    pub lane_start: Vec2,
    pub lane_heading: f32,
    pub last_layer: u8,
    pub playback: Playback,
    /// Composition frozen at dawn.
    pub built: Option<Vec<Piece>>,
}

impl WorldWeaver {
    pub fn scenario(t: &Tuning) -> Self {
        let p = Piece::new;
        let plain = p(false, false, false);
        // World 1 baseline: the lane enters through sector 7's outer gap (south-west, the only way
        // in from open water), the harbor's inner gaps sit at sectors 11 and 0 (north), and radial
        // walls at 3 and 10 seal both ways round the channel between them.
        let mut world1 = vec![plain; 12];
        world1[7] = p(true, false, false);
        world1[3] = p(false, false, true);
        world1[10] = p(false, false, true);
        world1[11] = p(false, true, false);
        world1[0] = p(false, true, false);
        // World 2: opens the east (sector 3) with an inner shortcut, but walls 5 and 8.
        let mut world2 = vec![plain; 12];
        world2[0] = p(false, true, false);
        world2[3] = p(false, true, false);
        world2[4] = p(true, false, false);
        world2[5] = p(false, false, true);
        world2[8] = p(false, false, true);
        world2[9] = p(true, false, false);
        world2[11] = p(true, false, false);
        // World 3: opens the west (sector 10), but walls 2 and 6 and closes the harbor gap there.
        let mut world3 = vec![plain; 12];
        world3[2] = p(false, false, true);
        world3[6] = p(false, false, true);
        world3[7] = p(true, false, false);
        world3[9] = p(false, true, false);
        world3[10] = p(true, false, false);
        world3[1] = p(true, false, false);
        // World 4: walls at 3 and 10 again, inner gaps at 2 and 8 (lagoon shortcuts).
        let mut world4 = vec![plain; 12];
        world4[2] = p(false, true, false);
        world4[3] = p(false, false, true);
        world4[8] = p(false, true, false);
        world4[10] = p(false, false, true);
        world4[7] = p(false, false, false); // copying this closes the entrance: a real mistake
        world4[5] = p(true, false, false);
        Self {
            worlds: vec![world1.clone(), world2, world3, world4],
            assembled: world1,
            edited: vec![false; t.weaver_sectors],
            lane_start: islands::polar(225.0, 92.0),
            lane_heading: 45f32.to_radians(),
            last_layer: 0,
            playback: Playback::default(),
            built: None,
        }
    }

    /// Index of the world the beam currently inspects (cyclic over the four).
    pub fn layer_for(&self, sea: &Sea) -> u8 {
        sea.beam.revolution().rem_euclid(sea.tuning.weaver_layers as i32) as u8
    }

    /// Piece shown for a sector when inspecting `layer`: World 1 shows the assembled result.
    pub fn piece(&self, layer: u8, sector: usize) -> Piece {
        if layer == 0 {
            self.assembled[sector]
        } else {
            self.worlds[layer as usize][sector]
        }
    }

    /// Land of one previewed slice.
    pub fn slice_geometry(&self, layer: u8, sector: usize, t: &Tuning) -> Vec<Circle> {
        self.piece(layer, sector).geometry(sector, t)
    }

    /// All land of an assembled composition (every sector).
    pub fn composition_land(pieces: &[Piece], t: &Tuning) -> Vec<Circle> {
        pieces.iter().enumerate().flat_map(|(s, p)| p.geometry(s, t)).collect()
    }

    pub fn harbor_goal(t: &Tuning) -> Vec2 {
        t.harbor_center
    }

    /// Route finder over a composition, from the lane start to the harbor.
    pub fn find_passage(&self, pieces: &[Piece], t: &Tuning) -> Option<Vec<Vec2>> {
        let mut land = vec![Circle::new(Vec2::ZERO, t.island_radius)];
        land.extend(Self::composition_land(pieces, t));
        route::find_route(
            &land,
            t.sea_radius,
            t.weaver_route_cell,
            t.ship_radius + t.weaver_route_margin,
            self.lane_start,
            Self::harbor_goal(t),
        )
    }
}

pub fn step_night(ww: &mut WorldWeaver, sea: &mut Sea, input: Input) {
    let layer = ww.layer_for(sea);
    if layer != ww.last_layer {
        ww.last_layer = layer;
        sea.events.push(Event::LayerChanged { layer });
    }
    if input.capture {
        let sector = sea.beam.sector_index(&sea.tuning);
        if layer == 0 {
            sea.events.push(Event::AssembledWorld);
        } else {
            // Copy, not swap: the source world is untouched.
            ww.assembled[sector] = ww.worlds[layer as usize][sector];
            ww.edited[sector] = true;
            let fp = sea.beam.footprint(&sea.tuning);
            sea.charge.stamp(&fp, sea.tuning.weaver_capture_glow);
            sea.events.push(Event::Captured { sector, layer });
        }
    }
}

/// Dawn: freeze exactly the current World 1, lay its land, and look for a passage.
pub fn freeze_and_build(ww: &mut WorldWeaver, sea: &mut Sea) {
    let t = sea.tuning.clone();
    let built = ww.assembled.clone();
    let land = WorldWeaver::composition_land(&built, &t);
    sea.charge.add_land(&land);
    sea.rocks.extend(land);
    let route = ww.find_passage(&built, &t);
    let speed = route
        .as_ref()
        .map(|r| (route::length(r) / t.weaver_playback_target).max(t.weaver_playback_min_speed))
        .unwrap_or(t.weaver_playback_min_speed);
    let id = sea.spawn("Expedition", Form::Ship, ww.lane_start, ww.lane_heading);
    if let Some(e) = sea.entity_mut(id) {
        e.brain.target = Some(Target::Waypoint(1));
        e.brain.desired = ww.lane_heading;
    }
    ww.built = Some(built);
    ww.playback = Playback { route, speed, ship: Some(id), ..Default::default() };
}

pub fn begin_voyage(ww: &mut WorldWeaver, sea: &mut Sea) {
    sea.events.push(Event::VoyageBegins);
    if ww.playback.route.is_none() {
        sea.events.push(Event::NoPassage);
    }
}

/// Returns true when the voyage has resolved.
pub fn step_playback(ww: &mut WorldWeaver, sea: &mut Sea, dt: f32) -> bool {
    let t = sea.tuning.clone();
    let pb = &mut ww.playback;
    let Some(ship_id) = pb.ship else { return true };
    pb.elapsed += dt;
    let Some(idx) = sea.entities.iter().position(|e| e.id == ship_id) else { return true };

    // A failure plays a short beat (drifting or sinking) before the result.
    if pb.failure.is_some() {
        pb.beat += dt;
        return pb.beat >= 2.5;
    }
    let Some(route) = pb.route.clone() else {
        // No passage: the ship holds at the lane entrance for a beat, then the result explains.
        pb.failure = Some("No passage to harbor.".into());
        return false;
    };

    let harbor = sea.harbor();
    let arrived = steering::steer_along_route(&mut sea.entities[idx], &route, pb.speed, 140.0, dt);
    let hull = sea.entities[idx].circle();
    if arrived || harbor.contains(hull.center) {
        pb.arrived = true;
        sea.events.push(Event::VoyageArrived);
        sea.secure(ship_id);
        return true;
    }
    if let Some(rock) = sea.rocks.iter().find(|r| r.overlaps(&hull)).copied() {
        sea.entities[idx].status = Status::Sunk;
        sea.events.push(Event::Sunk { id: ship_id, pos: hull.center, cause: Cause::Rock });
        pb.failure = Some(format!("The ship struck the {} rocks.", compass_word(rock.center)));
        return false;
    }
    if pb.elapsed > t.world_weaver_playback_limit * 2.0 {
        pb.failure = Some("The ship never reached harbor.".into());
    }
    false
}

pub fn outcome(ww: &WorldWeaver, _sea: &Sea) -> Outcome {
    let pb = &ww.playback;
    let edited = ww.edited.iter().filter(|e| **e).count();
    let mut details = vec![format!("{edited} of {} sectors copied into World 1.", ww.edited.len())];
    if let Some(built) = &ww.built {
        let summary: Vec<String> = built
            .iter()
            .enumerate()
            .filter(|(s, _)| ww.edited[*s])
            .map(|(s, p)| {
                let mut parts = Vec::new();
                if p.outer_gap {
                    parts.push("outer gap");
                }
                if p.inner_gap {
                    parts.push("inner gap");
                }
                if p.wall {
                    parts.push("wall");
                }
                if parts.is_empty() {
                    parts.push("reefs");
                }
                format!("sector {}: {}", s + 1, parts.join(", "))
            })
            .collect();
        if !summary.is_empty() {
            details.push(summary.join("; "));
        }
    }
    if let Some(route) = &pb.route {
        details.push(format!("Passage found: {:.0} units through the assembled sea.", route::length(route)));
    }
    let headline = match (&pb.failure, pb.arrived) {
        (Some(reason), _) => reason.clone(),
        (None, true) => "The ship found its way through your sea to the harbor.".into(),
        (None, false) => "The voyage did not resolve.".into(),
    };
    Outcome { success: pb.arrived, headline, details, rescued: pb.arrived as usize, total: 1 }
}

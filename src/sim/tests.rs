//! Scenario-level checks: each authored scenario has a demonstrated successful run driven
//! through the real beam controls, plus the failure modes the design promises.

use super::autopilot::{self, Keeper};
use super::geom::bearing_of;
use super::spiral_voyage::{winding_in_world, world_of};
use super::world_weaver::WorldWeaver;
use super::*;
use std::f32::consts::TAU;

const DT: f32 = 1.0 / 60.0;

fn run_until_finished(w: &mut World, mut policy: impl FnMut(&World) -> Input) {
    let cap = w.night_length.unwrap_or(600.0) + 120.0;
    let max_steps = (cap * 60.0) as usize;
    for _ in 0..max_steps {
        if w.phase == Phase::Finished {
            return;
        }
        let input = policy(w);
        w.step(input, DT);
    }
    panic!("session never finished: {:?}", w.phase);
}

fn skip_dusk(w: &mut World) {
    while w.phase != Phase::Night {
        w.step(Input::default(), DT);
    }
}

// ---------------------------------------------------------------- shared handling

#[test]
fn dusk_shows_the_first_boats_then_darkness_falls_and_nothing_moved() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    let boats: Vec<_> = w.sea.entities.iter().map(|e| (e.name, e.pos)).collect();
    assert_eq!(boats.len(), 2, "two boats should be present at dusk: {boats:?}");
    assert!(w.dusk() > 0.99);
    for e in &w.sea.entities {
        assert_eq!(w.entity_visibility(e), Visibility::Lit, "{} hidden at dusk", e.name);
    }
    // Aim during the fade; nothing else changes.
    for _ in 0..(60.0 * w.tuning().intro_seconds * 0.9) as usize {
        w.step(Input { rotate: 1.0, ..Default::default() }, DT);
    }
    assert!(w.sea.beam.winding > 0.3, "aiming during dusk must work");
    assert!(w.sea.charge.charge.iter().all(|c| *c == 0.0), "no charge before the night");
    for (name, pos) in &boats {
        assert_eq!(w.sea.entity_by_name(name).pos, *pos, "{name} moved during dusk");
    }
    skip_dusk(&mut w);
    assert_eq!(w.dusk(), 0.0);
    // The beam points east now; both boats are off the beam and on unlit water: hidden.
    for e in &w.sea.entities {
        assert_eq!(w.entity_visibility(e), Visibility::Hidden, "{} still visible in darkness", e.name);
    }
}

impl Sea {
    fn entity_by_name(&self, name: &str) -> &Entity {
        self.entities.iter().find(|e| e.name == name).unwrap()
    }
}

#[test]
fn silhouettes_follow_surrounding_glow_and_fade_with_it() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    skip_dusk(&mut w);
    let alder = w.sea.entity_by_name("Alder").id;
    let pos = w.sea.entity(alder).unwrap().pos;
    // Glow beside the hull, not under its centre, still outlines it.
    let side = pos + Vec2::new(w.tuning().ship_radius, 0.0);
    let idx = w.sea.charge.index_of(side).unwrap();
    w.sea.charge.charge[idx] = w.tuning().strong_threshold;
    let vis = |w: &World| w.entity_visibility(w.sea.entity(alder).unwrap());
    assert!(matches!(vis(&w), Visibility::Silhouette(k) if k > 0.99), "{:?}", vis(&w));
    w.sea.charge.charge[idx] = (w.tuning().silhouette_min_glow + w.tuning().strong_threshold) * 0.5;
    assert!(matches!(vis(&w), Visibility::Silhouette(k) if k > 0.3 && k < 0.7));
    w.sea.charge.charge[idx] = w.tuning().silhouette_min_glow * 0.5;
    assert_eq!(vis(&w), Visibility::Hidden);
}

// ---------------------------------------------------------------- Night Watch

#[test]
fn night_watch_unguided_ships_do_not_find_harbor() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    // A keeper who only stares at open water south of the tower guides nobody; the beam's
    // default rest position would otherwise light the harbor approach all night.
    let idle = Vec2::new(0.0, -40.0);
    run_until_finished(&mut w, |w| autopilot::aim_at(w, idle));
    let o = w.outcome.as_ref().unwrap();
    // Alder's authored approach runs straight down the harbor's meridian and hull avoidance can
    // carry it past the rocks in its way, so one lucky mooring is possible; the night still fails.
    assert!(!o.success, "{o:?}");
    assert!(o.rescued <= 1, "{o:?}");
}

#[test]
fn night_watch_attentive_keeper_rescues_target() {
    let mut bot = Keeper::for_mode(Mode::NightWatch);
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    let mut losses = Vec::new();
    for _ in 0..((w.night_length.unwrap() + 120.0) * 60.0) as usize {
        if w.phase == Phase::Finished {
            break;
        }
        let input = bot.input(&w);
        w.step(input, DT);
        for event in w.drain_events() {
            if let Event::Sunk { id, cause, .. } = event {
                let name = w.sea.entity(id).map(|e| e.name).unwrap_or("unknown");
                losses.push((name, cause));
            }
        }
    }
    let o = w.outcome.as_ref().expect("session did not finish");
    assert!(o.success, "expected >= 3 rescues, got {o:#?}; losses: {losses:?}");
}

#[test]
fn night_watch_predator_appears_eats_and_can_be_turned_off() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    let mut appeared = false;
    let steps = ((w.night_length.unwrap() + w.tuning().intro_seconds) * 60.0) as usize;
    for _ in 0..steps {
        w.step(Input::default(), DT);
        appeared |= w.drain_events().iter().any(|e| matches!(e, Event::CreatureAppears { .. }));
    }
    assert!(appeared);
    let mut t = Tuning::default();
    t.night_watch_monster = false;
    let mut w = World::new(Mode::NightWatch, t);
    run_until_finished(&mut w, |_| Input::default());
    assert!(w.sea.entities.iter().all(|e| e.form != Form::Creature), "monster-off setting ignored");
}

#[test]
fn predator_diverted_by_a_brighter_decoy_leaves_the_route_alone() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    skip_dusk(&mut w);
    // Place the predator between a route and a decoy, with no ships around it.
    w.sea.entities.clear();
    let id = w.sea.spawn("Leviathan", Form::Creature, Vec2::new(0.0, -40.0), 0.0);
    if let Rules::NightWatch(nw) = &mut w.rules {
        nw.creature_id = Some(id);
        nw.schedule.clear();
    }
    // The route is detectable from the start but not from the decoy, so the predator's choice
    // between them is made once and the route is not a second target afterwards.
    let route = Vec2::new(35.0, -40.0);
    let decoy = Vec2::new(-18.0, -40.0);
    let ri = w.sea.charge.index_of(route).unwrap();
    let di = w.sea.charge.index_of(decoy).unwrap();
    // The route outlives the run by decaying only; the decoy is eaten well before it would fade.
    w.sea.charge.charge[ri] = 25.0;
    w.sea.charge.charge[di] = 28.0;
    for _ in 0..(60 * 20) {
        w.step(Input::default(), DT);
    }
    let p = w.sea.entity(id).unwrap();
    assert!(p.pos.x < -12.0, "predator did not take the decoy: {:?}", p.pos);
    // The eaten decoy is gone from the same field ships and rendering read; the route decayed only.
    assert!(w.sea.charge.charge[di] < 4.0, "decoy not consumed: {}", w.sea.charge.charge[di]);
    assert!((w.sea.charge.charge[ri] - 5.0).abs() < 0.6, "route was touched: {}", w.sea.charge.charge[ri]);
}

// ---------------------------------------------------------------- Mutable Sea (suspended)

#[test]
fn mutable_sea_is_hidden_from_the_menu_but_its_rules_do_not_run_elsewhere() {
    assert!(!Mode::MENU.contains(&Mode::MutableSea));
    assert_eq!(Mode::MENU.len(), 3);
    for mode in Mode::MENU {
        let w = World::new(mode, Tuning::default());
        assert!(w.sea.entities.iter().all(|e| e.mutable.is_none()), "{mode:?} carries transformation timers");
    }
}

#[test]
fn mutable_sea_still_runs_when_selected_directly() {
    let mut w = World::new(Mode::MutableSea, Tuning::default());
    run_until_finished(&mut w, |_| Input::default());
    assert!(w.outcome.is_some());
}

// ---------------------------------------------------------------- World Weaver

fn weaver_world(commits: &[(usize, u8)]) -> World {
    let mut w = World::new(Mode::WorldWeaver, Tuning::default());
    if let Rules::WorldWeaver(ww) = &mut w.rules {
        for &(s, l) in commits {
            ww.assembled[s] = ww.worlds[l as usize][s];
            ww.edited[s] = true;
        }
    }
    skip_dusk(&mut w);
    w.night_elapsed = w.night_length.unwrap();
    w
}

fn weaver(w: &World) -> &WorldWeaver {
    match &w.rules {
        Rules::WorldWeaver(ww) => ww,
        _ => unreachable!(),
    }
}

#[test]
fn world_weaver_baseline_has_no_passage_and_fails_clearly() {
    let mut w = weaver_world(&[]);
    assert!(weaver(&w).find_passage(&weaver(&w).assembled, w.tuning()).is_none());
    run_until_finished(&mut w, |_| Input::default());
    let o = w.outcome.as_ref().unwrap();
    assert!(!o.success);
    assert_eq!(o.headline, "No passage to harbor.");
}

#[test]
fn world_weaver_two_different_assemblies_connect_the_endpoints() {
    for plan in [autopilot::world_weaver_solution(), autopilot::world_weaver_alternative()] {
        let mut w = weaver_world(&plan);
        let route = weaver(&w).find_passage(&weaver(&w).assembled, w.tuning()).expect("passage");
        // The route crosses several sectors, not one radial strip.
        let sectors: std::collections::BTreeSet<usize> = route
            .iter()
            .map(|p| (bearing_of(*p) / w.tuning().sector_angle()) as usize)
            .collect();
        assert!(sectors.len() >= 4, "route only touched sectors {sectors:?}");
        run_until_finished(&mut w, |_| Input::default());
        let o = w.outcome.as_ref().unwrap();
        assert!(o.success, "plan {plan:?} failed: {o:#?}");
        // Playback stayed within the target window.
        let pb = &weaver(&w).playback;
        assert!(pb.elapsed <= w.tuning().world_weaver_playback_limit, "playback took {}", pb.elapsed);
    }
}

#[test]
fn world_weaver_copy_changes_only_the_destination_and_the_source_is_unchanged() {
    let mut w = World::new(Mode::WorldWeaver, Tuning::default());
    skip_dusk(&mut w);
    let t = w.tuning().clone();
    let steps_per_sector = (t.beam_turn_seconds / t.weaver_sectors as f32 * 60.0) as usize;
    let forward = Input { rotate: 1.0, ..Default::default() };
    let baseline = weaver(&w).worlds[0].clone();
    let worlds_before = weaver(&w).worlds.clone();
    // Space while inspecting World 1 does nothing but say so.
    w.step(Input { capture: true, ..Default::default() }, DT);
    assert!(w.drain_events().contains(&Event::AssembledWorld));
    assert_eq!(weaver(&w).assembled, baseline);
    // Wind forward into World 2 and stop in sector 3 (the beam has inertia, so wind until there).
    let mut guard = 0;
    while !(weaver(&w).layer_for(&w.sea) == 1 && w.sea.beam.sector_index(&t) == 3) {
        w.step(forward, DT);
        guard += 1;
        assert!(guard < 60 * 60, "never reached World 2 sector 3");
    }
    w.step(Input { capture: true, ..forward }, DT);
    let ww = weaver(&w);
    assert_eq!(ww.assembled[3], ww.worlds[1][3]);
    assert!(ww.edited[3]);
    for s in 0..t.weaver_sectors {
        if s != 3 {
            assert_eq!(ww.assembled[s], baseline[s], "sector {s} changed");
            assert!(!ww.edited[s]);
        }
    }
    assert_eq!(ww.worlds, worlds_before, "copying must not alter any source world");
    // Winding back to World 1 shows the accumulated edit in the preview.
    let back = Input { rotate: -1.0, ..Default::default() };
    for _ in 0..(steps_per_sector * 12) {
        w.step(back, DT);
    }
    assert_eq!(weaver(&w).layer_for(&w.sea), 0);
    assert_eq!(weaver(&w).piece(0, 3), weaver(&w).worlds[1][3]);
    // Browsing another world afterwards never silently changes it.
    for _ in 0..(steps_per_sector * 26) {
        w.step(forward, DT);
    }
    assert_eq!(weaver(&w).assembled[3], weaver(&w).worlds[1][3]);
}

#[test]
fn world_weaver_pieces_keep_rock_centres_inside_their_sector() {
    let t = Tuning::default();
    let ww = WorldWeaver::scenario(&t);
    let a = t.sector_angle();
    for (wi, world) in ww.worlds.iter().enumerate() {
        for (s, piece) in world.iter().enumerate() {
            for rock in piece.geometry(s, &t) {
                let sector = (bearing_of(rock.center) / a) as usize;
                assert_eq!(sector, s, "world {wi} sector {s} rock at {:?}", rock.center);
            }
        }
    }
}

#[test]
fn world_weaver_dawn_freezes_exactly_the_assembled_world_and_the_route_is_executable() {
    let mut w = weaver_world(&autopilot::world_weaver_solution());
    let expected = weaver(&w).assembled.clone();
    let expected_land = WorldWeaver::composition_land(&expected, w.tuning());
    while w.phase != Phase::Playback {
        w.step(Input::default(), DT);
    }
    assert_eq!(weaver(&w).built.as_ref().unwrap(), &expected);
    assert_eq!(w.sea.rocks.len(), 1 + expected_land.len());
    // Every route segment keeps hull clearance.
    let route = weaver(&w).playback.route.clone().unwrap();
    let clearance = w.tuning().ship_radius + w.tuning().weaver_route_margin;
    for seg in route.windows(2) {
        assert!(super::route::segment_clear(seg[0], seg[1], &w.sea.rocks, w.tuning().sea_radius, clearance));
    }
}

// ---------------------------------------------------------------- Spiral Voyage

fn spiral(w: &World) -> &spiral_voyage::SpiralVoyage {
    match &w.rules {
        Rules::SpiralVoyage(sv) => sv,
        _ => unreachable!(),
    }
}

#[test]
fn spiral_beam_charges_the_world_each_cell_resolves_to_from_the_ship() {
    let mut tuning = Tuning::default();
    tuning.spiral_ship_speed_factor = 0.0;
    let mut w = World::new(Mode::SpiralVoyage, tuning);
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    // Ship in World 1 just east of the south seam, bow west. Straight ahead across the seam is
    // World 2 water; the open water east of the lighthouse behind it is still World 1.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = Vec2::new(6.0, -40.0);
        e.winding = winding_in_world(0, bearing_of(e.pos));
        e.heading = 270f32.to_radians();
        e.brain.desired = e.heading;
    }
    // (target, world its light must land in). Afterglow fades while the beam travels, so each
    // target is checked right after its dwell.
    let ahead = (Vec2::new(-6.0, -40.0), 1);
    let behind = (Vec2::new(40.0, 0.0), 0);
    for (target, lit_world) in [ahead, behind] {
        for _ in 0..(60 * 20) {
            w.step(autopilot::aim_at(&w, target), DT);
            if w.footprint().contains(target) {
                break;
            }
        }
        assert!(w.footprint().contains(target), "beam never reached {target:?}");
        for _ in 0..60 {
            w.step(Input::default(), DT);
        }
        let sv = spiral(&w);
        for (i, world) in sv.worlds.iter().enumerate() {
            let c = world.charge.charge_at(target);
            if i == lit_world {
                assert!(c > 0.5, "world {} unlit at {target:?}", i + 1);
            } else {
                assert_eq!(c, 0.0, "world {} lit at {target:?}", i + 1);
            }
        }
        // The composite the player sees is exactly the ship's reading of the spiral.
        assert_eq!(w.view_charge().charge_at(target), sv.worlds[lit_world].charge.charge_at(target));
    }
    // The ship is in the world on view whatever the beam does; only darkness can hide it.
    let e = w.sea.entity(ship).unwrap();
    assert_eq!(w.entity_world(e), w.inspected_world());
}

#[test]
fn spiral_view_is_continuous_at_the_seam_and_differs_only_at_the_antipode() {
    let w = World::new(Mode::SpiralVoyage, Tuning::default());
    let n = w.tuning().spiral_worlds;
    // Bearings run clockwise from north; a clockwise ship passes the south seam from 179° to 181°.
    // Ship just before the seam in World 1.
    let before = spiral_voyage::Perspective { winding: winding_in_world(0, 179f32.to_radians()), bearing: 179f32.to_radians(), worlds: n };
    // Ship just after it in World 2.
    let after = spiral_voyage::Perspective { winding: winding_in_world(1, 181f32.to_radians()), bearing: 181f32.to_radians(), worlds: n };
    let r = 50.0;
    for deg in (0..360).step_by(3) {
        let p = geom::dir((deg as f32).to_radians()) * r;
        let (a, b) = (before.world_at(p), after.world_at(p));
        // Only positions within a few degrees of the ship's antipode (north) may disagree.
        if geom::angle_delta(0.0, bearing_of(p)).abs() > 4f32.to_radians() {
            assert_eq!(a, b, "view changed at bearing {deg} while the ship crossed the seam");
        }
    }
    // Past the seam is the next world, before it the previous, for both observers.
    let past_seam = geom::dir(181f32.to_radians()) * r;
    let before_seam = geom::dir(179f32.to_radians()) * r;
    assert_eq!(before.world_at(past_seam), 1);
    assert_eq!(before.world_at(before_seam), 0);
    assert_eq!(after.world_at(past_seam), 1);
    assert_eq!(after.world_at(before_seam), 0);
}

#[test]
fn spiral_ship_crosses_the_seam_by_sailing_in_both_directions_with_continuous_motion() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    let t = w.tuning().clone();
    let row = (1..level::rows(level::MODE4_LEVEL1) - 1)
        .rev()
        .find(|&row| {
            level::is_free(level::MODE4_LEVEL1, 0, level::COLUMNS - 1, row)
                && level::is_free(level::MODE4_LEVEL1, 1, 0, row)
        })
        .unwrap();
    let exit = level::cell_center(level::MODE4_LEVEL1, level::COLUMNS - 1, row, t.island_radius, t.sea_radius);
    let entry = level::cell_center(level::MODE4_LEVEL1, 0, row, t.island_radius, t.sea_radius);
    // Point the ship clockwise across the authored south-seam opening.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = exit;
        e.winding = winding_in_world(0, bearing_of(e.pos));
        e.heading = (bearing_of(exit) + std::f32::consts::FRAC_PI_2).rem_euclid(TAU);
        e.brain.desired = e.heading;
    }
    let mut crossed = Vec::new();
    for _ in 0..(60 * 6) {
        w.step(Input::default(), DT);
        for ev in w.drain_events() {
            if let Event::ShipCrossed { world } = ev {
                crossed.push(world);
            }
        }
        if !crossed.is_empty() {
            break;
        }
    }
    assert_eq!(crossed, vec![1], "crossing events: {crossed:?}");
    let e = w.sea.entity(ship).unwrap();
    let continuous_limit =
        exit.length() * (spiral_voyage::SEAM_BAND + TAU / (2.0 * level::COLUMNS as f32)) + t.ship_length;
    assert!(e.pos.distance(exit) < continuous_limit, "position jumped at the seam: {:?}", e.pos);
    assert!(e.is_active(), "ship grounded in the authored seam opening");
    // Start just inside World 2 and retrace east across the same physical seam.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = entry;
        e.winding = winding_in_world(1, bearing_of(e.pos));
        e.heading = (bearing_of(entry) - std::f32::consts::FRAC_PI_2).rem_euclid(TAU);
        e.brain.desired = e.heading;
    }
    let mut back = Vec::new();
    for _ in 0..(60 * 8) {
        w.step(Input::default(), DT);
        for ev in w.drain_events() {
            if let Event::ShipCrossed { world } = ev {
                back.push(world);
            }
        }
        if !back.is_empty() {
            break;
        }
    }
    assert_eq!(back, vec![0]);
}

#[test]
fn spiral_ship_cannot_sail_before_world_one() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = Vec2::new(-3.0, -40.0);
        e.winding = winding_in_world(0, bearing_of(e.pos)); // just inside World 1's lower boundary
        e.heading = 90f32.to_radians();
        e.brain.desired = e.heading;
    }
    for _ in 0..(60 * 6) {
        w.step(Input::default(), DT);
    }
    let e = w.sea.entity(ship).unwrap();
    assert_eq!(world_of(e.winding, 4), 0);
    assert!(e.winding >= level::SEAM);
    assert!(e.is_active(), "the voyage boundary must not sink the ship");
}

#[test]
fn spiral_seam_sampling_reads_the_neighbouring_world() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    // Ship in World 1 just before the south seam, heading west; a bright trail lies just after
    // that seam in World 2. The ship must read the neighbouring world's water.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = Vec2::new(6.0, -40.0);
        e.winding = winding_in_world(0, bearing_of(e.pos));
        e.heading = 270f32.to_radians();
        e.brain.desired = e.heading;
    }
    if let Rules::SpiralVoyage(sv) = &mut w.rules {
        for x in 0..12 {
            let p = Vec2::new(-(x as f32) * 1.5, -40.0 + x as f32 * 0.6);
            let idx = sv.worlds[1].charge.index_of(p).unwrap();
            sv.worlds[1].charge.charge[idx] = 15.0;
        }
    }
    for _ in 0..30 {
        w.step(Input::default(), DT);
    }
    let e = w.sea.entity(ship).unwrap();
    assert!(e.brain.desired_score > w.tuning().guidance_min_score, "seam-adjacent trail not read: {}", e.brain.desired_score);
}

#[test]
fn spiral_keeper_brings_the_ship_through_four_worlds_to_harbor() {
    let mut bot = Keeper::for_mode(Mode::SpiralVoyage);
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    let mut crossings = Vec::new();
    let mut elapsed = 0.0;
    let mut grounding = None;
    loop {
        if w.phase == Phase::Finished {
            break;
        }
        let input = bot.input(&w);
        w.step(input, DT);
        elapsed += DT;
        for ev in w.drain_events() {
            match ev {
                Event::ShipCrossed { world } => crossings.push(world),
                Event::Sunk { id, pos, cause } if Some(id) == spiral(&w).ship => {
                    let world = spiral(&w).ship_world;
                    let route = &bot.world_routes[world];
                    let nearest = route
                        .iter()
                        .enumerate()
                        .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)));
                    let rock = spiral(&w).worlds[world]
                        .rocks
                        .iter()
                        .min_by(|a, b| a.center.distance(pos).total_cmp(&b.center.distance(pos)));
                    grounding = Some(format!(
                        "{cause:?} at {pos:?}, world {}, nearest route {:?}, nearest rock {:?}, beam {:.1}°/{:.1}",
                        world + 1,
                        nearest.map(|(i, p)| (i, *p, p.distance(pos))),
                        rock.map(|r| (r.center, r.radius, r.center.distance(pos))),
                        w.sea.beam.bearing().to_degrees(),
                        w.sea.beam.range,
                    ));
                }
                _ => {}
            }
        }
        if elapsed >= 900.0 {
            let ship = spiral(&w).ship.and_then(|id| w.sea.entity(id));
            panic!(
                "voyage never resolved; crossings {crossings:?}; ship {ship:#?}; beam {:.1}°/{:.1}",
                w.sea.beam.bearing().to_degrees(),
                w.sea.beam.range,
            );
        }
    }
    let o = w.outcome.as_ref().unwrap();
    assert!(o.success, "{o:#?} crossings {crossings:?}; grounding: {grounding:?}");
    assert_eq!(crossings, vec![1, 2, 3]);
    let route_lengths: Vec<_> = bot.world_routes.iter().map(|route| super::route::length(route)).collect();
    assert!(elapsed < 420.0, "voyage too long: {elapsed:.0} s; routes {route_lengths:?}");
}

// ---------------------------------------------------------------- integration

#[test]
fn modes_start_fresh_and_carry_no_state() {
    for mode in Mode::ALL {
        let a = World::new(mode, Tuning::default());
        let mut b = World::new(mode, Tuning::default());
        for _ in 0..600 {
            b.step(Input { rotate: 1.0, range: 1.0, capture: true }, DT);
        }
        let c = World::new(mode, Tuning::default());
        assert_eq!(a.sea.entities.len(), c.sea.entities.len());
        assert_eq!(c.sea.time, 0.0);
        assert_eq!(c.sea.beam.winding, a.sea.beam.winding);
        assert!(c.view_charge().charge.iter().all(|c| *c == 0.0));
        if let (Rules::WorldWeaver(x), Rules::WorldWeaver(y)) = (&a.rules, &c.rules) {
            assert_eq!(x.assembled, y.assembled);
            assert!(y.edited.iter().all(|e| !e));
        }
    }
}

#[test]
fn arriving_ship_shows_at_the_edge_then_fades() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    // Cormorant is the first night-time arrival (the dusk boats are visible anyway).
    let ship = loop {
        w.step(Input::default(), DT);
        if let Some(s) = w.sea.entities.iter().find(|e| e.name == "Cormorant") {
            break s.id;
        }
        assert!(w.phase != Phase::Finished, "Cormorant never arrived");
    };
    // Fade in, full in the middle, fade out: sample the strength through the window.
    let strength = |w: &World| match w.entity_visibility(w.sea.entity(ship).unwrap()) {
        Visibility::Silhouette(k) => k,
        _ => 0.0,
    };
    let frames = (60.0 * w.tuning().ship_arrival_reveal_seconds) as usize;
    let mut samples = Vec::with_capacity(frames);
    for _ in 0..frames {
        w.step(Input::default(), DT);
        samples.push(strength(&w));
    }
    let (early, mid, late) = (samples[frames / 10], samples[frames / 2], samples[frames - frames / 10]);
    assert!(mid > 0.5, "arrival never revealed the ship: mid {mid}");
    assert!(early < mid && late < mid, "no fade: early {early} mid {mid} late {late}");
    w.step(Input::default(), DT);
    assert_eq!(w.entity_visibility(w.sea.entity(ship).unwrap()), Visibility::Hidden, "reveal did not end");
}

#[test]
fn creature_stays_dark_through_its_calls_and_skip_to_dawn_ends_the_night() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    let creature = loop {
        w.step(Input::default(), DT);
        if let Some(c) = w.sea.entities.iter().find(|e| e.form == Form::Creature) {
            break c.id;
        }
        assert!(w.phase == Phase::Night || matches!(w.phase, Phase::Intro { .. }), "creature never appeared");
    };
    // Unlit water never shows the body: not on arrival, not when it calls (the call is a sound
    // cue; its glowing eyes are presentation). Run past two call periods with the beam parked.
    let mut calls = 0;
    for _ in 0..(60.0 * w.tuning().creature_call_period * 2.2) as usize {
        w.step(Input::default(), DT);
        calls += w.drain_events().iter().filter(|e| matches!(e, Event::CreatureCall { .. })).count();
        let e = w.sea.entity(creature).unwrap();
        if w.sea.charge.charge_at(e.pos) < w.tuning().silhouette_min_glow {
            assert_eq!(w.entity_visibility(e), Visibility::Hidden, "creature showed in the dark");
        }
    }
    assert!(calls >= 2, "creature never called: {calls}");
    w.skip_to_dawn();
    w.step(Input::default(), DT);
    assert!(matches!(w.phase, Phase::Dawn { .. }), "{:?}", w.phase);
}

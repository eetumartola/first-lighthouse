//! Scenario-level checks: each authored scenario has a demonstrated successful run driven
//! through the real beam controls, plus the failure modes the design promises.

use super::autopilot::{self, Keeper};
use super::geom::{angle_delta, bearing_of};
use super::spiral_voyage::world_of;
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
    run_until_finished(&mut w, |_| Input::default());
    let o = w.outcome.as_ref().unwrap();
    assert_eq!(o.rescued, 0, "{o:?}");
    assert!(!o.success);
}

#[test]
fn night_watch_attentive_keeper_rescues_target() {
    let mut bot = Keeper::for_mode(Mode::NightWatch);
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    run_until_finished(&mut w, |w| bot.input(w));
    let o = w.outcome.as_ref().unwrap();
    assert!(o.success, "expected >= 3 rescues, got {o:#?}");
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
fn spiral_beam_and_ship_can_be_in_different_worlds_and_charge_stays_local() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    let ship_world0 = w.entity_world(w.sea.entity(ship).unwrap());
    assert_eq!(ship_world0, 0);
    // Put the ship on open dark water (south, sailing west) so the beam's full-circuit sweep
    // through World 1 lights nothing within its lookahead; the test is about the ship sailing on.
    if let Some(e) = w.sea.entity_mut(ship) {
        e.pos = Vec2::new(0.0, -60.0);
        e.heading = 270f32.to_radians();
        e.brain.desired = e.heading;
        e.winding = bearing_of(e.pos);
    }
    // Wind the beam a full circuit forward into World 2 and dwell there.
    let t = w.tuning().clone();
    for _ in 0..(t.beam_turn_seconds * 60.0) as usize + 60 {
        w.step(Input { rotate: 1.0, ..Default::default() }, DT);
    }
    assert_eq!(w.inspected_world(), 1);
    let before = w.sea.entity(ship).unwrap().pos;
    for _ in 0..(60 * 3) {
        w.step(Input::default(), DT);
    }
    let after = w.sea.entity(ship).unwrap().pos;
    assert!(before.distance(after) > 5.0, "ship paused while the beam was elsewhere");
    assert_eq!(w.entity_world(w.sea.entity(ship).unwrap()), 0, "ship teleported with the beam");
    // Charge landed in World 2 only, at the footprint.
    let fp = w.footprint();
    let center = fp.center();
    let sv = spiral(&w);
    assert!(sv.worlds[1].charge.charge_at(center) > 5.0);
    assert_eq!(sv.worlds[0].charge.charge_at(center), 0.0);
    assert_eq!(sv.worlds[2].charge.charge_at(center), 0.0);
    // The ship is hidden while another world is inspected.
    assert_eq!(w.entity_visibility(w.sea.entity(ship).unwrap()), Visibility::Hidden);
}

#[test]
fn spiral_beam_winding_is_finite_and_never_wraps_back() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let t = w.tuning().clone();
    // Beam-only check: keep the unguided ship where it started so the voyage cannot end.
    let ship = w.sea.entities[0].id;
    let start = w.sea.entities[0].pos;
    let wind = |w: &mut World, rotate: f32| {
        for _ in 0..(t.beam_turn_seconds * 60.0 * 6.0) as usize {
            w.step(Input { rotate, ..Default::default() }, DT);
            w.sea.entity_mut(ship).unwrap().pos = start;
        }
    };
    wind(&mut w, 1.0);
    assert_eq!(w.phase, Phase::Night);
    assert_eq!(w.inspected_world(), t.spiral_worlds - 1);
    assert!(w.sea.beam.winding < t.spiral_worlds as f32 * TAU);
    wind(&mut w, -1.0);
    assert_eq!(w.inspected_world(), 0);
    assert_eq!(w.sea.beam.winding, 0.0);
}

#[test]
fn spiral_ship_crosses_the_seam_by_sailing_in_both_directions_with_continuous_motion() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    // Point the ship straight across the seam from just west of north.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = Vec2::new(-3.0, 40.0);
        e.winding = bearing_of(e.pos);
        e.heading = 90f32.to_radians();
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
    }
    let e = w.sea.entity(ship).unwrap();
    assert_eq!(crossed, vec![1], "crossing events: {crossed:?}");
    assert_eq!(world_of(e.winding, 4), 1);
    assert!(e.pos.x > 3.0 && (e.pos.y - 40.0).abs() < 2.0, "position jumped at the seam: {:?}", e.pos);
    assert!(angle_delta(e.heading, 90f32.to_radians()).abs() < 0.05, "heading changed at the seam");
    // Sail back west across the seam: retraces into World 1.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.heading = 270f32.to_radians();
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
        e.pos = Vec2::new(3.0, 40.0);
        e.winding = bearing_of(e.pos); // just east of north in World 1
        e.heading = 270f32.to_radians();
        e.brain.desired = e.heading;
    }
    for _ in 0..(60 * 6) {
        w.step(Input::default(), DT);
    }
    let e = w.sea.entity(ship).unwrap();
    assert_eq!(world_of(e.winding, 4), 0);
    assert!(e.winding >= 0.0);
    assert!(e.is_active(), "the voyage boundary must not sink the ship");
}

#[test]
fn spiral_seam_sampling_reads_the_neighbouring_world() {
    let mut w = World::new(Mode::SpiralVoyage, Tuning::default());
    skip_dusk(&mut w);
    let ship = spiral(&w).ship.unwrap();
    // Ship in World 1 just west of north, heading east; a bright trail lies just east of north in
    // World 2 (where it will arrive) and nothing in World 1 there. The ship should keep heading
    // toward it rather than treating the seam as dark.
    {
        let e = w.sea.entity_mut(ship).unwrap();
        e.pos = Vec2::new(-6.0, 40.0);
        e.winding = bearing_of(e.pos);
        e.heading = 100f32.to_radians();
        e.brain.desired = e.heading;
    }
    if let Rules::SpiralVoyage(sv) = &mut w.rules {
        for x in 0..12 {
            let p = Vec2::new(x as f32 * 1.5, 40.0 - x as f32 * 0.6);
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
    loop {
        if w.phase == Phase::Finished {
            break;
        }
        let input = bot.input(&w);
        w.step(input, DT);
        elapsed += DT;
        for ev in w.drain_events() {
            if let Event::ShipCrossed { world } = ev {
                crossings.push(world);
            }
        }
        assert!(elapsed < 900.0, "voyage never resolved; crossings {crossings:?}");
    }
    let o = w.outcome.as_ref().unwrap();
    assert!(o.success, "{o:#?} crossings {crossings:?}");
    assert_eq!(crossings, vec![1, 2, 3]);
    assert!(elapsed < 420.0, "voyage too long: {elapsed:.0} s");
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

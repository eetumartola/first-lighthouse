//! Scenario-level checks: each authored scenario has a demonstrated successful run driven
//! through the real beam controls, plus the failure modes the design promises.

use super::autopilot::{aim_at, Keeper};
use super::geom::{angle_delta, bearing_of};
use super::*;
use std::collections::HashMap;

const DT: f32 = 1.0 / 60.0;

fn run_until_finished(w: &mut World, mut policy: impl FnMut(&World) -> Input) {
    let max_steps = ((w.night_length + 60.0) * 60.0) as usize;
    for _ in 0..max_steps {
        if w.phase == Phase::Finished {
            return;
        }
        let input = policy(w);
        w.step(input, DT);
    }
    panic!("session never finished: {:?}", w.phase);
}

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
fn night_watch_arrivals_overlap_and_creature_appears() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    let mut max_active = 0;
    let mut creature_seen = false;
    let steps = ((w.night_length + w.tuning().intro_seconds) * 60.0) as usize;
    for _ in 0..steps {
        w.step(Input::default(), DT);
        let active = w.sea.entities.iter().filter(|e| e.is_active_ship()).count();
        max_active = max_active.max(active);
        for ev in w.drain_events() {
            if matches!(ev, Event::CreatureAppears { .. }) {
                creature_seen = true;
            }
        }
    }
    assert!(max_active >= 2, "arrivals never overlapped");
    assert!(max_active <= w.tuning().night_watch_max_active_ships);
    assert!(creature_seen);
}

#[test]
fn mutable_sea_light_pauses_but_never_resets_timers() {
    let mut w = World::new(Mode::MutableSea, Tuning::default());
    while w.phase != Phase::Night {
        w.step(Input::default(), DT);
    }
    let id = w.sea.entities.iter().find(|e| e.name == "Merlin").unwrap().id;
    let start = w.sea.entity(id).unwrap().mutable.unwrap().progress;
    // Track Merlin with the footprint until it is directly lit.
    let mut lit_at = None;
    for _ in 0..(60 * 6) {
        let pos = w.sea.entity(id).unwrap().pos;
        let input = aim_at(&w, pos);
        w.step(input, DT);
        if lit_at.is_none() && w.footprint().contains(w.sea.entity(id).unwrap().pos) {
            lit_at = Some(w.sea.entity(id).unwrap().mutable.unwrap().progress);
        }
    }
    let lit_at = lit_at.expect("beam never reached Merlin");
    assert!(lit_at >= start, "light reset the timer: {lit_at} < {start}");
    // Keep tracking: the timer must stand still under direct light.
    for _ in 0..(60 * 4) {
        let pos = w.sea.entity(id).unwrap().pos;
        let input = aim_at(&w, pos);
        w.step(input, DT);
    }
    let merlin = w.sea.entity(id).unwrap();
    assert_eq!(merlin.form, Form::Ship, "Merlin transformed despite being lit");
    let held = merlin.mutable.unwrap().progress;
    assert!((held - lit_at).abs() < 0.3, "light did not pause the timer: {lit_at} -> {held}");
    // Turn the beam away. The ship sits on strongly charged water for a few seconds (still
    // preserved, same threshold as visibility), then sails into darkness, the timer resumes and
    // the ship becomes a wreck.
    let needed = w.tuning().mutable_dark_durations[0] - held;
    let mut wrecked_after = None;
    for step in 0..(((needed + 6.0) * 60.0) as usize) {
        w.step(Input { rotate: 1.0, ..Default::default() }, DT);
        for ev in w.drain_events() {
            if let Event::Transformed { id: eid, from: Form::Ship, to: Form::Wreck, .. } = ev {
                if eid == id {
                    wrecked_after = Some(step as f32 * DT);
                }
            }
        }
    }
    let wrecked_after = wrecked_after.expect("darkness never advanced the timer to a wreck");
    assert!(wrecked_after > needed + 1.0, "strong afterglow did not preserve the ship: {wrecked_after}");
}

#[test]
fn mutable_sea_darkness_cycles_forms_and_preserves_identity() {
    let mut w = World::new(Mode::MutableSea, Tuning::default());
    let osprey_id = w.sea.entities.iter().find(|e| e.name == "Osprey").unwrap().id;
    let d = w.tuning().mutable_dark_durations;
    // Never touch the beam; Osprey (creature, progress 6) becomes an island after 6 s of night,
    // then a ship 8 s later.
    let intro = w.tuning().intro_seconds;
    let mut forms = Vec::new();
    for _ in 0..(((intro + (d[2] - 6.0) + d[3] + 1.0) * 60.0) as usize) {
        w.step(Input::default(), DT);
        for ev in w.drain_events() {
            if let Event::Transformed { id, from, to, .. } = ev {
                if id == osprey_id {
                    forms.push((from, to));
                }
            }
        }
    }
    assert_eq!(forms, vec![(Form::Creature, Form::Island), (Form::Island, Form::Ship)]);
    let osprey = w.sea.entity(osprey_id).unwrap();
    assert_eq!(osprey.name, "Osprey");
    assert_eq!(osprey.form, Form::Ship);
}

#[test]
fn mutable_sea_attentive_keeper_secures_two() {
    let mut bot = Keeper::for_mode(Mode::MutableSea);
    let mut w = World::new(Mode::MutableSea, Tuning::default());
    run_until_finished(&mut w, |w| bot.input(w));
    let o = w.outcome.as_ref().unwrap();
    assert!(o.success, "expected >= 2 secured, got {o:#?}");
}

#[test]
fn mutable_sea_secured_identities_never_transform() {
    let mut bot = Keeper::for_mode(Mode::MutableSea);
    let mut w = World::new(Mode::MutableSea, Tuning::default());
    let mut secured_forms: HashMap<EntityId, Form> = HashMap::new();
    run_until_finished(&mut w, |w| {
        for e in w.sea.entities.iter().filter(|e| e.status == Status::Secured) {
            let prev = secured_forms.insert(e.id, e.form);
            assert!(prev.is_none_or(|p| p == e.form), "secured identity changed form");
        }
        bot.input(w)
    });
    assert!(!secured_forms.is_empty());
    for (id, f) in &secured_forms {
        assert_eq!(*f, Form::Ship);
        assert_eq!(w.sea.entity(*id).unwrap().status, Status::Secured);
    }
}

fn weaver_with(commits: &[(usize, u8)]) -> World {
    let mut w = World::new(Mode::WorldWeaver, Tuning::default());
    if let Rules::WorldWeaver(ww) = &mut w.rules {
        for &(s, l) in commits {
            ww.committed[s] = Some(l);
        }
    }
    // Jump to dawn: the night is only editing.
    w.night_elapsed = w.night_length;
    while w.phase != Phase::Night {
        w.step(Input::default(), DT);
    }
    w
}

#[test]
fn world_weaver_default_composition_fails_with_a_named_reason() {
    let mut w = weaver_with(&[]);
    run_until_finished(&mut w, |_| Input::default());
    let o = w.outcome.as_ref().unwrap();
    assert!(!o.success);
    assert_eq!(o.headline, "The north-eastern island blocked the route.");
}

#[test]
fn world_weaver_authored_solution_succeeds_with_bonus_rescues() {
    let mut w = weaver_with(&autopilot::world_weaver_solution());
    run_until_finished(&mut w, |_| Input::default());
    let o = w.outcome.as_ref().unwrap();
    assert!(o.success, "{o:#?}");
    assert!(o.rescued >= 3, "expected bonus rescues, got {o:#?}");
    if let Rules::WorldWeaver(ww) = &w.rules {
        assert!(!ww.voyage.handled_wrecks.is_empty(), "the Fulmar wreck should have delayed the voyage");
    }
}

#[test]
fn world_weaver_creature_on_the_lane_is_a_real_threat() {
    let mut w = weaver_with(&[(1, 1), (2, 1), (3, 1), (4, 1), (5, 3), (6, 1)]);
    run_until_finished(&mut w, |_| Input::default());
    let o = w.outcome.as_ref().unwrap();
    assert!(!o.success, "{o:#?}");
    assert!(o.headline.contains("creature"), "{o:#?}");
}

#[test]
fn world_weaver_preview_is_stable_and_capture_is_explicit() {
    let mut w = World::new(Mode::WorldWeaver, Tuning::default());
    while w.phase != Phase::Night {
        w.step(Input::default(), DT);
    }
    let t = w.tuning().clone();
    let steps_per_sector = (t.beam_turn_seconds / t.weaver_sectors as f32 * 60.0) as usize;
    let forward = Input { rotate: 1.0, ..Default::default() };
    let backward = Input { rotate: -1.0, ..Default::default() };
    let layer = |w: &World| match &w.rules {
        Rules::WorldWeaver(ww) => ww.layer_for(&w.sea),
        _ => unreachable!(),
    };
    let committed = |w: &World| match &w.rules {
        Rules::WorldWeaver(ww) => ww.committed.clone(),
        _ => unreachable!(),
    };
    // The beam starts mid-sector 0. Wind one full revolution: layer 1, sector 0.
    for _ in 0..(steps_per_sector * 12) {
        w.step(forward, DT);
    }
    assert_eq!(layer(&w), 1);
    assert_eq!(w.sea.beam.sector_index(&t), 0);
    assert!(committed(&w).iter().all(Option::is_none), "browsing must not commit");
    // Preview of sector 0 in layer 1 is deterministic.
    let preview_a: Vec<_> = match &w.rules {
        Rules::WorldWeaver(ww) => ww.preview(1, 1).map(|(a, f)| (a.name, f)).collect(),
        _ => unreachable!(),
    };
    // Capture sector 0 at layer 1.
    w.step(Input { capture: true, ..forward }, DT);
    assert_eq!(committed(&w)[0], Some(1));
    // Reverse back below the seam: layer 0 again, sector 11; sector 0's commitment untouched.
    for _ in 0..steps_per_sector {
        w.step(backward, DT);
    }
    assert_eq!(layer(&w), 0);
    assert_eq!(w.sea.beam.sector_index(&t), 11);
    assert_eq!(committed(&w)[0], Some(1));
    assert!(committed(&w)[11].is_none());
    // Wind forward again to sector 1 in layer 1: the same candidates appear.
    for _ in 0..(steps_per_sector * 2) {
        w.step(forward, DT);
    }
    assert_eq!(layer(&w), 1);
    let preview_b: Vec<_> = match &w.rules {
        Rules::WorldWeaver(ww) => ww.preview(1, 1).map(|(a, f)| (a.name, f)).collect(),
        _ => unreachable!(),
    };
    assert_eq!(preview_a, preview_b);
    // Capture traces persist until dawn.
    assert!(w.sea.charge.charge_at(geom::dir(0.26) * 50.0) > 0.0);
}

#[test]
fn world_weaver_anchors_sit_wholly_inside_their_sectors() {
    let t = Tuning::default();
    let (ww, _) = world_weaver::WorldWeaver::scenario(&t);
    let a = t.sector_angle();
    for anchor in &ww.anchors {
        let b = bearing_of(anchor.pos);
        assert_eq!((b / a) as usize, anchor.sector, "{}", anchor.name);
        let r = anchor.pos.length();
        let margin = Form::Island.radius(&t);
        let d_start = r * angle_delta(anchor.sector as f32 * a, b).abs().sin();
        let d_end = r * angle_delta(b, (anchor.sector + 1) as f32 * a).abs().sin();
        assert!(d_start > margin && d_end > margin, "{} too close to a sector edge", anchor.name);
    }
}

#[test]
fn world_weaver_reefs_sit_wholly_inside_their_sectors() {
    let t = Tuning::default();
    let (ww, _) = world_weaver::WorldWeaver::scenario(&t);
    let a = t.sector_angle();
    let mut violations = Vec::new();
    for reef in &ww.reefs {
        assert!((reef.layer as usize) < t.weaver_layers);
        for rock in &reef.rocks {
            let b = bearing_of(rock.center);
            let r = rock.center.length();
            let d_start = r * angle_delta(reef.sector as f32 * a, b).abs().sin();
            let d_end = r * angle_delta(b, (reef.sector + 1) as f32 * a).abs().sin();
            let inside = (b / a) as usize == reef.sector && d_start >= rock.radius && d_end >= rock.radius;
            if !inside || r + rock.radius > t.sea_radius {
                violations.push(format!(
                    "sector {} layer {} rock ({:.1},{:.1}) bearing {:.1} r {:.1} margins {:.1}/{:.1}",
                    reef.sector, reef.layer, rock.center.x, rock.center.y, b.to_degrees(), r, d_start, d_end
                ));
            }
        }
    }
    assert!(violations.is_empty(), "reef rocks outside their sector:\n{}", violations.join("\n"));
    // Every reef is one connected cluster: consecutive rocks overlap.
    for reef in &ww.reefs {
        assert!(reef.rocks.len() >= 2, "sector {} layer {}: a reef is a cluster, not a lone rock", reef.sector, reef.layer);
        for pair in reef.rocks.windows(2) {
            assert!(pair[0].overlaps(&pair[1]), "sector {} layer {}: reef rocks not connected", reef.sector, reef.layer);
        }
    }
    // Layers are different seas: no two layers of a sector share rock geometry.
    for sector in 0..t.weaver_sectors {
        let layers: Vec<Vec<(i32, i32)>> = (0..t.weaver_layers as u8)
            .map(|layer| {
                let mut rocks: Vec<(i32, i32)> = ww
                    .reefs
                    .iter()
                    .filter(|r| r.sector == sector && r.layer == layer)
                    .flat_map(|r| r.rocks.iter().map(|c| ((c.center.x * 10.0) as i32, (c.center.y * 10.0) as i32)))
                    .collect();
                rocks.sort();
                assert!(!rocks.is_empty(), "sector {sector} layer {layer} has no reef");
                rocks
            })
            .collect();
        for i in 0..layers.len() {
            for j in (i + 1)..layers.len() {
                assert_ne!(layers[i], layers[j], "sector {sector}: layers {i} and {j} share rock geometry");
            }
        }
    }
}

#[test]
fn world_weaver_every_lane_sector_has_one_reef_blocked_layer() {
    let t = Tuning::default();
    let (ww, _) = world_weaver::WorldWeaver::scenario(&t);
    let lane_sectors: Vec<usize> = ww.anchors.iter().map(|a| a.sector).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let clearance = t.ship_radius;
    for sector in lane_sectors {
        let blocked: Vec<u8> = (0..t.weaver_layers as u8)
            .filter(|layer| {
                ww.reefs.iter().filter(|r| r.sector == sector && r.layer == *layer).any(|reef| {
                    reef.rocks.iter().any(|rock| {
                        ww.route.windows(2).any(|seg| geom::segment_distance(rock.center, seg[0], seg[1]) < rock.radius + clearance)
                    })
                })
            })
            .collect();
        assert_eq!(blocked.len(), 1, "sector {sector} should have exactly one reef-blocked layer, got {blocked:?}");
        // The authored solution never picks a reef-blocked layer.
        let chosen = autopilot::world_weaver_solution().iter().find(|(s, _)| *s == sector).map(|(_, l)| *l);
        assert!(chosen.is_some_and(|l| !blocked.contains(&l)), "solution picks a reef-blocked layer in sector {sector}");
    }
}

#[test]
fn world_weaver_reef_across_the_lane_grounds_the_expedition() {
    // Sector 1 layer III carries a reef across the lane; everything else follows the solution.
    let mut plan = autopilot::world_weaver_solution();
    plan[0] = (1, 2);
    let mut w = weaver_with(&plan);
    run_until_finished(&mut w, |_| Input::default());
    let o = w.outcome.as_ref().unwrap();
    assert!(!o.success, "{o:#?}");
    assert_eq!(o.headline, "The north-eastern rocks blocked the route.");
    // Only the chosen layers' reefs were laid.
    if let Rules::WorldWeaver(ww) = &w.rules {
        let built = ww.built.as_ref().unwrap();
        let expected: usize = ww.reefs.iter().filter(|r| built[r.sector] == r.layer).map(|r| r.rocks.len()).sum();
        assert_eq!(w.sea.rocks.len(), 1 + 3 + expected);
        // Laid reefs are land: their cells hold no glow and cannot charge.
        for rock in w.sea.rocks.iter().skip(4) {
            let idx = w.sea.charge.index_of(rock.center).unwrap();
            assert!(!w.sea.charge.sea[idx] && w.sea.charge.charge[idx] == 0.0, "reef cell still sea: {:?}", rock.center);
        }
    }
}

#[test]
fn creature_surfaces_when_it_calls_and_skip_to_dawn_ends_the_night() {
    let mut w = World::new(Mode::NightWatch, Tuning::default());
    // Run until the creature is active, beam parked so it stays in darkness.
    let creature = loop {
        w.step(Input::default(), DT);
        if let Some(c) = w.sea.entities.iter().find(|e| e.form == Form::Creature) {
            break c.id;
        }
        assert!(w.phase == Phase::Night || matches!(w.phase, Phase::Intro { .. }), "creature never appeared");
    };
    // It shows itself as it arrives, so the appearance cue coincides with a sighting.
    let arriving = w.sea.entity(creature).unwrap();
    assert_eq!(w.entity_visibility(arriving), Visibility::Silhouette);
    // Then it sinks back into darkness, and a later call surfaces it again: the sequence is
    // visible (arrival) → hidden → visible (call).
    let mut phases: Vec<Visibility> = vec![Visibility::Silhouette];
    for _ in 0..(60.0 * w.tuning().creature_call_period * 1.5) as usize {
        w.step(Input::default(), DT);
        let vis = w.entity_visibility(w.sea.entity(creature).unwrap());
        if phases.last() != Some(&vis) {
            phases.push(vis);
        }
    }
    assert!(
        phases.starts_with(&[Visibility::Silhouette, Visibility::Hidden, Visibility::Silhouette]),
        "unexpected visibility sequence: {phases:?}"
    );
    w.skip_to_dawn();
    w.step(Input::default(), DT);
    assert!(matches!(w.phase, Phase::Dawn { .. }), "{:?}", w.phase);
}

#[test]
fn first_light_reveals_the_sea_once_then_darkness_falls() {
    let mut w = World::new(Mode::MutableSea, Tuning::default());
    let mut revealed = false;
    while w.phase != Phase::Night {
        w.step(Input::default(), DT);
        if w.flare() > 0.5 {
            revealed = true;
            for e in &w.sea.entities {
                assert_eq!(w.entity_visibility(e), Visibility::Lit, "{} hidden during the flare", e.name);
            }
        }
    }
    assert!(revealed, "the flare never peaked");
    // Night: the beam points north at mid range; identities far from it are hidden again.
    w.step(Input::default(), DT);
    let hidden = w.sea.entities.iter().filter(|e| w.entity_visibility(e) == Visibility::Hidden).count();
    assert_eq!(hidden, w.sea.entities.len(), "identities stayed visible after the flare");
}

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
        assert!(c.sea.charge.charge.iter().all(|c| *c == 0.0));
    }
}

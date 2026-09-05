//! Engine-agnostic simulation. Nothing in this module knows about rendering or Bevy.
//!
//! `World` owns the shared foundation (`Sea`) and one variant's rules. The presentation layer
//! steps it at a fixed rate, reads state for drawing, and drains `events` for audio/feedback.

pub mod autopilot;
pub mod beam;
pub mod charge;
pub mod entity;
pub mod geom;
pub mod guidance;
pub mod mutable_sea;
pub mod night_watch;
pub mod steering;
pub mod tuning;
pub mod world_weaver;

#[cfg(test)]
mod tests;

pub use beam::{Beam, Footprint, FootprintKind, Input};
pub use charge::ChargeField;
pub use entity::{Entity, EntityId, Form, Status};
pub use geom::Circle;
pub use guidance::Guidance;
pub use tuning::Tuning;

use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    NightWatch,
    MutableSea,
    WorldWeaver,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::NightWatch, Mode::MutableSea, Mode::WorldWeaver];

    pub fn title(self) -> &'static str {
        match self {
            Mode::NightWatch => "Night Watch",
            Mode::MutableSea => "Mutable Sea",
            Mode::WorldWeaver => "World Weaver",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Mode::NightWatch => "Paint safe passage. Remember what moves in the dark.",
            Mode::MutableSea => "What you abandon to darkness may become something else.",
            Mode::WorldWeaver => "Build the sea by night. Test it at first light.",
        }
    }

    /// Two-sentence rule summary shown before the session starts.
    pub fn rules_summary(self) -> &'static str {
        match self {
            Mode::NightWatch => {
                "Ships follow the glowing water you paint ahead of them and keep their heading when the trail fades. \
                 Guide them into the southern harbor before dawn; a creature follows the brightest light it can see."
            }
            Mode::MutableSea => {
                "Ships follow the glowing water you paint ahead of them, and only your light keeps a thing what it is: \
                 in darkness each of the three identities turns ship, wreck, creature, island, ship again. \
                 Bring two of them into the southern harbor as ships before dawn."
            }
            Mode::WorldWeaver => {
                "Each turn of the beam shows another layer of the sea; press Space to capture the lit sector into the world you are building. \
                 Uncaptured sectors use the calm first layer. At dawn the expedition sails the buoy lane through what you made."
            }
        }
    }

    /// One line that stays on screen all night.
    pub fn objective(self) -> &'static str {
        match self {
            Mode::NightWatch => "Paint glowing routes ahead of ships and bring them to the southern harbor.",
            Mode::MutableSea => "Your light holds a form still; darkness changes it. Bring 2 of 3 home as ships.",
            Mode::WorldWeaver => "Space captures the lit sector's layer. At dawn the expedition sails your world.",
        }
    }

    pub fn controls(self) -> &'static str {
        match self {
            Mode::NightWatch | Mode::MutableSea => "A / D rotate beam    W / S move the patch farther / nearer    Esc pause",
            Mode::WorldWeaver => "A / D wind backward / forward    Space capture lit sector    Esc pause",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Beacon ignition; no input yet.
    Intro { elapsed: f32 },
    Night,
    /// Sunrise transition; the sea is revealed and rules stop.
    Dawn { elapsed: f32 },
    /// World Weaver only: the assembled world plays out.
    Playback,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Rock,
    Creature,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Ignite,
    NightBegins,
    Dawn,
    ShipArrived { id: EntityId, pos: Vec2 },
    CreatureAppears { id: EntityId, pos: Vec2 },
    Rescued { id: EntityId, pos: Vec2 },
    Sunk { id: EntityId, pos: Vec2, cause: Cause },
    /// Mutable Sea damage: the ship became a wreck.
    Wrecked { id: EntityId, pos: Vec2, cause: Cause },
    Transformed { id: EntityId, pos: Vec2, from: Form, to: Form },
    Bell { pos: Vec2 },
    CreatureCall { pos: Vec2 },
    LayerChanged { layer: u8 },
    Captured { sector: usize, layer: u8 },
    VoyageBegins,
    VoyageDelay { pos: Vec2 },
    VoyageBlocked { pos: Vec2 },
    VoyageJoined { id: EntityId, pos: Vec2 },
    VoyageArrived,
    SessionEnded,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub success: bool,
    pub headline: String,
    pub details: Vec<String>,
    pub rescued: usize,
    pub total: usize,
}

/// How much the presentation may show of a position right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Hidden,
    /// Strong afterglow: rough silhouette only.
    Silhouette,
    /// Direct illumination (or the whole sea after dawn).
    Lit,
}

/// Shared foundation used by every variant.
#[derive(Clone, Debug)]
pub struct Sea {
    pub tuning: Tuning,
    pub time: f32,
    pub beam: Beam,
    pub charge: ChargeField,
    pub guidance: Guidance,
    /// Fixed obstacles including the central island (always index 0).
    pub rocks: Vec<Circle>,
    pub entities: Vec<Entity>,
    pub events: Vec<Event>,
    next_id: EntityId,
}

impl Sea {
    fn new(tuning: Tuning, kind: FootprintKind, rocks: Vec<Circle>) -> Self {
        let mut land = vec![Circle::new(Vec2::ZERO, tuning.island_radius)];
        land.extend(rocks.iter().copied());
        Self {
            beam: Beam::new(kind, &tuning),
            charge: ChargeField::new(&tuning, &land),
            guidance: Guidance::default(),
            rocks: land,
            entities: Vec::new(),
            events: Vec::new(),
            time: 0.0,
            next_id: 1,
            tuning,
        }
    }

    pub fn spawn(&mut self, name: &'static str, form: Form, pos: Vec2, heading: f32) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        self.entities
            .push(Entity::new(id, name, form, pos, heading, &self.tuning));
        id
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn harbor(&self) -> Circle {
        Circle::new(self.tuning.harbor_center, self.tuning.harbor_radius)
    }

    /// Land circles that block movement: fixed rocks plus any island-form entities.
    pub fn land_for(&self, exclude: EntityId) -> Vec<Circle> {
        let mut land = self.rocks.clone();
        land.extend(
            self.entities
                .iter()
                .filter(|e| e.id != exclude && e.is_active() && e.form == Form::Island)
                .map(Entity::circle),
        );
        land
    }

    /// Whether a position is under the direct footprint or strong afterglow (preserving light).
    pub fn is_preserved(&self, pos: Vec2) -> bool {
        self.beam.footprint(&self.tuning).contains(pos) || self.charge.is_strong(pos, &self.tuning)
    }

    /// Moor a rescued ship at a visible slot beside the harbor.
    pub fn secure(&mut self, id: EntityId) {
        let slot = self
            .entities
            .iter()
            .filter(|e| e.status == Status::Secured)
            .count();
        let harbor = self.tuning.harbor_center;
        let Some(e) = self.entity_mut(id) else { return };
        e.status = Status::Secured;
        e.pos = harbor + Vec2::new(-3.0 + 1.5 * slot as f32, -1.5 + 0.8 * (slot % 2) as f32);
        e.heading = std::f32::consts::PI;
        e.brain = Default::default();
    }

    /// Advance a ship's movement and resolve harbor entry and groundings.
    /// Returns `Some(cause)` if the ship struck land or a solid entity this step.
    pub fn move_ship(&mut self, idx: usize, dt: f32) -> Option<Cause> {
        let (guidance, charge, tuning, time) = (&self.guidance, &self.charge, &self.tuning, self.time);
        let e = &mut self.entities[idx];
        steering::steer_ship(e, guidance, charge, tuning, time, dt);
        let pos = e.pos;
        let radius = e.radius;
        let id = e.id;
        if self.harbor().contains(pos) {
            self.secure(id);
            self.events.push(Event::Rescued { id, pos });
            return None;
        }
        let hit_rock = self.rocks.iter().any(|r| r.overlaps(&Circle::new(pos, radius)));
        let hit_solid = self.entities.iter().any(|o| {
            o.id != id && o.is_active() && o.form.is_solid() && o.circle().overlaps(&Circle::new(pos, radius))
        });
        (hit_rock || hit_solid).then_some(Cause::Rock)
    }

    /// Shared periodic sound information from darkness. A calling creature also breaks the
    /// surface for a moment, so the cue has a visible counterpart.
    fn emit_ambient_cues(&mut self, dt: f32) {
        let prev = self.time - dt;
        let now = self.time;
        let surface_until = now + self.tuning.creature_surface_seconds;
        let call_period = self.tuning.creature_call_period;
        let mut cues = Vec::new();
        for e in self.entities.iter_mut().filter(|e| e.is_active()) {
            let (period, offset) = match e.form {
                Form::Ship => (9.0, e.id as f32 * 1.7),
                Form::Creature => (call_period, e.id as f32 * 2.3),
                _ => continue,
            };
            let k_prev = ((prev + offset) / period).floor();
            let k_now = ((now + offset) / period).floor();
            if k_now > k_prev {
                cues.push(match e.form {
                    Form::Ship => Event::Bell { pos: e.pos },
                    _ => {
                        e.surfaced_until = surface_until;
                        Event::CreatureCall { pos: e.pos }
                    }
                });
            }
        }
        self.events.extend(cues);
    }
}

#[derive(Clone, Debug)]
pub enum Rules {
    NightWatch(night_watch::NightWatch),
    MutableSea(mutable_sea::MutableSea),
    WorldWeaver(world_weaver::WorldWeaver),
}

#[derive(Clone, Debug)]
pub struct World {
    pub mode: Mode,
    pub phase: Phase,
    pub sea: Sea,
    pub rules: Rules,
    pub night_length: f32,
    pub night_elapsed: f32,
    pub outcome: Option<Outcome>,
}

impl World {
    pub fn new(mode: Mode, tuning: Tuning) -> Self {
        let (rules, sea, night_length) = match mode {
            Mode::NightWatch => {
                let (rules, rocks) = night_watch::NightWatch::scenario();
                let sea = Sea::new(tuning.clone(), FootprintKind::Spot, rocks);
                (Rules::NightWatch(rules), sea, tuning.night_watch_night)
            }
            Mode::MutableSea => {
                let (rules, rocks) = mutable_sea::MutableSea::scenario();
                let mut sea = Sea::new(tuning.clone(), FootprintKind::Spot, rocks);
                mutable_sea::populate(&rules, &mut sea);
                (Rules::MutableSea(rules), sea, tuning.mutable_sea_night)
            }
            Mode::WorldWeaver => {
                let (rules, rocks) = world_weaver::WorldWeaver::scenario(&tuning);
                let mut sea = Sea::new(tuning.clone(), FootprintKind::Sector, rocks);
                // Start in the middle of sector 0 so a twitch at the seam cannot flip layers.
                sea.beam.winding = tuning.sector_angle() * 0.5;
                (Rules::WorldWeaver(rules), sea, tuning.world_weaver_night)
            }
        };
        let mut world = Self {
            mode,
            phase: Phase::Intro { elapsed: 0.0 },
            sea,
            rules,
            night_length,
            night_elapsed: 0.0,
            outcome: None,
        };
        world.sea.events.push(Event::Ignite);
        world
    }

    pub fn tuning(&self) -> &Tuning {
        &self.sea.tuning
    }

    pub fn night_remaining(&self) -> f32 {
        (self.night_length - self.night_elapsed).max(0.0)
    }

    /// Developer shortcut: end the night on the next step.
    pub fn skip_to_dawn(&mut self) {
        if self.phase == Phase::Night {
            self.night_elapsed = self.night_length;
        }
    }

    pub fn footprint(&self) -> Footprint {
        self.sea.beam.footprint(&self.sea.tuning)
    }

    /// Whether the beam is currently lighting the water (not during intro/dawn/playback).
    pub fn beam_active(&self) -> bool {
        matches!(self.phase, Phase::Night)
    }

    pub fn step(&mut self, input: Input, dt: f32) {
        self.sea.time += dt;
        match self.phase {
            Phase::Intro { elapsed } => {
                let elapsed = elapsed + dt;
                if elapsed >= self.sea.tuning.intro_seconds {
                    self.phase = Phase::Night;
                    self.sea.events.push(Event::NightBegins);
                } else {
                    self.phase = Phase::Intro { elapsed };
                }
            }
            Phase::Night => {
                self.sea.beam.update(input, &self.sea.tuning, dt);
                let fp = self.footprint();
                match &mut self.rules {
                    Rules::WorldWeaver(ww) => {
                        // Weaver: glowing water only records captures and persists until dawn.
                        world_weaver::step_night(ww, &mut self.sea, input, dt);
                    }
                    Rules::NightWatch(nw) => {
                        self.sea.charge.step(Some(&fp), &self.sea.tuning, dt);
                        self.sea
                            .guidance
                            .paint(fp.center(), &self.sea.charge, &self.sea.tuning);
                        self.sea.guidance.prune(&self.sea.charge);
                        night_watch::step(nw, &mut self.sea, dt);
                        self.sea.emit_ambient_cues(dt);
                    }
                    Rules::MutableSea(ms) => {
                        self.sea.charge.step(Some(&fp), &self.sea.tuning, dt);
                        self.sea
                            .guidance
                            .paint(fp.center(), &self.sea.charge, &self.sea.tuning);
                        self.sea.guidance.prune(&self.sea.charge);
                        mutable_sea::step(ms, &mut self.sea, dt);
                        self.sea.emit_ambient_cues(dt);
                    }
                }
                self.night_elapsed += dt;
                if self.night_elapsed >= self.night_length {
                    self.phase = Phase::Dawn { elapsed: 0.0 };
                    self.sea.events.push(Event::Dawn);
                    if let Rules::WorldWeaver(ww) = &mut self.rules {
                        world_weaver::freeze_and_build(ww, &mut self.sea);
                    }
                }
            }
            Phase::Dawn { elapsed } => {
                let elapsed = elapsed + dt;
                self.sea.charge.step(None, &self.sea.tuning, dt);
                if elapsed >= self.sea.tuning.dawn_seconds {
                    match &mut self.rules {
                        Rules::WorldWeaver(ww) => {
                            self.phase = Phase::Playback;
                            world_weaver::begin_voyage(ww, &mut self.sea);
                        }
                        _ => self.finish(),
                    }
                } else {
                    self.phase = Phase::Dawn { elapsed };
                }
            }
            Phase::Playback => {
                self.sea.charge.step(None, &self.sea.tuning, dt);
                if let Rules::WorldWeaver(ww) = &mut self.rules {
                    if world_weaver::step_playback(ww, &mut self.sea, dt) {
                        self.finish();
                    }
                }
            }
            Phase::Finished => {}
        }
    }

    fn finish(&mut self) {
        self.phase = Phase::Finished;
        self.outcome = Some(match &self.rules {
            Rules::NightWatch(nw) => night_watch::outcome(nw, &self.sea),
            Rules::MutableSea(ms) => mutable_sea::outcome(ms, &self.sea),
            Rules::WorldWeaver(ww) => world_weaver::outcome(ww, &self.sea),
        });
        self.sea.events.push(Event::SessionEnded);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.sea.events)
    }

    /// How strongly the first light floods the whole sea during ignition: 0 before the flame
    /// catches, 1 while it flares, falling back to 0 as the night begins.
    pub fn flare(&self) -> f32 {
        let Phase::Intro { elapsed } = self.phase else { return 0.0 };
        let t = elapsed / self.sea.tuning.intro_seconds;
        let rise = ((t - 0.25) / 0.15).clamp(0.0, 1.0);
        let fall = ((1.0 - t) / 0.2).clamp(0.0, 1.0);
        rise.min(fall)
    }

    /// Visibility of a world position under the mode's rules. Rendering must not show more.
    pub fn visibility(&self, pos: Vec2) -> Visibility {
        match self.phase {
            Phase::Dawn { .. } | Phase::Playback | Phase::Finished => Visibility::Lit,
            // The first light floods the whole sea once, then darkness falls.
            Phase::Intro { .. } if self.flare() > 0.0 => Visibility::Lit,
            Phase::Intro { .. } => Visibility::Hidden,
            Phase::Night => {
                if self.footprint().contains(pos) {
                    Visibility::Lit
                } else if self.sea.charge.is_strong(pos, &self.sea.tuning) {
                    Visibility::Silhouette
                } else {
                    Visibility::Hidden
                }
            }
        }
    }

    /// Secured (moored) ships are always visible: the cumulative record of success. A creature
    /// that has just called shows at least a silhouette while it is surfaced.
    pub fn entity_visibility(&self, e: &Entity) -> Visibility {
        match e.status {
            Status::Secured => Visibility::Lit,
            Status::Sunk => Visibility::Hidden,
            Status::Active => {
                let vis = self.visibility(e.pos);
                if vis == Visibility::Hidden && e.form == Form::Creature && self.sea.time < e.surfaced_until {
                    Visibility::Silhouette
                } else {
                    vis
                }
            }
        }
    }
}

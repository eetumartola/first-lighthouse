//! Engine-agnostic simulation. Nothing in this module knows about rendering or Bevy.
//!
//! `World` owns the shared foundation (`Sea`) and one variant's rules. The presentation layer
//! steps it at a fixed rate, reads state for drawing, and drains `events` for audio/feedback.

pub mod autopilot;
pub mod beam;
pub mod charge;
pub mod entity;
pub mod geom;
pub mod islands;
pub mod level;
pub mod mutable_sea;
pub mod night_watch;
pub mod route;
pub mod spiral_voyage;
pub mod steering;
pub mod tuning;
pub mod world_weaver;

#[cfg(test)]
mod tests;

pub use beam::{Beam, Footprint, FootprintKind, Input};
pub use charge::ChargeField;
pub use entity::{Entity, EntityId, Form, Status};
pub use geom::Circle;
pub use tuning::Tuning;

use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    NightWatch,
    /// Suspended: kept for its retained implementation, hidden from the menu.
    MutableSea,
    WorldWeaver,
    SpiralVoyage,
}

impl Mode {
    /// Every mode, including the suspended one (tests).
    #[cfg(test)]
    pub const ALL: [Mode; 4] = [Mode::NightWatch, Mode::MutableSea, Mode::WorldWeaver, Mode::SpiralVoyage];
    /// What the player can choose. Mutable Sea and World Weaver are suspended: their rules still
    /// run, but neither is offered.
    pub const MENU: [Mode; 2] = [Mode::NightWatch, Mode::SpiralVoyage];

    pub fn title(self) -> &'static str {
        match self {
            Mode::NightWatch => "Night Watch",
            Mode::MutableSea => "Mutable Sea",
            Mode::WorldWeaver => "World Weaver",
            Mode::SpiralVoyage => "Spiral Voyage",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Mode::NightWatch => "Paint safe passage. Remember what moves in the dark.",
            Mode::MutableSea => "What you abandon to darkness may become something else.",
            Mode::WorldWeaver => "Assemble the sea by night. Find the passage at first light.",
            Mode::SpiralVoyage => "One ship, four worlds. Scout ahead, then bring it through.",
        }
    }

    /// Two-sentence rule summary shown before the session starts.
    pub fn rules_summary(self) -> &'static str {
        match self {
            Mode::NightWatch => {
                "Ships follow the glowing water you paint ahead of them and keep their heading when the trail fades. \
                 Guide them into the northern harbor before dawn; a ship-sized predator eats the glow it finds and sinks what it touches."
            }
            Mode::MutableSea => {
                "Ships follow the glowing water you paint ahead of them, and only your light keeps a thing what it is: \
                 in darkness each of the three identities turns ship, wreck, creature, island, ship again. \
                 Bring two of them into the northern harbor as ships before dawn."
            }
            Mode::WorldWeaver => {
                "World 1 is the sea the ship will sail; winding the beam onward shows Worlds 2 to 4, and Space copies the lit \
                 sector's land into World 1. At dawn a ship enters from the marked lane and must find any passage to the harbor."
            }
            Mode::SpiralVoyage => {
                "Paint glowing water ahead of your ship exactly as in Night Watch. The sea is a spiral: each clockwise \
                 pass of the south seam is the next world, and the water ahead of the ship already belongs to it. \
                 Bring the ship through Worlds 1 to 4 to the harbor in World 4."
            }
        }
    }

    /// One line that stays on screen all night.
    pub fn objective(self) -> &'static str {
        match self {
            Mode::NightWatch => "Paint glowing routes ahead of ships and bring them to the northern harbor.",
            Mode::MutableSea => "Your light holds a form still; darkness changes it. Bring 2 of 3 home as ships.",
            Mode::WorldWeaver => "Copy sectors from Worlds 2 to 4 into World 1 until the lane connects to the harbor.",
            Mode::SpiralVoyage => {
                "Guide the ship across the south seam through all four worlds to the harbor in World 4."
            }
        }
    }

    pub fn controls(self) -> &'static str {
        match self {
            Mode::NightWatch | Mode::MutableSea => {
                "A / D turn spotlight    W / S move the patch farther / nearer    Esc pause"
            }
            Mode::WorldWeaver => "A / D wind backward / forward    Space copy lit sector into World 1    Esc pause",
            Mode::SpiralVoyage => {
                "A / D turn spotlight and wind through worlds    W / S move the patch farther / nearer    Esc pause"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Dusk: the sea is visible and fades to darkness; the player may aim but nothing moves yet.
    Intro {
        elapsed: f32,
    },
    Night,
    /// Sunrise transition; the sea is revealed and rules stop.
    Dawn {
        elapsed: f32,
    },
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
    ShipArrived {
        id: EntityId,
        pos: Vec2,
    },
    CreatureAppears {
        id: EntityId,
        pos: Vec2,
    },
    Rescued {
        id: EntityId,
        pos: Vec2,
    },
    Sunk {
        id: EntityId,
        pos: Vec2,
        cause: Cause,
    },
    /// Mutable Sea damage: the ship became a wreck.
    Wrecked {
        id: EntityId,
        pos: Vec2,
        cause: Cause,
    },
    Transformed {
        id: EntityId,
        pos: Vec2,
        from: Form,
        to: Form,
    },
    Bell {
        pos: Vec2,
    },
    CreatureCall {
        pos: Vec2,
    },
    /// The inspected world changed (World Weaver browsing).
    LayerChanged {
        layer: u8,
    },
    /// World Weaver: a sector's land was copied into World 1 from `layer`.
    Captured {
        sector: usize,
        layer: u8,
    },
    /// World Weaver: Space pressed while inspecting the assembled world itself.
    AssembledWorld,
    VoyageBegins,
    /// World Weaver: no passage exists from the lane to the harbor.
    NoPassage,
    VoyageArrived,
    /// Spiral Voyage: the ship crossed the seam into `world` (0-based).
    ShipCrossed {
        world: u8,
    },
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Visibility {
    Hidden,
    /// Readable as an opaque dark shape against surrounding glow; 0 = barely, 1 = clear outline.
    Silhouette(f32),
    /// Direct illumination (or the whole sea at dusk and after dawn).
    Lit,
}

impl Visibility {
    pub fn is_visible(self) -> bool {
        !matches!(self, Visibility::Hidden)
    }
}

/// Shared foundation used by every variant.
#[derive(Clone, Debug)]
pub struct Sea {
    pub tuning: Tuning,
    pub time: f32,
    pub beam: Beam,
    pub charge: ChargeField,
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
        self.entities.push(Entity::new(id, name, form, pos, heading, &self.tuning));
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
        let slot = self.entities.iter().filter(|e| e.status == Status::Secured).count();
        let harbor = self.tuning.harbor_center;
        let Some(e) = self.entity_mut(id) else { return };
        e.status = Status::Secured;
        e.pos = harbor + Vec2::new(-3.0 + 1.5 * slot as f32, 1.5 - 0.8 * (slot % 2) as f32);
        e.heading = std::f32::consts::PI;
        e.brain = Default::default();
    }

    /// Advance a ship by reading the light around it, then resolve harbor entry and groundings.
    /// Returns `Some(cause)` if the ship struck land or a solid entity this step.
    pub fn move_ship(&mut self, idx: usize, dt: f32) -> Option<Cause> {
        let (charge, tuning) = (&self.charge, &self.tuning);
        let e = &mut self.entities[idx];
        steering::steer_ship(e, charge, tuning, dt);
        let pos = e.pos;
        let radius = e.radius;
        let id = e.id;
        if self.harbor().contains(pos) {
            self.secure(id);
            self.events.push(Event::Rescued { id, pos });
            return None;
        }
        let hull = Circle::new(pos, radius);
        let hit_rock = self.rocks.iter().any(|r| r.overlaps(&hull));
        let hit_solid = self
            .entities
            .iter()
            .any(|o| o.id != id && o.is_active() && o.form.is_solid() && o.circle().overlaps(&hull));
        (hit_rock || hit_solid).then_some(Cause::Rock)
    }

    /// Shared periodic sound information from darkness: ship bells and the creature's call.
    fn emit_ambient_cues(&mut self, dt: f32) {
        let prev = self.time - dt;
        let now = self.time;
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
                    // The call is heard, not seen: in the dark only the eyes give the creature away.
                    _ => Event::CreatureCall { pos: e.pos },
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
    SpiralVoyage(spiral_voyage::SpiralVoyage),
}

#[derive(Clone, Debug)]
pub struct World {
    pub mode: Mode,
    pub phase: Phase,
    pub sea: Sea,
    pub rules: Rules,
    /// `None`: no deadline (Spiral Voyage ends when the ship arrives or is lost).
    pub night_length: Option<f32>,
    pub night_elapsed: f32,
    pub outcome: Option<Outcome>,
}

impl World {
    pub fn new(mode: Mode, tuning: Tuning) -> Self {
        let (rules, sea, night_length) = match mode {
            Mode::NightWatch => {
                let (mut rules, rocks) = night_watch::NightWatch::scenario(&tuning);
                let mut sea = Sea::new(tuning.clone(), FootprintKind::Spot, rocks);
                // The first boats are present before the dusk fade so they can be seen.
                night_watch::dusk_boats(&mut rules, &mut sea);
                (Rules::NightWatch(rules), sea, Some(tuning.night_watch_night))
            }
            Mode::MutableSea => {
                let (rules, rocks) = mutable_sea::MutableSea::scenario();
                let mut sea = Sea::new(tuning.clone(), FootprintKind::Spot, rocks);
                mutable_sea::populate(&rules, &mut sea);
                (Rules::MutableSea(rules), sea, Some(tuning.mutable_sea_night))
            }
            Mode::WorldWeaver => {
                let rules = world_weaver::WorldWeaver::scenario(&tuning);
                let mut sea = Sea::new(tuning.clone(), FootprintKind::Sector, Vec::new());
                // Start in the middle of sector 0 so a twitch at the seam cannot flip worlds.
                sea.beam.winding = tuning.sector_angle() * 0.5;
                (Rules::WorldWeaver(rules), sea, Some(tuning.world_weaver_night))
            }
            Mode::SpiralVoyage => {
                let mut rules = spiral_voyage::SpiralVoyage::scenario(&tuning);
                let mut sea = Sea::new(tuning.clone(), FootprintKind::Spot, Vec::new());
                spiral_voyage::populate(&mut rules, &mut sea);
                (Rules::SpiralVoyage(rules), sea, None)
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

    pub fn night_remaining(&self) -> Option<f32> {
        self.night_length.map(|len| (len - self.night_elapsed).max(0.0))
    }

    /// Developer shortcut: end the night on the next step. Modes without a deadline (Spiral
    /// Voyage) go straight to dawn, which resolves the voyage as unfinished.
    pub fn skip_to_dawn(&mut self) {
        if self.phase != Phase::Night {
            return;
        }
        match self.night_length {
            Some(len) => self.night_elapsed = len,
            None => {
                self.phase = Phase::Dawn { elapsed: 0.0 };
                self.sea.events.push(Event::Dawn);
            }
        }
    }

    pub fn footprint(&self) -> Footprint {
        self.sea.beam.footprint(&self.sea.tuning)
    }

    /// Whether the beam is currently lighting the water (aiming during dusk counts).
    pub fn beam_active(&self) -> bool {
        matches!(self.phase, Phase::Night | Phase::Intro { .. })
    }

    /// Dusk light level: 1 at the start of the introduction, 0 once the night begins.
    pub fn dusk(&self) -> f32 {
        match self.phase {
            Phase::Intro { elapsed } => (1.0 - elapsed / self.sea.tuning.intro_seconds).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    /// Index of the world in focus: World Weaver's inspected layer, the Spiral Voyage ship's
    /// world (0 elsewhere).
    pub fn inspected_world(&self) -> usize {
        match &self.rules {
            Rules::WorldWeaver(ww) => ww.layer_for(&self.sea) as usize,
            Rules::SpiralVoyage(sv) => sv.ship_world,
            _ => 0,
        }
    }

    /// The charge field the presentation should draw: the spiral's composite seen from the ship.
    pub fn view_charge(&self) -> &ChargeField {
        match &self.rules {
            Rules::SpiralVoyage(sv) => &sv.view,
            _ => &self.sea.charge,
        }
    }

    /// World index an entity currently occupies.
    pub fn entity_world(&self, e: &Entity) -> usize {
        match &self.rules {
            Rules::SpiralVoyage(sv) if sv.ship == Some(e.id) => sv.ship_world,
            Rules::SpiralVoyage(_) => spiral_voyage::world_of(e.winding, self.sea.tuning.spiral_worlds),
            _ => 0,
        }
    }

    pub fn step(&mut self, input: Input, dt: f32) {
        self.sea.time += dt;
        match self.phase {
            Phase::Intro { elapsed } => {
                // Dusk: aim freely; nothing moves and no charge accumulates until dark.
                self.sea.beam.update(input, &self.sea.tuning, dt);
                if let Rules::SpiralVoyage(sv) = &self.rules {
                    spiral_voyage::rebase_beam(sv.perspective(&self.sea), &mut self.sea);
                }
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
                        // Weaver: glowing water only records copies and persists until dawn.
                        world_weaver::step_night(ww, &mut self.sea, input);
                    }
                    Rules::NightWatch(nw) => {
                        self.sea.charge.step(Some(&fp), &self.sea.tuning, dt);
                        night_watch::step(nw, &mut self.sea, dt);
                        self.sea.emit_ambient_cues(dt);
                    }
                    Rules::MutableSea(ms) => {
                        self.sea.charge.step(Some(&fp), &self.sea.tuning, dt);
                        mutable_sea::step(ms, &mut self.sea, dt);
                        self.sea.emit_ambient_cues(dt);
                    }
                    Rules::SpiralVoyage(sv) => {
                        if spiral_voyage::step(sv, &mut self.sea, &fp, dt) {
                            self.sea.emit_ambient_cues(dt);
                            self.phase = Phase::Dawn { elapsed: 0.0 };
                            self.sea.events.push(Event::Dawn);
                            return;
                        }
                        self.sea.emit_ambient_cues(dt);
                    }
                }
                self.night_elapsed += dt;
                if self.night_length.is_some_and(|len| self.night_elapsed >= len) {
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
            Rules::SpiralVoyage(sv) => spiral_voyage::outcome(sv, &self.sea),
        });
        self.sea.events.push(Event::SessionEnded);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.sea.events)
    }

    /// Silhouette strength for a shape of `radius` at `pos`, from the glow touching it.
    fn silhouette(&self, field: &ChargeField, pos: Vec2, radius: f32) -> Visibility {
        let t = &self.sea.tuning;
        let glow = field.glow_around(pos, radius);
        if glow < t.silhouette_min_glow {
            Visibility::Hidden
        } else {
            let k = ((glow - t.silhouette_min_glow) / (t.strong_threshold - t.silhouette_min_glow)).clamp(0.0, 1.0);
            Visibility::Silhouette(k)
        }
    }

    /// Visibility of a shape in the inspected world under the mode's rules. Rendering must not
    /// show more than this.
    pub fn visibility(&self, pos: Vec2, radius: f32) -> Visibility {
        match self.phase {
            Phase::Dawn { .. } | Phase::Playback | Phase::Finished => Visibility::Lit,
            // Dusk: the whole sea is still visible while the light fades.
            Phase::Intro { .. } => Visibility::Lit,
            Phase::Night => {
                if self.footprint().contains(pos) {
                    Visibility::Lit
                } else {
                    self.silhouette(self.view_charge(), pos, radius)
                }
            }
        }
    }

    /// Entity visibility: moored ships are always visible; entities in a world other than the
    /// inspected one are hidden; a creature that has just called shows at least a faint outline.
    pub fn entity_visibility(&self, e: &Entity) -> Visibility {
        match e.status {
            Status::Secured => Visibility::Lit,
            Status::Sunk => Visibility::Hidden,
            Status::Active => {
                if self.phase == Phase::Night && self.entity_world(e) != self.inspected_world() {
                    return Visibility::Hidden;
                }
                let vis = self.visibility(e.pos, e.radius);
                let reveal = e.reveal(self.sea.time);
                if vis == Visibility::Hidden && reveal > 0.02 {
                    Visibility::Silhouette(0.6 * reveal)
                } else {
                    vis
                }
            }
        }
    }
}

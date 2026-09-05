//! Persistent entity model shared by every variant: identity, form, position, heading,
//! steering memory and the optional Mutable Sea transformation state.

use super::tuning::Tuning;
use glam::Vec2;

pub type EntityId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    Ship,
    Wreck,
    Creature,
    Island,
}

impl Form {
    pub const CYCLE: [Form; 4] = [Form::Ship, Form::Wreck, Form::Creature, Form::Island];

    pub fn index(self) -> usize {
        match self {
            Form::Ship => 0,
            Form::Wreck => 1,
            Form::Creature => 2,
            Form::Island => 3,
        }
    }

    pub fn next(self) -> Form {
        Form::CYCLE[(self.index() + 1) % 4]
    }

    pub fn name(self) -> &'static str {
        match self {
            Form::Ship => "ship",
            Form::Wreck => "wreck",
            Form::Creature => "creature",
            Form::Island => "island",
        }
    }

    pub fn radius(self, t: &Tuning) -> f32 {
        match self {
            Form::Ship => t.ship_radius,
            Form::Wreck => t.wreck_radius,
            Form::Creature => t.creature_radius,
            Form::Island => t.island_form_radius,
        }
    }

    pub fn is_solid(self) -> bool {
        matches!(self, Form::Wreck | Form::Island)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Active,
    /// Rescued and moored; can never be lost or counted twice.
    Secured,
    /// Night Watch: lost. Never used in Mutable Sea, where damage produces a wreck.
    Sunk,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// A painted route sample at a fixed position.
    Sample(Vec2),
    /// Another entity's lantern.
    Lantern(EntityId),
    /// World Weaver authored route waypoint.
    Waypoint(usize),
}

#[derive(Clone, Debug, Default)]
pub struct Brain {
    pub target: Option<Target>,
    /// Passed samples and the time until which they must not be re-selected.
    pub visited: Vec<(Vec2, f32)>,
}

/// Mutable Sea transformation progress. Light pauses the timer; it never resets it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mutable {
    /// Seconds of darkness accumulated in the current form.
    pub progress: f32,
    /// True while a due transformation waits for clear placement.
    pub deferred: bool,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: EntityId,
    pub name: &'static str,
    pub form: Form,
    pub status: Status,
    pub pos: Vec2,
    pub heading: f32,
    pub radius: f32,
    /// Ships carry a small light that attracts creatures but preserves nothing.
    pub lantern: bool,
    pub brain: Brain,
    pub mutable: Option<Mutable>,
    /// Creatures surface briefly when they call; visible as a silhouette until this sim time.
    pub surfaced_until: f32,
}

impl Entity {
    pub fn new(id: EntityId, name: &'static str, form: Form, pos: Vec2, heading: f32, t: &Tuning) -> Self {
        Self {
            id,
            name,
            form,
            status: Status::Active,
            pos,
            heading,
            radius: form.radius(t),
            lantern: form == Form::Ship,
            brain: Brain::default(),
            mutable: None,
            surfaced_until: 0.0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == Status::Active
    }

    pub fn is_active_ship(&self) -> bool {
        self.is_active() && self.form == Form::Ship
    }

    pub fn set_form(&mut self, form: Form, t: &Tuning) {
        self.form = form;
        self.radius = form.radius(t);
        self.lantern = form == Form::Ship;
        self.brain = Brain::default();
        self.surfaced_until = 0.0;
    }

    pub fn circle(&self) -> super::geom::Circle {
        super::geom::Circle::new(self.pos, self.radius)
    }
}

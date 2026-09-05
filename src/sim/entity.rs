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
    /// A charged patch the predator is heading for.
    Patch(Vec2),
    /// Computed route waypoint (World Weaver playback).
    Waypoint(usize),
}

/// Steering memory. Intent (`desired`) and hull heading are separate: the light-reading
/// decision can change instantly while the hull turns gradually.
#[derive(Clone, Debug)]
pub struct Brain {
    pub target: Option<Target>,
    /// Accepted desired heading (compass radians).
    pub desired: f32,
    /// Corridor score of the incumbent direction at the last evaluation.
    pub desired_score: f32,
    /// Seconds since intent was last reconsidered.
    pub since_eval: f32,
    /// A better direction has to keep winning for the tuned dwell before it displaces a live
    /// incumbent; this is the heading it has been winning with and for how long.
    pub challenger: f32,
    pub challenger_for: f32,
}

impl Default for Brain {
    fn default() -> Self {
        Self {
            target: None,
            desired: f32::NAN,
            desired_score: 0.0,
            since_eval: f32::MAX,
            challenger: f32::NAN,
            challenger_for: 0.0,
        }
    }
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
    /// Reveal window: an entity that has surfaced (a calling creature, a ship arriving at the
    /// edge) shows as a silhouette fading in and out between these sim times.
    pub surfaced_at: f32,
    pub surfaced_until: f32,
    /// Spiral Voyage: unwrapped compass angle of `pos`, so seam crossings change world.
    /// `floor(winding / TAU)` is the entity's world index. Unused (0) elsewhere.
    pub winding: f32,
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
            surfaced_at: 0.0,
            surfaced_until: 0.0,
            winding: 0.0,
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
        self.surfaced_at = 0.0;
        self.surfaced_until = 0.0;
    }

    pub fn circle(&self) -> super::geom::Circle {
        super::geom::Circle::new(self.pos, self.radius)
    }

    /// Show as a silhouette from `now` for `seconds`, fading in and out.
    pub fn surface(&mut self, now: f32, seconds: f32) {
        self.surfaced_at = now;
        self.surfaced_until = now + seconds;
    }

    /// Silhouette strength of the reveal at `now`: 0 outside the window, rising over the first
    /// third, full in the middle, fading over the last third.
    pub fn reveal(&self, now: f32) -> f32 {
        let span = self.surfaced_until - self.surfaced_at;
        if span <= 0.0 || now < self.surfaced_at || now >= self.surfaced_until {
            return 0.0;
        }
        let x = (now - self.surfaced_at) / span;
        let smooth = |a: f32, b: f32, v: f32| {
            let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        smooth(0.0, 0.33, x) * (1.0 - smooth(0.67, 1.0, x))
    }
}

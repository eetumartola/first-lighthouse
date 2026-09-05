//! Flat-sea geometry helpers. Bearings are compass radians: 0 = north (+y), clockwise positive.

use glam::Vec2;
use std::f32::consts::{PI, TAU};

/// Unit vector for a compass bearing.
pub fn dir(bearing: f32) -> Vec2 {
    Vec2::new(bearing.sin(), bearing.cos())
}

/// Compass bearing of a vector (undefined for zero; returns 0).
pub fn bearing_of(v: Vec2) -> f32 {
    if v.length_squared() < 1e-12 {
        0.0
    } else {
        v.x.atan2(v.y).rem_euclid(TAU)
    }
}

/// Signed shortest angular difference `to - from` in (-PI, PI].
pub fn angle_delta(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(TAU);
    if d > PI {
        d - TAU
    } else {
        d
    }
}

/// Rotate `heading` toward `target` by at most `max_step` radians.
pub fn turn_toward(heading: f32, target: f32, max_step: f32) -> f32 {
    let d = angle_delta(heading, target);
    (heading + d.clamp(-max_step, max_step)).rem_euclid(TAU)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub center: Vec2,
    pub radius: f32,
}

impl Circle {
    pub fn new(center: Vec2, radius: f32) -> Self {
        Self { center, radius }
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.distance_squared(self.center) <= self.radius * self.radius
    }
    pub fn overlaps(&self, other: &Circle) -> bool {
        let r = self.radius + other.radius;
        self.center.distance_squared(other.center) < r * r
    }
}

/// Push `pos` (with `radius`) out of `obstacle` along the contact normal. Returns the corrected
/// position and whether contact occurred. Sliding results from repeated small corrections.
pub fn resolve_circle(pos: Vec2, radius: f32, obstacle: &Circle) -> (Vec2, bool) {
    let d = pos - obstacle.center;
    let min = radius + obstacle.radius;
    let len_sq = d.length_squared();
    if len_sq >= min * min {
        return (pos, false);
    }
    let n = if len_sq < 1e-8 { Vec2::Y } else { d / len_sq.sqrt() };
    (obstacle.center + n * min, true)
}

/// Eight-wind compass word for a position relative to the lighthouse.
pub fn compass_word(p: Vec2) -> &'static str {
    const WORDS: [&str; 8] = [
        "northern",
        "north-eastern",
        "eastern",
        "south-eastern",
        "southern",
        "south-western",
        "western",
        "north-western",
    ];
    let b = bearing_of(p);
    let idx = ((b + PI / 8.0).rem_euclid(TAU) / (PI / 4.0)) as usize;
    WORDS[idx.min(7)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearing_round_trips() {
        for i in 0..16 {
            let b = i as f32 * TAU / 16.0;
            let back = bearing_of(dir(b));
            assert!(angle_delta(b, back).abs() < 1e-4, "{b} -> {back}");
        }
    }

    #[test]
    fn turn_toward_takes_shortest_arc() {
        // From 350° toward 10°: should go clockwise through north, not the long way round.
        let h = turn_toward(350f32.to_radians(), 10f32.to_radians(), 5f32.to_radians());
        assert!(angle_delta(h, 355f32.to_radians()).abs() < 1e-4);
    }

    #[test]
    fn compass_words() {
        assert_eq!(compass_word(Vec2::new(0.0, 10.0)), "northern");
        assert_eq!(compass_word(Vec2::new(10.0, 0.0)), "eastern");
        assert_eq!(compass_word(Vec2::new(-7.0, -7.0)), "south-western");
    }
}

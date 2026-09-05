//! Authoring helpers for small islands, reefs and coastlines built from overlapping circles.
//! Collision shapes match silhouettes: every piece is a connected cluster, never a lone dot field.

use super::geom::{dir, Circle};
use glam::Vec2;

/// Position from a compass bearing (degrees) and radius.
pub fn polar(bearing_deg: f32, r: f32) -> Vec2 {
    dir(bearing_deg.to_radians()) * r
}

/// `count` rocks of `radius` from `start`, each `step` further along. Overlapping when
/// `|step| < 2 * radius`, which every caller should respect.
pub fn chain(start: Vec2, step: Vec2, count: usize, radius: f32) -> Vec<Circle> {
    (0..count).map(|i| Circle::new(start + step * i as f32, radius)).collect()
}

/// Chain running outward along a bearing from polar `(bearing_deg, r0)`.
pub fn radial(bearing_deg: f32, r0: f32, count: usize, spacing: f32, radius: f32) -> Vec<Circle> {
    chain(polar(bearing_deg, r0), dir(bearing_deg.to_radians()) * spacing, count, radius)
}

/// Reef following an arc of the circle of radius `r` around the lighthouse between two bearings.
/// Rocks are spaced at 1.5 radii so the reef reads as one coastline.
pub fn arc(r: f32, from_deg: f32, to_deg: f32, rock_radius: f32) -> Vec<Circle> {
    let span = (to_deg - from_deg).to_radians().abs() * r;
    let count = (span / (rock_radius * 1.5)).ceil().max(1.0) as usize;
    (0..=count)
        .map(|i| {
            let deg = from_deg + (to_deg - from_deg) * i as f32 / count as f32;
            Circle::new(polar(deg, r), rock_radius)
        })
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_and_chains_are_connected() {
        let reef = arc(46.0, -20.0, 20.0, 3.5);
        assert!(reef.len() >= 3);
        for pair in reef.windows(2) {
            assert!(pair[0].overlaps(&pair[1]));
        }
        let ch = radial(90.0, 30.0, 4, 4.5, 2.5);
        for pair in ch.windows(2) {
            assert!(pair[0].overlaps(&pair[1]));
        }
    }
}

//! Beam controller: bearing, range, footprint, total winding, optional constant-speed rotation.

use super::geom::{angle_delta, dir};
use super::tuning::Tuning;
use glam::Vec2;
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// -1 = A (counter-clockwise), +1 = D (clockwise), 0 = none.
    pub rotate: f32,
    /// +1 = farther (W/Up), -1 = nearer (S/Down).
    pub range: f32,
    /// Space pressed this step (edge-triggered by the caller).
    pub capture: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootprintKind {
    /// Spotlight patch with independent bearing and range.
    Spot,
    /// World Weaver: one complete radial sector.
    Sector,
}

#[derive(Clone, Debug)]
pub struct Beam {
    /// Unwrapped compass angle; grows without bound so revolutions can be counted.
    pub winding: f32,
    /// Distance from the lighthouse to the footprint centre.
    pub range: f32,
    /// Developer experiment: rotation continues after release in this direction.
    pub constant_speed: bool,
    pub auto_direction: f32,
    pub kind: FootprintKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footprint {
    Spot {
        bearing: f32,
        half_angle: f32,
        r_min: f32,
        r_max: f32,
    },
    Sector {
        index: usize,
        angle_start: f32,
        angle_end: f32,
        r_min: f32,
        r_max: f32,
    },
}

impl Footprint {
    pub fn contains(&self, p: Vec2) -> bool {
        let r = p.length();
        let b = super::geom::bearing_of(p);
        match *self {
            Footprint::Spot {
                bearing,
                half_angle,
                r_min,
                r_max,
            } => r >= r_min && r <= r_max && angle_delta(bearing, b).abs() <= half_angle,
            Footprint::Sector {
                angle_start,
                angle_end,
                r_min,
                r_max,
                ..
            } => {
                let a = (b - angle_start).rem_euclid(TAU);
                r >= r_min && r <= r_max && a < (angle_end - angle_start)
            }
        }
    }

    pub fn center(&self) -> Vec2 {
        match *self {
            Footprint::Spot {
                bearing,
                r_min,
                r_max,
                ..
            } => dir(bearing) * ((r_min + r_max) * 0.5),
            Footprint::Sector {
                angle_start,
                angle_end,
                r_min,
                r_max,
                ..
            } => dir((angle_start + angle_end) * 0.5) * ((r_min + r_max) * 0.5),
        }
    }

    pub fn bearing(&self) -> f32 {
        match *self {
            Footprint::Spot { bearing, .. } => bearing,
            Footprint::Sector {
                angle_start,
                angle_end,
                ..
            } => (angle_start + angle_end) * 0.5,
        }
    }

    /// Conservative bounding circle for grid iteration.
    pub fn bounds(&self) -> (Vec2, f32) {
        match *self {
            Footprint::Spot {
                half_angle,
                r_min,
                r_max,
                ..
            } => {
                let c = self.center();
                let half_len = (r_max - r_min) * 0.5;
                let half_w = r_max * half_angle.sin();
                (c, (half_len * half_len + half_w * half_w).sqrt() + 1.0)
            }
            Footprint::Sector { r_max, .. } => (Vec2::ZERO, r_max + 1.0),
        }
    }
}

impl Beam {
    pub fn new(kind: FootprintKind, tuning: &Tuning) -> Self {
        Self {
            winding: 0.0,
            range: (tuning.beam_min_range() + tuning.beam_max_range()) * 0.5,
            constant_speed: false,
            auto_direction: 0.0,
            kind,
        }
    }

    /// Compass bearing in [0, TAU).
    pub fn bearing(&self) -> f32 {
        self.winding.rem_euclid(TAU)
    }

    /// Number of completed clockwise revolutions (negative when wound backward).
    pub fn revolution(&self) -> i32 {
        (self.winding / TAU).floor() as i32
    }

    pub fn update(&mut self, input: Input, tuning: &Tuning, dt: f32) {
        let rotate = if self.constant_speed {
            if input.rotate != 0.0 {
                self.auto_direction = input.rotate.signum();
            }
            self.auto_direction * TAU / tuning.auto_turn_seconds
        } else {
            input.rotate.clamp(-1.0, 1.0) * TAU / tuning.beam_turn_seconds
        };
        self.winding += rotate * dt;

        if self.kind == FootprintKind::Spot {
            self.range = (self.range + input.range.clamp(-1.0, 1.0) * tuning.beam_range_speed * dt)
                .clamp(tuning.beam_min_range(), tuning.beam_max_range());
        }
    }

    pub fn sector_index(&self, tuning: &Tuning) -> usize {
        ((self.bearing() / tuning.sector_angle()) as usize).min(tuning.weaver_sectors - 1)
    }

    pub fn footprint(&self, tuning: &Tuning) -> Footprint {
        match self.kind {
            FootprintKind::Spot => Footprint::Spot {
                bearing: self.bearing(),
                half_angle: tuning.beam_width_deg.to_radians() * 0.5,
                r_min: self.range - tuning.beam_length * 0.5,
                r_max: self.range + tuning.beam_length * 0.5,
            },
            FootprintKind::Sector => {
                let index = self.sector_index(tuning);
                let a = tuning.sector_angle();
                Footprint::Sector {
                    index,
                    angle_start: index as f32 * a,
                    angle_end: (index + 1) as f32 * a,
                    r_min: tuning.island_radius,
                    r_max: tuning.sea_radius,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_turn_takes_configured_seconds() {
        let t = Tuning::default();
        let mut b = Beam::new(FootprintKind::Spot, &t);
        let steps = (t.beam_turn_seconds * 60.0) as usize;
        for _ in 0..steps {
            b.update(
                Input {
                    rotate: 1.0,
                    ..Default::default()
                },
                &t,
                1.0 / 60.0,
            );
        }
        assert!((b.winding - TAU).abs() < 1e-3);
        assert_eq!(b.revolution(), 1);
    }

    #[test]
    fn spot_footprint_contains_its_centre_and_not_the_lighthouse() {
        let t = Tuning::default();
        let b = Beam::new(FootprintKind::Spot, &t);
        let f = b.footprint(&t);
        assert!(f.contains(f.center()));
        assert!(!f.contains(Vec2::ZERO));
        // Just outside the angular edge.
        let off = dir(t.beam_width_deg.to_radians() * 0.5 + 0.01) * b.range;
        assert!(!f.contains(off));
    }

    #[test]
    fn sector_footprint_never_straddles_north_seam() {
        let t = Tuning::default();
        let mut b = Beam::new(FootprintKind::Sector, &t);
        b.winding = -0.01; // just west of north
        match b.footprint(&t) {
            Footprint::Sector { index, .. } => assert_eq!(index, t.weaver_sectors - 1),
            _ => panic!(),
        }
        b.winding = 0.01;
        match b.footprint(&t) {
            Footprint::Sector { index, .. } => assert_eq!(index, 0),
            _ => panic!(),
        }
    }
}

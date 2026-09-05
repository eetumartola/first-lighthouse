//! Coarse 2D plankton charge grid. Charge is measured in seconds of remaining afterglow.
//! Gameplay and rendering both read this field; nothing else decides where light exists.

use super::beam::Footprint;
use super::geom::Circle;
use super::tuning::Tuning;
use glam::Vec2;

#[derive(Clone, Debug)]
pub struct ChargeField {
    pub size: usize,
    pub cell: f32,
    pub origin: Vec2,
    /// Remaining afterglow per cell in seconds.
    pub charge: Vec<f32>,
    /// False for land or cells outside the sea.
    pub sea: Vec<bool>,
}

impl ChargeField {
    pub fn new(tuning: &Tuning, land: &[Circle]) -> Self {
        let size = (2.0 * tuning.sea_radius / tuning.cell_size).ceil() as usize;
        let origin = Vec2::splat(-tuning.sea_radius);
        let mut sea = vec![false; size * size];
        for j in 0..size {
            for i in 0..size {
                let c = origin + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * tuning.cell_size;
                let in_sea = c.length() <= tuning.sea_radius;
                let on_land = land.iter().any(|l| l.contains(c));
                sea[j * size + i] = in_sea && !on_land;
            }
        }
        Self {
            size,
            cell: tuning.cell_size,
            origin,
            charge: vec![0.0; size * size],
            sea,
        }
    }

    pub fn cell_center(&self, index: usize) -> Vec2 {
        let i = index % self.size;
        let j = index / self.size;
        self.origin + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * self.cell
    }

    pub fn index_of(&self, p: Vec2) -> Option<usize> {
        let rel = (p - self.origin) / self.cell;
        if rel.x < 0.0 || rel.y < 0.0 {
            return None;
        }
        let (i, j) = (rel.x as usize, rel.y as usize);
        (i < self.size && j < self.size).then(|| j * self.size + i)
    }

    pub fn charge_at(&self, p: Vec2) -> f32 {
        self.index_of(p).map_or(0.0, |i| self.charge[i])
    }

    pub fn is_strong(&self, p: Vec2, tuning: &Tuning) -> bool {
        self.charge_at(p) >= tuning.strong_threshold
    }

    pub fn is_land(&self, p: Vec2) -> bool {
        self.index_of(p).is_some_and(|i| !self.sea[i])
    }

    /// Strongest glow touching a silhouette: the centre plus four points on its radius.
    pub fn glow_around(&self, center: Vec2, radius: f32) -> f32 {
        let mut best = self.charge_at(center);
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2;
            best = best.max(self.charge_at(center + Vec2::new(a.cos(), a.sin()) * radius));
        }
        best
    }

    /// The predator eats: remove charge from every cell within `radius` of `center`.
    pub fn consume(&mut self, center: Vec2, radius: f32, amount: f32) {
        let lo = ((center - Vec2::splat(radius) - self.origin) / self.cell).floor();
        let hi = ((center + Vec2::splat(radius) - self.origin) / self.cell).ceil();
        let i0 = lo.x.max(0.0) as usize;
        let j0 = lo.y.max(0.0) as usize;
        let i1 = (hi.x.max(0.0) as usize).min(self.size);
        let j1 = (hi.y.max(0.0) as usize).min(self.size);
        let r2 = radius * radius;
        for j in j0..j1 {
            for i in i0..i1 {
                let idx = j * self.size + i;
                if self.cell_center(idx).distance_squared(center) <= r2 {
                    self.charge[idx] = (self.charge[idx] - amount).max(0.0);
                }
            }
        }
    }

    /// Brightest charged cell within `radius` of `from` that holds at least `min` charge,
    /// scored by charge falling off with distance. Returns (centre, charge).
    pub fn strongest_within(&self, from: Vec2, radius: f32, min: f32) -> Option<(Vec2, f32)> {
        let lo = ((from - Vec2::splat(radius) - self.origin) / self.cell).floor();
        let hi = ((from + Vec2::splat(radius) - self.origin) / self.cell).ceil();
        let i0 = lo.x.max(0.0) as usize;
        let j0 = lo.y.max(0.0) as usize;
        let i1 = (hi.x.max(0.0) as usize).min(self.size);
        let j1 = (hi.y.max(0.0) as usize).min(self.size);
        let r2 = radius * radius;
        let mut best: Option<(f32, Vec2, f32)> = None;
        for j in j0..j1 {
            for i in i0..i1 {
                let idx = j * self.size + i;
                let c = self.charge[idx];
                if c < min {
                    continue;
                }
                let p = self.cell_center(idx);
                let d2 = p.distance_squared(from);
                if d2 > r2 {
                    continue;
                }
                let score = c / (1.0 + d2.sqrt() / 20.0);
                if best.is_none_or(|(s, _, _)| score > s) {
                    best = Some((score, p, c));
                }
            }
        }
        best.map(|(_, p, c)| (p, c))
    }

    /// Charge under the footprint, then advance decay everywhere.
    pub fn step(&mut self, footprint: Option<&Footprint>, tuning: &Tuning, dt: f32) {
        if let Some(fp) = footprint {
            let (center, radius) = fp.bounds();
            let lo = ((center - Vec2::splat(radius) - self.origin) / self.cell).floor();
            let hi = ((center + Vec2::splat(radius) - self.origin) / self.cell).ceil();
            let i0 = lo.x.max(0.0) as usize;
            let j0 = lo.y.max(0.0) as usize;
            let i1 = (hi.x.max(0.0) as usize).min(self.size);
            let j1 = (hi.y.max(0.0) as usize).min(self.size);
            // Diminishing returns: a cell gains at the full rate when dark and ever more slowly as
            // it fills, so a lingering beam brightens a spot gently instead of saturating it. The
            // extra `dt` is taken back by the decay pass below, so a lit cell never loses glow and
            // a fresh one gains exactly `charge_rate * dt`.
            for j in j0..j1 {
                for i in i0..i1 {
                    let idx = j * self.size + i;
                    if !self.sea[idx] {
                        continue;
                    }
                    if fp.contains(self.cell_center(idx)) {
                        let c = self.charge[idx];
                        let headroom = (1.0 - c / tuning.charge_cap).max(0.0);
                        self.charge[idx] = (c + (tuning.charge_rate * headroom + 1.0) * dt).min(tuning.charge_cap + dt);
                    }
                }
            }
        }
        for c in &mut self.charge {
            *c = (*c - dt).max(0.0);
        }
    }

    /// Instantly set every cell inside the footprint to a fixed value (World Weaver capture trace).
    pub fn stamp(&mut self, fp: &Footprint, value: f32) {
        for idx in 0..self.charge.len() {
            if self.sea[idx] && fp.contains(self.cell_center(idx)) {
                self.charge[idx] = value;
            }
        }
    }

    /// Mark newly laid land: those cells stop being sea and lose any stored glow.
    pub fn add_land(&mut self, land: &[Circle]) {
        for idx in 0..self.charge.len() {
            if self.sea[idx] && land.iter().any(|l| l.contains(self.cell_center(idx))) {
                self.sea[idx] = false;
                self.charge[idx] = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::beam::{Beam, FootprintKind};
    use super::*;

    #[test]
    fn charge_saturates_softly_and_decays_linearly() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        let b = Beam::new(FootprintKind::Spot, &t);
        let fp = b.footprint(&t);
        let p = fp.center();
        let dt = 1.0 / 60.0;
        let second = |f: &mut ChargeField| {
            let before = f.charge_at(p);
            for _ in 0..60 {
                f.step(Some(&fp), &t, dt);
            }
            f.charge_at(p) - before
        };
        // A fresh cell gains exactly one frame of charging; then each second yields less than the last.
        f.step(Some(&fp), &t, dt);
        assert!((f.charge_at(p) - t.charge_rate * dt).abs() < 1e-5, "{}", f.charge_at(p));
        let first = second(&mut f);
        let next = second(&mut f);
        assert!(first > 0.8 * t.charge_rate && first < t.charge_rate, "{first}");
        assert!(next < first, "{next} >= {first}");
        // A cell already at the cap holds there under the beam.
        let idx = f.index_of(p).unwrap();
        f.charge[idx] = t.charge_cap;
        f.step(Some(&fp), &t, dt);
        assert_eq!(f.charge_at(p), t.charge_cap);
        f.charge[idx] = 0.0;
        // A lingering beam creeps toward the cap and never exceeds it at any step.
        for _ in 0..(60.0 * 30.0) as usize {
            f.step(Some(&fp), &t, dt);
            assert!(f.charge_at(p) <= t.charge_cap, "{}", f.charge_at(p));
        }
        let held = f.charge_at(p);
        assert!(held > 0.9 * t.charge_cap, "{held}");
        // Outside the footprint nothing charged.
        assert_eq!(f.charge_at(-p), 0.0);
        // Decay: one second of darkness costs one second of glow.
        for _ in 0..60 {
            f.step(None, &t, dt);
        }
        assert!((f.charge_at(p) - (held - 1.0)).abs() < 0.05);
    }

    #[test]
    fn land_cells_never_charge() {
        let t = Tuning::default();
        let rock = Circle::new(Vec2::new(0.0, 50.0), 3.0);
        let mut f = ChargeField::new(&t, &[rock]);
        let mut b = Beam::new(FootprintKind::Spot, &t);
        b.range = 50.0;
        let fp = b.footprint(&t);
        for _ in 0..120 {
            f.step(Some(&fp), &t, 1.0 / 60.0);
        }
        assert_eq!(f.charge_at(rock.center), 0.0);
        assert!(f.charge_at(Vec2::new(0.0, 46.0)) > 0.0);
    }
}

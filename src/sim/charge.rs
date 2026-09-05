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

    /// Advance decay everywhere and charge under the footprint.
    pub fn step(&mut self, footprint: Option<&Footprint>, tuning: &Tuning, dt: f32) {
        for c in &mut self.charge {
            *c = (*c - dt).max(0.0);
        }
        let Some(fp) = footprint else { return };
        let (center, radius) = fp.bounds();
        let lo = ((center - Vec2::splat(radius) - self.origin) / self.cell).floor();
        let hi = ((center + Vec2::splat(radius) - self.origin) / self.cell).ceil();
        let i0 = lo.x.max(0.0) as usize;
        let j0 = lo.y.max(0.0) as usize;
        let i1 = (hi.x.max(0.0) as usize).min(self.size);
        let j1 = (hi.y.max(0.0) as usize).min(self.size);
        // Charging also undoes this step's decay so a lit cell gains exactly `charge_rate * dt`.
        let gain = (tuning.charge_rate + 1.0) * dt;
        for j in j0..j1 {
            for i in i0..i1 {
                let idx = j * self.size + i;
                if !self.sea[idx] {
                    continue;
                }
                if fp.contains(self.cell_center(idx)) {
                    self.charge[idx] = (self.charge[idx] + gain).min(tuning.charge_cap);
                }
            }
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
    fn charge_rate_cap_and_decay_follow_tuning() {
        let t = Tuning::default();
        let mut f = ChargeField::new(&t, &[]);
        let b = Beam::new(FootprintKind::Spot, &t);
        let fp = b.footprint(&t);
        let p = fp.center();
        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            f.step(Some(&fp), &t, dt);
        }
        assert!((f.charge_at(p) - t.charge_rate).abs() < 0.05, "{}", f.charge_at(p));
        for _ in 0..(60.0 * 20.0) as usize {
            f.step(Some(&fp), &t, dt);
        }
        assert!((f.charge_at(p) - t.charge_cap).abs() < 1e-3);
        // Outside the footprint nothing charged.
        assert_eq!(f.charge_at(-p), 0.0);
        // Decay: one second of darkness costs one second of glow.
        for _ in 0..60 {
            f.step(None, &t, dt);
        }
        assert!((f.charge_at(p) - (t.charge_cap - 1.0)).abs() < 0.05);
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

//! Sparse route samples created as the player paints. Brightness always comes from the
//! charge field, so guidance and plankton can never disagree.

use super::charge::ChargeField;
use super::tuning::Tuning;
use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub pos: Vec2,
    pub cell: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Guidance {
    pub samples: Vec<Sample>,
}

impl Guidance {
    /// Record the footprint centre as a route sample if no sample is already nearby.
    pub fn paint(&mut self, center: Vec2, field: &ChargeField, tuning: &Tuning) {
        let Some(cell) = field.index_of(center) else { return };
        if !field.sea[cell] {
            return;
        }
        let spacing_sq = tuning.sample_spacing * tuning.sample_spacing;
        if self
            .samples
            .iter()
            .any(|s| s.pos.distance_squared(center) < spacing_sq)
        {
            return;
        }
        self.samples.push(Sample {
            pos: center,
            cell,
        });
    }

    /// Drop samples whose water has gone dark.
    pub fn prune(&mut self, field: &ChargeField) {
        self.samples.retain(|s| field.charge[s.cell] > 0.0);
    }

    pub fn usable<'a>(
        &'a self,
        field: &'a ChargeField,
        tuning: &'a Tuning,
    ) -> impl Iterator<Item = (&'a Sample, f32)> + 'a {
        self.samples.iter().filter_map(move |s| {
            let c = field.charge[s.cell];
            (c >= tuning.usable_sample_threshold).then_some((s, c))
        })
    }
}

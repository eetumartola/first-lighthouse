//! Authored levels as polar ASCII maps. Columns are bearing, 12° each, starting at the seam
//! bearing (south) and running clockwise; rows are radius bands from the sea's edge (top) down
//! to the lighthouse island (bottom row, which is the island itself and is ignored). `X` is rock.
//! A file may hold several worlds side by side, one 30-column block each (Spiral Voyage).

use super::geom::{dir, Circle};
use glam::Vec2;
use std::f32::consts::TAU;

/// Columns per full revolution.
pub const COLUMNS: usize = 30;
/// Where column 0 starts: the seam bearing (south).
pub const SEAM: f32 = std::f32::consts::PI;

pub const MODE1_LEVEL1: &str = include_str!("../../assets/levels/mode1_level1.txt");
pub const MODE4_LEVEL1: &str = include_str!("../../assets/levels/mode4_level1.txt");

/// Bearing of a cell column within a world (its centre).
pub fn column_bearing(col: usize) -> f32 {
    (SEAM + (col as f32 + 0.5) * TAU / COLUMNS as f32).rem_euclid(TAU)
}

/// Radius band: `rows` sea rows between the island and the edge, row 0 outermost.
pub fn row_radius(row: usize, rows: usize, island: f32, sea: f32) -> f32 {
    let band = (sea - island) / rows as f32;
    sea - (row as f32 + 0.5) * band
}

/// Rock circles for every world in the map. The polar mask is resampled at approximately one
/// `band` of arc per sample: outer rows gain samples, while inner rows vote across source columns.
/// This keeps physical detail density roughly constant instead of concentrating it near the island.
pub fn parse(text: &str, island: f32, sea: f32) -> Vec<Vec<Circle>> {
    let lines: Vec<&str> = text.lines().map(|line| line.trim_end_matches('\r')).collect();
    let rows = lines.len().saturating_sub(1); // the bottom row is the island
    assert!(rows >= 1, "level needs at least one sea row above the island row");
    let width = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
    let worlds = width.div_ceil(COLUMNS).max(1);
    let band = (sea - island) / rows as f32;
    // Slightly under half a band leaves visibly navigable water between neighbouring reefs.
    let radius = band * 0.405;
    let mut out = vec![Vec::new(); worlds];

    for (row, line) in lines.iter().take(rows).enumerate() {
        let r = row_radius(row, rows, island, sea);
        let samples = (TAU * r / band).round().max(1.0) as usize;
        for world in 0..worlds {
            for sample in 0..samples {
                let land = if samples >= COLUMNS {
                    let col = ((sample as f32 + 0.5) * COLUMNS as f32 / samples as f32) as usize;
                    source_is_land(line, world, col.min(COLUMNS - 1))
                } else {
                    let first = sample * COLUMNS / samples;
                    let end = ((sample + 1) * COLUMNS).div_ceil(samples);
                    let occupied = (first..end).filter(|&col| source_is_land(line, world, col)).count();
                    occupied * 2 >= end - first
                };
                if land {
                    let bearing = SEAM + (sample as f32 + 0.5) * TAU / samples as f32;
                    out[world].push(Circle::new(dir(bearing) * r, radius));
                }
            }
        }
    }
    out
}

fn source_is_land(line: &str, world: usize, col: usize) -> bool {
    line.chars().nth(world * COLUMNS + col).is_some_and(|ch| ch == 'X' || ch == 'x')
}

/// Whether a cell is free water: used to choose seam crossings and waypoints on authored maps.
pub fn is_free(text: &str, world: usize, col: usize, row: usize) -> bool {
    text.lines().nth(row).and_then(|l| l.chars().nth(world * COLUMNS + col)).is_none_or(|ch| ch != 'X' && ch != 'x')
}

/// Number of sea rows in the map.
pub fn rows(text: &str) -> usize {
    text.lines().count().saturating_sub(1)
}

/// Centre of a cell in world coordinates.
pub fn cell_center(text: &str, col: usize, row: usize, island: f32, sea: f32) -> Vec2 {
    dir(column_bearing(col)) * row_radius(row, rows(text), island, sea)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_start_south_and_run_clockwise() {
        assert!((column_bearing(0).to_degrees() - 186.0).abs() < 1e-3);
        assert!((column_bearing(15).to_degrees() - 6.0).abs() < 1e-3);
    }

    #[test]
    fn island_row_is_ignored_and_worlds_split_by_block() {
        let map = "XXXXXXXX                      \n                              XXXXXXXX\nXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
        let worlds = parse(map, 8.0, 100.0);
        assert_eq!(worlds.len(), 2);
        // Angular voting retains authored clusters while each ring's physical sample spacing
        // remains roughly constant. The bottom island row produces no circles.
        assert!(worlds[0].iter().all(|c| c.center.y < 0.0 && c.center.length() > 60.0));
        assert!(worlds[1].iter().all(|c| c.center.length() < 60.0));
        assert!(!worlds[0].is_empty() && !worlds[1].is_empty());
    }

    #[test]
    fn authored_levels_parse_and_leave_harbor_and_spawns_clear() {
        let t = super::super::tuning::Tuning::default();
        let nw = parse(MODE1_LEVEL1, t.island_radius, t.sea_radius);
        assert_eq!(nw.len(), 1);
        let harbor = Circle::new(t.harbor_center, t.harbor_radius + t.ship_radius);
        assert!(nw[0].iter().all(|r| !r.overlaps(&harbor)), "rock in the harbor");
        let sv = parse(MODE4_LEVEL1, t.island_radius, t.sea_radius);
        assert_eq!(sv.len(), 4);
        assert!(sv[3].iter().all(|r| !r.overlaps(&harbor)), "rock in World 4's harbor");
    }
}

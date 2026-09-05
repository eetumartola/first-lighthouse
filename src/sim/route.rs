//! Grid route finder for World Weaver's dawn playback. Land is inflated by the hull clearance so
//! a point path is a hull path; the result is simplified with clearance-checked line of sight.
//! Modes 1 and 4 never use this: their ships read light.

use super::geom::Circle;
use glam::Vec2;
use std::collections::BinaryHeap;

struct Grid {
    size: usize,
    cell: f32,
    origin: Vec2,
    blocked: Vec<bool>,
}

impl Grid {
    fn new(land: &[Circle], sea_radius: f32, cell: f32, clearance: f32) -> Self {
        let size = (2.0 * sea_radius / cell).ceil() as usize;
        let origin = Vec2::splat(-sea_radius);
        let mut blocked = vec![false; size * size];
        for j in 0..size {
            for i in 0..size {
                let c = origin + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * cell;
                let outside = c.length() > sea_radius - clearance;
                let on_land = land.iter().any(|l| l.center.distance(c) < l.radius + clearance);
                blocked[j * size + i] = outside || on_land;
            }
        }
        Self { size, cell, origin, blocked }
    }

    fn index(&self, p: Vec2) -> Option<(usize, usize)> {
        let rel = (p - self.origin) / self.cell;
        if rel.x < 0.0 || rel.y < 0.0 {
            return None;
        }
        let (i, j) = (rel.x as usize, rel.y as usize);
        (i < self.size && j < self.size).then_some((i, j))
    }

    fn center(&self, i: usize, j: usize) -> Vec2 {
        self.origin + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * self.cell
    }

    fn free(&self, i: isize, j: isize) -> bool {
        i >= 0 && j >= 0 && (i as usize) < self.size && (j as usize) < self.size && !self.blocked[j as usize * self.size + i as usize]
    }

    /// Nearest free cell to `p` (search rings outward).
    fn nearest_free(&self, p: Vec2) -> Option<(usize, usize)> {
        let (i0, j0) = self.index(p)?;
        for ring in 0..(self.size as isize) {
            for dj in -ring..=ring {
                for di in -ring..=ring {
                    if di.abs() != ring && dj.abs() != ring {
                        continue;
                    }
                    let (i, j) = (i0 as isize + di, j0 as isize + dj);
                    if self.free(i, j) {
                        return Some((i as usize, j as usize));
                    }
                }
            }
        }
        None
    }
}

#[derive(PartialEq, Eq)]
struct Node {
    f: u32,
    idx: usize,
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.f.cmp(&self.f).then_with(|| other.idx.cmp(&self.idx))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// True when the segment keeps `clearance` from all land and stays inside the sea.
pub fn segment_clear(a: Vec2, b: Vec2, land: &[Circle], sea_radius: f32, clearance: f32) -> bool {
    let len = a.distance(b);
    let steps = (len / 0.5).ceil().max(1.0) as usize;
    (0..=steps).all(|k| {
        let p = a.lerp(b, k as f32 / steps as f32);
        p.length() <= sea_radius - clearance && land.iter().all(|l| l.center.distance(p) >= l.radius + clearance)
    })
}

/// Find a hull-clearance route from `start` to `goal`, or `None` when no passage exists.
pub fn find_route(land: &[Circle], sea_radius: f32, cell: f32, clearance: f32, start: Vec2, goal: Vec2) -> Option<Vec<Vec2>> {
    let grid = Grid::new(land, sea_radius, cell, clearance);
    let (si, sj) = grid.nearest_free(start)?;
    let (gi, gj) = grid.nearest_free(goal)?;
    let n = grid.size;
    let to_idx = |i: usize, j: usize| j * n + i;
    let start_idx = to_idx(si, sj);
    let goal_idx = to_idx(gi, gj);

    // Costs in tenths of a cell so diagonals (14) versus straights (10) stay integral.
    let heuristic = |idx: usize| -> u32 {
        let (i, j) = ((idx % n) as i64, (idx / n) as i64);
        let (dx, dy) = ((i - gi as i64).abs(), (j - gj as i64).abs());
        (dx.max(dy) * 10 + dx.min(dy) * 4) as u32
    };
    let mut g = vec![u32::MAX; n * n];
    let mut came = vec![usize::MAX; n * n];
    let mut open = BinaryHeap::new();
    g[start_idx] = 0;
    open.push(Node { f: heuristic(start_idx), idx: start_idx });
    const DIRS: [(isize, isize, u32); 8] = [
        (1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10),
        (1, 1, 14), (1, -1, 14), (-1, 1, 14), (-1, -1, 14),
    ];
    while let Some(Node { idx, .. }) = open.pop() {
        if idx == goal_idx {
            break;
        }
        let (i, j) = ((idx % n) as isize, (idx / n) as isize);
        for (di, dj, cost) in DIRS {
            let (ni, nj) = (i + di, j + dj);
            if !grid.free(ni, nj) {
                continue;
            }
            // No corner cutting between two blocked orthogonal neighbours.
            if di != 0 && dj != 0 && !(grid.free(i + di, j) && grid.free(i, j + dj)) {
                continue;
            }
            let nidx = nj as usize * n + ni as usize;
            let tentative = g[idx] + cost;
            if tentative < g[nidx] {
                g[nidx] = tentative;
                came[nidx] = idx;
                open.push(Node { f: tentative + heuristic(nidx), idx: nidx });
            }
        }
    }
    if g[goal_idx] == u32::MAX {
        return None;
    }
    let mut cells = vec![goal_idx];
    let mut cur = goal_idx;
    while cur != start_idx {
        cur = came[cur];
        cells.push(cur);
    }
    cells.reverse();
    let mut points: Vec<Vec2> = cells.iter().map(|&c| grid.center(c % n, c / n)).collect();
    points[0] = start;
    *points.last_mut().unwrap() = goal;

    // Simplify: from each kept point, jump to the farthest later point still in clear sight.
    let mut route = vec![points[0]];
    let mut i = 0;
    while i + 1 < points.len() {
        let mut j = points.len() - 1;
        while j > i + 1 && !segment_clear(points[i], points[j], land, sea_radius, clearance) {
            j -= 1;
        }
        route.push(points[j]);
        i = j;
    }
    Some(route)
}

/// Total length of a polyline.
pub fn length(route: &[Vec2]) -> f32 {
    route.windows(2).map(|w| w[0].distance(w[1])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_threads_a_gap_with_clearance_and_reports_no_passage_when_walled() {
        // A wall across the sea with one gap.
        let mut land: Vec<Circle> = (0..20)
            .map(|k| Circle::new(Vec2::new(-95.0 + k as f32 * 10.0, 0.0), 5.5))
            .collect();
        land.remove(10); // gap around x = 5
        let start = Vec2::new(0.0, 60.0);
        let goal = Vec2::new(0.0, -60.0);
        let route = find_route(&land, 100.0, 1.5, 2.6, start, goal).expect("gap should be passable");
        assert_eq!(route[0], start);
        assert_eq!(*route.last().unwrap(), goal);
        for w in route.windows(2) {
            assert!(segment_clear(w[0], w[1], &land, 100.0, 2.6), "segment cuts a corner: {:?}", w);
        }
        // Close the gap: no route may be invented through land.
        land.push(Circle::new(Vec2::new(5.0, 0.0), 5.5));
        assert!(find_route(&land, 100.0, 1.5, 2.6, start, goal).is_none());
    }
}

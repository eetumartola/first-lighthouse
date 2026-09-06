//! Procedural low-poly models in a Dredge-like register: flat-shaded, angular, a little
//! exaggerated. Everything is built from triangle lists with per-face normals so the beam and
//! moonlight carve visible facets. Coordinates follow the render convention (`x` east, `y` up,
//! `z` south), so a model at the origin sits on the water with its bow toward `-z`.

use crate::sim::Circle;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use glam::Vec2;
use std::f32::consts::TAU;

/// Triangle soup with one normal per face and an optional flat colour per face.
#[derive(Default)]
pub struct Soup {
    tris: Vec<[Vec3; 3]>,
    colors: Vec<[f32; 4]>,
    /// Colour applied to faces added without an explicit one.
    pub color: Option<[f32; 4]>,
}

impl Soup {
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        self.tris.push([a, b, c]);
        self.colors.push(self.color.unwrap_or([1.0; 4]));
    }

    /// Quad `a b c d` in winding order, split along the shorter diagonal so long thin quads fold
    /// into two honest facets.
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3) {
        if a.distance_squared(c) <= b.distance_squared(d) {
            self.tri(a, b, c);
            self.tri(a, c, d);
        } else {
            self.tri(a, b, d);
            self.tri(b, c, d);
        }
    }

    pub fn mesh(self) -> Mesh {
        let mut positions = Vec::with_capacity(self.tris.len() * 3);
        let mut normals = Vec::with_capacity(self.tris.len() * 3);
        let mut colors = Vec::with_capacity(self.tris.len() * 3);
        for ([a, b, c], color) in self.tris.iter().zip(&self.colors) {
            let n = (*b - *a).cross(*c - *a).normalize_or(Vec3::Y);
            for p in [a, b, c] {
                positions.push(p.to_array());
                normals.push(n.to_array());
                colors.push(*color);
            }
        }
        let uvs = vec![[0.0f32, 0.0]; positions.len()];
        let indices = (0..positions.len() as u32).collect::<Vec<_>>();
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(indices))
    }
}

/// Distance from `p` to the segment `a b`.
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Deterministic hash noise in [0, 1).
fn hash(x: f32, y: f32, seed: f32) -> f32 {
    ((x * 127.1 + y * 311.7 + seed * 74.7).sin() * 43758.5453).fract().abs()
}

/// Smooth value noise in [0, 1) at unit lattice spacing.
fn value_noise(p: Vec2, seed: f32) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let n = |dx: f32, dy: f32| hash(i.x + dx, i.y + dy, seed);
    let a = n(0.0, 0.0) + (n(1.0, 0.0) - n(0.0, 0.0)) * u.x;
    let b = n(0.0, 1.0) + (n(1.0, 1.0) - n(0.0, 1.0)) * u.x;
    a + (b - a) * u.y
}

/// Polynomial smooth minimum: blends two distances within `k` of each other.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = ((b - a) / k * 0.5 + 0.5).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// Circles closer than this multiple of their summed radii belong to one island: authored maps
/// place neighbouring rocks a cell apart with a sliver of water between, which is no channel.
const ADJACENT: f32 = 1.35;

fn adjacent(a: &Circle, b: &Circle) -> bool {
    let r = (a.radius + b.radius) * ADJACENT;
    a.center.distance_squared(b.center) < r * r
}

/// Group circles into connected islands (overlapping or nearly touching circles).
pub fn clusters(rocks: &[Circle]) -> Vec<Vec<Circle>> {
    let mut parent: Vec<usize> = (0..rocks.len()).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..rocks.len() {
        for j in (i + 1)..rocks.len() {
            if adjacent(&rocks[i], &rocks[j]) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a] = b;
                }
            }
        }
    }
    let mut out: Vec<(usize, Vec<Circle>)> = Vec::new();
    for (i, rock) in rocks.iter().enumerate() {
        let root = find(&mut parent, i);
        match out.iter_mut().find(|(r, _)| *r == root) {
            Some((_, list)) => list.push(*rock),
            None => out.push((root, vec![*rock])),
        }
    }
    out.into_iter().map(|(_, list)| list).collect()
}

/// Centroid of a cluster, weighted by area.
pub fn cluster_center(rocks: &[Circle]) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut w = 0.0;
    for r in rocks {
        let a = r.radius * r.radius;
        sum += r.center * a;
        w += a;
    }
    sum / w.max(1e-6)
}

/// One island for a cluster of circles: a faceted heightfield rising from the waterline over
/// the union of the circles, with the waists between neighbouring rocks filled so a chain reads
/// as one ridge. Faces are coloured by height and slope (dark wet base, pale dry tops) for a
/// material with a white base colour. The mesh is local to `origin` (render coordinates) and
/// covers every collision circle. `plateau` caps the height for a flat-topped island.
pub fn island(rocks: &[Circle], origin: Vec2, plateau: Option<f32>) -> Mesh {
    let seed = hash(origin.x, origin.y, 3.0) * 100.0;
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for r in rocks {
        lo = lo.min(r.center - Vec2::splat(r.radius));
        hi = hi.max(r.center + Vec2::splat(r.radius));
    }
    let max_r = rocks.iter().map(|r| r.radius).fold(0.0, f32::max);
    // Ridges between neighbouring rocks: capsules a little thinner than the rocks themselves.
    let mut ridges = Vec::new();
    for (i, a) in rocks.iter().enumerate() {
        for b in &rocks[i + 1..] {
            if adjacent(a, b) {
                ridges.push((a.center, b.center, a.radius.min(b.radius) * 0.8));
            }
        }
    }
    let k = (max_r * 0.5).max(0.5);
    let sdf = |p: Vec2| {
        rocks
            .iter()
            .map(|r| p.distance(r.center) - r.radius)
            .chain(ridges.iter().map(|(a, b, r)| segment_distance(p, *a, *b) - r))
            .reduce(|a, b| smin(a, b, k))
            .unwrap_or(f32::INFINITY)
    };
    let height = |p: Vec2| -> f32 {
        let d = sdf(p);
        if d >= 0.0 {
            return 0.0;
        }
        // Cliffs from the shore, then crags: coarse noise for the massing, fine for facets.
        let inside = (-d / max_r).min(1.0);
        let cliff = inside.powf(0.4);
        let coarse = value_noise(p / (max_r * 0.9) + Vec2::splat(seed), seed);
        let fine = value_noise(p / (max_r * 0.3) + Vec2::splat(seed * 1.7), seed + 1.0);
        let h = max_r * (0.3 * cliff + 0.6 * cliff * coarse + 0.3 * inside * fine);
        match plateau {
            Some(cap) => h.min(cap),
            None => h,
        }
    };
    let cell = (max_r / 4.0).clamp(0.5, 1.5);
    let nx = ((hi.x - lo.x) / cell).ceil() as usize + 1;
    let ny = ((hi.y - lo.y) / cell).ceil() as usize + 1;
    let vertex = |i: usize, j: usize| -> Vec3 {
        let p = lo + Vec2::new(i as f32, j as f32) * cell;
        // Break the grid so facets read as rock rather than mesh.
        let jitter = Vec2::new(hash(p.x, p.y, seed) - 0.5, hash(p.y, p.x, seed + 5.0) - 0.5) * cell * 0.45;
        let q = p + jitter;
        let h = height(q);
        let rel = q - origin;
        Vec3::new(rel.x, h, -rel.y)
    };
    let peak = max_r * 1.0;
    // Dark enough that unlit rock stays unreadable under moonlight; the beam brings out the tone.
    let wet = Vec3::new(0.05, 0.06, 0.07);
    let dry = Vec3::new(0.30, 0.29, 0.26);
    let mut soup = Soup::default();
    let shade = |soup: &mut Soup, a: Vec3, b: Vec3, c: Vec3| {
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
        let h = (a.y + b.y + c.y) / 3.0;
        // Height lifts the tone; flat faces are drier and paler than cliffs.
        let t = ((h / peak).clamp(0.0, 1.0) * 0.7 + n.y.max(0.0) * 0.3).clamp(0.0, 1.0);
        let col = wet.lerp(dry, t);
        soup.color = Some([col.x, col.y, col.z, 1.0]);
        soup.tri(a, b, c);
    };
    for j in 0..ny {
        for i in 0..nx {
            let a = vertex(i, j);
            let b = vertex(i + 1, j);
            let c = vertex(i + 1, j + 1);
            let d = vertex(i, j + 1);
            if a.y <= 0.0 && b.y <= 0.0 && c.y <= 0.0 && d.y <= 0.0 {
                continue;
            }
            // Sink shore vertices slightly so the waterline never shows a hairline gap.
            let drop = |v: Vec3| if v.y <= 0.0 { v - Vec3::Y * 0.3 } else { v };
            let (a, b, c, d) = (drop(a), drop(b), drop(c), drop(d));
            // Sim y (north) maps to -z, so this winding faces up.
            if a.distance_squared(c) <= b.distance_squared(d) {
                shade(&mut soup, a, b, c);
                shade(&mut soup, a, c, d);
            } else {
                shade(&mut soup, a, b, d);
                shade(&mut soup, b, c, d);
            }
        }
    }
    soup.mesh()
}

/// Hull station: distance along the keel (`z`, bow negative), half-beam at the gunwale, gunwale
/// height, keel depth (below the waterline) and half-beam at the chine.
struct Station {
    z: f32,
    beam: f32,
    gunwale: f32,
    keel: f32,
    chine: f32,
}

/// Loft a hull from stations: gunwale → chine → keel on each side, a deck on top. Sides and
/// transom take `paint`, the deck `deck`.
fn loft_hull(stations: &[Station], soup: &mut Soup, paint: [f32; 4], deck: [f32; 4]) {
    let ring = |s: &Station| -> [Vec3; 7] {
        [
            Vec3::new(-s.beam, s.gunwale, s.z),
            Vec3::new(-s.chine, -s.keel * 0.35, s.z),
            Vec3::new(-s.chine * 0.35, -s.keel, s.z),
            Vec3::new(0.0, -s.keel, s.z),
            Vec3::new(s.chine * 0.35, -s.keel, s.z),
            Vec3::new(s.chine, -s.keel * 0.35, s.z),
            Vec3::new(s.beam, s.gunwale, s.z),
        ]
    };
    for pair in stations.windows(2) {
        let (r0, r1) = (ring(&pair[0]), ring(&pair[1]));
        soup.color = Some(paint);
        for k in 0..6 {
            soup.quad(r0[k], r0[k + 1], r1[k + 1], r1[k]);
        }
        // Deck between the two gunwales.
        soup.color = Some(deck);
        soup.quad(r0[0], r1[0], r1[6], r0[6]);
    }
    // Transom.
    soup.color = Some(paint);
    let last = ring(stations.last().unwrap());
    for k in 0..5 {
        soup.tri(last[6], last[k], last[k + 1]);
    }
}

/// A small working boat: raised bow, hard chine, wheelhouse aft, mast forward with a lantern.
/// About 3.6 long and 1.5 wide; the lantern sits at `y ≈ 4.2`.
pub fn ship() -> Mesh {
    let mut soup = Soup::default();
    let stations = [
        Station { z: -1.95, beam: 0.02, gunwale: 0.95, keel: 0.15, chine: 0.02 },
        Station { z: -1.5, beam: 0.36, gunwale: 0.82, keel: 0.32, chine: 0.22 },
        Station { z: -0.8, beam: 0.62, gunwale: 0.66, keel: 0.42, chine: 0.46 },
        Station { z: 0.0, beam: 0.72, gunwale: 0.58, keel: 0.44, chine: 0.56 },
        Station { z: 0.8, beam: 0.7, gunwale: 0.56, keel: 0.4, chine: 0.54 },
        Station { z: 1.45, beam: 0.58, gunwale: 0.62, keel: 0.3, chine: 0.42 },
    ];
    let paint = [0.40, 0.17, 0.13, 1.0];
    let deck = [0.46, 0.34, 0.21, 1.0];
    let timber = [0.17, 0.12, 0.09, 1.0];
    loft_hull(&stations, &mut soup, paint, deck);
    // Gunwale rail: a thin lip along each side.
    soup.color = Some(timber);
    for pair in stations.windows(2) {
        for side in [-1.0f32, 1.0] {
            let (a, b) = (&pair[0], &pair[1]);
            let o = 0.06;
            let a0 = Vec3::new(side * a.beam, a.gunwale, a.z);
            let b0 = Vec3::new(side * b.beam, b.gunwale, b.z);
            let b1 = Vec3::new(side * (b.beam + o), b.gunwale + o, b.z);
            let a1 = Vec3::new(side * (a.beam + o), a.gunwale + o, a.z);
            if side > 0.0 {
                soup.quad(a0, b0, b1, a1);
            } else {
                soup.quad(a0, a1, b1, b0);
            }
        }
    }
    // Wheelhouse: a box with a raked front and a flat roof, aft of midships; pale planked walls
    // so it reads against the dark hull.
    soup.color = Some([0.55, 0.50, 0.40, 1.0]);
    house(&mut soup, Vec3::new(0.0, 0.58, 0.85), Vec3::new(0.9, 0.75, 1.0), 0.18);
    soup.mesh()
}

/// Deckhouse with a raked front wall. `base` is the centre of the floor; `size` is width, height,
/// depth; `rake` pulls the roof's front edge aft.
fn house(soup: &mut Soup, base: Vec3, size: Vec3, rake: f32) {
    let (w, h, d) = (size.x * 0.5, size.y, size.z * 0.5);
    let f0 = base + Vec3::new(-w, 0.0, -d);
    let f1 = base + Vec3::new(w, 0.0, -d);
    let b0 = base + Vec3::new(-w, 0.0, d);
    let b1 = base + Vec3::new(w, 0.0, d);
    let tf0 = f0 + Vec3::new(0.0, h, rake);
    let tf1 = f1 + Vec3::new(0.0, h, rake);
    let tb0 = b0 + Vec3::new(0.0, h, 0.0);
    let tb1 = b1 + Vec3::new(0.0, h, 0.0);
    soup.quad(f0, tf0, tf1, f1); // front
    soup.quad(b1, tb1, tb0, b0); // back
    soup.quad(f1, tf1, tb1, b1); // right
    soup.quad(b0, tb0, tf0, f0); // left
    soup.quad(tf0, tb0, tb1, tf1); // roof
}

/// Wheelhouse windows and mast fittings are separate meshes so they take other materials.
pub fn wheelhouse_windows() -> Mesh {
    let mut soup = Soup::default();
    let y = 0.58 + 0.42;
    for (x, z0, z1) in [(-0.46, 0.55, 1.2), (0.46, 0.55, 1.2)] {
        let n = if x < 0.0 { -0.01 } else { 0.01 };
        soup.quad(
            Vec3::new(x + n, y, z0),
            Vec3::new(x + n, y + 0.22, z0),
            Vec3::new(x + n, y + 0.22, z1),
            Vec3::new(x + n, y, z1),
        );
        soup.quad(
            Vec3::new(x + n, y, z1),
            Vec3::new(x + n, y + 0.22, z1),
            Vec3::new(x + n, y + 0.22, z0),
            Vec3::new(x + n, y, z0),
        );
    }
    // Front pane on the raked wall.
    let z = 0.85 - 0.5 + 0.18 * 0.6 - 0.01;
    soup.quad(
        Vec3::new(-0.34, y, z),
        Vec3::new(-0.34, y + 0.22, z + 0.06),
        Vec3::new(0.34, y + 0.22, z + 0.06),
        Vec3::new(0.34, y, z),
    );
    soup.mesh()
}

/// A sea serpent's back: a lofted body with a sharp dorsal ridge, sagging below the waterline
/// between two humps, and a wedge head with a jaw line. Length about 5.
pub fn serpent() -> Mesh {
    let mut soup = Soup::default();
    // (z, half-width, ridge height, belly depth)
    let profile: [(f32, f32, f32, f32); 11] = [
        (-3.5, 0.06, 0.28, 0.08),
        (-2.9, 0.46, 0.5, 0.3),
        (-2.5, 0.3, 0.55, 0.3),
        (-2.2, 0.42, 0.75, 0.4),
        (-1.7, 0.55, 0.95, 0.5),
        (-1.1, 0.5, 0.7, 0.5),
        (-0.5, 0.48, 0.35, 0.5),
        (0.1, 0.55, 0.9, 0.5),
        (0.8, 0.6, 1.25, 0.5),
        (1.6, 0.5, 0.8, 0.45),
        (2.5, 0.12, 0.2, 0.15),
    ];
    let ring = |&(z, w, h, b): &(f32, f32, f32, f32)| -> [Vec3; 6] {
        [
            Vec3::new(0.0, h, z),
            Vec3::new(-w, h * 0.45, z),
            Vec3::new(-w * 0.8, -b * 0.4, z),
            Vec3::new(0.0, -b, z),
            Vec3::new(w * 0.8, -b * 0.4, z),
            Vec3::new(w, h * 0.45, z),
        ]
    };
    for pair in profile.windows(2) {
        let (r0, r1) = (ring(&pair[0]), ring(&pair[1]));
        for k in 0..6 {
            let k1 = (k + 1) % 6;
            soup.quad(r0[k], r0[k1], r1[k1], r1[k]);
        }
    }
    // Dorsal fins: thin triangular plates along the ridge.
    for &(z, _, h, _) in &profile[3..10] {
        soup.tri(
            Vec3::new(0.0, h - 0.05, z - 0.18),
            Vec3::new(0.0, h + 0.55, z + 0.05),
            Vec3::new(0.0, h - 0.05, z + 0.28),
        );
        soup.tri(
            Vec3::new(0.0, h - 0.05, z + 0.28),
            Vec3::new(0.0, h + 0.55, z + 0.05),
            Vec3::new(0.0, h - 0.05, z - 0.18),
        );
    }
    soup.mesh()
}

/// Octagonal frustum stack: `(radius, height)` per level, bottom first, starting at `y = 0`.
pub fn tower(levels: &[(f32, f32)], sides: usize) -> Mesh {
    let mut soup = Soup::default();
    let ring = |r: f32, y: f32| -> Vec<Vec3> {
        (0..sides)
            .map(|i| {
                let a = i as f32 / sides as f32 * TAU + TAU / (2.0 * sides as f32);
                Vec3::new(a.cos() * r, y, a.sin() * r)
            })
            .collect()
    };
    let mut y = 0.0;
    let mut prev = ring(levels[0].0, 0.0);
    for (i, &(r, h)) in levels.iter().enumerate() {
        let bottom = if i == 0 { prev.clone() } else { ring(r, y) };
        // Ledge from the previous level's top to this level's bottom.
        if i > 0 {
            for k in 0..sides {
                let k1 = (k + 1) % sides;
                soup.quad(prev[k], bottom[k], bottom[k1], prev[k1]);
            }
        }
        let top = ring(r, y + h);
        for k in 0..sides {
            let k1 = (k + 1) % sides;
            soup.quad(bottom[k], top[k], top[k1], bottom[k1]);
        }
        y += h;
        prev = top;
    }
    // Cap.
    let c = Vec3::new(0.0, y, 0.0);
    for k in 0..sides {
        let k1 = (k + 1) % sides;
        soup.tri(prev[k], c, prev[k1]);
    }
    soup.mesh()
}

/// Flat compass rose lying on the water just outside the playable disc: an outer ring, degree
/// ticks (cardinals longest), four long cardinal points and four shorter intercardinal points as
/// slim diamonds. North is toward `-z`. Vertex colour carries the tone: the north point brightest.
pub fn compass_rose(inner: f32, outer: f32) -> Mesh {
    let mut soup = Soup::default();
    let y = 0.06;
    let at = |bearing: f32, r: f32| Vec3::new(bearing.sin() * r, y, -bearing.cos() * r);
    let dim = [0.30, 0.42, 0.60, 1.0];
    let mid = [0.48, 0.62, 0.82, 1.0];
    let bright = [0.95, 1.0, 1.0, 1.0];
    let segments = 192;
    for (r0, r1) in [(inner, inner + 0.25), (outer - 0.25, outer)] {
        soup.color = Some(dim);
        for i in 0..segments {
            let a = i as f32 / segments as f32 * TAU;
            let b = (i + 1) as f32 / segments as f32 * TAU;
            soup.quad(at(a, r0), at(a, r1), at(b, r1), at(b, r0));
        }
    }
    // Ticks every 10°: cardinals span the band, every 30° two thirds, the rest one third.
    for i in 0..36 {
        let a = i as f32 / 36.0 * TAU;
        let len = if i % 9 == 0 { 1.0 } else if i % 3 == 0 { 0.62 } else { 0.3 };
        let half = if i % 9 == 0 { 0.006 } else { 0.0035 };
        let (r0, r1) = (inner + 0.4, inner + 0.4 + (outer - inner - 0.8) * len);
        soup.color = Some(if i % 9 == 0 { mid } else { dim });
        soup.quad(at(a - half, r0), at(a - half, r1), at(a + half, r1), at(a + half, r0));
    }
    // Points: slim diamonds reaching in over the water's edge, north longest and brightest.
    let point = |soup: &mut Soup, bearing: f32, reach: f32, width: f32, color: [f32; 4]| {
        let tip_in = at(bearing, inner - reach);
        let tip_out = at(bearing, outer + reach * 0.35);
        let waist = (inner + outer) * 0.5;
        let side = bearing + std::f32::consts::FRAC_PI_2;
        let w = Vec3::new(side.sin(), 0.0, -side.cos()) * width;
        let mid_point = at(bearing, waist);
        soup.color = Some(color);
        soup.quad(tip_in, mid_point - w, tip_out, mid_point + w);
    };
    for i in 0..4 {
        let bearing = i as f32 * TAU / 4.0;
        let color = if i == 0 { bright } else { mid };
        point(&mut soup, bearing, if i == 0 { 7.0 } else { 5.0 }, 1.1, color);
    }
    for i in 0..4 {
        point(&mut soup, TAU / 8.0 + i as f32 * TAU / 4.0, 3.0, 0.7, dim);
    }
    soup.mesh()
}

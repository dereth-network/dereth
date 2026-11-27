//! AC2's triangles joined into AC1's larger flat polygons, without moving a point.
//!
//! AC2 keeps its meshes as triangle lists; AC1 keeps polygons of any number of corners, and its
//! models pay for every polygon (its header, its corners, its place in the model's trees). Two
//! triangles of one material that share an edge and lie in one plane draw and collide exactly as
//! the four-cornered polygon they make, so they are joined, and the join is joined again while it
//! stays flat and convex. Nothing is added or taken away: the shape is the same shape.

use std::collections::BTreeMap;

/// The most corners a joined polygon is given.
pub const MAX_CORNERS: usize = 12;
/// How far from one plane, in meters, points may lie and count as on it; and how far a corner may
/// bend the wrong way and still count as straight.
const FLAT: f64 = 1e-3;

type P = [f64; 3];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: P, b: P) -> P {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The unit normal of a polygon's corners (Newell's), or none for a degenerate one.
fn normal(pts: &[P]) -> Option<P> {
    let mut n = [0.0; 3];
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = dot(n, n).sqrt();
    (l > 1e-12).then(|| [n[0] / l, n[1] / l, n[2] / l])
}

/// Whether `ring` (indexes into `pos`) is a flat, convex polygon facing `n`: every corner within
/// [`FLAT`] of the first corner's plane, and every corner on the inner side of every edge, or
/// within [`FLAT`] of its line. Turning the same way at each corner is not enough: a ring that
/// runs out to a corner and straight back turns there by half a circle, which reads as no turn
/// at all, and leaves corners outside the polygon's own edges.
fn flat_convex(ring: &[u16], pos: &[P], n: P) -> bool {
    let p: Vec<P> = ring.iter().map(|&i| pos[usize::from(i)]).collect();
    let d = dot(n, p[0]);
    if p.iter().any(|q| (dot(n, *q) - d).abs() > FLAT) {
        return false;
    }
    let k = p.len();
    for i in 0..k {
        let (a, b) = (p[i], p[(i + 1) % k]);
        let edge = sub(b, a);
        let reach = FLAT * dot(edge, edge).sqrt();
        if p.iter().any(|q| dot(cross(edge, sub(*q, a)), n) < -reach) {
            return false;
        }
    }
    // No corner used twice.
    let mut seen: Vec<u16> = ring.to_vec();
    seen.sort_unstable();
    seen.windows(2).all(|w| w[0] != w[1])
}

/// `a` and `b` joined across an edge they share running opposite ways, when they share one.
fn join(a: &[u16], b: &[u16]) -> Option<Vec<u16>> {
    for i in 0..a.len() {
        let (u, v) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            if b[j] == v && b[(j + 1) % b.len()] == u {
                // a up to u, then b from after v round to before u... b runs v, u, rest; take its rest.
                let mut out = Vec::with_capacity(a.len() + b.len() - 2);
                for k in 0..a.len() {
                    out.push(a[(i + 1 + k) % a.len()]);
                }
                // `out` is v .. u (a's ring starting at v, ending at u); then b's corners after u
                // and before v.
                for k in 2..b.len() {
                    out.push(b[(j + k) % b.len()]);
                }
                return Some(out);
            }
        }
    }
    None
}

/// Polygons (each a ring of corners into `pos`, with a key that must match to join, such as its
/// surface and sides) joined wherever two of one key share an edge in one plane and make a flat,
/// convex polygon of at most [`MAX_CORNERS`] corners. Deterministic: the result depends only on
/// the polygons' order.
#[must_use]
pub fn join_flat<K: Ord + Copy>(polys: &[(K, Vec<u16>)], pos: &[P]) -> Vec<(K, Vec<u16>)> {
    // By key and plane, so only polygons that could join are tried together.
    let mut groups: BTreeMap<(K, [i64; 4]), Vec<Vec<u16>>> = BTreeMap::new();
    let mut order: Vec<(K, [i64; 4])> = Vec::new();
    let mut lone: Vec<(K, Vec<u16>)> = Vec::new();
    for (k, ring) in polys {
        let pts: Vec<P> = ring.iter().map(|&i| pos[usize::from(i)]).collect();
        let Some(n) = normal(&pts) else {
            lone.push((*k, ring.clone()));
            continue;
        };
        let d = dot(n, pts[0]);
        #[allow(clippy::cast_possible_truncation)]
        let q = |v: f64| (v * 1000.0).round() as i64;
        let key = (*k, [q(n[0]), q(n[1]), q(n[2]), q(d)]);
        if !groups.contains_key(&key) {
            order.push(key);
        }
        groups.entry(key).or_default().push(ring.clone());
    }
    let mut out = Vec::with_capacity(polys.len());
    for key in order {
        let mut rings = groups.remove(&key).unwrap_or_default();
        let pts0: Vec<P> = rings[0].iter().map(|&i| pos[usize::from(i)]).collect();
        let n = normal(&pts0).unwrap_or([0.0, 0.0, 1.0]);
        let mut joined = true;
        while joined {
            joined = false;
            let mut i = 0;
            while i < rings.len() {
                let mut j = i + 1;
                while j < rings.len() {
                    if rings[i].len() + rings[j].len() - 2 <= MAX_CORNERS {
                        if let Some(r) = join(&rings[i], &rings[j]) {
                            if flat_convex(&r, pos, n) {
                                rings[i] = r;
                                rings.remove(j);
                                joined = true;
                                continue;
                            }
                        }
                    }
                    j += 1;
                }
                i += 1;
            }
        }
        out.extend(rings.into_iter().map(|r| (key.0, r)));
    }
    out.extend(lone);
    out
}

/// Where a model would draw one place twice, drawn once: polygons over the same corners (to the
/// millimeter) are found, a copy with the same surface and the same winding is left out, and two
/// facing opposite ways are each drawn from their own side only. AC2's meshes give a leaf its
/// back as a face of its own, and a cut-out material drawn from both sides would then lay the leaf
/// twice at one depth from either side, the two fighting as the eye moves.
///
/// `polys` is each polygon's surface, sides (0 one side, more both) and ring of corners into
/// `pos`. Returns each polygon's sides, or `None` where it is left out.
#[must_use]
pub fn unfought(polys: &[(u16, i32, &[u16])], pos: &[P]) -> Vec<Option<i32>> {
    #[allow(clippy::cast_possible_truncation)] // millimeters of a model a few hundred meters across
    let mm = |i: u16| -> [i64; 3] {
        let q = pos[usize::from(i)];
        [
            (q[0] * 1000.0).round() as i64,
            (q[1] * 1000.0).round() as i64,
            (q[2] * 1000.0).round() as i64,
        ]
    };
    let mut out: Vec<Option<i32>> = polys.iter().map(|p| Some(p.1)).collect();
    let mut by_corners: BTreeMap<Vec<[i64; 3]>, Vec<usize>> = BTreeMap::new();
    for (k, p) in polys.iter().enumerate() {
        let mut key: Vec<[i64; 3]> = p.2.iter().map(|&i| mm(i)).collect();
        key.sort_unstable();
        by_corners.entry(key).or_default().push(k);
    }
    for group in by_corners.values().filter(|g| g.len() > 1) {
        for (n, &a) in group.iter().enumerate() {
            for &b in &group[n + 1..] {
                if out[a].is_none() || out[b].is_none() {
                    continue;
                }
                let ra: Vec<[i64; 3]> = polys[a].2.iter().map(|&i| mm(i)).collect();
                let rb: Vec<[i64; 3]> = polys[b].2.iter().map(|&i| mm(i)).collect();
                let k = ra.len();
                let same = (0..k).any(|s| (0..k).all(|i| ra[i] == rb[(i + s) % k]));
                let opposite = (0..k).any(|s| (0..k).all(|i| ra[i] == rb[(k + s - i) % k]));
                if same && polys[a].0 == polys[b].0 {
                    out[b] = None;
                } else if opposite {
                    out[a] = Some(0);
                    out[b] = Some(0);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's shapes)
    use super::*;

    fn square() -> Vec<P> {
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
        ]
    }

    #[test]
    fn two_triangles_of_a_square_are_one_square() {
        let pos = square();
        let out = join_flat(&[(0u16, vec![0, 1, 2]), (0, vec![0, 2, 3])], &pos);
        assert_eq!(out.len(), 1);
        let mut c = out[0].1.clone();
        c.sort_unstable();
        assert_eq!(c, [0, 1, 2, 3]);
        assert!(flat_convex(&out[0].1, &pos, [0.0, 0.0, 1.0]));
    }

    #[test]
    fn a_strip_of_four_triangles_is_one_rectangle_and_other_materials_stay_apart() {
        let pos = square();
        let tris = [
            (0u16, vec![0, 1, 2]),
            (0, vec![0, 2, 3]),
            (0, vec![1, 4, 5]),
            (0, vec![1, 5, 2]),
            (1, vec![9, 9, 9]),
        ];
        let mut pos9 = pos.clone();
        pos9.extend([[0.0; 3]; 4]);
        let out = join_flat(&tris, &pos9);
        let zero: Vec<_> = out.iter().filter(|(k, _)| *k == 0).collect();
        assert_eq!(zero.len(), 1, "{out:?}");
        assert_eq!(
            zero[0].1.len(),
            6,
            "a rectangle with its middle corners kept"
        );
        assert_eq!(out.iter().filter(|(k, _)| *k == 1).count(), 1);
    }

    #[test]
    fn triangles_out_of_one_plane_or_making_a_dent_are_not_joined() {
        let mut pos = square();
        pos.push([1.0, 0.5, 1.0]);
        // Folded along the diagonal.
        let out = join_flat(&[(0u16, vec![0, 1, 2]), (0, vec![0, 2, 6])], &pos);
        assert_eq!(out.len(), 2);
        // An L of three squares' triangles cannot become one convex polygon.
        let pos = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 2.0, 0.0],
            [2.0, 1.0, 0.0],
            [2.0, 2.0, 0.0],
        ];
        let tris = [
            (0u16, vec![0, 1, 2]),
            (0, vec![0, 2, 3]),
            (0, vec![3, 2, 5]),
            (0, vec![3, 5, 4]),
            (0, vec![2, 6, 7]),
            (0, vec![2, 7, 5]),
        ];
        let out = join_flat(&tris, &pos);
        assert!(out.len() >= 2);
        for (_, r) in &out {
            assert!(flat_convex(r, &pos, [0.0, 0.0, 1.0]));
        }
        // Every triangle's area is still covered: the corners are the same set.
        let area = |r: &Vec<u16>| {
            let p: Vec<P> = r.iter().map(|&i| pos[usize::from(i)]).collect();
            let mut a = 0.0;
            for i in 0..p.len() {
                let (u, v) = (p[i], p[(i + 1) % p.len()]);
                a += u[0] * v[1] - v[0] * u[1];
            }
            a / 2.0
        };
        let total: f64 = out.iter().map(|(_, r)| area(r)).sum();
        assert!((total - 3.0).abs() < 1e-9, "{total}");
    }

    /// A face of AC2's collision with a corner standing on another polygon's edge (a signpost's
    /// side, its x and height, laid flat): joined in its order, a ring would run out to that
    /// corner and back along the edge, a polygon a body walks through.
    #[test]
    fn a_ring_that_runs_out_and_back_along_an_edge_is_not_made() {
        let pos: Vec<P> = [
            (0.532_993_7, 3.300_760_5),
            (-1.996_146_3, 2.523_478),
            (0.532_993_7, -1.651_556),
            (-2.467_006_2, 2.378_768_4),
            (-2.467_006_2, -1.651_556),
            (-2.223_403_2, 2.898_627_5),
            (-0.775_475_86, 2.898_627_5),
            (-2.467_006_2, 3.300_760_5),
            (-2.467_006_2, 2.898_627_8),
        ]
        .iter()
        .map(|&(x, z)| [x, -z, 0.0])
        .collect();
        let polys = [
            (0u16, vec![5, 1, 6]),
            (0, vec![8, 3, 1, 5]),
            (0, vec![0, 1, 2]),
            (0, vec![3, 4, 2, 1]),
            (0, vec![0, 7, 5, 6]),
            (0, vec![7, 8, 5]),
        ];
        let n = [0.0, 0.0, -1.0];
        for (_, r) in &polys {
            assert!(flat_convex(r, &pos, n), "{r:?}");
        }
        let out = join_flat(&polys, &pos);
        for (_, r) in &out {
            assert!(flat_convex(r, &pos, n), "{r:?}");
            // No corner outside any edge: each corner is on the inner side of every edge.
            let p: Vec<P> = r.iter().map(|&i| pos[usize::from(i)]).collect();
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                for q in &p {
                    assert!(dot(cross(sub(b, a), sub(*q, a)), n) > -1e-3, "{r:?}");
                }
            }
        }
        // The face's area is all still there.
        let area = |r: &Vec<u16>| {
            let p: Vec<P> = r.iter().map(|&i| pos[usize::from(i)]).collect();
            (0..p.len())
                .map(|i| {
                    let (u, v) = (p[i], p[(i + 1) % p.len()]);
                    u[0] * v[1] - v[0] * u[1]
                })
                .sum::<f64>()
                .abs()
                / 2.0
        };
        let before: f64 = polys.iter().map(|(_, r)| area(r)).sum();
        let after: f64 = out.iter().map(|(_, r)| area(r)).sum();
        assert!((before - after).abs() < 1e-9, "{before} {after}");
    }

    #[test]
    fn a_leaf_with_its_own_back_is_drawn_from_each_side_once() {
        let pos = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [5.0, 5.0, 5.0],
            [6.0, 5.0, 5.0],
            [5.0, 6.0, 5.0],
        ];
        let (front, back, copy, other) = ([0u16, 1, 2], [0u16, 2, 1], [1u16, 2, 0], [3u16, 4, 5]);
        // Front and back both two-sided: each drawn from its own side; a lone leaf keeps both.
        let polys = [(7u16, 1, &front[..]), (7, 1, &back[..]), (7, 1, &other[..])];
        assert_eq!(unfought(&polys, &pos), vec![Some(0), Some(0), Some(1)]);
        // A copy over the same corners the same way round, with the same surface: left out.
        let polys = [(7u16, 0, &front[..]), (7, 0, &copy[..])];
        assert_eq!(unfought(&polys, &pos), vec![Some(0), None]);
        // The same corners with another surface are not a copy.
        let polys = [(7u16, 0, &front[..]), (8, 0, &copy[..])];
        assert_eq!(unfought(&polys, &pos), vec![Some(0), Some(0)]);
        // A one-sided leaf and its one-sided back stay as they are.
        let polys = [(7u16, 0, &front[..]), (7, 0, &back[..])];
        assert_eq!(unfought(&polys, &pos), vec![Some(0), Some(0)]);
    }
}

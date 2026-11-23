//! AC2's roads on the converted grid: which converted vertices carry a road, and AC2's road
//! masks as AC1's road alpha maps.
//!
//! AC2 draws a road from its vertices' road values on its 10 m squares, each square laying the
//! road through a mask for the corners that carry one: along a side, a diagonal or round a corner,
//! the road reaching about a third of the square in from them (a road along a line of vertices is
//! some 6.4 m wide). AC1 draws the same way on its own squares, with its own masks.
//!
//! **The vertices.** Each AC2 road vertex lands on the converted vertex nearest it; each pair of
//! AC2 road vertices AC2 joins (side by side, or across a square whose other two corners carry
//! none) is joined on the converted grid too, by the converted vertex nearest their midpoint
//! where their own two do not touch; and an AC2 square that is road at all four corners covers
//! every converted vertex from its south-west corner's to its north-east corner's. So a road one
//! vertex wide in AC2 is one vertex wide on the converted grid wherever it lies, as the masks need:
//! AC2's 10 m and the converted 8 m grids meet only every 40 m, and taking each converted vertex's
//! nearest AC2 vertex instead makes a road two vertices wide on one AC2 line in four.
//!
//! **The masks** ([`Masks`]): AC2's own, turned to each AC1 map's corners and stretched by AC2's
//! square over the converted one, so the road reaches as many meters into an 8 m square as AC2's
//! reaches into its 10 m one.

use crate::ac2_world::ground::Ac2Ground;

/// The converted vertex (its index along one axis, `step` meters apart) nearest a point `m` meters
/// along that axis; the lower of the two on a tie.
#[must_use]
pub fn snap(m: f64, step: f64) -> i64 {
    // LINT-OK: world coordinates, a few thousand vertices.
    #[allow(clippy::cast_possible_truncation)]
    let j = (m / step - 0.5).ceil() as i64;
    j
}

/// The road value of AC2 vertex `(i, k)` (world-wide indices), 0 off the world or where there is
/// none.
fn road_of(g: &Ac2Ground, i: i64, k: i64) -> u8 {
    let side = i64::try_from(g.side()).unwrap_or(0);
    if !(0..side).contains(&i) || !(0..side).contains(&k) {
        return 0;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let v = (i * side + k) as usize;
    g.road[v]
}

/// The road the converted vertex `(jx, jy)` (`step` meters apart) carries, 0 for none: the value
/// of the nearest AC2 road vertex among those whose road lands on it (see the module's notes).
#[must_use]
pub fn road_at(g: &Ac2Ground, step: f64, jx: i64, jy: i64) -> u8 {
    #[allow(clippy::cast_precision_loss)]
    let (x, y) = (jx as f64 * step, jy as f64 * step);
    let l = g.square_len;
    // Every AC2 vertex a feature landing here can start from: a square reaches `l` past its
    // south-west corner, and a point lands here from within half a step.
    let reach = l + step;
    #[allow(clippy::cast_possible_truncation)]
    let range = |c: f64| ((c - reach) / l).floor() as i64..=((c + reach) / l).ceil() as i64;
    let (xs, ys) = (range(x), range(y));
    if !xs
        .clone()
        .any(|i| ys.clone().any(|k| road_of(g, i, k) != 0))
    {
        return 0;
    }
    #[allow(clippy::cast_precision_loss)]
    let at = |i: i64| i as f64 * l;
    let here = |px: f64, py: f64| snap(px, step) == jx && snap(py, step) == jy;
    // The nearest source vertex so far: its squared distance and road.
    let mut best: Option<(f64, u8)> = None;
    let mut take = |i: i64, k: i64| {
        let r = road_of(g, i, k);
        let d = (at(i) - x).powi(2) + (at(k) - y).powi(2);
        if r != 0 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, r));
        }
    };
    // Two AC2 road vertices joined: on the converted vertex nearest their midpoint, where their
    // own converted vertices do not touch.
    let joins = |(i0, k0): (i64, i64), (i1, k1): (i64, i64)| {
        let apart = (snap(at(i0), step) - snap(at(i1), step))
            .abs()
            .max((snap(at(k0), step) - snap(at(k1), step)).abs());
        apart > 1 && here((at(i0) + at(i1)) / 2.0, (at(k0) + at(k1)) / 2.0)
    };
    for i in xs {
        for k in ys.clone() {
            let [sw, se, ne, nw] = [(i, k), (i + 1, k), (i + 1, k + 1), (i, k + 1)];
            let road = |(a, b): (i64, i64)| road_of(g, a, b) != 0;
            if road(sw) && here(at(i), at(k)) {
                take(i, k);
            }
            // The square's south and west sides (every side is some square's), and its
            // diagonals where its other two corners carry no road.
            for (a, b) in [(sw, se), (sw, nw)] {
                if road(a) && road(b) && joins(a, b) {
                    take(a.0, a.1);
                    take(b.0, b.1);
                }
            }
            for (a, b, c, d) in [(sw, ne, se, nw), (se, nw, sw, ne)] {
                if road(a) && road(b) && !road(c) && !road(d) && joins(a, b) {
                    take(a.0, a.1);
                    take(b.0, b.1);
                }
            }
            // A square that is road at all four corners covers what lies between its corners'
            // converted vertices.
            if [sw, se, ne, nw].into_iter().all(road)
                && (snap(at(i), step)..=snap(at(i + 1), step)).contains(&jx)
                && (snap(at(k), step)..=snap(at(k + 1), step)).contains(&jy)
            {
                for c in [sw, se, ne, nw] {
                    take(c.0, c.1);
                }
            }
        }
    }
    best.map_or(0, |(_, r)| r)
}

/// The shapes AC1's road alpha maps take, by the corners a map's code names (bit 1 south-west,
/// 2 south-east, 4 north-east, 8 north-west).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One corner.
    Corner,
    /// Two corners on one side.
    Side,
    /// Two opposite corners.
    Diagonal,
}

/// The corners' places in a road alpha map, as AC1's own maps lay them (`u` east, `v` south, each
/// 0 to 1): south-west, south-east, north-east, north-west.
const CORNERS: [(f64, f64); 4] = [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)];

/// The shape a road code draws and its corners' places; `None` for a code of no one shape.
#[must_use]
pub fn shape_of(code: u32) -> Option<(Shape, Vec<(f64, f64)>)> {
    let corners: Vec<(f64, f64)> = (0..4)
        .filter(|b| code & (1 << b) != 0)
        .map(|b| CORNERS[b])
        .collect();
    match corners.as_slice() {
        [_] => Some((Shape::Corner, corners)),
        [a, b] if a.0 == b.0 || a.1 == b.1 => Some((Shape::Side, corners)),
        [_, _] => Some((Shape::Diagonal, corners)),
        _ => None,
    }
}

/// One of AC2's road masks: its texels' road share (0 to 1), rows from the top, as AC2 lays it in
/// its own picture: a side's road along the top, a corner's at the top left, and a diagonal's from
/// the bottom left to the top right.
pub struct Mask<'a> {
    pub width: usize,
    pub height: usize,
    pub share: &'a [f32],
}

impl Mask<'_> {
    /// The road share at `(u, v)` (0 to 1 across and down), bilinear between texel centers and
    /// clamped at the edges.
    #[must_use]
    pub fn at(&self, u: f64, v: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.width as f64, self.height as f64);
        let (fx, fy) = (
            (u * w - 0.5).clamp(0.0, w - 1.0),
            (v * h - 0.5).clamp(0.0, h - 1.0),
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        #[allow(clippy::cast_precision_loss)]
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let s = |x: usize, y: usize| f64::from(self.share[y * self.width + x]);
        let top = s(x0, y0) * (1.0 - tx) + s(x1, y0) * tx;
        let bottom = s(x0, y1) * (1.0 - tx) + s(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// AC2's masks for the three shapes.
pub struct Masks<'a> {
    pub corner: Mask<'a>,
    pub side: Mask<'a>,
    pub diagonal: Mask<'a>,
}

/// The road share at `(u, v)` of an AC1 road alpha map of shape `shape` at `corners` (from
/// [`shape_of`]), drawn from AC2's mask of that shape with its depth stretched by `stretch` (AC2's
/// square over the converted one): what lies `d` of the way in from the road's corners or side
/// takes AC2's share `d / stretch` of the way in.
#[must_use]
pub fn share_at(
    m: &Masks<'_>,
    shape: Shape,
    corners: &[(f64, f64)],
    stretch: f64,
    u: f64,
    v: f64,
) -> f64 {
    match shape {
        Shape::Corner => {
            let (cu, cv) = corners[0];
            m.corner
                .at((u - cu).abs() / stretch, (v - cv).abs() / stretch)
        }
        Shape::Side => {
            let (a, b) = (corners[0], corners[1]);
            // Depth in from the side, and the way along it from its first corner.
            let (depth, along) = if a.0 == b.0 {
                ((u - a.0).abs(), (v - a.1).abs())
            } else {
                ((v - a.1).abs(), (u - a.0).abs())
            };
            m.side.at(along, depth / stretch)
        }
        Shape::Diagonal => {
            // From the first corner towards the other, and across that line.
            let (a, b) = (corners[0], corners[1]);
            let (du, dv) = (b.0 - a.0, b.1 - a.1);
            let (pu, pv) = (u - a.0, v - a.1);
            let along = (pu * du + pv * dv) / 2.0;
            let across = (pu * dv - pv * du) / 2.0 / stretch;
            // AC2's own diagonal runs from (0, 1) to (1, 0): along it `(t, 1 - t)`, and across
            // it `(s, s)`.
            m.diagonal.at(along + across, 1.0 - along + across)
        }
    }
}

/// An AC1 road alpha map `size` texels a side for road code `code`, as AC1's alpha maps hold
/// it (one byte a texel, rows from the top, 0 where the road is laid and 255 where the ground
/// beneath stays); `None` for a code of no one shape.
#[must_use]
pub fn alpha_map(m: &Masks<'_>, code: u32, stretch: f64, size: usize) -> Option<Vec<u8>> {
    let (shape, corners) = shape_of(code)?;
    #[allow(clippy::cast_precision_loss)]
    let n = size as f64;
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            #[allow(clippy::cast_precision_loss)]
            let (u, v) = ((x as f64 + 0.5) / n, (y as f64 + 0.5) / n);
            let s = share_at(m, shape, &corners, stretch, u, v).clamp(0.0, 1.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            out.push(((1.0 - s) * 255.0).round() as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's roads)
    use super::*;

    fn cut_all(_: i32, _: i32) -> bool {
        true
    }

    /// AC2 ground of 10 m squares, `n` a side in one block, with roads at `roads` (vertex indices).
    fn ground(n: usize, roads: &[(usize, usize)]) -> Ac2Ground {
        let mut g = Ac2Ground::new(1, n, 10.0, cut_all);
        let v = (n + 1) * (n + 1);
        let mut road = vec![0u8; v];
        for &(i, k) in roads {
            road[i * (n + 1) + k] = 1;
        }
        g.put_block(0, 0, &vec![100.0; v], &vec![1; v], &vec![0; v], &road);
        g
    }

    /// The converted vertices (8 m apart) carrying a road, over `side` of them each way.
    fn converted(g: &Ac2Ground, side: i64) -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        for jx in 0..side {
            for jy in 0..side {
                if road_at(g, 8.0, jx, jy) != 0 {
                    out.push((jx, jy));
                }
            }
        }
        out
    }

    #[test]
    fn a_point_snaps_to_the_nearest_vertex_and_the_lower_on_a_tie() {
        assert_eq!(snap(0.0, 8.0), 0);
        assert_eq!(snap(10.0, 8.0), 1);
        assert_eq!(snap(20.0, 8.0), 2);
        assert_eq!(snap(30.0, 8.0), 4);
        assert_eq!(snap(40.0, 8.0), 5);
        assert_eq!(snap(60.0, 8.0), 7);
        assert_eq!(snap(-4.0, 8.0), -1);
    }

    /// A road along a line of AC2 vertices is one converted vertex wide wherever the line lies
    /// (the line at 20 m too, halfway between two converted vertices), and unbroken along it.
    #[test]
    fn a_road_one_vertex_wide_stays_one_vertex_wide_and_unbroken() {
        for line in 0..=4usize {
            let roads: Vec<_> = (0..=8).map(|k| (line, k)).collect();
            let g = ground(8, &roads);
            let got = converted(&g, 11);
            let columns: std::collections::BTreeSet<i64> = got.iter().map(|p| p.0).collect();
            assert_eq!(columns.len(), 1, "line {line}: {got:?}");
            let col = *columns.iter().next().unwrap();
            assert!(
                (col as f64 * 8.0 - line as f64 * 10.0).abs() <= 4.0,
                "line {line} at column {col}"
            );
            assert_eq!(
                got.iter().map(|p| p.1).collect::<Vec<_>>(),
                (0..=10).collect::<Vec<_>>(),
                "line {line}"
            );
        }
    }

    /// A diagonal road, and one that steps one way then the other, stay one vertex wide and
    /// unbroken: each converted road vertex touches the next, corner to corner at most.
    #[test]
    fn a_winding_road_stays_unbroken_and_one_vertex_wide() {
        let diagonal: Vec<_> = (0..=8).map(|k| (k, k)).collect();
        let knight: Vec<_> = (0..=8).map(|k| (k / 2 + 2, k)).collect();
        for path in [diagonal, knight] {
            let g = ground(8, &path);
            let got = converted(&g, 11);
            // Each AC2 vertex's own converted vertex carries the road.
            for &(i, k) in &path {
                assert!(
                    got.contains(&(snap(i as f64 * 10.0, 8.0), snap(k as f64 * 10.0, 8.0))),
                    "{path:?}"
                );
            }
            // Unbroken: every road vertex reached from the first through touching ones.
            let mut seen = vec![got[0]];
            let mut k = 0;
            while k < seen.len() {
                let (x, y) = seen[k];
                for &p in &got {
                    if (p.0 - x).abs() <= 1 && (p.1 - y).abs() <= 1 && !seen.contains(&p) {
                        seen.push(p);
                    }
                }
                k += 1;
            }
            assert_eq!(seen.len(), got.len(), "{got:?}");
            // One vertex wide: no converted square is road at all four corners.
            for &(x, y) in &got {
                let square = [(x + 1, y), (x, y + 1), (x + 1, y + 1)];
                assert!(!square.iter().all(|p| got.contains(p)), "{got:?}");
            }
        }
        // The straight diagonal is one vertex a row.
        let g = ground(8, &(0..=8).map(|k| (k, k)).collect::<Vec<_>>());
        assert_eq!(
            converted(&g, 11),
            (0..=10).map(|k| (k, k)).collect::<Vec<_>>()
        );
    }

    /// A square that is road at all four corners covers every converted vertex between its
    /// corners'; a lone road vertex is one converted vertex.
    #[test]
    fn a_square_of_road_is_covered_and_a_lone_road_vertex_is_one_vertex() {
        let g = ground(4, &[(1, 1), (2, 1), (1, 2), (2, 2)]);
        assert_eq!(converted(&g, 7), vec![(1, 1), (1, 2), (2, 1), (2, 2)]);
        let g = ground(4, &[(1, 1), (2, 1), (1, 2), (2, 2), (3, 1), (3, 2)]);
        // From 10 m to 30 m: converted 1 to 4.
        assert_eq!(converted(&g, 7).len(), 8);
        let g = ground(4, &[(3, 3)]);
        assert_eq!(converted(&g, 7), vec![(4, 4)]);
    }

    /// Two road vertices across a square whose other corners are road too are not joined across
    /// it: the road goes round by the third corner, as AC2 draws it.
    #[test]
    fn a_diagonal_is_joined_only_where_the_square_has_no_third_road_corner() {
        let g = ground(4, &[(2, 2), (3, 3)]);
        assert_eq!(converted(&g, 7), vec![(2, 2), (3, 3), (4, 4)]);
        let g = ground(4, &[(2, 2), (3, 3), (3, 2)]);
        assert!(
            !converted(&g, 7).contains(&(3, 3)),
            "{:?}",
            converted(&g, 7)
        );
    }

    #[test]
    fn a_codes_corners_give_its_shape() {
        assert_eq!(shape_of(8).map(|s| s.0), Some(Shape::Corner));
        assert_eq!(shape_of(9).map(|s| s.0), Some(Shape::Side));
        assert_eq!(shape_of(6).map(|s| s.0), Some(Shape::Side));
        assert_eq!(shape_of(10).map(|s| s.0), Some(Shape::Diagonal));
        assert_eq!(shape_of(5).map(|s| s.0), Some(Shape::Diagonal));
        assert!(shape_of(7).is_none() && shape_of(0).is_none() && shape_of(15).is_none());
    }

    /// AC2-like masks: a side's road the top `depth` of it, a corner's within `depth` of the top
    /// left, a diagonal's within `depth` across of the line from the bottom left to the top right.
    fn masks(n: usize, depth: f64) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let cell = |f: &dyn Fn(f64, f64) -> bool| -> Vec<f32> {
            let mut out = Vec::new();
            for y in 0..n {
                for x in 0..n {
                    let (u, v) = ((x as f64 + 0.5) / n as f64, (y as f64 + 0.5) / n as f64);
                    out.push(if f(u, v) { 1.0 } else { 0.0 });
                }
            }
            out
        };
        (
            cell(&|_, v| v < depth),
            cell(&|u, v| (u * u + v * v).sqrt() < depth),
            cell(&|u, v| (u - (1.0 - v)).abs() / 2.0 < depth / 2.0),
        )
    }

    /// Each AC1 map lays the road at its own corners, and as far in as AC2's reaches into its own
    /// square, stretched: a side's road a third of the way into AC2's square reaches 1.25 thirds
    /// into the map at a stretch of 1.25.
    #[test]
    fn each_map_lays_the_road_at_its_corners_stretched_by_the_squares() {
        let (side, corner, diagonal) = masks(128, 0.3);
        let m = Masks {
            corner: Mask {
                width: 128,
                height: 128,
                share: &corner,
            },
            side: Mask {
                width: 128,
                height: 128,
                share: &side,
            },
            diagonal: Mask {
                width: 128,
                height: 128,
                share: &diagonal,
            },
        };
        let size = 200;
        let road = |a: &[u8], u: f64, v: f64| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (x, y) = ((u * size as f64) as usize, (v * size as f64) as usize);
            a[y * size + x] < 128
        };
        // West side (south-west and north-west): the road 0.375 in from the left.
        let west = alpha_map(&m, 9, 1.25, size).unwrap();
        assert!(
            road(&west, 0.36, 0.5)
                && !road(&west, 0.39, 0.5)
                && road(&west, 0.1, 0.02)
                && road(&west, 0.1, 0.98)
        );
        // East side.
        let east = alpha_map(&m, 6, 1.25, size).unwrap();
        assert!(road(&east, 0.64, 0.5) && !road(&east, 0.61, 0.5));
        // North-west corner.
        let nw = alpha_map(&m, 8, 1.25, size).unwrap();
        assert!(road(&nw, 0.25, 0.25) && !road(&nw, 0.28, 0.28) && !road(&nw, 0.9, 0.9));
        // The diagonal from south-east to north-west: on it, and off it to either side.
        let d = alpha_map(&m, 10, 1.25, size).unwrap();
        assert!(road(&d, 0.5, 0.5) && road(&d, 0.1, 0.1) && road(&d, 0.9, 0.9));
        assert!(road(&d, 0.6, 0.4) && !road(&d, 0.9, 0.1) && !road(&d, 0.1, 0.9));
        // The other diagonal.
        let d = alpha_map(&m, 5, 1.25, size).unwrap();
        assert!(
            road(&d, 0.1, 0.9) && road(&d, 0.9, 0.1) && !road(&d, 0.1, 0.1) && !road(&d, 0.9, 0.9)
        );
    }
}

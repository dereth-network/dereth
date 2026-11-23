//! AC2's ground as one world-wide grid: its heights in AC2 meters, its vertex words, which blocks
//! it holds, and each block's water plane; and the ground's height anywhere on AC2's own
//! triangles.

/// AC2's land, world-wide. Vertex `(gx, gy)` is block `(gx / squares, gy / squares)`'s vertex
/// `(gx % squares, gy % squares)`; neighboring blocks share their edge vertices.
#[derive(Clone)]
pub struct Ac2Ground {
    /// Blocks along a side of the world.
    pub blocks: usize,
    /// Squares along a block's side.
    pub squares: usize,
    /// A square's side in meters.
    pub square_len: f64,
    /// `blocks * squares + 1` vertices a side; heights in AC2 meters (NaN where no block holds
    /// the vertex), x major.
    pub height: Vec<f32>,
    /// The vertices' terrain kind, scene and road, x major.
    pub kind: Vec<u8>,
    pub scene: Vec<u8>,
    pub road: Vec<u8>,
    /// Per block, x major: whether the world holds it.
    pub have: Vec<bool>,
    /// Per block, x major: its water plane in AC2 meters, `None` for a block without water.
    pub plane: Vec<Option<f32>>,
    /// Whether global square `(gx, gy)` is cut south-west to north-east.
    pub split: fn(i32, i32) -> bool,
    /// The vertices' material group: the group of their kind's surface their slope falls in, as
    /// AC2 chooses it (0 until worked out), x major.
    pub group: Vec<u8>,
    /// By kind and group, the slot and variant a vertex takes where its group draws another
    /// picture than its kind's flat ground ([`crate::ac2_world::steep::allocate`]); the rest take their kind's.
    pub steep: crate::ac2_world::steep::ByKind,
}

/// What the converter needs of the ground at one point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// The ground's height on AC2's triangles, AC2 meters.
    pub ground: f64,
    /// The water plane there (the lowest of the blocks the point touches; none if one of them
    /// has no water).
    pub plane: Option<f64>,
    /// Whether the blocks the point touches have different planes, so the lowest-plane rule
    /// chose between them.
    pub edge_rule: bool,
}

impl Ac2Ground {
    /// An empty world of `blocks` a side, `squares` a block side of `square_len` meters.
    #[must_use]
    pub fn new(
        blocks: usize,
        squares: usize,
        square_len: f64,
        split: fn(i32, i32) -> bool,
    ) -> Self {
        let side = blocks * squares + 1;
        Self {
            blocks,
            squares,
            square_len,
            height: vec![f32::NAN; side * side],
            kind: vec![0; side * side],
            scene: vec![0; side * side],
            road: vec![0; side * side],
            have: vec![false; blocks * blocks],
            plane: vec![None; blocks * blocks],
            split,
            group: vec![0; side * side],
            steep: std::collections::BTreeMap::new(),
        }
    }

    /// The AC1 slot and variant vertex `v` takes: its kind's, or its steep group's.
    #[must_use]
    pub fn slot_of(&self, v: usize) -> (u16, u16) {
        let kind = self.kind[v];
        self.steep
            .get(&(kind, self.group[v]))
            .copied()
            .unwrap_or_else(|| crate::ac2_world::kinds::land_slot(u32::from(kind)))
    }

    /// The slope at vertex `(gx, gy)` as AC2 chooses a material group by it: `n.x² + n.y²` of its
    /// unit normal, the equal-weight sum of the unit normals of every triangle touching it (each
    /// square cut as AC2 cuts it), normalized. A vertex no block holds reads height 0, as AC2
    /// reads a missing block.
    #[must_use]
    pub fn pitch(&self, gx: i64, gy: i64) -> f32 {
        let side = i64::try_from(self.side()).unwrap_or(0);
        #[allow(clippy::cast_possible_truncation)]
        let len = self.square_len as f32;
        let point = |x: i64, y: i64| -> [f32; 3] {
            let h = if (0..side).contains(&x) && (0..side).contains(&y) {
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                let k = (x * side + y) as usize;
                let h = self.height[k];
                if h.is_nan() {
                    0.0
                } else {
                    h
                }
            } else {
                0.0
            };
            #[allow(clippy::cast_precision_loss)]
            [x as f32 * len, y as f32 * len, h]
        };
        let unit = |v: [f32; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 0.0 {
                [v[0] / l, v[1] / l, v[2] / l]
            } else {
                [0.0; 3]
            }
        };
        let mut sum = [0.0f32; 3];
        for (a, b) in [(gx - 1, gy - 1), (gx, gy - 1), (gx - 1, gy), (gx, gy)] {
            let (sw, se, ne, nw) = ((a, b), (a + 1, b), (a + 1, b + 1), (a, b + 1));
            #[allow(clippy::cast_possible_truncation)]
            let cut = (self.split)(a as i32, b as i32);
            let tris = if cut {
                [[sw, se, ne], [sw, ne, nw]]
            } else {
                [[sw, se, nw], [ne, nw, se]]
            };
            for t in tris {
                if !t.contains(&(gx, gy)) {
                    continue;
                }
                let [p0, p1, p2] = t.map(|(x, y)| point(x, y));
                let (u, v) = (
                    [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]],
                    [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]],
                );
                let n = unit([
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ]);
                sum = [sum[0] + n[0], sum[1] + n[1], sum[2] + n[2]];
            }
        }
        let n = unit(sum);
        n[0] * n[0] + n[1] * n[1]
    }

    /// Vertices along a side.
    #[must_use]
    pub fn side(&self) -> usize {
        self.blocks * self.squares + 1
    }

    /// A block's side in meters.
    #[must_use]
    pub fn block_len(&self) -> f64 {
        self.squares as f64 * self.square_len
    }

    /// Lay block `(bx, by)`: its `(squares + 1)²` heights (meters) and kinds, scenes and roads, x
    /// major.
    pub fn put_block(
        &mut self,
        bx: usize,
        by: usize,
        h: &[f32],
        kind: &[u8],
        scene: &[u8],
        road: &[u8],
    ) {
        let n = self.squares + 1;
        let side = self.side();
        for i in 0..n {
            for j in 0..n {
                let g = (bx * self.squares + i) * side + by * self.squares + j;
                let k = i * n + j;
                self.height[g] = h[k];
                self.kind[g] = kind[k];
                self.scene[g] = scene[k];
                self.road[g] = road[k];
            }
        }
        self.have[bx * self.blocks + by] = true;
    }

    /// The square holding world point `(x, y)` and the point's place in it (0..=1 each way);
    /// `None` off the world.
    fn square_of(&self, x: f64, y: f64) -> Option<(usize, usize, f64, f64)> {
        let n = self.blocks * self.squares;
        let (u, v) = (x / self.square_len, y / self.square_len);
        if !(0.0..=n as f64).contains(&u) || !(0.0..=n as f64).contains(&v) {
            return None;
        }
        // LINT-OK: inside the world, a few thousand squares.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (gx, gy) = (
            (u.floor() as usize).min(n - 1),
            (v.floor() as usize).min(n - 1),
        );
        Some((gx, gy, u - gx as f64, v - gy as f64))
    }

    /// The ground's height at world point `(x, y)` on AC2's own triangles; `None` off the world or
    /// in a block it does not hold.
    #[must_use]
    pub fn ground(&self, x: f64, y: f64) -> Option<f64> {
        let (gx, gy, fu, fv) = self.square_of(x, y)?;
        if !self.have[(gx / self.squares) * self.blocks + gy / self.squares] {
            return None;
        }
        let side = self.side();
        let h = |i: usize, j: usize| f64::from(self.height[i * side + j]);
        let (h00, h10, h11, h01) = (h(gx, gy), h(gx + 1, gy), h(gx + 1, gy + 1), h(gx, gy + 1));
        if [h00, h10, h11, h01].iter().any(|v| v.is_nan()) {
            return None;
        }
        // LINT-OK: square coordinates of a world a few thousand squares wide.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let cut = (self.split)(gx as i32, gy as i32);
        Some(on_square(cut, [h00, h10, h11, h01], fu, fv))
    }

    /// The blocks the point touches along one axis: two on a block edge, else one; clamped to the
    /// world.
    fn touching(&self, x: f64) -> (usize, usize) {
        let b = x / self.block_len();
        let last = self.blocks - 1;
        // LINT-OK: block coordinates of the world.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let f = (b.floor().max(0.0) as usize).min(last);
        if b.fract() != 0.0 || b <= 0.0 {
            (f, f)
        } else if b > last as f64 {
            (last, last)
        } else {
            (f - 1, f)
        }
    }

    /// The water planes of the blocks world point `(x, y)` touches: one inside a block, two on
    /// an edge, four on a corner.
    #[must_use]
    pub fn planes_touching(&self, x: f64, y: f64) -> Vec<Option<f64>> {
        let (x0, x1) = self.touching(x);
        let (y0, y1) = self.touching(y);
        let nx = if x0 == x1 { 1 } else { 2 };
        let ny = if y0 == y1 { 1 } else { 2 };
        let mut out = Vec::with_capacity(4);
        for bx in [x0, x1].into_iter().take(nx) {
            for by in [y0, y1].into_iter().take(ny) {
                out.push(self.plane[bx * self.blocks + by].map(f64::from));
            }
        }
        out
    }

    /// The water plane at world point `(x, y)`: the lowest of the planes of the blocks it touches,
    /// none when one of them has no water; and whether those blocks' planes differ.
    #[must_use]
    pub fn plane(&self, x: f64, y: f64) -> (Option<f64>, bool) {
        let planes = self.planes_touching(x, y);
        let differ = planes.windows(2).any(|w| w[0] != w[1]);
        let plane = planes
            .iter()
            .try_fold(f64::INFINITY, |low, p| p.map(|p| low.min(p)));
        (plane, differ)
    }

    /// The ground and water at world point `(x, y)`; `None` where AC2 has no ground.
    #[must_use]
    pub fn sample(&self, x: f64, y: f64) -> Option<Sample> {
        let ground = self.ground(x, y)?;
        let (plane, edge_rule) = self.plane(x, y);
        Some(Sample {
            ground,
            plane,
            edge_rule,
        })
    }

    /// Whether AC2 vertex `(gx, gy)` is land: above the water plane there (or with none).
    pub fn is_land(&self, gx: usize, gy: usize) -> bool {
        let h = self.height[gx * self.side() + gy];
        if h.is_nan() {
            return false;
        }
        let (x, y) = (gx as f64 * self.square_len, gy as f64 * self.square_len);
        self.plane(x, y).0.is_none_or(|p| f64::from(h) > p)
    }

    /// The AC2 vertex whose kind and scene a converted vertex at `(x, y)` takes: the nearest
    /// corner of its square that is land, else the nearest corner (ties go south-west, south-east,
    /// north-west, north-east). Its index into the vertex arrays.
    #[must_use]
    pub fn kind_vertex(&self, x: f64, y: f64) -> Option<usize> {
        let (gx, gy, fu, fv) = self.square_of(x, y)?;
        let side = self.side();
        let mut corners = [
            (fu * fu + fv * fv, gx, gy),
            ((1.0 - fu).powi(2) + fv * fv, gx + 1, gy),
            (fu * fu + (1.0 - fv).powi(2), gx, gy + 1),
            ((1.0 - fu).powi(2) + (1.0 - fv).powi(2), gx + 1, gy + 1),
        ];
        // A stable sort keeps the fixed order on ties.
        corners.sort_by(|a, b| a.0.total_cmp(&b.0));
        let pick = corners
            .iter()
            .find(|c| self.is_land(c.1, c.2))
            .unwrap_or(&corners[0]);
        Some(pick.1 * side + pick.2)
    }

    /// The AC2 vertex nearest world point `(x, y)` (rounding half up), its index.
    #[must_use]
    pub fn nearest_vertex(&self, x: f64, y: f64) -> Option<usize> {
        let (gx, gy, fu, fv) = self.square_of(x, y)?;
        let i = if fu >= 0.5 { gx + 1 } else { gx };
        let j = if fv >= 0.5 { gy + 1 } else { gy };
        Some(i * self.side() + j)
    }
}

/// The height at `(fu, fv)` (0..=1 each way, east and north) on a square with corner heights
/// south-west, south-east, north-east, north-west, cut south-west to north-east when `cut`, else
/// south-east to north-west.
#[must_use]
pub fn on_square(cut: bool, [h00, h10, h11, h01]: [f64; 4], fu: f64, fv: f64) -> f64 {
    if cut {
        if fu >= fv {
            h00 + fu * (h10 - h00) + fv * (h11 - h10)
        } else {
            h00 + fv * (h01 - h00) + fu * (h11 - h01)
        }
    } else if fu + fv <= 1.0 {
        h00 + fu * (h10 - h00) + fv * (h01 - h00)
    } else {
        h11 + (1.0 - fu) * (h01 - h11) + (1.0 - fv) * (h10 - h11)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's ground)
    use super::*;

    fn cut_all(_: i32, _: i32) -> bool {
        true
    }
    fn cut_none(_: i32, _: i32) -> bool {
        false
    }

    /// A vertex's slope reads as AC2 chooses a group by it: on a plane rising at angle a, every
    /// vertex inside reads sin(a) squared, whichever way its squares are cut; flat ground reads 0.
    #[test]
    fn a_vertex_on_a_plane_reads_the_square_of_its_slopes_sine() {
        for split in [cut_all as fn(i32, i32) -> bool, cut_none] {
            for degrees in [0.0f64, 20.0, 35.0, 50.0] {
                let mut g = Ac2Ground::new(1, 4, 10.0, split);
                let side = g.side();
                let rise = dereth_primitives::num::math::tan(degrees.to_radians());
                for x in 0..side {
                    for y in 0..side {
                        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                        let h =
                            (x as f64 * 10.0 * rise * 0.6 + y as f64 * 10.0 * rise * 0.8) as f32;
                        g.height[x * side + y] = h;
                    }
                }
                let want = dereth_primitives::num::math::sin(degrees.to_radians()).powi(2);
                for (x, y) in [(1, 1), (2, 3), (3, 2)] {
                    let got = f64::from(g.pitch(x, y));
                    assert!(
                        (got - want).abs() < 1e-4,
                        "{degrees} degrees at ({x}, {y}): {got} vs {want}"
                    );
                }
            }
        }
    }

    /// One block of one square, 10 m, with corner heights sw, se, ne, nw.
    fn square(corners: [f32; 4], split: fn(i32, i32) -> bool) -> Ac2Ground {
        let mut g = Ac2Ground::new(1, 1, 10.0, split);
        let [sw, se, ne, nw] = corners;
        // x major: (0,0) sw, (0,1) nw, (1,0) se, (1,1) ne.
        g.put_block(0, 0, &[sw, nw, se, ne], &[0; 4], &[0; 4], &[0; 4]);
        g
    }

    #[test]
    fn the_ground_follows_ac2s_own_cut_not_a_blend() {
        // A ridge: only the north-east corner raised.
        let sw_ne = square([0.0, 0.0, 10.0, 0.0], cut_all);
        let se_nw = square([0.0, 0.0, 10.0, 0.0], cut_none);
        // The center lies on the cut south-west to north-east: half the ridge.
        assert_eq!(sw_ne.ground(5.0, 5.0), Some(5.0));
        // Cut the other way, the center lies on the flat south-east to north-west edge.
        assert_eq!(se_nw.ground(5.0, 5.0), Some(0.0));
        // A bilinear blend would say 2.5 for both.
        // Corners are exact on either cut.
        for g in [&sw_ne, &se_nw] {
            assert_eq!(g.ground(10.0, 10.0), Some(10.0));
            assert_eq!(g.ground(0.0, 10.0), Some(0.0));
            assert_eq!(g.ground(10.0, 0.0), Some(0.0));
        }
        // Inside the north-east triangle of the other cut: a plane through ne, nw, se.
        assert!((se_nw.ground(7.5, 7.5).unwrap() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn every_point_of_a_square_lies_on_one_of_its_two_triangles() {
        let hs = [3.0, -1.0, 7.0, 2.0];
        for cut in [true, false] {
            for a in 0..=10 {
                for b in 0..=10 {
                    let (fu, fv) = (f64::from(a) / 10.0, f64::from(b) / 10.0);
                    let h = on_square(cut, hs, fu, fv);
                    let lo = hs.iter().copied().fold(f64::MAX, f64::min);
                    let hi = hs.iter().copied().fold(f64::MIN, f64::max);
                    assert!((lo..=hi).contains(&h));
                }
            }
            // The edges are the straight lines between their corners.
            assert!((on_square(cut, hs, 0.5, 0.0) - 1.0).abs() < 1e-12);
            assert!((on_square(cut, hs, 0.0, 0.5) - 2.5).abs() < 1e-12);
        }
    }

    #[test]
    fn where_blocks_with_different_water_meet_the_lower_plane_wins() {
        let mut g = Ac2Ground::new(2, 1, 10.0, cut_all);
        for bx in 0..2 {
            for by in 0..2 {
                g.put_block(bx, by, &[0.0; 4], &[0; 4], &[0; 4], &[0; 4]);
            }
        }
        g.plane = vec![Some(5.0), Some(5.0), Some(3.0), Some(3.0)];
        // Inside each block, its own plane.
        assert_eq!(g.plane(5.0, 5.0), (Some(5.0), false));
        assert_eq!(g.plane(15.0, 5.0), (Some(3.0), false));
        // On the shared edge (x = 10 m) both blocks write that vertex: the lower plane, from
        // either side.
        assert_eq!(g.plane(10.0, 5.0).0, Some(3.0));
        // A corner of four blocks with one dry block: no water.
        g.plane[1] = None;
        assert_eq!(g.plane(10.0, 10.0).0, None);
        // The world's outer edge touches only its own block.
        assert_eq!(g.plane(20.0, 5.0), (Some(3.0), false));
        assert_eq!(g.plane(0.0, 5.0), (Some(5.0), false));
    }

    #[test]
    fn a_missing_block_and_the_world_beyond_have_no_ground() {
        let mut g = Ac2Ground::new(2, 1, 10.0, cut_all);
        g.put_block(0, 0, &[1.0; 4], &[0; 4], &[0; 4], &[0; 4]);
        assert_eq!(g.ground(5.0, 5.0), Some(1.0));
        assert_eq!(g.ground(15.0, 15.0), None);
        assert_eq!(g.ground(25.0, 5.0), None);
        assert_eq!(g.ground(-1.0, 5.0), None);
    }

    #[test]
    fn a_vertex_takes_its_kind_from_the_nearest_land_corner() {
        let mut g = Ac2Ground::new(1, 1, 10.0, cut_all);
        // sw under water (height 0 against a plane at 1), the rest land.
        g.put_block(
            0,
            0,
            &[0.0, 5.0, 5.0, 5.0],
            &[7, 8, 9, 10],
            &[0; 4],
            &[0; 4],
        );
        g.plane = vec![Some(1.0)];
        let side = g.side();
        // Nearest corner is the south-west, which is water: the next nearest land corner, the
        // north-west, wins.
        let v = g.kind_vertex(1.0, 2.0).unwrap();
        assert_eq!(v, 1); // (0, 1), x major
        assert_eq!(g.kind[v], 8);
        // Nearer the east side, the south-east.
        let v = g.kind_vertex(6.0, 1.0).unwrap();
        assert_eq!(v, side);
        assert_eq!(g.kind[v], 9);
        // All water: the nearest corner anyway.
        g.plane = vec![Some(9.0)];
        assert_eq!(g.kind_vertex(1.0, 2.0), Some(0));
    }
}

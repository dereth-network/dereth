//! The drawn ground: the landscape as a presentation with finer art draws it, retail's surface
//! plus a visual offset. Collision, line of sight, the server and everything the game reads stay
//! on retail's triangles; only what is drawn moves, and whatever stands on the ground is drawn
//! moved with it.
//!
//! **The offset** is worked out a block at a time, a sample a meter, from retail's own records:
//! - **Walkable ground** takes the smooth surface through every retail vertex (a bicubic through
//!   the 24-meter vertex grid), its distance from retail's flat triangles reached softly up to a
//!   cap. It is nought at every retail vertex, where the two surfaces meet.
//! - **Unwalkable ground** (steeper than bodies stand on) takes rock bands: retail's own surface
//!   cut into treads and risers, nought within an edge of walkable ground (where a body stands at
//!   the brink) and with room growing past it up to a cap, a larger one on tall faces.
//! - **Pins**: buildings, static objects, roads and water keep retail's height, the offset rising
//!   from them over a fade.
//! - **Bodies** stand on the offset averaged over their stance (the low-pass, [`body_offset`]); the
//!   fine relief the presentation adds over it is too small for them.
//!
//! Far from the viewer the offset is the smooth surface uncapped, with an eroded relief over it
//! ([`far_offset`]), and the two meet in a band ([`morph`]).
//!
//! Everything here is a pure function of the records and the [`Knobs`]: the presentation's own
//! copy of the offset and the shift of what stands on the ground come from the same numbers.

pub mod knobs;
pub mod shore;

pub use knobs::{Knob, KnobSpec, Knobs, KNOBS, KNOB_COUNT};

use crate::consts::CELL_SIZE;
use crate::land::mesh::sw_to_ne_cut;
use crate::road::on_road_cell;
use dereth_primitives::num::{floor_to_i32, math};

/// The steepest plane a body stands on: a triangle whose normal leans further from straight up
/// than this (48.38 degrees) is unwalkable, and bodies slide on it.
pub const WALKABLE_NORMAL_Z: f32 = 0.664_174_1;
/// Meters between the field's samples.
pub const SAMPLE: f32 = 1.0;
/// Samples along a block's side, from its south-west corner; the block's north and east edges are
/// the next blocks' first samples.
pub const TILE: usize = 192;
/// Samples along a side of a block's field as kept, both edges included.
pub const SIDE: usize = TILE + 1;
/// A block's side, meters.
pub const BLOCK_METERS: f32 = 192.0;
/// The world's landscape vertices along a side.
pub const WORLD_VERTICES: i32 = 2041;

/// One landscape vertex: its height and its terrain word.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Meters.
    pub height: f32,
    /// The terrain word: road bits, terrain type, scenery.
    pub word: u16,
}

/// Where the landscape's vertices come from: global vertex coordinates, `0..2041` a side, eight
/// to a block (a block's last vertex is its neighbor's first).
pub trait GroundSource {
    /// The vertex at `(gx, gy)`, or `None` outside the world or where the records hold none.
    fn vertex(&self, gx: i32, gy: i32) -> Option<Vertex>;
}

/// Ground kept at retail's height: a footprint, a rectangle turned about its middle, and the
/// margin round it, global meters. A round footprint is a rectangle of no size with its margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pin {
    /// The middle, east, meters.
    pub x: f32,
    /// The middle, north, meters.
    pub y: f32,
    /// Half the rectangle's size along its own two axes, meters.
    pub half: (f32, f32),
    /// The rectangle's first axis, a unit vector east and north.
    pub axis: (f32, f32),
    /// The margin round it, meters.
    pub margin: f32,
}

impl Pin {
    /// A round of `radius` meters about `(x, y)`.
    #[must_use]
    pub const fn round(x: f32, y: f32, radius: f32) -> Self {
        Self {
            x,
            y,
            half: (0.0, 0.0),
            axis: (1.0, 0.0),
            margin: radius,
        }
    }

    /// How far `(x, y)` lies outside the pinned ground, meters; nought or less inside.
    #[must_use]
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = (x - self.x, y - self.y);
        let (u, v) = (
            dx * self.axis.0 + dy * self.axis.1,
            -dx * self.axis.1 + dy * self.axis.0,
        );
        let (ou, ov) = (
            (u.abs() - self.half.0).max(0.0),
            (v.abs() - self.half.1).max(0.0),
        );
        (ou * ou + ov * ov).sqrt() - self.margin
    }

    /// The pinned ground's reach from its middle, meters.
    #[must_use]
    pub fn reach(&self) -> f32 {
        (self.half.0 * self.half.0 + self.half.1 * self.half.1).sqrt() + self.margin
    }
}

/// Whether terrain word `word` names water.
#[must_use]
pub const fn is_water(word: u16) -> bool {
    matches!((word >> 2) & 0x1F, 16..=20)
}

/// Whether terrain word `word` has a road through its vertex.
#[must_use]
pub const fn is_road(word: u16) -> bool {
    word & 3 != 0
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    if hi <= lo {
        return if x >= hi { 1.0 } else { 0.0 };
    }
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `x` reached softly up to `cap` either way: nearly itself while small, never past the cap.
#[must_use]
pub fn soft_cap(x: f32, cap: f32) -> f32 {
    if cap <= 0.0 {
        return 0.0;
    }
    let e = math::expf(-2.0 * (x / cap).abs());
    cap * (1.0 - e) / (1.0 + e) * x.signum()
}

/// The vertices round a point, read once: the 6 by 6 about the cell it lies in.
struct Stencil {
    h: [[f32; 6]; 6],
    w: [[u16; 6]; 6],
    /// The cell, global.
    cell: (i32, i32),
}

impl Stencil {
    fn at(src: &dyn GroundSource, cell: (i32, i32)) -> Self {
        let mut h = [[0.0; 6]; 6];
        let mut w = [[0u16; 6]; 6];
        for (i, row) in h.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                // LINT-OK: a stencil index, 0..6.
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                let (gx, gy) = (
                    (cell.0 + i as i32 - 2).clamp(0, WORLD_VERTICES - 1),
                    (cell.1 + j as i32 - 2).clamp(0, WORLD_VERTICES - 1),
                );
                let vx = src.vertex(gx, gy).unwrap_or(Vertex {
                    height: 0.0,
                    word: 20 << 2,
                });
                *v = vx.height;
                w[i][j] = vx.word;
            }
        }
        Self { h, w, cell }
    }
}

/// What the ground is at one point: retail's height and slope, the smooth surface, and what keeps
/// it at retail's height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Retail's height.
    pub retail: f32,
    /// The smooth surface through retail's vertices.
    pub smooth: f32,
    /// Retail's triangle's rise east and north.
    pub gradient: (f32, f32),
    /// Whether retail's triangle is walkable.
    pub walkable: bool,
    /// Whether a road runs there, by the scenery's road test.
    pub road: bool,
    /// How much water the point holds, 0..1, from its cell's corners.
    pub water: f32,
    /// The height range of the 6 by 6 vertices about the cell: how tall the face it is on is.
    pub relief: f32,
}

/// Catmull-Rom weights at `t`.
fn catmull(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

impl Point {
    fn from_stencil(s: &Stencil, x: f32, y: f32) -> Self {
        // LINT-OK: global cell coordinates, far inside f32's exact range.
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (
            (x / CELL_SIZE - s.cell.0 as f32).clamp(0.0, 1.0),
            (y / CELL_SIZE - s.cell.1 as f32).clamp(0.0, 1.0),
        );
        let (h00, h10, h01, h11) = (s.h[2][2], s.h[3][2], s.h[2][3], s.h[3][3]);
        let (retail, gx, gy) = if sw_to_ne_cut(s.cell.0, s.cell.1) {
            if fx >= fy {
                (
                    h00 + fx * (h10 - h00) + fy * (h11 - h10),
                    h10 - h00,
                    h11 - h10,
                )
            } else {
                (
                    h00 + fy * (h01 - h00) + fx * (h11 - h01),
                    h11 - h01,
                    h01 - h00,
                )
            }
        } else if fx + fy <= 1.0 {
            (
                h00 + fx * (h10 - h00) + fy * (h01 - h00),
                h10 - h00,
                h01 - h00,
            )
        } else {
            (
                h11 + (1.0 - fx) * (h01 - h11) + (1.0 - fy) * (h10 - h11),
                h11 - h01,
                h11 - h10,
            )
        };
        let gradient = (gx / CELL_SIZE, gy / CELL_SIZE);
        let limit = 1.0 / (WALKABLE_NORMAL_Z * WALKABLE_NORMAL_Z) - 1.0;
        let walkable = gradient.0 * gradient.0 + gradient.1 * gradient.1 <= limit;
        let (wx, wy) = (catmull(fx), catmull(fy));
        let mut smooth = 0.0;
        for (i, a) in wx.iter().enumerate() {
            for (j, b) in wy.iter().enumerate() {
                smooth += a * b * s.h[i + 1][j + 1];
            }
        }
        let corner = |i: usize, j: usize| s.w[i][j];
        let road = on_road_cell(
            is_road(corner(2, 2)),
            is_road(corner(2, 3)),
            is_road(corner(3, 2)),
            is_road(corner(3, 3)),
            fx * CELL_SIZE,
            fy * CELL_SIZE,
        );
        let wet = |i: usize, j: usize| if is_water(corner(i, j)) { 1.0 } else { 0.0 };
        let water = (wet(2, 2) * (1.0 - fx) + wet(3, 2) * fx) * (1.0 - fy)
            + (wet(2, 3) * (1.0 - fx) + wet(3, 3) * fx) * fy;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for row in &s.h {
            for h in row {
                lo = lo.min(*h);
                hi = hi.max(*h);
            }
        }
        Self {
            retail,
            smooth,
            gradient,
            walkable,
            road,
            water,
            relief: hi - lo,
        }
    }

    /// The ground at global `(x, y)`, meters.
    #[must_use]
    pub fn at(src: &dyn GroundSource, x: f32, y: f32) -> Self {
        let cell = (
            floor_to_i32(x / CELL_SIZE).clamp(0, WORLD_VERTICES - 2),
            floor_to_i32(y / CELL_SIZE).clamp(0, WORLD_VERTICES - 2),
        );
        Self::from_stencil(&Stencil::at(src, cell), x, y)
    }
}

/// Retail's height at global `(x, y)`: the plane of the triangle the point lies on, as physics
/// and the server have it.
#[must_use]
pub fn retail_height(src: &dyn GroundSource, x: f32, y: f32) -> f32 {
    Point::at(src, x, y).retail
}

/// A hash of a lattice point, 0..1, the same arithmetic in the presentation's shaders.
#[must_use]
pub fn lattice_hash(x: i32, y: i32) -> f32 {
    // LINT-OK: the hash takes the coordinates' bits as they are.
    #[allow(clippy::cast_sign_loss)]
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841);
    h = (h ^ (h >> 13)).wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 15;
    // LINT-OK: sixteen bits, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let v = (h & 0xFFFF) as f32 / 65535.0;
    v
}

/// Smooth noise over a lattice `cell` meters wide, 0..1.
#[must_use]
pub fn value_noise(x: f32, y: f32, cell: f32) -> f32 {
    let (qx, qy) = (x / cell, y / cell);
    let (ix, iy) = (floor_to_i32(qx), floor_to_i32(qy));
    // LINT-OK: lattice coordinates of points inside the world, far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    let (fx, fy) = (qx - ix as f32, qy - iy as f32);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = lattice_hash(ix, iy);
    let b = lattice_hash(ix + 1, iy);
    let c = lattice_hash(ix, iy + 1);
    let d = lattice_hash(ix + 1, iy + 1);
    (a + (b - a) * sx) + ((c + (d - c) * sx) - (a + (b - a) * sx)) * sy
}

/// The eroded relief far off at global `(x, y)` on ground rising `slope` (rise over run): ridged
/// noise in three octaves of `scale` meters and up, `amplitude` meters at most, more on slopes
/// than on the flat, centered on nought.
#[must_use]
pub fn far_relief(x: f32, y: f32, scale: f32, amplitude: f32, slope: f32) -> f32 {
    if amplitude <= 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut weight = 1.0;
    let mut cell = scale.max(1.0);
    for octave in 0..3 {
        // LINT-OK: an octave index.
        #[allow(clippy::cast_precision_loss)]
        let offset = 37.0 * octave as f32;
        let n = value_noise(x + offset, y - offset, cell);
        let ridge = 1.0 - (2.0 * n - 1.0).abs();
        sum += weight * (ridge * ridge - 0.45);
        weight *= 0.5;
        cell *= 0.5;
    }
    amplitude * sum / 1.75 * (0.3 + slope.min(1.2))
}

/// The offset the drawn ground takes far off at a point: the smooth surface uncapped, as much of
/// it as the knobs keep, and the eroded relief.
#[must_use]
pub fn far_offset(p: &Point, x: f32, y: f32, knobs: &Knobs) -> f32 {
    let slope = (p.gradient.0 * p.gradient.0 + p.gradient.1 * p.gradient.1).sqrt();
    knobs.get(Knob::FarRound) * (p.smooth - p.retail)
        + far_relief(
            x,
            y,
            knobs.get(Knob::FarDetailScale),
            knobs.get(Knob::FarDetail),
            slope,
        )
}

/// The water the drawn ground lays, while it is drawn as water.
#[derive(Debug, Clone, Copy)]
pub struct Water<'a> {
    /// Where it lies and its level.
    pub shore: shore::Shore<'a>,
    /// The deepest the bed lies below the water's surface, meters.
    pub depth: f32,
    /// How far from the shore the bed reaches that depth, meters.
    pub shelf: f32,
}

/// The drawn ground at one point: how far it is drawn off retail's surface by the offset, and
/// the height it is drawn at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawn {
    /// The offset, near and far ones mixed, before the water's bed and the floor beside it.
    pub offset: f32,
    /// The height the ground is drawn at: retail's and the offset, the bed lowered under the
    /// water, and beside it no lower than its floor.
    pub height: f32,
}

/// The drawn ground at global `(x, y)`, whose ground is `p`, with the offset `near` there (the
/// near offset, or a body's averaged over its stance), `far` of the way given way to the far
/// ground, over `water` while it is drawn as water: retail's height and the offset (far off the
/// rounding and the eroded relief, kept out of the water and its shore), the bed lowered under the
/// water, and beside it land no lower than its floor ([`shore::floor`]). The presentation's own
/// arithmetic, less the fine relief, which is leveled under bodies.
#[must_use]
pub fn drawn_at(
    p: &Point,
    x: f32,
    y: f32,
    near: f32,
    far: f32,
    water: Option<&Water<'_>>,
    knobs: &Knobs,
) -> Drawn {
    let inside = water.map_or(-1000.0, |w| w.shore.distance(x, y, 0.0));
    let mut offset = near;
    if far > 0.0 {
        let dry = 1.0 - smoothstep(-shore::CELL, 0.0, inside);
        let away = far_offset(p, x, y, knobs) * dry;
        offset = offset * (1.0 - far) + away * far;
    }
    let mut height = p.retail + offset;
    if let Some(w) = water {
        height -= shore::bed_depth(inside, w.depth, w.shelf);
        if inside <= 0.0 && inside > -shore::NEAR {
            height = height.max(shore::floor(w.shore.level(x, y), inside));
        }
    }
    Drawn { offset, height }
}

/// How far a thing that physics puts at height `z` over ground `ground` (its offset averaged over
/// its stance, see [`drawn_at`]) is drawn above where it stands: by the offset, as much of it as
/// `keep` leaves (things in the air keep less), and never lower than the drawn ground under it,
/// so a body that retail sinks into its water stands on the bed where the bed is shallower than
/// retail sinks it, and one beside the water stands on the floor the bank keeps.
#[must_use]
pub fn shift(z: f32, ground: Drawn, keep: f32) -> f32 {
    let moved = keep * ground.offset;
    moved + keep * (ground.height - (z + moved)).max(0.0)
}

/// How far a point `distance` meters from the eye has given way to the far ground: 0 near, 1 far.
#[must_use]
pub fn morph(distance: f32, knobs: &Knobs) -> f32 {
    if !knobs.on(Knob::Far) {
        return 0.0;
    }
    smoothstep(
        knobs.get(Knob::MorphStart),
        knobs.get(Knob::MorphEnd).max(knobs.get(Knob::MorphStart)),
        distance,
    )
}

/// The offset a body standing at `(x, y)` is drawn at: the offset `at` gives, averaged over a
/// round as wide as the low-pass (a body's stance and a little more), so that the fine shape of
/// the drawn ground does not make a walking body bob. Nine points: the middle, and four on each
/// of two rings.
pub fn body_offset(at: impl Fn(f32, f32) -> f32, x: f32, y: f32, knobs: &Knobs) -> f32 {
    let r = knobs.get(Knob::WalkLowpass) * 0.5;
    if r < 0.05 {
        return at(x, y);
    }
    let mut sum = at(x, y) * 2.0;
    let mut weight = 2.0;
    for (ring, share, turn) in [(0.5, 1.0, 0.0f32), (1.0, 0.5, std::f32::consts::FRAC_PI_4)] {
        for k in 0..4 {
            // LINT-OK: four quarter turns.
            #[allow(clippy::cast_precision_loss)]
            let a = turn + k as f32 * std::f32::consts::FRAC_PI_2;
            sum += at(x + r * ring * math::cosf(a), y + r * ring * math::sinf(a)) * share;
            weight += share;
        }
    }
    sum / weight
}

/// A rock band's offset at a point of retail height `h`: the surface cut into treads of height
/// `band`, each rising over `riser` of the band at its end, centered on retail's surface;
/// `wander` meters added to the height first so the bands' edges wander.
#[must_use]
pub fn band_offset(h: f32, band: f32, riser: f32, wander: f32) -> f32 {
    if band <= 0.0 {
        return 0.0;
    }
    let q = (h + wander) / band;
    // LINT-OK: a band count of a height, far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    let f = q - floor_to_i32(q) as f32;
    let r = riser.clamp(0.01, 0.99);
    (smoothstep(1.0 - r, 1.0, f) - f + (1.0 - r) * 0.5) * band
}

/// The offset of the near ground at a point before the pins: the walkable rounding, and the rock
/// bands of unwalkable ground `to_walkable` meters from walkable ground.
fn near_raw(p: &Point, x: f32, y: f32, to_walkable: f32, knobs: &Knobs) -> f32 {
    let walk_cap = knobs.get(Knob::WalkCap);
    let round = soft_cap(knobs.get(Knob::Round) * (p.smooth - p.retail), walk_cap);
    if p.walkable || !knobs.on(Knob::Cliffs) {
        return round;
    }
    let edge = knobs.get(Knob::CliffEdge);
    let tall = smoothstep(
        knobs.get(Knob::CliffTallRelief) * 0.8,
        knobs.get(Knob::CliffTallRelief) * 1.2,
        p.relief,
    );
    let (small, big) = (knobs.get(Knob::CliffCap), knobs.get(Knob::CliffCapTall));
    let cap = small + (big - small) * tall;
    let room = (knobs.get(Knob::CliffGrowth) * (to_walkable - edge)).clamp(0.0, cap);
    let band = knobs.get(Knob::CliffBand);
    let riser = knobs.get(Knob::CliffRiser);
    let wander = (value_noise(x, y, knobs.get(Knob::CliffNoiseScale)) - 0.5)
        * 2.0
        * knobs.get(Knob::CliffNoise);
    let bands = |c: f32| band_offset(p.retail, band * c, riser, wander * band * c);
    let banded = bands(small) + (bands(big) - bands(small)) * tall;
    round * (1.0 - smoothstep(0.0, edge.max(1e-3), to_walkable)) + soft_cap(banded, room)
}

/// The near offset of one block, a sample a meter: [`SIDE`] by [`SIDE`], row by row from the
/// south (`north * SIDE + east`), what bodies stand on and how much of the offset the pins leave.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockField {
    /// The block's coordinates.
    pub block: (i32, i32),
    /// The offset bodies stand on, meters.
    pub offset: Vec<f32>,
    /// How much of the offset the pins leave, 0 at a pin to 1 clear of them.
    pub weight: Vec<f32>,
}

impl BlockField {
    /// The offset and the pins' weight at block-local `(x, y)` meters, between the samples.
    #[must_use]
    pub fn at(&self, x: f32, y: f32) -> (f32, f32) {
        (sample(&self.offset, x, y), sample(&self.weight, x, y))
    }
}

/// A block's samples `values` ([`SIDE`] by [`SIDE`], row by row from the south) at block-local
/// `(x, y)` meters, between the samples; clamped to the block.
#[must_use]
pub fn sample(values: &[f32], x: f32, y: f32) -> f32 {
    // LINT-OK: the tile's side, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let side = TILE as f32;
    let fx = (x / SAMPLE).clamp(0.0, side);
    let fy = (y / SAMPLE).clamp(0.0, side);
    // LINT-OK: clamped to the tile, 0..=192.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (i, j) = ((fx as usize).min(TILE - 1), (fy as usize).min(TILE - 1));
    // LINT-OK: sample indices, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let (tx, ty) = (fx - i as f32, fy - j as f32);
    let v = |a: usize, b: usize| values.get(b * SIDE + a).copied().unwrap_or(0.0);
    let south = v(i, j) + (v(i + 1, j) - v(i, j)) * tx;
    let north = v(i, j + 1) + (v(i + 1, j + 1) - v(i, j + 1)) * tx;
    south + (north - south) * ty
}

/// The squared distance, in samples, from each sample of an `n` by `n` grid to the nearest one
/// `set` holds; infinity where it holds none. Two passes of the exact one-dimensional transform.
fn distance_squared(set: &[bool], n: usize) -> Vec<f32> {
    const FAR: f64 = 1e18;
    let mut grid: Vec<f64> = set.iter().map(|s| if *s { 0.0 } else { FAR }).collect();
    let mut line = vec![0.0f64; n];
    let mut out = vec![0.0f64; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut transform = |f: &[f64], out: &mut [f64]| {
        let mut k = 0usize;
        v[0] = 0;
        z[0] = f64::MIN;
        z[1] = f64::MAX;
        for q in 1..n {
            loop {
                let p = v[k];
                // LINT-OK: grid indices, exact in f64.
                #[allow(clippy::cast_precision_loss)]
                let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64))
                    / (2.0 * (q as f64 - p as f64));
                if s <= z[k] && k > 0 {
                    k -= 1;
                } else {
                    k += 1;
                    v[k] = q;
                    z[k] = s;
                    z[k + 1] = f64::MAX;
                    break;
                }
            }
        }
        k = 0;
        for (q, o) in out.iter_mut().enumerate().take(n) {
            // LINT-OK: grid indices, exact in f64.
            #[allow(clippy::cast_precision_loss)]
            let qf = q as f64;
            while z[k + 1] < qf {
                k += 1;
            }
            let p = v[k];
            // LINT-OK: grid indices, exact in f64.
            #[allow(clippy::cast_precision_loss)]
            let d = qf - p as f64;
            *o = d * d + f[p];
        }
    };
    for row in 0..n {
        line.copy_from_slice(&grid[row * n..row * n + n]);
        transform(&line, &mut out);
        grid[row * n..row * n + n].copy_from_slice(&out);
    }
    for col in 0..n {
        for row in 0..n {
            line[row] = grid[row * n + col];
        }
        transform(&line, &mut out);
        for row in 0..n {
            grid[row * n + col] = out[row];
        }
    }
    // LINT-OK: squared distances of a few hundred samples, or the far mark, as f32.
    #[allow(clippy::cast_possible_truncation)]
    grid.into_iter().map(|d| d as f32).collect()
}

/// The samples beyond a block's edge its field is worked out over, so that what lies just past
/// the edge (a pin, walkable ground, the low-pass) counts as it does for the next block.
fn margin(knobs: &Knobs) -> usize {
    let pins = knobs.get(Knob::PinFade);
    let cliffs = knobs.get(Knob::CliffEdge)
        + knobs.get(Knob::CliffCapTall).max(knobs.get(Knob::CliffCap))
            / knobs.get(Knob::CliffGrowth).max(0.05);
    // LINT-OK: a few dozen meters, bounded below.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let m = (pins.max(cliffs.min(96.0)) / SAMPLE).ceil() as usize + 2;
    m.min(100)
}

/// The near offset of block `(bx, by)` from the landscape `src`, the `pins` that stand on it or
/// within a fade of it (global meters, margins included) and the `knobs`.
#[must_use]
pub fn block_field(
    src: &dyn GroundSource,
    block: (i32, i32),
    pins: &[Pin],
    knobs: &Knobs,
) -> BlockField {
    let m = margin(knobs);
    let n = SIDE + 2 * m;
    // LINT-OK: block coordinates and sample offsets, far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (
        block.0 as f32 * BLOCK_METERS - m as f32 * SAMPLE,
        block.1 as f32 * BLOCK_METERS - m as f32 * SAMPLE,
    );
    // LINT-OK: sample indices, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let pos = |k: usize| k as f32 * SAMPLE;
    let mut points = Vec::with_capacity(n * n);
    let mut stencil: Option<Stencil> = None;
    for j in 0..n {
        let y = oy + pos(j);
        for i in 0..n {
            let x = ox + pos(i);
            let cell = (
                floor_to_i32(x / CELL_SIZE).clamp(0, WORLD_VERTICES - 2),
                floor_to_i32(y / CELL_SIZE).clamp(0, WORLD_VERTICES - 2),
            );
            if stencil.as_ref().is_none_or(|s| s.cell != cell) {
                stencil = Some(Stencil::at(src, cell));
            }
            if let Some(s) = stencil.as_ref() {
                points.push(Point::from_stencil(s, x, y));
            }
        }
    }
    // What keeps the ground at retail's height.
    let level = knobs.get(Knob::PinWaterLevel);
    let mut pinned: Vec<bool> = points
        .iter()
        .map(|p| {
            (knobs.on(Knob::PinRoads) && p.road)
                || (knobs.on(Knob::PinWater) && p.water > 0.0 && p.water >= level)
        })
        .collect();
    for pin in pins {
        let (cx, cy) = ((pin.x - ox) / SAMPLE, (pin.y - oy) / SAMPLE);
        let r = pin.reach() / SAMPLE;
        let lo = |c: f32| floor_to_i32(c - r).max(0);
        let hi = |c: f32| floor_to_i32(c + r + 1.0);
        // LINT-OK: the grid side, a few hundred.
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let last = n as i32 - 1;
        for j in lo(cy)..=hi(cy).min(last) {
            for i in lo(cx)..=hi(cx).min(last) {
                // LINT-OK: grid indices, exact in f32.
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (ox + i as f32 * SAMPLE, oy + j as f32 * SAMPLE);
                if pin.distance(x, y) <= 0.0 {
                    // LINT-OK: inside the grid, checked above.
                    #[allow(clippy::cast_sign_loss)]
                    {
                        pinned[j as usize * n + i as usize] = true;
                    }
                }
            }
        }
    }
    let to_pin = distance_squared(&pinned, n);
    let walkable: Vec<bool> = points.iter().map(|p| p.walkable).collect();
    let to_walkable = distance_squared(&walkable, n);
    let fade = knobs.get(Knob::PinFade);
    let mut raw = Vec::with_capacity(n * n);
    for (k, p) in points.iter().enumerate() {
        let (i, j) = (k % n, k / n);
        raw.push(near_raw(
            p,
            ox + pos(i),
            oy + pos(j),
            to_walkable[k].sqrt() * SAMPLE,
            knobs,
        ));
    }
    let mut offset = Vec::with_capacity(SIDE * SIDE);
    let mut weight = Vec::with_capacity(SIDE * SIDE);
    for j in m..m + SIDE {
        for i in m..m + SIDE {
            let k = j * n + i;
            let w = if pinned[k] {
                0.0
            } else {
                smoothstep(0.0, fade, to_pin[k].sqrt() * SAMPLE)
            };
            offset.push(raw[k] * w);
            weight.push(w);
        }
    }
    BlockField {
        block,
        offset,
        weight,
    }
}

#[cfg(test)]
mod tests;

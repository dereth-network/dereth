//! The shores: strips along the line where the detailed blocks' water meets their ground, laid
//! from the waterline out over the water.
//!
//! The line is found once each time the detailed landscape is laid out about a new block: every
//! triangle of the ground that crosses its block's water surface is cut by it, and the two points
//! of each cut are joined. Joined points run in chains, open where a shore leaves the detailed
//! blocks and closed round a pond or an island; each chain becomes a strip, a pair of vertices a
//! point: one at the waterline (pulled 5 cm up the bank), one out over the water, as far as twelve
//! times the ground's upward slant there. Along the strip runs how far each point lies from its
//! start; across it one random offset per strip.
//!
//! The strips are drawn twice: first into the frame's alpha alone, before the water (how much of
//! the water shows near the shore), and then, after the water and its reflection, as foam (every
//! strip but closed ones shorter than 20 m).

use std::sync::Arc;

use auberean_physics::geom::PlaneOps;
use auberean_primitives::random::Random;
use dereth_primitives::shape::Plane;

use crate::mesh::MeshVertex;

/// One detailed block as its shore is found from it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShoreBlock {
    /// The height of the block's water surface, when the block has water.
    pub water: Option<f32>,
    /// Whether the viewer stands in this block.
    pub own: bool,
    /// The block's ground as triangles in the frame's frame: square by square, x major, each
    /// square's two by its split, each triangle's corners in the order the ground is drawn.
    pub triangles: Vec<[[f32; 3]; 3]>,
}

/// A point of the shore.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShorePoint {
    /// Where the ground's edge crosses the water; once the strips are measured, pulled 5 cm back
    /// up the bank.
    pub at: [f32; 3],
    /// The sum of the normals of the cut triangles it lies on; once measured, its direction across
    /// the ground, level.
    pub normal: [f32; 3],
    /// The strip's far edge: the point out over the water.
    pub out: [f32; 3],
    /// The strip's offset across the foam's picture.
    pub across: f32,
    /// How far along its strip the point lies, in meters.
    pub along: f32,
    /// The two points it is joined to, when it has them.
    pub links: [Option<usize>; 2],
    /// The strip it belongs to.
    pub strip: Option<usize>,
}

impl ShorePoint {
    fn at(p: [f32; 3]) -> Self {
        Self {
            at: p,
            normal: [0.0; 3],
            out: [0.0; 3],
            across: 0.0,
            along: 0.0,
            links: [None; 2],
            strip: None,
        }
    }

    /// The point it goes on to from `from` (none for its first): its first link, unless that is
    /// none or `from`; else its second, unless that is none or `from`.
    fn next(&self, from: Option<usize>) -> Option<usize> {
        match self.links[0] {
            Some(a) if Some(a) != from => Some(a),
            _ => self.links[1].filter(|&b| Some(b) != from),
        }
    }
}

/// One chain of joined points.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShoreStrip {
    /// Its points in order (a closed one ends with a copy of its first).
    pub points: Vec<usize>,
    /// Its length, in meters, from its first point to its last.
    pub length: f32,
    /// Whether it closes on itself (a pond's or an island's shore).
    pub closed: bool,
}

impl ShoreStrip {
    /// Whether it carries foam: an open strip, or a closed one at least 20 m round.
    #[must_use]
    pub fn foamed(&self) -> bool {
        !self.closed || self.length >= FOAM_LEAST_LENGTH
    }
}

/// A closed shore shorter than this carries no foam, in meters.
pub const FOAM_LEAST_LENGTH: f32 = 20.0;

/// How far the strip reaches over the water at a level shore, in meters: its reach is this times
/// the upward part of the ground's normal at the waterline.
pub const SHORE_REACH: f32 = 12.0;

/// How far each waterline point is pulled back up the bank, in meters.
pub const SHORE_PULL: f32 = 0.05;

/// The detailed landscape's shores.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shore {
    pub points: Vec<ShorePoint>,
    pub strips: Vec<ShoreStrip>,
    /// Two vertices a point: at the waterline, then out over the water. Their first coordinate set
    /// runs along the strip in meters and across it from the strip's offset (one more at the far
    /// edge); their second along the strip and from 0 at the waterline to 1 at the far edge.
    pub vertices: Arc<Vec<MeshVertex>>,
}

impl Shore {
    /// The strips as triangles, each strip's vertex pairs in its points' order, the triangles
    /// as a strip of them makes (every other one turned to keep their facing): every strip, or
    /// with `foam` only those that carry it.
    #[must_use]
    pub fn triangles(&self, foam: bool) -> Vec<u16> {
        let mut out = Vec::new();
        for s in self.strips.iter().filter(|s| !foam || s.foamed()) {
            let strip: Vec<u16> = s
                .points
                .iter()
                .flat_map(|&p| {
                    let p = u16::try_from(p).unwrap_or(u16::MAX / 2);
                    [2 * p, 2 * p + 1]
                })
                .collect();
            for k in 0..strip.len().saturating_sub(2) {
                if k % 2 == 0 {
                    out.extend([strip[k], strip[k + 1], strip[k + 2]]);
                } else {
                    out.extend([strip[k + 1], strip[k], strip[k + 2]]);
                }
            }
        }
        out
    }
}

/// `a - b`, worked wide.
fn sub(a: [f32; 3], b: [f32; 3]) -> [f64; 3] {
    [0, 1, 2].map(|k| f64::from(a[k]) - f64::from(b[k]))
}

/// `v` scaled to unit length as the original's vector library scales it: none left none, one
/// within 0.00001 of unit length (its square) left as it is.
#[allow(clippy::cast_possible_truncation)]
fn unit(v: [f32; 3]) -> [f32; 3] {
    let sq = (f64::from(v[0]) * f64::from(v[0])
        + f64::from(v[1]) * f64::from(v[1])
        + f64::from(v[2]) * f64::from(v[2])) as f32;
    if sq == 0.0 {
        return [0.0; 3];
    }
    if (sq - 1.0).abs() <= 0.00001 {
        return v;
    }
    let l = f64::from(sq).sqrt();
    v.map(|c| (f64::from(c) / l) as f32)
}

/// The unit normal of triangle `p`: the cross of its second corner less its first with its third
/// less its first.
// D2-10954
#[allow(clippy::cast_possible_truncation)]
fn triangle_normal(p: &[[f32; 3]; 3]) -> [f32; 3] {
    let a = sub(p[1], p[0]);
    let b = sub(p[2], p[0]);
    unit([
        (a[1] * b[2] - b[1] * a[2]) as f32,
        (a[2] * b[0] - b[2] * a[0]) as f32,
        (b[1] * a[0] - b[0] * a[1]) as f32,
    ])
}

/// Where triangle `p` crosses `plane`: its edges from the first corner to the second, the second
/// to the third and the third to the first, each crossing found until two are. None when no edge
/// crosses; with one, the second point is `second` as it stood (the last cut's second point).
// D2-10947
fn cut(p: &[[f32; 3]; 3], plane: &Plane, second: &mut [f32; 3]) -> Option<[f32; 3]> {
    let v = |c: [f32; 3]| dereth_primitives::Vec3::new(c[0], c[1], c[2]);
    let mut first = None;
    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
        if let Some(x) = plane.intersect_line_segment(v(p[a]), v(p[b])) {
            let x = [x.x, x.y, x.z];
            if first.is_some() {
                *second = x;
                break;
            }
            first = Some(x);
        }
    }
    first
}

/// The point of `points` within 0.0002 of `at` along every axis, the first such.
// D2-10955
fn find_point(points: &[ShorePoint], at: [f32; 3]) -> Option<usize> {
    points
        .iter()
        .position(|p| (0..3).all(|k| (f64::from(p.at[k]) - f64::from(at[k])).abs() < 0.0002))
}

/// The detailed landscape's shores: found from `blocks` (in the landscape's order, x major from
/// the south-west), each strip's offset across the foam rolled from `dice`. None when no water
/// meets the ground; no vertices when fewer than two points were found.
///
/// A block whose water stands more than 0.1 m below the viewer's own block's water is passed over.
// D2-10987
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
pub fn generate(blocks: &[ShoreBlock], dice: &Random) -> Option<Shore> {
    if !blocks.iter().any(|b| b.water.is_some()) {
        return None;
    }
    let own = blocks.iter().find(|b| b.own).and_then(|b| b.water);
    let mut points: Vec<ShorePoint> = Vec::new();
    let mut second = [0.0f32; 3];
    for b in blocks {
        let Some(h) = b.water else { continue };
        if own.is_some_and(|o| f64::from(h) + f64::from(0.1_f32) < f64::from(o)) {
            continue;
        }
        let plane = Plane::new(dereth_primitives::Vec3::new(0.0, 0.0, 1.0), -h);
        for t in &b.triangles {
            let z = t.map(|c| c[2]);
            let reaches_up = z.iter().any(|&z| h <= z);
            let reaches_down = z.iter().any(|&z| z <= h);
            if !(reaches_up && reaches_down) {
                continue;
            }
            let Some(first) = cut(t, &plane, &mut second) else {
                continue;
            };
            let n = triangle_normal(t);
            let ends = [first, second].map(|at| {
                find_point(&points, at).unwrap_or_else(|| {
                    points.push(ShorePoint::at(at));
                    points.len() - 1
                })
            });
            for (k, &e) in ends.iter().enumerate() {
                let other = ends[1 - k];
                let p = &mut points[e];
                for (sum, add) in p.normal.iter_mut().zip(n) {
                    *sum += add;
                }
                if p.links[0].is_none() {
                    p.links[0] = Some(other);
                } else {
                    p.links[1] = Some(other);
                }
            }
        }
    }

    // The points in chains: from each point not yet in one, back to the chain's end (or round to
    // the point before it, for a closed chain), then forward from there.
    let mut strips: Vec<ShoreStrip> = Vec::new();
    let mut i = 0;
    while i < points.len() {
        if points[i].strip.is_some() {
            i += 1;
            continue;
        }
        // A walk longer than there are points goes round a loop that leaves out its start (the
        // links of a point joined more than twice); it stops there, where the original's walk
        // would run on for ever.
        let (mut from, mut at) = (None, Some(i));
        let mut closed = false;
        let mut steps = 0;
        while let Some(cur) = at {
            let next = points[cur].next(from);
            from = Some(cur);
            at = next;
            if next == Some(i) {
                closed = true;
                break;
            }
            steps += 1;
            if steps > points.len() {
                break;
            }
        }
        let start = from.unwrap_or(i);
        let index = strips.len();
        let mut strip = ShoreStrip {
            closed,
            ..ShoreStrip::default()
        };
        let (mut from, mut at) = (None, Some(start));
        while let Some(cur) = at {
            strip.points.push(cur);
            points[cur].strip = Some(index);
            let next = points[cur].next(from);
            from = Some(cur);
            at = next;
            if next == Some(start) || strip.points.len() > points.len() {
                break;
            }
        }
        // A closed chain ends with a copy of its first point, joined to it alone.
        if closed {
            let first = strip.points[0];
            let mut copy = points[first];
            copy.links = [Some(first), None];
            strip.points.push(points.len());
            points.push(copy);
        }
        strips.push(strip);
        i += 1;
    }

    // Each strip measured along, its points pulled up the bank and given their far edge.
    for s in &mut strips {
        s.length = 0.0;
        let across = dice.roll_float(0.0, 0.8);
        for k in 0..s.points.len() {
            if k > 0 {
                let d = sub(points[s.points[k]].at, points[s.points[k - 1]].at);
                let step = (d[0] * d[0] + d[2] * d[2] + d[1] * d[1]).sqrt();
                s.length = (step + f64::from(s.length)) as f32;
            }
            let p = &mut points[s.points[k]];
            let n = unit(p.normal);
            let reach = (f64::from(SHORE_REACH) * f64::from(n[2])) as f32;
            let n = unit([n[0], n[1], 0.0]);
            p.normal = n;
            let pull = n.map(|c| f64::from(c) * f64::from(SHORE_PULL));
            for c in 0..3 {
                p.at[c] = (f64::from(p.at[c]) - pull[c]) as f32;
                p.out[c] = (f64::from(reach) * f64::from(n[c]) + f64::from(p.at[c])) as f32;
            }
            p.along = s.length;
            p.across = across;
        }
    }

    let vertices = if points.len() < 2 {
        Vec::new()
    } else {
        points
            .iter()
            .flat_map(|p| {
                let v = |at: [f32; 3], edge: f32| MeshVertex {
                    position: at,
                    normal: [0.0, 0.0, 1.0],
                    diffuse: 0xFFFF_FFFF,
                    uv: [
                        [p.along, p.across + edge],
                        [p.along, edge],
                        [p.along, p.across + edge],
                        [p.along, p.across + edge],
                    ],
                    tangent_u: [1.0, 0.0, 0.0],
                    tangent_v: [0.0, 1.0, 0.0],
                    to_light: [0.0; 3],
                };
                [v(p.at, 0.0), v(p.out, 1.0)]
            })
            .collect()
    };
    Some(Shore {
        points,
        strips,
        vertices: Arc::new(vertices),
    })
}

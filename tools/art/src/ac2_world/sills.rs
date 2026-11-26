//! The converted ground at the doorways of AC2's buildings, kept under their floors.
//!
//! **Why:** where a building's rooms reach into a hillside, AC2 opens a hole in its ground (the
//! land's squares round a vertex of its hole kind are not there), and the shell covers it. AC1's
//! land has no holes, so the converted ground fills them, and at a doorway into a hill it stands
//! over the doorway's floor. AC1 meets the land from a room while a body is within its reach of
//! the plane of a doorway out, so a body walked in from outside is held up on that ground over
//! the floor, short of the room.
//!
//! **What:** across each doorway out to the land, along its width and from its plane to
//! [`REACH_IN`] inside, every [`SAMPLE`] meters, where the converted ground stands more than
//! [`TOLERANCE`] over the doorway's floor (the room's floor just inside it, [`across`]), it is
//! lowered to the floor ([`DenseWorld::lower_under`]): the corners of its square on the room's
//! side of the doorway go down to the floor, and those outside only as far as the point needs,
//! so the land a body walks in over is changed as little as it can be. A corner is lowered no
//! more than [`CAP`], nor one with water; a doorway whose ground stays over its floor for that
//! is counted. A doorway that is not upright (a hatch in a floor or a roof) is left alone.

use dereth_primitives::Vec3;

use crate::ac2_world::dense::{DenseWorld, Under};
use crate::ac2_world::ground::Ac2Ground;

/// How far inside a doorway's plane its ground is kept under its floor, meters: AC1 meets the
/// land from a room while a body's sphere (a human's are 0.48 m) reaches the plane.
pub const REACH_IN: f64 = 1.0;
/// The ground is read this far apart across a doorway, meters.
pub const SAMPLE: f64 = 0.5;
/// Ground at most this far over a doorway's floor is left: a body steps up it, meters.
pub const TOLERANCE: f64 = 0.25;
/// The most a corner of the land is lowered, meters: a hillside can rise steeply over a doorway
/// into it.
pub const CAP: f64 = 40.0;

/// A doorway out of a building to the land, world meters: its corners, a point inside the room
/// it leads into, and the room's floor near it (triangles a body stands on).
#[derive(Clone, Debug, PartialEq)]
pub struct Doorway {
    pub corners: Vec<Vec3>,
    pub inside: Vec3,
    pub floor: Vec<[Vec3; 3]>,
}

/// An upright doorway laid out on the ground, world meters: its middle, the level way into its
/// room (a unit vector), its width either side of the middle along the way across (the inward
/// way turned a quarter left), and its floor's height: the lowest of the room's floor under the
/// points [`REACH_IN`] inside it, or the doorway's lowest corner where no floor is found there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Across {
    pub middle: [f64; 2],
    pub inward: [f64; 2],
    pub from: f64,
    pub to: f64,
    pub floor: f64,
}

/// `d` laid out on the ground; `None` for a doorway that is not upright, or has no width.
#[must_use]
pub fn across(d: &Doorway) -> Option<Across> {
    let p: Vec<[f64; 3]> = d
        .corners
        .iter()
        .map(|c| [f64::from(c.x), f64::from(c.y), f64::from(c.z)])
        .collect();
    if p.len() < 3 {
        return None;
    }
    // Its plane's normal by Newell's method, and its middle.
    let mut n = [0.0f64; 3];
    for (i, a) in p.iter().enumerate() {
        let b = p[(i + 1) % p.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let flat = (n[0] * n[0] + n[1] * n[1]).sqrt();
    // Upright: its normal leans no more than 30 degrees off level.
    if len < 1e-9 || flat < 0.866 * len {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let k = 1.0 / p.len() as f64;
    let mid = p
        .iter()
        .fold([0.0; 2], |m, q| [m[0] + q[0] * k, m[1] + q[1] * k]);
    // The level normal, facing into the room, and the way across.
    let mut inward = [n[0] / flat, n[1] / flat];
    let to_room = [
        f64::from(d.inside.x) - mid[0],
        f64::from(d.inside.y) - mid[1],
    ];
    if inward[0] * to_room[0] + inward[1] * to_room[1] < 0.0 {
        inward = [-inward[0], -inward[1]];
    }
    let across = [-inward[1], inward[0]];
    let along = |q: &[f64; 3]| (q[0] - mid[0]) * across[0] + (q[1] - mid[1]) * across[1];
    let lo = p.iter().map(along).fold(f64::INFINITY, f64::min);
    let hi = p.iter().map(along).fold(f64::NEG_INFINITY, f64::max);
    if hi - lo < SAMPLE {
        return None;
    }
    let bottom = p.iter().map(|q| q[2]).fold(f64::INFINITY, f64::min);
    let mut floor = f64::INFINITY;
    for t in steps(lo, hi) {
        for s in steps(0.0, REACH_IN).into_iter().skip(1) {
            let (x, y) = (
                mid[0] + across[0] * t + inward[0] * s,
                mid[1] + across[1] * t + inward[1] * s,
            );
            for tri in &d.floor {
                if let Some(z) = height_on(tri, x, y) {
                    if (bottom - FLOOR_BELOW..=bottom + FLOOR_ABOVE).contains(&z) {
                        floor = floor.min(z);
                    }
                }
            }
        }
    }
    Some(Across {
        middle: mid,
        inward,
        from: lo,
        to: hi,
        floor: if floor.is_finite() { floor } else { bottom },
    })
}

/// A room's floor near a doorway is taken as its floor from this far under the doorway's lowest
/// corner to this far over it, meters: past that, it is another level of the room.
const FLOOR_BELOW: f64 = 0.5;
const FLOOR_ABOVE: f64 = 2.0;

/// `a` to `b` in steps of at most [`SAMPLE`], both ends among them.
fn steps(a: f64, b: f64) -> Vec<f64> {
    // LINT-OK: a few meters in half-meter steps.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = ((b - a) / SAMPLE).ceil() as usize;
    #[allow(clippy::cast_precision_loss)]
    (0..=count)
        .map(|s| a + (b - a) * s as f64 / count.max(1) as f64)
        .collect()
}

/// The height of triangle `t` over `(x, y)`, `None` where it is not over it or stands on edge.
fn height_on(t: &[Vec3; 3], x: f64, y: f64) -> Option<f64> {
    let p = t.map(|v| [f64::from(v.x), f64::from(v.y), f64::from(v.z)]);
    let det = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
    if det.abs() < 1e-9 {
        return None;
    }
    let a = ((p[1][1] - p[2][1]) * (x - p[2][0]) + (p[2][0] - p[1][0]) * (y - p[2][1])) / det;
    let b = ((p[2][1] - p[0][1]) * (x - p[2][0]) + (p[0][0] - p[2][0]) * (y - p[2][1])) / det;
    let c = 1.0 - a - b;
    let e = -1e-9;
    (a >= e && b >= e && c >= e).then(|| a * p[0][2] + b * p[1][2] + c * p[2][2])
}

/// Where `d`'s ground is read: the points across it (world meters), and the doorway laid out.
/// `None` for a doorway [`across`] leaves alone.
#[must_use]
pub fn samples(d: &Doorway) -> Option<(Vec<(f64, f64)>, Across)> {
    let laid = across(d)?;
    let Across {
        middle: mid,
        inward,
        from: lo,
        to: hi,
        ..
    } = laid;
    let across = [-inward[1], inward[0]];
    let mut at = Vec::new();
    for t in steps(lo, hi) {
        for s in steps(0.0, REACH_IN) {
            at.push((
                mid[0] + across[0] * t + inward[0] * s,
                mid[1] + across[1] * t + inward[1] * s,
            ));
        }
    }
    Some((at, laid))
}

/// How far `d`'s ground stands over its floor at most, meters (below it, negative); `None` for a
/// doorway [`samples`] leaves alone.
#[must_use]
pub fn over(w: &DenseWorld, d: &Doorway) -> Option<f64> {
    let (at, Across { floor, .. }) = samples(d)?;
    Some(
        at.iter()
            .map(|&(x, y)| w.height_at(x, y, true) - floor)
            .fold(f64::NEG_INFINITY, f64::max),
    )
}

/// What lowering the ground at the doorways did: how far each doorway's ground stood over its
/// floor before and after (`None`: left alone), and how many vertices were lowered.
#[derive(Debug, Default)]
pub struct Lowered {
    pub over: Vec<(Option<f64>, Option<f64>)>,
    pub vertices: usize,
}

impl Lowered {
    /// The doorways whose ground stood more than [`TOLERANCE`] over their floor, before and
    /// after.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let high = |o: Option<f64>| o.is_some_and(|o| o > TOLERANCE + 1e-6);
        (
            self.over.iter().filter(|(b, _)| high(*b)).count(),
            self.over.iter().filter(|(_, a)| high(*a)).count(),
        )
    }
}

/// The converted ground at every one of `doorways` lowered under its floor (the module's rule).
/// The cuts of the squares lowered are chosen again, which can lift a point beside them: the
/// doorways are gone over again, a few rounds, until none of their ground is over its floor.
pub fn lower(w: &mut DenseWorld, g: &Ac2Ground, doorways: &[Doorway]) -> Lowered {
    const ROUNDS: usize = 4;
    let before: Vec<Option<f64>> = doorways.iter().map(|d| over(w, d)).collect();
    let laid: Vec<_> = doorways.iter().map(samples).collect();
    let was = w.height.clone();
    for _ in 0..ROUNDS {
        let under: Vec<Under> = laid
            .iter()
            .flatten()
            .flat_map(|(at, a)| {
                at.iter()
                    .filter(|&&(x, y)| w.height_at(x, y, true) > a.floor + TOLERANCE)
                    .map(|&(x, y)| Under {
                        x,
                        y,
                        top: a.floor,
                        at: a.middle,
                        inward: a.inward,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        if under.is_empty() {
            break;
        }
        if w.lower_under(g, &under, CAP).0 == 0 {
            break;
        }
    }
    let after = doorways.iter().map(|d| over(w, d));
    Lowered {
        over: before.into_iter().zip(after).collect(),
        vertices: w.height.iter().zip(&was).filter(|(a, b)| a != b).count(),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's doorways into its holes)
    use super::*;
    use crate::ac2_world::dense::Grid;
    use crate::ac2_world::table::SEA;

    fn cut_all(_: i32, _: i32) -> bool {
        true
    }

    /// AC2: 2 x 2 blocks of 2 squares of 10 m, flat at 100 m (29.05 m converted). AC1: 8 m
    /// squares in a 0.25 m table, with the land from 16 m east raised 3 m: a hill a doorway at
    /// 12 m east leads into.
    fn hill() -> (Ac2Ground, DenseWorld) {
        let mut g = Ac2Ground::new(2, 2, 10.0, cut_all);
        for bx in 0..2 {
            for by in 0..2 {
                g.put_block(bx, by, &[100.0; 9], &[1; 9], &[0; 9], &[0; 9]);
            }
        }
        g.plane = vec![Some(SEA); 4];
        let grid = Grid {
            blocks: 3,
            cells: 1,
            cell_len: 16.0,
            steps: 2,
        };
        let mut w = DenseWorld::sample(&g, grid);
        w.finish(&g, std::array::from_fn(|i| i as f32 * 0.25));
        let side = w.grid.side();
        for i in 2..=4 {
            for j in 0..=4 {
                w.height[i * side + j] += 12;
            }
        }
        (g, w)
    }

    /// The doorway: 6 m wide and 3 m tall, its floor at 29.30 m, facing east into its room.
    fn doorway() -> Doorway {
        Doorway {
            corners: vec![
                Vec3::new(12.0, 12.0, 29.3),
                Vec3::new(12.0, 18.0, 29.3),
                Vec3::new(12.0, 18.0, 32.3),
                Vec3::new(12.0, 12.0, 32.3),
            ],
            inside: Vec3::new(20.0, 15.0, 30.0),
            floor: Vec::new(),
        }
    }

    #[test]
    fn the_ground_a_doorway_into_a_hill_stands_over_is_lowered_to_its_floor() {
        let (g, mut w) = hill();
        let before = w.height.clone();
        let d = doorway();
        let over_before = over(&w, &d).expect("an upright doorway");
        assert!(
            over_before > 1.5,
            "the hill stands {over_before:.2} m over the floor"
        );
        let lowered = lower(&mut w, &g, std::slice::from_ref(&d));
        let after = over(&w, &d).expect("an upright doorway");
        assert!(
            after <= 1e-9,
            "the ground stands {after:.2} m over the floor"
        );
        assert_eq!(lowered.counts(), (1, 0));
        assert!(lowered.vertices > 0);
        // What was lowered went to the floor, not below it: the table's 0.25 m step under it.
        for k in (0..w.height.len()).filter(|&k| w.height[k] != before[k]) {
            let h = f64::from(w.table[usize::from(w.height[k])]);
            assert!(h <= 29.3 && h > 29.3 - 0.25, "a corner lowered to {h:.2} m");
        }
        // The land away from the doorway is as it was: the west edge, and the hill's far side.
        let side = w.grid.side();
        for j in 0..side {
            assert_eq!(w.height[j], before[j]);
            assert_eq!(w.height[4 * side + j], before[4 * side + j]);
        }
        // Ground a doorway's floor already stands over is left alone.
        let kept = w.height.clone();
        let quiet = lower(&mut w, &g, std::slice::from_ref(&d));
        assert_eq!(quiet.vertices, 0);
        assert_eq!(kept, w.height);
    }

    #[test]
    fn a_corner_is_lowered_no_further_than_the_cap_nor_one_with_water() {
        let (g, mut w) = hill();
        let side = w.grid.side();
        // A cap of half a meter: the hill's 3 m are not taken down, and the point is left.
        let point = Under {
            x: 13.0,
            y: 15.0,
            top: 29.3,
            at: [12.0, 15.0],
            inward: [1.0, 0.0],
        };
        let (_, left) = w.lower_under(&g, &[point], 0.5);
        assert_eq!(left, vec![0]);
        let (g2, mut w2) = hill();
        for k in [side + 1, side + 2, 2 * side + 1, 2 * side + 2] {
            w2.vertex[k].water = true;
        }
        let before = w2.height.clone();
        let (n, left) = w2.lower_under(&g2, &[point], CAP);
        assert_eq!((n, left), (0, vec![0]));
        assert_eq!(w2.height, before);
    }

    #[test]
    fn a_hatch_in_a_floor_is_not_a_doorway_to_the_land() {
        let hatch = Doorway {
            corners: vec![
                Vec3::new(10.0, 10.0, 30.0),
                Vec3::new(12.0, 10.0, 30.0),
                Vec3::new(12.0, 12.0, 30.0),
                Vec3::new(10.0, 12.0, 30.0),
            ],
            inside: Vec3::new(11.0, 11.0, 28.0),
            floor: Vec::new(),
        };
        assert!(samples(&hatch).is_none());
        let a = across(&doorway()).expect("upright");
        assert_eq!(a.inward, [1.0, 0.0]);
        assert!((a.to - a.from - 6.0).abs() < 1e-9 && (a.floor - 29.3).abs() < 1e-6);
    }

    #[test]
    fn a_doorways_floor_is_its_rooms_floor_just_inside_it() {
        // The room's floor 0.5 m over the doorway's lowest corner, and a gallery 3 m over it.
        let slab = |z: f32| {
            [
                [
                    Vec3::new(12.0, 10.0, z),
                    Vec3::new(20.0, 10.0, z),
                    Vec3::new(20.0, 20.0, z),
                ],
                [
                    Vec3::new(12.0, 10.0, z),
                    Vec3::new(20.0, 20.0, z),
                    Vec3::new(12.0, 20.0, z),
                ],
            ]
        };
        let mut d = doorway();
        d.floor = slab(29.8).into_iter().chain(slab(32.3)).collect();
        let a = across(&d).expect("upright");
        assert!(
            (a.floor - 29.8).abs() < 1e-6,
            "the floor at {:.2} m",
            a.floor
        );
        // The land is lowered to that floor, not to the doorway's foot.
        let (g, mut w) = hill();
        let before = w.height.clone();
        lower(&mut w, &g, std::slice::from_ref(&d));
        assert!(over(&w, &d).expect("upright") <= 1e-9);
        for k in (0..w.height.len()).filter(|&k| w.height[k] != before[k]) {
            let h = f64::from(w.table[usize::from(w.height[k])]);
            assert!(h <= 29.8 && h > 29.8 - 0.25, "a corner lowered to {h:.2} m");
        }
        // No floor inside: the doorway's foot.
        d.floor = slab(26.0).to_vec();
        assert!((across(&d).expect("upright").floor - 29.3).abs() < 1e-6);
    }
}

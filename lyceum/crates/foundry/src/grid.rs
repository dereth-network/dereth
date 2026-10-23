//! The grid a dungeon is laid out on, from above, one floor at a time.
//!
//! **Retail's grid.** Retail's dungeon rooms stand on a square grid: of its 647,100 dungeon rooms,
//! 99.1% have an origin a multiple of [`PITCH`] meters across both ways, 99.8% are turned a
//! whole number of quarter turns, and 99.4% stand on floors [`RISE`] meters apart; 90% of its
//! 1,970 dungeons stand wholly on the grid (`tests/dat/dungeon.rs` measures it). A piece's
//! doorways meet its neighbors' where the grid puts them, so a piece set on a square of the grid
//! at a quarter turn joins the rooms around it.
//!
//! **Fit.** A piece placed at a frame fits when its space is its own ([`crate::room`]), every open
//! doorway of the dungeon that opens into it is met by a doorway of its own, none of its own opens
//! into another room's wall, and (in a dungeon with rooms already) it joins at least one. The
//! placements of a piece that join an open doorway are found by setting each of its doorways
//! against each open one at each quarter turn ([`placements`]); those that land on the grid stand
//! on its squares, and any other keeps the doorway it was snapped to.

use std::collections::BTreeMap;

use dereth_assets::Environment;
use dereth_primitives::frame::V3;
use dereth_primitives::{Frame, Vec3};

use crate::compile::heading_quat;
use crate::pieces::{mismatch, opening, Opening};
use crate::room::{clashes, Clash, Shape, Solid};
use crate::source::Link;

/// The grid's squares' side, in meters.
pub const PITCH: f32 = 10.0;

/// How far apart a dungeon's floors stand, in meters.
pub const RISE: f32 = 6.0;

/// How far apart two doorways' corners may be and still meet, in meters.
pub const MEET: f32 = 0.05;

/// How far through a doorway the space beyond it is looked at, in meters.
const THROUGH: f32 = 0.5;

/// Whether `frame` (landblock meters) stands on the grid: its origin a multiple of [`PITCH`] both
/// ways across.
#[must_use]
pub fn on_lattice(frame: &Frame) -> bool {
    let on = |v: f32| {
        let r = v.rem_euclid(PITCH);
        r < 0.01 || r > PITCH - 0.01
    };
    on(frame.origin.x) && on(frame.origin.y)
}

/// How many quarter turns clockwise `frame` is turned, when it is turned by a whole number of
/// them about the vertical only.
#[must_use]
pub fn quarter_turns(frame: &Frame) -> Option<u8> {
    let q = frame.rotation;
    if q.x.abs() > 1e-4 || q.y.abs() > 1e-4 {
        return None;
    }
    let deg = (-2.0 * dereth_primitives::num::math::atan2f(q.z, q.w))
        .to_degrees()
        .rem_euclid(360.0);
    let k = (deg / 90.0).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 4
    ((deg - k * 90.0).abs() < 0.05).then_some((k as u8) % 4)
}

/// The square under landblock meters `(x, y)`: the grid point nearest it.
#[must_use]
pub fn square_of(x: f32, y: f32) -> (i32, i32) {
    (
        dereth_primitives::num::floor_to_i32(x / PITCH + 0.5),
        dereth_primitives::num::floor_to_i32(y / PITCH + 0.5),
    )
}

/// The middle of square `s`, in landblock meters.
#[must_use]
pub fn square_middle(s: (i32, i32)) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)] // a square of a landblock, a few hundred at most
    (s.0 as f32 * PITCH, s.1 as f32 * PITCH)
}

/// A piece set on square `s`, at height `z`, turned `turn` quarter turns clockwise.
#[must_use]
pub fn square_frame(s: (i32, i32), z: f32, turn: u8) -> Frame {
    let (x, y) = square_middle(s);
    Frame::new(Vec3::new(x, y, z), heading_quat(f32::from(turn % 4) * 90.0))
}

/// An open doorway of the dungeon: where it is, and the link a new piece's doorway that meets it
/// is given.
#[derive(Clone, Debug, PartialEq)]
pub struct Doorway {
    pub link: Link,
    pub opening: Opening,
}

/// Where a piece goes: its frame, each of its doorways' links in order, and how many open
/// doorways it joins.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub frame: Frame,
    pub links: Vec<Link>,
    pub joins: usize,
}

impl Placement {
    /// The square it stands on, when it stands on the grid at a quarter turn.
    #[must_use]
    pub fn square(&self) -> Option<((i32, i32), u8)> {
        let turn = quarter_turns(&self.frame)?;
        on_lattice(&self.frame).then(|| (square_of(self.frame.origin.x, self.frame.origin.y), turn))
    }
}

/// Why a piece does not fit where it was set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Misfit {
    /// It shares space with this room.
    Overlaps(u16),
    /// It crosses the land's ground.
    Ground,
    /// An open doorway opens into it where it has no doorway.
    Blocks,
    /// A doorway of its own opens into another room's wall.
    IntoWall,
    /// It joins none of the dungeon's open doorways.
    Alone,
}

/// The dungeon a piece is set in: its rooms' spaces (by cell), its open doorways, and its land's
/// ground (none for the open sea floor).
pub struct Dungeon<'a> {
    pub rooms: &'a BTreeMap<u16, Solid>,
    pub doorways: &'a [Doorway],
    pub ground: Option<&'a dyn Fn(f32, f32) -> Option<f32>>,
}

impl std::fmt::Debug for Dungeon<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dungeon")
            .field("rooms", &self.rooms.len())
            .field("doorways", &self.doorways.len())
            .field("land", &self.ground.is_some())
            .finish()
    }
}

/// Piece `piece` of `env` (whose space is `space`) set at `frame` in `dungeon`: the doorways it
/// joins, or why it does not fit.
///
/// # Errors
/// Why it does not fit ([`Misfit`]).
pub fn fit(
    env: &Environment,
    piece: u16,
    space: &Shape,
    frame: &Frame,
    dungeon: &Dungeon<'_>,
) -> Result<Placement, Misfit> {
    let solid = space.placed(frame);
    if let Some(c) = clashes(&solid, dungeon.rooms, &[], dungeon.ground).first() {
        return Err(match c {
            Clash::Room(n) => Misfit::Overlaps(*n),
            Clash::Ground => Misfit::Ground,
        });
    }
    let slots = env
        .cells
        .get(usize::from(piece))
        .map_or(0, |cs| cs.portals.len());
    let own: Vec<Option<Opening>> = (0..slots).map(|s| opening(env, piece, s, frame)).collect();
    let mut links = vec![Link::Shut; slots];
    let mut joins = 0;
    for d in dungeon.doorways {
        let met = own.iter().enumerate().find(|(s, o)| {
            links[*s] == Link::Shut && o.as_ref().is_some_and(|o| mismatch(o, &d.opening) < MEET)
        });
        match met {
            Some((s, _)) => {
                links[s] = d.link.clone();
                joins += 1;
            }
            None => {
                // An open doorway into this piece's space, with no doorway of its own there.
                let beyond = d.opening.middle.add(d.opening.out.mul(THROUGH));
                if solid.holds(beyond) {
                    return Err(Misfit::Blocks);
                }
            }
        }
    }
    for (s, o) in own.iter().enumerate() {
        let Some(o) = o else { continue };
        if links[s] != Link::Shut {
            continue;
        }
        let beyond = o.middle.add(o.out.mul(THROUGH));
        if dungeon.rooms.values().any(|r| r.holds(beyond)) {
            return Err(Misfit::IntoWall);
        }
    }
    if joins == 0 && !dungeon.rooms.is_empty() {
        return Err(Misfit::Alone);
    }
    Ok(Placement {
        frame: *frame,
        links,
        joins,
    })
}

/// Every placement of piece `piece` of `env` (whose space is `space`) that joins one of the
/// dungeon's open doorways `among` (their places in its doorways) and fits: each of its doorways
/// set against each of those at each quarter turn. One placement for each frame, in the order
/// found.
#[must_use]
pub fn placements(
    env: &Environment,
    piece: u16,
    space: &Shape,
    dungeon: &Dungeon<'_>,
    among: &[usize],
) -> Vec<Placement> {
    let slots = env
        .cells
        .get(usize::from(piece))
        .map_or(0, |cs| cs.portals.len());
    let mut out: Vec<Placement> = Vec::new();
    for &i in among {
        let Some(d) = dungeon.doorways.get(i) else {
            continue;
        };
        for slot in 0..slots {
            for turn in 0..4u8 {
                let q = heading_quat(f32::from(turn) * 90.0);
                let Some(local) = opening(env, piece, slot, &Frame::new(Vec3::ZERO, q)) else {
                    continue;
                };
                if local.out.dot(d.opening.out) > -0.99 {
                    continue;
                }
                let frame = Frame::new(d.opening.middle.sub(local.middle), q);
                let same = |p: &Placement| {
                    p.frame
                        .origin
                        .sub(frame.origin)
                        .dot(p.frame.origin.sub(frame.origin))
                        < 1e-4
                        && quarter_turns(&p.frame) == quarter_turns(&frame)
                };
                if out.iter().any(same) {
                    continue;
                }
                if let Ok(p) = fit(env, piece, space, &frame, dungeon) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// An opening of a piece's own frame placed by `frame`.
#[must_use]
pub fn placed(o: &Opening, frame: &Frame) -> Opening {
    let turn = Frame::new(Vec3::ZERO, frame.rotation);
    Opening {
        corners: o
            .corners
            .iter()
            .map(|c| dereth_primitives::frame::localtoglobal(frame, *c))
            .collect(),
        middle: dereth_primitives::frame::localtoglobal(frame, o.middle),
        out: dereth_primitives::frame::localtoglobal(&turn, o.out),
    }
}

/// Where a kit piece goes: its frame, each part's links in order (its doorways' as they join the
/// dungeon, shut elsewhere and at the openings its parts share, which a placement fills), and the
/// open doorways of the dungeon it joins, by their place among them.
#[derive(Clone, Debug, PartialEq)]
pub struct PiecePlacement {
    pub frame: Frame,
    pub links: Vec<Vec<Link>>,
    pub joined: Vec<usize>,
}

impl PiecePlacement {
    /// How many open doorways it joins.
    #[must_use]
    pub fn joins(&self) -> usize {
        self.joined.len()
    }

    /// The square it stands on, when it stands on the grid at a quarter turn.
    #[must_use]
    pub fn square(&self) -> Option<((i32, i32), u8)> {
        let turn = quarter_turns(&self.frame)?;
        on_lattice(&self.frame).then(|| (square_of(self.frame.origin.x, self.frame.origin.y), turn))
    }
}

/// An opening's area, in square meters.
#[must_use]
pub fn area(o: &Opening) -> f32 {
    let c = &o.corners;
    let mut n = Vec3::ZERO;
    for k in 0..c.len() {
        let (a, b) = (c[k], c[(k + 1) % c.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n.dot(n).sqrt() / 2.0
}

/// Whether opening `part` lies in opening `whole`'s plane, facing it, inside it: one of several
/// openings that meet one larger one.
#[must_use]
pub fn within(part: &Opening, whole: &Opening) -> bool {
    if part.out.dot(whole.out) > -0.99 || part.middle.sub(whole.middle).dot(whole.out).abs() > MEET
    {
        return false;
    }
    let w = &whole.corners;
    let n = whole.out;
    // Inside every edge of the (convex) whole, by the same hand, a little either way.
    let side = |p: Vec3| -> Vec<f32> {
        (0..w.len())
            .map(|k| {
                let (a, b) = (w[k], w[(k + 1) % w.len()]);
                b.sub(a).cross(p.sub(a)).dot(n)
            })
            .collect()
    };
    let hand: f32 = side(whole.middle).iter().sum::<f32>().signum();
    part.corners
        .iter()
        .all(|p| side(*p).iter().all(|v| v * hand >= -MEET * 10.0))
}

/// Whether the areas `parts` add up to `whole`'s, to a part in fifty.
fn covers(parts: f32, whole: f32) -> bool {
    (parts - whole).abs() <= whole * 0.02
}

/// Kit piece `form` (each part's space in `spaces`) set at `frame` in `dungeon`: as [`fit`]
/// judges one part, over every part, its doorways the piece's own. A doorway meets an open one
/// of the dungeon whole, or several of them together meet one larger one (as a room's ceiling
/// of several parts meets the floor of the room above), or one meets several smaller ones.
///
/// # Errors
/// Why it does not fit ([`Misfit`]).
#[allow(clippy::too_many_lines)]
pub fn fit_piece(
    form: &crate::kit::Form,
    spaces: &[Shape],
    frame: &Frame,
    dungeon: &Dungeon<'_>,
) -> Result<PiecePlacement, Misfit> {
    let solids: Vec<Solid> = spaces.iter().map(|s| s.placed(frame)).collect();
    for solid in &solids {
        if let Some(c) = clashes(solid, dungeon.rooms, &[], dungeon.ground).first() {
            return Err(match c {
                Clash::Room(n) => Misfit::Overlaps(*n),
                Clash::Ground => Misfit::Ground,
            });
        }
    }
    let own: Vec<Opening> = form
        .doors
        .iter()
        .map(|d| placed(&d.opening, frame))
        .collect();
    let mut links: Vec<Vec<Link>> = form.slots.iter().map(|n| vec![Link::Shut; *n]).collect();
    let set = |links: &mut Vec<Vec<Link>>, k: usize, l: Link| {
        let door = &form.doors[k];
        if let Some(slot) = links
            .get_mut(usize::from(door.part))
            .and_then(|p| p.get_mut(door.slot))
        {
            *slot = l;
        }
    };
    let ways = dungeon.doorways;
    let mut taken = vec![false; own.len()];
    let mut met = vec![false; ways.len()];
    let mut joined = Vec::new();
    // Whole to whole.
    for (i, d) in ways.iter().enumerate() {
        let found = (0..own.len()).find(|k| !taken[*k] && mismatch(&own[*k], &d.opening) < MEET);
        if let Some(k) = found {
            taken[k] = true;
            met[i] = true;
            set(&mut links, k, d.link.clone());
            joined.push(i);
        }
    }
    // Several of its doorways to one of the dungeon's.
    for (i, d) in ways.iter().enumerate() {
        if met[i] {
            continue;
        }
        let parts: Vec<usize> = (0..own.len())
            .filter(|k| !taken[*k] && within(&own[*k], &d.opening))
            .collect();
        if parts.len() > 1 && covers(parts.iter().map(|k| area(&own[*k])).sum(), area(&d.opening)) {
            for k in parts {
                taken[k] = true;
                set(&mut links, k, d.link.clone());
            }
            met[i] = true;
            joined.push(i);
        }
    }
    // One of its doorways to several of the dungeon's.
    for (k, o) in own.iter().enumerate() {
        if taken[k] {
            continue;
        }
        let parts: Vec<usize> = (0..ways.len())
            .filter(|i| !met[*i] && within(&ways[*i].opening, o))
            .collect();
        if parts.len() > 1 && covers(parts.iter().map(|i| area(&ways[*i].opening)).sum(), area(o)) {
            taken[k] = true;
            set(
                &mut links,
                k,
                Link::Many(parts.iter().map(|i| ways[*i].link.clone()).collect()),
            );
            for i in parts {
                met[i] = true;
                joined.push(i);
            }
        }
    }
    // An open doorway into its space it does not meet; one of its own into a wall.
    for (i, d) in ways.iter().enumerate() {
        if met[i] {
            continue;
        }
        let beyond = d.opening.middle.add(d.opening.out.mul(THROUGH));
        if solids.iter().any(|s| s.holds(beyond)) {
            return Err(Misfit::Blocks);
        }
    }
    for (k, o) in own.iter().enumerate() {
        if taken[k] {
            continue;
        }
        let beyond = o.middle.add(o.out.mul(THROUGH));
        if dungeon.rooms.values().any(|r| r.holds(beyond)) {
            return Err(Misfit::IntoWall);
        }
    }
    if joined.is_empty() && !dungeon.rooms.is_empty() {
        return Err(Misfit::Alone);
    }
    joined.sort_unstable();
    Ok(PiecePlacement {
        frame: *frame,
        links,
        joined,
    })
}

/// Every placement of kit piece `form` (each part's space in `spaces`) that joins one of the
/// dungeon's open doorways `among` (their places among its doorways) and fits: each of its
/// doorways set against each of those at each quarter turn. One for each frame, in the order
/// found.
#[must_use]
pub fn piece_placements(
    form: &crate::kit::Form,
    spaces: &[Shape],
    dungeon: &Dungeon<'_>,
    among: &[usize],
) -> Vec<PiecePlacement> {
    let mut out: Vec<PiecePlacement> = Vec::new();
    let mut tried: Vec<Frame> = Vec::new();
    for &i in among {
        let Some(d) = dungeon.doorways.get(i) else {
            continue;
        };
        for door in &form.doors {
            for turn in 0..4u8 {
                let q = heading_quat(f32::from(turn) * 90.0);
                let local = placed(&door.opening, &Frame::new(Vec3::ZERO, q));
                if local.out.dot(d.opening.out) > -0.99 {
                    continue;
                }
                let whole = Frame::new(d.opening.middle.sub(local.middle), q);
                // Met whole, or on the square under it: a doorway of several parts meets a
                // larger one there.
                let o = whole.origin;
                let (sx, sy) = square_middle(square_of(o.x, o.y));
                let on_square = Frame::new(Vec3::new(sx, sy, o.z), q);
                for frame in [whole, on_square] {
                    let same = |f: &Frame| {
                        f.origin.sub(frame.origin).dot(f.origin.sub(frame.origin)) < 1e-4
                            && quarter_turns(f) == quarter_turns(&frame)
                    };
                    if tried.iter().any(same) {
                        continue;
                    }
                    tried.push(frame);
                    if let Ok(p) = fit_piece(form, spaces, &frame, dungeon) {
                        if p.joined.contains(&i) {
                            out.push(p);
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the grid a dungeon is laid out on)
    use super::*;

    #[test]
    fn a_frame_is_on_the_grid_at_whole_multiples_of_its_pitch_and_turns_count_by_quarters() {
        assert!(on_lattice(&square_frame((3, -11), -24.0, 1)));
        assert!(!on_lattice(&Frame::new(
            Vec3::new(35.0, 10.0, 0.0),
            heading_quat(0.0)
        )));
        for turn in 0..4 {
            assert_eq!(quarter_turns(&square_frame((0, 0), 0.0, turn)), Some(turn));
        }
        assert_eq!(
            quarter_turns(&Frame::new(Vec3::ZERO, heading_quat(45.0))),
            None
        );
        assert_eq!(square_of(34.9, -15.1), (3, -2));
        assert_eq!(square_of(35.1, 4.9), (4, 0));
        assert_eq!(square_middle((4, -2)), (40.0, -20.0));
    }
}

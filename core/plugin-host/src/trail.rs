//! The way a journey goes next: the stretch of its route ahead of the character, as places a
//! short step apart along it. The host draws a shown journey with it (see `journey_trail`); a
//! renderer draws a faint wisp through those places, held over the ground under each.

use dereth_navigation::global;
use dereth_primitives::{CellId, Position, Vec3};

/// How far along the route ahead the trail reaches, metres.
pub const TRAIL_LENGTH: f32 = 30.0;

/// How far apart the trail's places are, metres: short enough that a wisp drawn through them
/// follows the ground's shape.
pub const TRAIL_STEP: f32 = 0.4;

/// A step along the route longer than this, metres, is a portal's or a recall's jump, not a way
/// to walk: the trail ends before it.
pub const TRAIL_JUMP: f32 = 30.0;

// A journey's way is shown in steps shorter than a jump, so a long straight stretch of a walk is
// never taken for one.
const _: () = assert!(dereth_client_runtime::travel::WAY_STEP < TRAIL_JUMP);

/// How near the journey's end has to be, metres, for it to be marked on the ground.
pub const GOAL_SIGHT: f32 = 40.0;

/// The landblock's side, metres.
const BLOCK: f32 = 192.0;

/// `p` moved by `d` metres (east, north, up): outdoors, in whichever landblock and cell it falls
/// in; indoors, in `p`'s own cell.
#[must_use]
pub fn moved(p: &Position, d: Vec3) -> Position {
    let mut q = *p;
    q.frame.origin = Vec3::new(
        q.frame.origin.x + d.x,
        q.frame.origin.y + d.y,
        q.frame.origin.z + d.z,
    );
    if q.cell.0 & 0xFFFF < 0x100 {
        let (mut bx, mut by) = ((q.cell.0 >> 24) as i32, ((q.cell.0 >> 16) & 0xFF) as i32);
        let (mut x, mut y) = (q.frame.origin.x, q.frame.origin.y);
        while x < 0.0 {
            x += BLOCK;
            bx -= 1;
        }
        while x >= BLOCK {
            x -= BLOCK;
            bx += 1;
        }
        while y < 0.0 {
            y += BLOCK;
            by -= 1;
        }
        while y >= BLOCK {
            y -= BLOCK;
            by += 1;
        }
        if (0..=0xFF).contains(&bx) && (0..=0xFF).contains(&by) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let index = ((x / 24.0) as u32).min(7) * 8 + ((y / 24.0) as u32).min(7) + 1;
            #[allow(clippy::cast_sign_loss)]
            {
                q.cell = CellId((bx as u32) << 24 | (by as u32) << 16 | index);
            }
            q.frame.origin = Vec3::new(x, y, q.frame.origin.z);
        }
    }
    q
}

fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    (dx * dx + dy * dy).sqrt()
}

/// The stretch of the way the trail is drawn along: from `here` (where the character stands) on
/// along `way` (the way still to walk, from the corner passed last; see the travel's `way`), its
/// corners turned round as a plugin's path's are ([`path_ahead`]), before the first step longer
/// than [`TRAIL_JUMP`] and at most [`TRAIL_LENGTH`] metres along.
#[must_use]
pub fn ahead(here: &Position, way: &[Position]) -> Vec<Position> {
    let way = rounded(way);
    let start = path_start(here, &way);
    cut(here, &way[start..], TRAIL_LENGTH)
}

/// From `from` through `rest`, before the first step longer than [`TRAIL_JUMP`] and at most
/// `reach` metres along.
fn cut(from: &Position, rest: &[Position], reach: f32) -> Vec<Position> {
    let mut line = vec![*from];
    let mut length = 0.0;
    let mut last = global(from);
    for p in rest {
        let here = global(p);
        let step = flat_distance(last, here);
        if step > TRAIL_JUMP {
            break;
        }
        if length + step >= reach {
            let prev = *line.last().expect("the start");
            line.push(between(&prev, p, (reach - length) / step.max(f32::EPSILON)));
            break;
        }
        line.push(*p);
        length += step;
        last = here;
    }
    line
}

/// Whether the way `now` is the way `before` walked along: the same, or with the corners passed
/// since dropped from its front. Anything else (a new plan, a new journey) is a new way.
#[must_use]
pub fn continues(before: &[Position], now: &[Position]) -> bool {
    !now.is_empty() && now.len() <= before.len() && before[before.len() - now.len()..] == *now
}

/// A journey trail as the host drew it last: its trip, the way it was drawn along, its line,
/// and since when (frame time) it has been coming in.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub trip: u64,
    pub way: Vec<Position>,
    pub line: Vec<Position>,
    pub since: f64,
    /// How far the way's first place lies along the way as the trail first drew it, metres: the
    /// corners dropped from its front since, so the pattern drawn along it stays on the ground.
    pub origin: f32,
    /// How far along the way the line starts, from the same place ([`path_along`]).
    pub along: f32,
}

/// How far `way`'s first place lies along the way `last` drew, metres, when `way` is that way
/// walked on (corners dropped from its front); 0 for any other.
#[must_use]
pub fn origin_after(last: Option<&Drawn>, trip: u64, way: &[Position]) -> f32 {
    match last {
        Some(d) if d.trip == trip && continues(&d.way, way) => {
            let dropped = d.way.len() - way.len();
            d.origin + length(&d.way[..=dropped.min(d.way.len().saturating_sub(1))])
        }
        _ => 0.0,
    }
}

/// What becomes of `last` (the trail drawn last) as a frame draws one along `way` for trip `trip`
/// at `now`, `None` where it draws none: since when the trail drawn now has been coming in, and
/// whether `last` goes out. A trail along the way drawn last, walked on, carries on; a new way or
/// a new trip comes in from now, and the one drawn last goes out.
#[must_use]
pub fn carried(
    last: Option<&Drawn>,
    now_drawn: Option<(u64, &[Position])>,
    now: f64,
) -> (f64, bool) {
    match (last, now_drawn) {
        (Some(d), Some((trip, way))) if d.trip == trip && continues(&d.way, way) => {
            (d.since, false)
        }
        (Some(_), _) => (now, true),
        (None, _) => (now, false),
    }
}

/// How long a trail drawn along a new way takes to come in, and the one it replaces to go,
/// seconds: the two cross-fade instead of the one replacing the other between two frames.
pub const CROSS_FADE: f64 = 0.5;

/// `line` as places [`TRAIL_STEP`] metres apart along it, from its first place to its last.
#[must_use]
pub fn resampled(line: &[Position]) -> Vec<Position> {
    let Some(first) = line.first() else {
        return Vec::new();
    };
    let mut out = vec![*first];
    let mut next = TRAIL_STEP;
    let mut walked = 0.0;
    for pair in line.windows(2) {
        let (a, b) = (global(&pair[0]), global(&pair[1]));
        let step = flat_distance(a, b);
        if step <= f32::EPSILON {
            continue;
        }
        while next < walked + step {
            let f = (next - walked) / step;
            out.push(moved(
                &pair[0],
                Vec3::new((b.x - a.x) * f, (b.y - a.y) * f, (b.z - a.z) * f),
            ));
            next += TRAIL_STEP;
        }
        walked += step;
    }
    if line.len() > 1 {
        out.push(*line.last().expect("two or more"));
    }
    out
}

/// The place `f` (0 to 1) of the way from `a` to `b`, in `a`'s cell outdoors or indoors as
/// [`moved`] takes it.
#[must_use]
pub fn between(a: &Position, b: &Position, f: f32) -> Position {
    let (ga, gb) = (global(a), global(b));
    moved(
        a,
        Vec3::new((gb.x - ga.x) * f, (gb.y - ga.y) * f, (gb.z - ga.z) * f),
    )
}

/// How far back along each leg a path's corner is turned round, metres, at most: about as far as
/// a walk cuts a corner across, so the way drawn turns where the character turns.
pub const CORNER_ROUND: f32 = 2.5;

/// A turn smaller than this, radians, is drawn as it is.
const CORNER_SLIGHT: f32 = 0.05;

/// How many steps a corner's turn is drawn in.
const CORNER_STEPS: u16 = 6;

/// `points` with each corner turned round a curve that leaves the leg before it and joins the
/// leg after it at most [`CORNER_ROUND`] metres from the corner (and at most halfway along
/// either leg), so the line turns as a body walking it does rather than folding at the point. A
/// step longer than [`TRAIL_JUMP`] (a jump, not a way to walk) is not turned round.
#[must_use]
pub fn rounded(points: &[Position]) -> Vec<Position> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len());
    out.push(points[0]);
    for w in points.windows(3) {
        let (a, p, b) = (global(&w[0]), global(&w[1]), global(&w[2]));
        let (la, lb) = (flat_distance(a, p), flat_distance(p, b));
        let r = CORNER_ROUND.min(la * 0.5).min(lb * 0.5);
        let turn = if la > 1e-3 && lb > 1e-3 {
            let (ux, uy) = ((p.x - a.x) / la, (p.y - a.y) / la);
            let (vx, vy) = ((b.x - p.x) / lb, (b.y - p.y) / lb);
            dereth_primitives::num::math::acosf((ux * vx + uy * vy).clamp(-1.0, 1.0))
        } else {
            0.0
        };
        if turn < CORNER_SLIGHT || r < 0.05 || la > TRAIL_JUMP || lb > TRAIL_JUMP {
            out.push(w[1]);
            continue;
        }
        // A curve from `r` before the corner to `r` after it, drawn toward the corner itself.
        let entry = between(&w[1], &w[0], r / la);
        let exit = between(&w[1], &w[2], r / lb);
        for i in 0..=CORNER_STEPS {
            let f = f32::from(i) / f32::from(CORNER_STEPS);
            let (q0, q1) = (between(&entry, &w[1], f), between(&w[1], &exit, f));
            out.push(between(&q0, &q1, f));
        }
    }
    out.push(points[points.len() - 1]);
    out
}

/// Where a path drawn from `here` along `points` (already turned round its corners) goes on
/// from: the first of `points` after the leg nearest `here`, or `points.len()` when that leg is
/// the last and `here` is past its end.
fn path_start(here: &Position, points: &[Position]) -> usize {
    let (nearest, t) = nearest_leg(here, points);
    if points.len() >= 2 && nearest + 2 == points.len() && t >= 1.0 {
        return points.len();
    }
    (nearest + 1).min(points.len())
}

/// A plugin's path as it is drawn this frame: from `player` (where the character stands, when
/// the path starts there) on along `points` from the leg of them nearest the character, or along
/// all of `points` without one, each corner turned round ([`rounded`]); ending before the first
/// step longer than [`TRAIL_JUMP`] and at most `reach` metres along; as places [`TRAIL_STEP`]
/// metres apart.
#[must_use]
pub fn path_ahead(player: Option<&Position>, points: &[Position], reach: f32) -> Vec<Position> {
    let points = rounded(points);
    let Some(here) = player else {
        let Some((first, rest)) = points.split_first() else {
            return Vec::new();
        };
        return resampled(&cut(first, rest, reach));
    };
    let start = path_start(here, &points);
    resampled(&cut(here, &points[start..], reach))
}

/// The leg of `points` nearest `here`, and how far along it (0 to 1) the place on it nearest
/// `here` is; the first leg, from its start, for fewer than two points.
fn nearest_leg(here: &Position, points: &[Position]) -> (usize, f32) {
    let h = global(here);
    points
        .windows(2)
        .enumerate()
        .map(|(i, w)| {
            let (a, b) = (global(&w[0]), global(&w[1]));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((h.x - a.x) * dx + (h.y - a.y) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (px, py) = (a.x + dx * t, a.y + dy * t);
            (i, t, (px - h.x) * (px - h.x) + (py - h.y) * (py - h.y))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map_or((0, 0.0), |(i, t, _)| (i, t))
}

/// How far along `points` (a plugin's path, turned round its corners) the line [`path_ahead`]
/// draws from `here` counts its start, metres along the ground from the path's first point: the
/// first of the path's own places the line goes through is then exactly as far along as it is on
/// the path, wherever the character stands, so a pattern worked out from it stays where it is on
/// the ground as the character walks and as the line starts on from one place to the next.
#[must_use]
pub fn path_along(here: &Position, points: &[Position]) -> f32 {
    let points = rounded(points);
    let start = path_start(here, &points);
    let Some(first) = points.get(start) else {
        return length(&points);
    };
    length(&points[..=start]) - flat_distance(global(here), global(first))
}

/// How long `line` is along the ground, metres.
#[must_use]
pub fn length(line: &[Position]) -> f32 {
    line.windows(2)
        .map(|p| flat_distance(global(&p[0]), global(&p[1])))
        .sum()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the arithmetic of laying the trail out; the behaviour is the journey's
    //! drawing, tested with the host)
    use super::*;
    use dereth_primitives::{Frame, Quat};

    fn at(cell: u32, x: f32, y: f32) -> Position {
        Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        )
    }

    #[test]
    fn a_place_moved_past_a_landblocks_edge_is_in_the_next_one() {
        let p = moved(&at(0xA9B4_0039, 190.0, 100.0), Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(p.cell, CellId(0xAAB4_0005));
        assert!((p.frame.origin.x - 3.0).abs() < 1e-4);
        let indoors = moved(&at(0x019E_0114, 10.0, -40.0), Vec3::new(3.0, 0.0, 0.0));
        assert_eq!(indoors.cell, CellId(0x019E_0114));
    }

    /// The trail runs on through the walk's corners and the walks after it to its length: a walk
    /// leg's end is no end of it. It stops before a jump (a portal's far side).
    #[test]
    fn the_trail_runs_on_through_the_ways_corners_and_walks_to_its_length_or_a_jump() {
        let way = [
            at(0xA9B4_0001, 10.0, 10.0), // the corner behind
            at(0xA9B4_0001, 10.0, 20.0), // the walk's next corner
            at(0xA9B4_0002, 20.0, 30.0), // the walk's end
            at(0xA9B4_0003, 20.0, 60.0), // the next walk's end
        ];
        let here = at(0xA9B4_0001, 10.0, 12.0);
        let line = ahead(&here, &way);
        assert_eq!(line.first(), Some(&here));
        assert!(
            (length(&line) - TRAIL_LENGTH).abs() < 1e-2,
            "{}",
            length(&line)
        );
        let jump = [
            at(0xA9B4_0001, 10.0, 10.0),
            at(0xA9B4_0001, 10.0, 20.0),
            at(0x019E_0114, 10.0, -40.0), // through a portal
        ];
        assert_eq!(
            ahead(&here, &jump).len(),
            2,
            "it ends before the portal's jump"
        );
        let far: Vec<Position> = (0..6)
            .map(|i| at(0xA9B4_0001, 10.0, 10.0 + 10.0 * i as f32))
            .collect();
        assert!((length(&ahead(&here, &far)) - TRAIL_LENGTH).abs() < 1e-3);
    }

    /// A trail walked along carries on as it was; a new way, a new trip or none at all brings the
    /// new one in from now and sends the one drawn last out.
    #[test]
    fn a_new_way_cross_fades_and_a_way_walked_along_carries_on() {
        let way = vec![
            at(0xA9B4_0001, 10.0, 10.0),
            at(0xA9B4_0001, 10.0, 20.0),
            at(0xA9B4_0001, 30.0, 20.0),
        ];
        let last = Drawn {
            trip: 7,
            way: way.clone(),
            line: Vec::new(),
            since: 3.0,
            origin: 5.0,
            along: 0.0,
        };
        // The pattern's place on the way carries on with it: its first corner dropped, the way
        // now starts 10 m further along; a new plan or journey starts afresh.
        assert!((origin_after(Some(&last), 7, &way[1..]) - 15.0).abs() < 1e-4);
        assert!((origin_after(Some(&last), 7, &way) - 5.0).abs() < 1e-4);
        assert_eq!(origin_after(Some(&last), 8, &way[1..]), 0.0);
        assert_eq!(
            carried(Some(&last), Some((7, &way[1..])), 9.0),
            (3.0, false),
            "walked on"
        );
        let replanned = [at(0xA9B4_0001, 12.0, 20.0), at(0xA9B4_0001, 30.0, 20.0)];
        assert_eq!(
            carried(Some(&last), Some((7, &replanned)), 9.0),
            (9.0, true),
            "a new plan"
        );
        assert_eq!(
            carried(Some(&last), Some((8, &way)), 9.0),
            (9.0, true),
            "a new journey"
        );
        assert_eq!(carried(Some(&last), None, 9.0), (9.0, true), "over");
        assert_eq!(
            carried(None, Some((7, &way)), 9.0),
            (9.0, false),
            "the first"
        );
    }

    /// Walking on past a corner, the corner drops from the way's front: the way continues, and
    /// the trail beyond the character stays where it was, about as long as before less the step
    /// walked.
    #[test]
    fn passing_a_corner_leaves_the_trail_ahead_where_it_was() {
        let way = [
            at(0xA9B4_0001, 10.0, 10.0),
            at(0xA9B4_0001, 10.0, 20.0),
            at(0xA9B4_0001, 30.0, 20.0),
            at(0xA9B4_0001, 30.0, 45.0),
        ];
        // Just short of the corner, cutting it a little, and just past it with it dropped.
        let before = ahead(&at(0xA9B4_0001, 10.2, 19.4), &way);
        let after = ahead(&at(0xA9B4_0001, 10.6, 19.8), &way[1..]);
        assert!(continues(&way, &way[1..]));
        assert!(
            !continues(&way, &[at(0xA9B4_0001, 11.0, 20.0)]),
            "a new plan is a new way"
        );
        // Past the corner, both lines run through the same places: where the next corner's curve
        // joins the leg after it.
        let corner_on = |line: &[Position]| {
            line.iter().map(global).any(|p| {
                (p.x - (0xA9 as f32 * 192.0 + 30.0)).abs() < 1e-3
                    && (p.y - (0xB4 as f32 * 192.0 + 22.5)).abs() < 1e-3
            })
        };
        assert!(corner_on(&before) && corner_on(&after));
        assert!(
            (length(&before) - length(&after)).abs() < 1.5,
            "{} then {}",
            length(&before),
            length(&after)
        );
    }

    #[test]
    fn a_path_starts_at_the_character_goes_on_from_its_nearest_leg_and_ends_at_its_reach() {
        let points: Vec<Position> = (0..10)
            .map(|i| at(0xA9B4_0001, 10.0, 10.0 + 10.0 * i as f32))
            .collect();
        // Standing beside the third leg (25 m up the path), 2 m off it.
        let here = at(0xA9B4_0001, 12.0, 35.0);
        let line = path_ahead(Some(&here), &points, 20.0);
        assert_eq!(line.first(), Some(&here));
        let second = global(&line[1]);
        assert!(
            second.y > 35.0,
            "on from the nearest leg, not back along it: {second:?}"
        );
        assert!((length(&line) - 20.0).abs() < 0.5, "{}", length(&line));
        let whole = path_ahead(None, &points, 200.0);
        assert_eq!(whole.first(), points.first());
        assert!((length(&whole) - 90.0).abs() < 0.5);
    }

    /// A path with a square corner, the character cutting inside it as a walk does: the line
    /// drawn from the character turns round the corner on a curve rather than going out to the
    /// corner's point and back, so it never comes within half a metre of the point.
    #[test]
    fn a_paths_corner_is_drawn_turning_round_not_folded_at_its_point() {
        let points = [
            at(0xA9B4_0001, 10.0, 10.0),
            at(0xA9B4_0001, 30.0, 10.0),
            at(0xA9B4_0001, 30.0, 30.0),
        ];
        let corner = global(&points[1]);
        for (x, y) in [(25.0, 10.5), (28.5, 11.5), (29.0, 12.0), (29.6, 14.0)] {
            let line = path_ahead(Some(&at(0xA9B4_0001, x, y)), &points, 30.0);
            let nearest = line
                .iter()
                .map(|p| flat_distance(global(p), corner))
                .fold(f32::MAX, f32::min);
            assert!(
                nearest > 0.5,
                "from {x} {y}: the line comes {nearest:.2} m from the corner's point"
            );
            let end = global(line.last().expect("a line"));
            assert!(
                (end.x - corner.x).abs() < 0.05 && end.y > corner.y + 10.0,
                "on up the second leg: {end:?}"
            );
        }
        // A slight turn and the path's own ends are drawn as they are.
        let straight = [
            at(0xA9B4_0001, 10.0, 10.0),
            at(0xA9B4_0001, 20.0, 10.0),
            at(0xA9B4_0001, 30.0, 10.0),
        ];
        assert_eq!(rounded(&straight), straight.to_vec());
        let r = rounded(&points);
        assert_eq!((r.first(), r.last()), (points.first(), points.last()));
    }

    #[test]
    fn how_far_along_a_path_the_character_is_runs_from_its_first_point_to_the_nearest_place() {
        let points: Vec<Position> = (0..10)
            .map(|i| at(0xA9B4_0001, 10.0, 10.0 + 10.0 * i as f32))
            .collect();
        // Beside the third leg, 2 m off it 25 m up: the line goes on to the place 30 m up, which
        // it reaches as far along as the path does.
        let beside = 30.0 - dereth_primitives::num::math::hypotf(2.0, 5.0);
        assert!((path_along(&at(0xA9B4_0001, 12.0, 35.0), &points) - beside).abs() < 1e-3);
        assert!((path_along(&at(0xA9B4_0001, 10.0, 130.0), &points) - 90.0).abs() < 1e-3);
        // 10 m before its first place: the line reaches its second, 10 m along, after 20 m.
        assert!((path_along(&at(0xA9B4_0001, 10.0, 0.0), &points) + 10.0).abs() < 1e-3);
        assert!(path_along(&at(0xA9B4_0001, 10.0, 0.0), &points[..1]).abs() < 1e-3);
    }

    #[test]
    fn the_trail_is_resampled_a_short_step_apart_from_its_start_to_its_end() {
        let line = [
            at(0xA9B4_0001, 10.0, 10.0),
            at(0xA9B4_0001, 10.0, 12.0),
            at(0xA9B4_0001, 11.0, 12.0),
        ];
        let r = resampled(&line);
        assert_eq!(r.first(), line.first());
        assert_eq!(r.last(), line.last());
        let gaps: Vec<f32> = r
            .windows(2)
            .map(|w| flat_distance(global(&w[0]), global(&w[1])))
            .collect();
        assert!(gaps.iter().all(|g| *g <= TRAIL_STEP + 0.01), "{gaps:?}");
        assert!(
            (length(&r) - 3.0).abs() < 0.2,
            "it keeps to the line's corner"
        );
    }
}

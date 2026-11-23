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

/// The stretch of the way the trail is drawn along: from `here` (where the character stands) to
/// the place nearest it on the way's nearest stretch, then on along `way` (the way still to walk,
/// from the corner passed last; see the travel's `way`) through its corners, before the first step
/// longer than [`TRAIL_JUMP`] and at most [`TRAIL_LENGTH`] metres along. As the character walks
/// on, and as the corner behind it drops from the way's front, the trail ahead stays where it was.
#[must_use]
pub fn ahead(here: &Position, way: &[Position]) -> Vec<Position> {
    if way.len() < 2 {
        return cut(here, way, TRAIL_LENGTH);
    }
    // The stretch nearest the character, and how far along it the nearest place lies.
    let h = global(here);
    let (i, t, _) = way
        .windows(2)
        .enumerate()
        .map(|(i, w)| {
            let (a, b) = (global(&w[0]), global(&w[1]));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((h.x - a.x) * dx + (h.y - a.y) * dy) / len2).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let (px, py) = (a.x + dx * t, a.y + dy * t);
            (i, t, (px - h.x) * (px - h.x) + (py - h.y) * (py - h.y))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .expect("two or more places");
    let mut rest = Vec::with_capacity(way.len() - i);
    let on = between(&way[i], &way[i + 1], t);
    if flat_distance(global(&on), h) > 0.05 && t < 1.0 {
        rest.push(on);
    }
    rest.extend_from_slice(&way[i + 1..]);
    cut(here, &rest, TRAIL_LENGTH)
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

/// A plugin's path as it is drawn this frame: from `player` (where the character stands, when
/// the path starts there) on along `points` from the leg of them nearest the character, or along
/// all of `points` without one; ending before the first step longer than [`TRAIL_JUMP`] and at
/// most `reach` metres along; as places [`TRAIL_STEP`] metres apart.
#[must_use]
pub fn path_ahead(player: Option<&Position>, points: &[Position], reach: f32) -> Vec<Position> {
    let Some(here) = player else {
        let Some((first, rest)) = points.split_first() else {
            return Vec::new();
        };
        return resampled(&cut(first, rest, reach));
    };
    // The leg the character is nearest: it goes on from that leg's far end.
    let h = global(here);
    let nearest = points
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
            (i, (px - h.x) * (px - h.x) + (py - h.y) * (py - h.y))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |(i, _)| i);
    resampled(&cut(
        here,
        &points[(nearest + 1).min(points.len())..],
        reach,
    ))
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
        };
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
        // Past the corner, both lines run through the same places.
        let corner_on = |line: &[Position]| {
            line.iter().map(global).any(|p| {
                (p.x - (0xA9 as f32 * 192.0 + 30.0)).abs() < 1e-3
                    && (p.y - (0xB4 as f32 * 192.0 + 20.0)).abs() < 1e-3
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

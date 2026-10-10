//! A body drawn moving between its physics ticks rather than standing still between them.
//!
//! Physics moves a body only on its ticks, thirty times a second; a frame drawn between two ticks
//! shows it where the last one left it, so at any higher frame rate it stands still and then
//! jumps. [`BetweenTicks`] draws it instead a tick behind, along its way from the tick before the
//! last to the last, by the share of a tick the frame has still to run, and turned back by the
//! same share of the turn the last tick made. A tick that carries it further than the others is
//! spread over a few frames by a short spring, which never carries the drawn body past the point
//! it is following.
//!
//! Only the drawing changes. The body is never drawn anywhere physics has not had it: a tick
//! behind, it is drawn on the way physics swept it along between two ticks, and it is drawn where
//! physics has it, begun again from there, whenever anything but a step of physics moved it — the
//! server placing it (a teleport, a portal, a recall, a correction too far to walk), physics
//! placing it rather than sweeping it there, a tick carrying it further than
//! [`DRAWN_STEP_LIMIT`] or turning it further than [`DRAWN_TURN_LIMIT`], or a pause in the ticks.
//!
//! The local body ([`crate::character::Character`]) and every object the server moves
//! ([`crate::world_state::WorldObject`]) are drawn this way by the same machinery.

use dereth_primitives::{Position, Vec3};

/// The furthest one physics tick carries a body that is still drawn between ticks; a longer step
/// (a portal, a teleport) is drawn where it lands at once.
pub const DRAWN_STEP_LIMIT: f32 = 4.0;

/// The sharpest turn, in degrees, one physics tick makes of a body that is still drawn turning
/// between ticks; a sharper one (the server setting its heading) is drawn where it lands at once.
pub const DRAWN_TURN_LIMIT: f32 = 45.0;

/// The time the drawn body's spring takes to settle onto its way between ticks, in seconds: short
/// against a tick (a thirtieth of a second), long enough to spread a double step over a few frames.
pub const DRAWN_SPRING_SECONDS: f32 = 0.05;

/// The longest a body's ticks can be apart and still be drawn between, in seconds: the longest
/// step physics takes in one go. Ticks further apart than that are not a body moving at a tick's
/// pace but one that was held (the world waiting on a portal, or the drawing between ticks just
/// turned on), and the body is drawn where physics has it, begun again from there.
pub const LONGEST_TICK: f64 = dereth_physics::globals::MAX_QUANTUM;

/// The body as it is drawn between ticks: the place physics had it when last stepped, the offset
/// from that place it is drawn at, and how fast that offset is changing.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DrawnBody {
    at: Position,
    offset: Vec3,
    speed: Vec3,
}

/// The body's turn as it is drawn between ticks: the heading physics had it at when last turned,
/// in degrees, the turn from that heading it is drawn at, and how fast that turn is changing.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DrawnTurn {
    at: f32,
    offset: f32,
    speed: f32,
}

/// One frame of `dt` seconds of the drawn body's critically damped spring: `x` from where it is
/// drawn to where it is going, `v` how fast it is closing, both after the frame. No overshoot, at
/// any frame rate: a frame that would carry it across where it is going leaves it there, at rest.
fn drawn_spring(x: f32, v: f32, dt: f32) -> (f32, f32) {
    let w = 2.0 / DRAWN_SPRING_SECONDS;
    let e = dereth_primitives::num::math::expf(-w * dt);
    let t = (v + w * x) * dt;
    let (to, speed) = ((x + t) * e, (v - w * t) * e);
    if to * x < 0.0 {
        return (0.0, 0.0);
    }
    (to, speed)
}

/// The signed difference from `from` to `to`, in degrees, the short way round.
fn turn_between(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

fn heading(p: &Position) -> f32 {
    dereth_primitives::frame::get_heading(&p.frame)
}

/// Whether `a` and `b` are the same place: a turn alone is not a move.
fn same_place(a: &Position, b: &Position) -> bool {
    a.cell == b.cell && a.frame.origin == b.frame.origin
}

/// One body's drawing between its physics ticks. See the module documentation.
///
/// Each frame takes the body's place as physics has it now (`here`), whether it is drawn between
/// ticks at all (`on`), and the space it is drawn in (`space`: a place's origin there, the same
/// for every call of one frame); [`Self::frame`] is told when physics has ticked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BetweenTicks {
    /// The body's place at its last two physics ticks and the time of each, for drawing it
    /// between them.
    ticks: [Option<(Position, f64)>; 2],
    /// How far, in degrees clockwise, the physics tick that last ran turned the body itself, from
    /// the way it faced as the tick began: a turn the body was given between ticks (faced where
    /// the camera looks) is not the tick's, and is not drawn again as one.
    tick_turn: f32,
    /// The time the frame being drawn is at.
    frame_time: f64,
    /// Where the body stood as the frame's tick began ([`Self::before_tick`]).
    before: Option<Position>,
    /// How many times physics had placed the body, rather than moved it, as of the last frame.
    placements: u32,
    /// Where the body is drawn, as an offset from where physics has it, and how fast that point
    /// moves: it follows the way between ticks on a short spring, so a tick that carries the body
    /// twice as far does not show as a lurch. `None` while it is not drawn between ticks.
    drawn: Option<DrawnBody>,
    /// The same for the way it is drawn facing: it follows its turn between ticks on the same
    /// spring. `None` while it is not drawn turning between ticks.
    turning: Option<DrawnTurn>,
}

impl BetweenTicks {
    /// The time of the frame being drawn, as the last [`Self::frame`] was told it.
    #[must_use]
    pub const fn frame_time(&self) -> f64 {
        self.frame_time
    }

    /// Where the body stands as this frame's physics tick, if it has one, begins: what the tick
    /// moves and turns it from. Anything that moved it since the last tick is not the tick's.
    pub fn before_tick(&mut self, here: Position) {
        self.before = Some(here);
    }

    /// Forget the ticks and the drawn body: it is drawn where physics has it until two ticks have
    /// passed again.
    pub fn begin_again(&mut self) {
        self.ticks = [None, None];
        self.drawn = None;
        self.turning = None;
    }

    /// One frame at time `now`: `ticked` when physics stepped the body this frame, which leaves it
    /// at `here`; `placements`, how many times physics has placed the body rather than moved it
    /// (zero for a body physics does not hold). The drawn body is then moved one frame along its
    /// spring.
    pub fn frame(
        &mut self,
        now: f64,
        ticked: bool,
        here: Position,
        placements: u32,
        on: bool,
        space: impl Fn(Position) -> Vec3,
    ) {
        let dt = (now - self.frame_time).max(0.0);
        self.frame_time = now;
        if placements != self.placements {
            // Put somewhere, not swept there: drawn there at once.
            self.placements = placements;
            self.begin_again();
        }
        if ticked {
            // Told nothing of where the tick began, nothing but the tick moved it.
            let from = self
                .before
                .take()
                .or(self.ticks[1].map(|(last, _)| last))
                .unwrap_or(here);
            let held = self.ticks[1]
                .is_some_and(|(last, t1)| !same_place(&from, &last) || now - t1 > LONGEST_TICK);
            if held {
                self.begin_again();
            }
            self.ticks = [self.ticks[1], Some((here, now))];
            self.tick_turn = turn_between(heading(&from), heading(&here));
        } else if self.ticks[1].is_some_and(|(last, _)| !same_place(&here, &last)) {
            self.begin_again();
        }
        self.step_drawn(dt, here, on, &space);
        self.step_drawn_turn(dt, here, on);
    }

    /// Move the drawn body one frame of `dt` seconds along its spring toward its way between
    /// ticks ([`Self::way_offset`]), first keeping it where it was drawn while physics moved the
    /// body under it.
    fn step_drawn(&mut self, dt: f64, here: Position, on: bool, space: &impl Fn(Position) -> Vec3) {
        let way = self.way_offset(here, on, space);
        let Some(mut d) = self
            .drawn
            .filter(|_| way != Vec3::ZERO || self.at_last_tick(here))
        else {
            self.drawn = (way != Vec3::ZERO).then_some(DrawnBody {
                at: here,
                offset: way,
                speed: Vec3::ZERO,
            });
            return;
        };
        // Physics moved the body; the drawn point stays where it was in the world.
        let (a, b) = (space(d.at), space(here));
        let moved = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        if moved.magnitude() > DRAWN_STEP_LIMIT {
            self.drawn = None;
            return;
        }
        d.offset = Vec3::new(
            d.offset.x - moved.x,
            d.offset.y - moved.y,
            d.offset.z - moved.z,
        );
        d.at = here;
        // A critically damped spring onto the way: no overshoot, at any frame rate.
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: one frame's seconds, a small number.
        let dt = dt as f32;
        let (x, vx) = drawn_spring(d.offset.x - way.x, d.speed.x, dt);
        let (y, vy) = drawn_spring(d.offset.y - way.y, d.speed.y, dt);
        let (z, vz) = drawn_spring(d.offset.z - way.z, d.speed.z, dt);
        d.offset = Vec3::new(way.x + x, way.y + y, way.z + z);
        d.speed = Vec3::new(vx, vy, vz);
        self.drawn = Some(d);
    }

    /// Whether the body stands where its last tick left it: nothing but a tick has moved it.
    fn at_last_tick(&self, here: Position) -> bool {
        self.ticks[1].is_some_and(|(last, _)| same_place(&here, &last))
    }

    /// How far from where physics has it the body is drawn this frame: its way between ticks
    /// ([`Self::way_offset`]) taken on a short spring, so that a tick carrying it further than the
    /// others is spread over a few frames.
    #[must_use]
    pub fn offset(&self, here: Position, on: bool, space: impl Fn(Position) -> Vec3) -> Vec3 {
        let way = self.way_offset(here, on, &space);
        if way == Vec3::ZERO && !self.at_last_tick(here) {
            return Vec3::ZERO;
        }
        self.drawn.map_or(way, |d| d.offset)
    }

    /// The body's way between ticks: back along its way from the last tick toward the
    /// one before, by the share of a tick still to run, so that it is drawn a tick behind and
    /// moving every frame. Nothing when it is not drawn between ticks (`on`), before two ticks,
    /// when something other than a tick has moved it since the last one (the server placing it;
    /// a turn alone is not a move), or when the last tick carried it further than a step (a
    /// portal, a teleport).
    #[must_use]
    pub fn way_offset(&self, here: Position, on: bool, space: impl Fn(Position) -> Vec3) -> Vec3 {
        let [Some((before, t0)), Some((last, t1))] = self.ticks else {
            return Vec3::ZERO;
        };
        // Only a change of place counts as something other than a tick moving it: the body
        // turned between ticks (to face where the camera looks) is still drawn on its way.
        if !on || !same_place(&here, &last) || t1 <= t0 {
            return Vec3::ZERO;
        }
        let (a, b) = (space(before), space(last));
        let back = Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
        if back.magnitude() > DRAWN_STEP_LIMIT {
            return Vec3::ZERO;
        }
        let to_run = self.tick_left(t0, t1);
        Vec3::new(back.x * to_run, back.y * to_run, back.z * to_run)
    }

    /// The share of the tick from `t0` to `t1` the frame being drawn still has to run.
    fn tick_left(&self, t0: f64, t1: f64) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a share of one tick, between zero and one.
        let to_run = (1.0 - ((self.frame_time - t1) / (t1 - t0)).clamp(0.0, 1.0)) as f32;
        to_run
    }

    /// How far, in degrees clockwise, the body is drawn turned from the way physics has it facing
    /// this frame: its turn between ticks ([`Self::way_turn`]) taken on the drawn body's spring,
    /// so that a tick turning it further than the others is spread over a few frames.
    #[must_use]
    pub fn turn(&self, here: Position, on: bool) -> f32 {
        let way = self.way_turn(here, on);
        if way == 0.0 && !self.turned_by_last_tick(here) {
            return 0.0;
        }
        self.turning.map_or(way, |d| d.offset)
    }

    /// Whether the body faces the way its last tick left it, standing where it left it: nothing
    /// but a tick has turned or moved it.
    fn turned_by_last_tick(&self, here: Position) -> bool {
        self.at_last_tick(here)
            && self.ticks[1].is_some_and(|(last, _)| here.frame.rotation == last.frame.rotation)
    }

    /// Turn the drawn body one frame of `dt` seconds along its spring toward its turn between
    /// ticks ([`Self::way_turn`]), first keeping it facing the way it was drawn while physics
    /// turned the body under it.
    fn step_drawn_turn(&mut self, dt: f64, here: Position, on: bool) {
        let way = self.way_turn(here, on);
        let faces = heading(&here);
        let Some(mut d) = self
            .turning
            .filter(|_| way != 0.0 || self.turned_by_last_tick(here))
        else {
            self.turning = (way != 0.0).then_some(DrawnTurn {
                at: faces,
                offset: way,
                speed: 0.0,
            });
            return;
        };
        // Physics turned the body; the drawn body keeps facing the way it did.
        let turned = turn_between(d.at, faces);
        if turned.abs() > DRAWN_TURN_LIMIT {
            self.turning = None;
            return;
        }
        d.offset -= turned;
        d.at = faces;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: one frame's seconds, a small number.
        let (x, v) = drawn_spring(d.offset - way, d.speed, dt as f32);
        d.offset = way + x;
        d.speed = v;
        self.turning = Some(d);
    }

    /// The body's turn between ticks, in degrees clockwise: back along the turn the last tick made
    /// ([`Self::tick_turn`]), by the share of a tick still to run, so that a body physics turns
    /// is drawn turning every frame, a tick behind, as it is drawn moving. Nothing when it is not
    /// drawn between ticks, before two ticks, when something other than a tick has turned or
    /// moved it since the last one, or when the last tick turned it further than
    /// [`DRAWN_TURN_LIMIT`]. A turn given between the ticks is not the tick's: a body faced where
    /// the camera looks every frame is drawn facing just that way, never turned back by the turn
    /// it was given since the tick before.
    #[must_use]
    pub fn way_turn(&self, here: Position, on: bool) -> f32 {
        let [Some((_, t0)), Some((_, t1))] = self.ticks else {
            return 0.0;
        };
        if !on || !self.turned_by_last_tick(here) || t1 <= t0 {
            return 0.0;
        }
        let back = -self.tick_turn;
        if back.abs() > DRAWN_TURN_LIMIT {
            return 0.0;
        }
        back * self.tick_left(t0, t1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{CellId, Frame, Quat};

    const CELL: CellId = CellId(0xA9B4_0001);
    /// Frames a second, and physics ticks a second.
    const FPS: f64 = 240.0;
    const TICKS: f64 = 30.0;
    /// How far a tick carries the body: a run, about nine metres a second.
    const STEP: f32 = 0.3;

    fn at(x: f32, y: f32, heading: f32) -> Position {
        let mut f = Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY);
        dereth_primitives::frame::set_heading(&mut f, heading);
        Position::new(CELL, f)
    }

    fn space(p: Position) -> Vec3 {
        p.frame.origin
    }

    /// A body physics carries `STEP` north on every tick and turns `turn` degrees, `frames` frames
    /// at `FPS`; `put` puts it somewhere else (by `by` east) between ticks on that frame, and
    /// `placed` has physics place it there on that frame's tick. Each frame, where physics has it
    /// and where it is drawn, and how it is drawn turned.
    fn run(
        frames: usize,
        turn: f32,
        put: Option<(usize, f32)>,
        placed: Option<(usize, f32)>,
    ) -> Vec<(Position, Vec3, f32)> {
        let mut b = BetweenTicks::default();
        let mut here = at(0.0, 0.0, 0.0);
        let mut placements = 0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let every = (FPS / TICKS).round() as usize;
        (0..frames)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let now = 10.0 + i as f64 / FPS;
                if let Some((_, by)) = put.filter(|(n, _)| *n == i) {
                    here.frame.origin.x += by;
                }
                b.before_tick(here);
                let ticked = i % every == 0;
                if ticked {
                    let heading = dereth_primitives::frame::get_heading(&here.frame) + turn;
                    here = at(here.frame.origin.x, here.frame.origin.y + STEP, heading);
                    if let Some((_, by)) = placed.filter(|(n, _)| *n == i) {
                        here.frame.origin.x += by;
                        placements += 1;
                    }
                }
                b.frame(now, ticked, here, placements, true, space);
                let off = b.offset(here, true, space);
                let o = here.frame.origin;
                (
                    here,
                    Vec3::new(o.x + off.x, o.y + off.y, o.z + off.z),
                    b.turn(here, true),
                )
            })
            .collect()
    }

    #[test]
    fn a_body_is_drawn_a_tick_behind_moving_every_frame_and_never_past_where_physics_has_it() {
        let frames = run(240, 0.0, None, None);
        // Two ticks in, it is drawn between them.
        for w in frames[16..].windows(2) {
            let ((_, a, _), (here, b, _)) = (w[0], w[1]);
            assert!(b.y > a.y, "it moves every frame: {a:?} then {b:?}");
            assert!(
                b.y - a.y < STEP * 0.5,
                "by much less than a tick: {}",
                b.y - a.y
            );
            assert!(b.y <= here.frame.origin.y, "never ahead of physics");
            assert!(
                here.frame.origin.y - b.y <= 3.0 * STEP,
                "and never far behind it: {}",
                here.frame.origin.y - b.y
            );
            assert_eq!(b.x, here.frame.origin.x, "on the way physics took");
        }
    }

    #[test]
    fn a_body_turned_by_its_ticks_is_drawn_turning_a_tick_behind_and_never_past_its_heading() {
        let frames = run(240, 3.0, None, None);
        for w in frames[16..].windows(2) {
            let ((a, _, ta), (b, _, tb)) = (w[0], w[1]);
            let drawn = |p: Position, t: f32| dereth_primitives::frame::get_heading(&p.frame) + t;
            let turned = turn_between(drawn(a, ta), drawn(b, tb));
            assert!(turned > 0.0, "it turns every frame: {turned}");
            assert!(turned < 1.5, "by much less than a tick's turn: {turned}");
            assert!(tb <= 0.0, "never turned past where physics has it: {tb}");
        }
    }

    #[test]
    fn a_body_put_somewhere_between_ticks_is_drawn_there_at_once_and_never_slid_back() {
        // Put 3 m east, less than a step, half way between two ticks.
        let put = 100;
        let frames = run(240, 0.0, Some((put, 3.0)), None);
        for (n, (here, drawn, _)) in frames.iter().enumerate().skip(put) {
            assert!(
                (drawn.x - here.frame.origin.x).abs() < 1e-6,
                "frame {n}: drawn {drawn:?}, physics {here:?}"
            );
        }
        assert!(
            frames[put + 24..]
                .iter()
                .any(|(here, drawn, _)| drawn.y < here.frame.origin.y),
            "and drawn between its ticks again two ticks on"
        );
    }

    #[test]
    fn a_body_physics_places_on_a_tick_is_drawn_where_it_was_placed_at_once() {
        let placed = 96;
        let frames = run(240, 0.0, None, Some((placed, 1.0)));
        // Until its next tick, which is drawn between again.
        for (n, (here, drawn, _)) in frames.iter().enumerate().skip(placed).take(8) {
            assert_eq!(
                *drawn, here.frame.origin,
                "frame {n}: a placement is not a step and is not drawn as one"
            );
        }
    }

    /// A tick at `now` that moves the body from `from` to `to`, drawn between ticks (`on`) or not.
    fn tick(b: &mut BetweenTicks, now: f64, from: Position, to: Position, on: bool) {
        b.before_tick(from);
        b.frame(now, true, to, 0, on, space);
    }

    #[test]
    fn ticks_further_apart_than_the_longest_step_are_not_drawn_between() {
        let mut b = BetweenTicks::default();
        let (p0, p1, p2) = (at(0.0, 0.0, 0.0), at(0.0, 0.3, 0.0), at(0.0, 0.6, 0.0));
        tick(&mut b, 1.0, p0, p0, true);
        tick(&mut b, 1.0 + 1.0 / TICKS, p0, p1, true);
        assert_ne!(
            b.way_offset(p1, true, space),
            Vec3::ZERO,
            "ticks a tick apart"
        );
        // Held a second, then a tick.
        let t = 2.0 + 1.0 / TICKS;
        tick(&mut b, t, p1, p2, true);
        b.frame(t + 0.01, false, p2, 0, true, space);
        assert_eq!(b.offset(p2, true, space), Vec3::ZERO);
        assert_eq!(b.way_offset(p2, true, space), Vec3::ZERO);
    }

    #[test]
    fn a_step_further_than_the_step_limit_is_drawn_where_it_lands() {
        let mut b = BetweenTicks::default();
        let (p0, p1) = (at(0.0, 0.0, 0.0), at(0.0, DRAWN_STEP_LIMIT + 0.5, 0.0));
        tick(&mut b, 1.0, p0, p0, true);
        tick(&mut b, 1.0 + 1.0 / TICKS, p0, p1, true);
        assert_eq!(b.offset(p1, true, space), Vec3::ZERO);
    }

    #[test]
    fn not_drawn_between_ticks_the_body_is_drawn_where_physics_has_it() {
        let mut b = BetweenTicks::default();
        let (p0, p1) = (at(0.0, 0.0, 0.0), at(0.0, 0.3, 10.0));
        tick(&mut b, 1.0, p0, p0, false);
        tick(&mut b, 1.0 + 1.0 / TICKS, p0, p1, false);
        b.frame(1.0 + 1.5 / TICKS, false, p1, 0, false, space);
        assert_eq!(b.offset(p1, false, space), Vec3::ZERO);
        assert_eq!(b.turn(p1, false), 0.0);
    }

    #[test]
    fn the_spring_never_carries_the_drawn_body_past_where_it_is_going() {
        for (x, v) in [
            (-1.0f32, 200.0f32),
            (1.0, -200.0),
            (-0.01, 5.0),
            (0.5, -0.1),
            (-0.3, 9.0),
        ] {
            let mut state = (x, v);
            for dt in [1.0 / 240.0, 1.0 / 60.0, 0.05] {
                let (to, _) = drawn_spring(state.0, state.1, dt);
                assert!(
                    to * x >= 0.0,
                    "from {x} at {v} over {dt}: {to} has crossed where it was going"
                );
                state = drawn_spring(state.0, state.1, dt);
            }
        }
    }
}

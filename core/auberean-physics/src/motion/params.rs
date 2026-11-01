//! How a body is told to move to a place or an object, or to turn ([`MovementParameters`]), and
//! what it reports when the move ends ([`MovementReturn`]).

use std::collections::VecDeque;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::{self, Heading, EPS};

/// The id that names no object.
pub const NO_OBJECT: u64 = 0;

/// What a move asks for, as flags.
pub mod flags {
    /// Move toward the target, to the desired distance.
    pub const MOVE_TOWARDS: u32 = 0x1;
    /// Turn to face the target, or to the target heading.
    pub const TURN: u32 = 0x2;
    /// Move away from the target.
    pub const MOVE_AWAY: u32 = 0x4;
    /// Keep following the target after reaching it.
    pub const PERSISTENT: u32 = 0x8;
    /// A persistent move ends when it fails.
    pub const PERSISTENT_FAIL_EXITS: u32 = 0x10;
    /// The target is an object, not a place.
    pub const OBJECT: u32 = 0x20;
    /// Distances are between the bodies' cylinders, not their centers.
    pub const USE_CYLINDERS: u32 = 0x40;
    /// The target is offset from the place or object.
    pub const OFFSET: u32 = 0x100;
    /// The offset is named by an id.
    pub const OFFSET_ID: u32 = 0x200;
    /// Report back once within range.
    pub const CALLBACK_WHEN_IN_RANGE: u32 = 0x400;
}

/// A move or a turn to a place or an object.
// D2-15524, D2-15527, D2-15521
#[derive(Clone, Debug, PartialEq)]
pub struct MovementParameters {
    pub flags: u32,
    /// The fraction of full speed to move at.
    pub desired_velocity: f32,
    /// Farther than this from the target, the move has failed.
    pub fail_distance: f32,
    /// How close to come.
    pub desired_distance: f32,
    pub max_success_distance: f32,
    pub min_success_distance: f32,
    pub target_object: u64,
    // D2-15507
    pub target_position: Position,
    /// Places to pass on the way, in order.
    pub waypoints: VecDeque<Position>,
    /// Where the target is from the place or object, in its frame.
    // D2-15509
    pub offset: Vec3,
    pub offset_id: u32,
    /// The caller's tag for this move.
    // D2-15510
    pub context: u32,
}

impl Default for MovementParameters {
    /// Full speed, no failing distance, a meter's desired and successful distance, 100 m the
    /// least, distances between cylinders, nothing else asked.
    // D2-15520
    fn default() -> Self {
        Self {
            flags: flags::USE_CYLINDERS,
            desired_velocity: 1.0,
            fail_distance: f32::MAX,
            desired_distance: 1.0,
            max_success_distance: 1.0,
            min_success_distance: 100.0,
            target_object: NO_OBJECT,
            target_position: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            waypoints: VecDeque::new(),
            offset: Vec3::ZERO,
            offset_id: 0,
            context: 0,
        }
    }
}

impl MovementParameters {
    fn set_flag(&mut self, flag: u32, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    // D2-15482
    pub fn set_persistent(&mut self, on: bool) {
        self.set_flag(flags::PERSISTENT, on);
    }

    // D2-15483
    pub fn set_persistent_fail_exits(&mut self, on: bool) {
        self.set_flag(flags::PERSISTENT_FAIL_EXITS, on);
    }

    // D2-15484
    pub fn set_move_towards(&mut self, on: bool) {
        self.set_flag(flags::MOVE_TOWARDS, on);
    }

    // D2-15485
    pub fn set_move_away(&mut self, on: bool) {
        self.set_flag(flags::MOVE_AWAY, on);
    }

    // D2-15486
    pub fn set_turn(&mut self, on: bool) {
        self.set_flag(flags::TURN, on);
    }

    // D2-15487
    pub fn set_object(&mut self, on: bool) {
        self.set_flag(flags::OBJECT, on);
    }

    // D2-15488
    pub fn set_use_cylinders(&mut self, on: bool) {
        self.set_flag(flags::USE_CYLINDERS, on);
    }

    // D2-15511
    pub fn set_callback_when_in_range(&mut self, on: bool) {
        self.set_flag(flags::CALLBACK_WHEN_IN_RANGE, on);
    }

    /// Aims the move at an object.
    // D2-15489
    pub fn set_target_object(&mut self, id: u64) {
        self.target_object = id;
        self.flags |= flags::OBJECT;
    }

    /// Aims the move at a place.
    // D2-15514
    pub fn set_target_position(&mut self, p: Position) {
        self.flags &= !flags::OBJECT;
        self.target_position = p;
    }

    /// How close to come; asks for a move toward the target.
    // D2-15490
    pub fn set_desired_distance(&mut self, d: f32) {
        self.desired_distance = d;
        self.flags |= flags::MOVE_TOWARDS;
    }

    /// The farthest that counts as arrived, which is also how close to come; asks for a move toward
    /// the target.
    // D2-15491
    pub fn set_max_success_distance(&mut self, d: f32) {
        self.max_success_distance = d;
        self.desired_distance = d;
        self.flags |= flags::MOVE_TOWARDS;
    }

    /// The nearest that counts as away, which is also how far to go; asks for a move away.
    // D2-15492
    pub fn set_min_success_distance(&mut self, d: f32) {
        self.min_success_distance = d;
        self.desired_distance = d;
        self.flags |= flags::MOVE_AWAY;
    }

    /// The heading to turn to: the target place's; asks for a turn.
    // D2-15493
    pub fn set_target_heading(&mut self, h: Heading) {
        geom::set_heading(&mut self.target_position.frame, h);
        self.flags |= flags::TURN;
    }

    /// The offset from the target; flagged when it is not nothing.
    // D2-15494
    pub fn set_offset(&mut self, v: Vec3) {
        self.offset = v;
        let zero = v.x.abs() < EPS && v.y.abs() < EPS && v.z.abs() < EPS;
        self.set_flag(flags::OFFSET, !zero);
    }

    // D2-15495
    pub fn set_context(&mut self, id: u32) {
        self.context = id;
    }

    // D2-15496
    #[must_use]
    pub const fn moves_towards(&self) -> bool {
        self.flags & flags::MOVE_TOWARDS != 0
    }

    // D2-15497
    #[must_use]
    pub const fn turns(&self) -> bool {
        self.flags & flags::TURN != 0
    }

    // D2-15498
    #[must_use]
    pub const fn moves_away(&self) -> bool {
        self.flags & flags::MOVE_AWAY != 0
    }

    // D2-15499
    #[must_use]
    pub const fn persistent(&self) -> bool {
        self.flags & flags::PERSISTENT != 0
    }

    // D2-15500
    #[must_use]
    pub const fn to_object(&self) -> bool {
        self.flags & flags::OBJECT != 0
    }

    // D2-15501
    #[must_use]
    pub const fn uses_cylinders(&self) -> bool {
        self.flags & flags::USE_CYLINDERS != 0
    }

    // D2-15506
    #[must_use]
    pub const fn target_object(&self) -> u64 {
        self.target_object
    }

    /// The heading to turn to: the one the target place faces.
    // D2-15508
    #[must_use]
    pub fn target_heading(&self) -> Heading {
        geom::frame_heading(&self.target_position.frame)
    }

    /// Takes the next waypoint.
    // D2-15518
    pub fn pop_waypoint(&mut self) -> Option<Position> {
        self.waypoints.pop_front()
    }

    // D2-15522
    pub fn set_waypoints(&mut self, list: VecDeque<Position>) {
        self.waypoints = list;
    }

    /// Whether waypoints remain.
    // D2-15515
    #[must_use]
    pub fn has_waypoints(&self) -> bool {
        !self.waypoints.is_empty()
    }
}

/// Why a move ended.
pub mod errors {
    /// Nothing set yet.
    pub const UNSET: u32 = 0x47;
    /// Another move took its place, or it was interrupted.
    pub const CANCELLED: u32 = 0x36;
    /// Its target object is gone.
    pub const TARGET_GONE: u32 = 0x37;
}

/// What a body reports when its move ends: the move, why it ended, where the target was and how
/// big it was.
// D2-15525, D2-15528, D2-07058
#[derive(Clone, Debug, PartialEq)]
pub struct MovementReturn {
    pub params: MovementParameters,
    pub error: u32,
    pub status: u32,
    pub position: Position,
    pub object_radius: f32,
    pub object_height: f32,
    pub persistent: bool,
}

impl Default for MovementReturn {
    // D2-15523
    fn default() -> Self {
        Self {
            params: MovementParameters::default(),
            error: errors::UNSET,
            status: 0,
            position: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            object_radius: 0.0,
            object_height: 0.0,
            persistent: false,
        }
    }
}

impl MovementReturn {
    // D2-15512
    pub fn set_error(&mut self, e: u32) {
        self.error = e;
    }

    // D2-15516
    pub fn set_position(&mut self, p: Position) {
        self.position = p;
    }

    // D2-10473
    pub fn set_object_radius_and_height(&mut self, radius: f32, height: f32) {
        self.object_radius = radius;
        self.object_height = height;
    }

    // D2-15526
    pub fn set_movement_parameters(&mut self, p: &MovementParameters) {
        self.params = p.clone();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the flags each setter raises, held to the client's)
    use super::*;

    #[test]
    fn setters_raise_the_flags_they_imply() {
        let mut p = MovementParameters::default();
        assert_eq!(p.flags, flags::USE_CYLINDERS);
        p.set_min_success_distance(4.0);
        assert!(p.moves_away() && !p.moves_towards());
        assert_eq!(p.desired_distance, 4.0);
        p.set_max_success_distance(2.0);
        assert!(p.moves_towards() && p.desired_distance == 2.0);
        p.set_target_object(7);
        assert!(p.to_object());
        p.set_target_position(p.target_position);
        assert!(!p.to_object());
        p.set_offset(Vec3::new(0.0001, 0.0, 0.0));
        assert_eq!(p.flags & flags::OFFSET, 0);
        p.set_offset(Vec3::new(0.0, 1.0, 0.0));
        assert_ne!(p.flags & flags::OFFSET, 0);
        p.set_target_heading(Heading::new(90.0));
        assert!(p.turns());
        assert!((p.target_heading().degrees() - 90.0).abs() < 1e-3);
    }
}

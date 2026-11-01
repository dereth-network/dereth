//! Moving to a place or an object, and turning to a heading or to face an object, as the server
//! asks: each tick the move-to manager steers the body (the motion to ask for and the heading to
//! turn to) and watches it make progress.
//!
//! A move starts toward its target, or the first of its waypoints, at the asked fraction of full
//! speed. The body walks only while it faces within a quarter turn of where it is going and turns
//! toward it otherwise. When it arrives and has turned (if asked), the next waypoint starts. The
//! client never ends a move on arriving: the server does. A move ends early when another takes its
//! place, when it is interrupted, or when its target object leaves.

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::{self, Heading, EPS};
use crate::motion::params::{errors, MovementParameters, MovementReturn};
use crate::path::{Headway, SimplePath, Step, Turner};

/// What the move-to manager knows of a target object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetBody {
    pub position: Position,
    pub radius: f32,
    pub height: f32,
}

/// Where a target object stands, as the body's target tracking last heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TargetStatus {
    /// Nothing heard yet.
    #[default]
    Undefined,
    /// In the world where last heard.
    Ok,
    /// Gone (left the world, teleported, picked up...): any other status.
    Gone(u32),
}

impl TargetStatus {
    /// From the status's number.
    #[must_use]
    pub const fn from_u32(n: u32) -> Self {
        match n {
            0 => Self::Undefined,
            1 => Self::Ok,
            other => Self::Gone(other),
        }
    }
}

/// What a move asks of the body under it: its place and size, the objects around it, and its
/// motion interpreter (the manager is handed them each call rather than holding them).
// D2-07046, D2-07050
pub trait MoveToHost {
    fn position(&self) -> Position;
    fn heading(&self) -> Heading;
    fn radius(&self) -> f32;
    fn height(&self) -> f32;
    /// A target object at the top of its containment, if it is in the world.
    // D2-07047
    fn target(&self, id: u64) -> Option<TargetBody>;
    /// Starts tracking a target object, with how close and how often.
    fn set_target(&mut self, id: u64, radius: f32, quantum: f32);
    fn clear_target(&mut self);
    fn affected_by_gravity(&self) -> bool;
    fn default_hover_height(&self) -> f32;
    /// The velocity in the body's frame that asking for `motion` gives.
    fn local_desired(&self, motion: Vec3) -> Vec3;
    /// Asks for motion.
    fn do_motion(&mut self, motion: Vec3);
    fn stop_turn_to(&mut self);
    /// The move ended.
    fn move_to_done(&mut self, result: MovementReturn);
    /// Faces the body at once to a heading and stops it turning.
    fn snap_heading(&mut self, h: Heading);
    /// Frees the body of the limits interpolation puts on its motion.
    fn release_rails(&mut self);
}

/// What the move-to manager steers each tick: the motion to ask for, the heading to turn to, and
/// whether that turn is made at once.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Steer {
    pub motion: Vec3,
    pub heading: Heading,
    pub instant: bool,
}

/// A body's move to a place or object. Dropping it drops its steering pieces.
// D2-07057, D2-07059
#[derive(Clone, Debug, PartialEq)]
pub struct Mover {
    pub path: Option<SimplePath>,
    pub progress: Headway,
    pub turn: Option<Turner>,
    pub params: MovementParameters,
    pub target_position: Position,
    pub target_height: f32,
    pub target_radius: f32,
    pub target_status: TargetStatus,
    pub moving_to: bool,
    pub persistent_target_reached: bool,
    pub last_update_position: Position,
    pub last_update_time: f64,
}

fn origin() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

impl Default for Mover {
    // D2-07056
    fn default() -> Self {
        Self {
            path: None,
            progress: Headway::new(0.0),
            turn: None,
            params: MovementParameters::default(),
            target_position: origin(),
            target_height: 0.0,
            target_radius: 0.0,
            target_status: TargetStatus::Undefined,
            moving_to: false,
            persistent_target_reached: false,
            last_update_position: origin(),
            last_update_time: 0.0,
        }
    }
}

impl Mover {
    /// Drops the steering pieces.
    // D2-07045
    pub fn destroy(&mut self) {
        self.path = None;
        self.turn = None;
    }

    // D2-07049
    pub fn update_target_info(
        &mut self,
        position: Position,
        radius: f32,
        height: f32,
        status: TargetStatus,
    ) {
        self.target_position = position;
        self.target_radius = radius;
        self.target_height = height;
        self.target_status = status;
    }

    /// Whether a move is under way.
    #[must_use]
    pub const fn is_moving_to(&self) -> bool {
        self.moving_to
    }

    /// Whether a persistent move has reached its target.
    #[must_use]
    pub const fn reached_persistent_target(&self) -> bool {
        self.persistent_target_reached
    }

    /// Starts the walking part of a move: the mover's size and the move's distances, and for a
    /// place, the next waypoint (or the place) moved by the offset.
    // D2-07051
    pub fn start_move_to(&mut self, host: &mut dyn MoveToHost, now: f64) -> bool {
        let path = self.path.get_or_insert_with(SimplePath::default);
        self.progress.reset(now);
        let p = &mut self.params;
        path.set_parameters(
            host.radius(),
            host.height(),
            p.desired_distance,
            p.max_success_distance,
            p.fail_distance,
            p.desired_velocity,
        );
        path.set_always_turn(p.turns());
        path.set_use_cylinders(p.uses_cylinders());
        if !p.to_object() {
            let mut dest = if p.has_waypoints() {
                p.pop_waypoint().unwrap_or(p.target_position)
            } else {
                p.target_position
            };
            geom::add_local_offset(&mut dest, p.offset);
            path.set_destination(dest, 0.0, 0.0);
        }
        true
    }

    /// Starts the turning part of a move: to the asked heading, unless the target is an object.
    // D2-07052
    pub fn start_turn_to(&mut self) -> bool {
        let turn = self.turn.get_or_insert_with(Turner::default);
        if !self.params.to_object() {
            turn.set_desired_heading(self.params.target_heading());
        }
        true
    }

    /// Ends the move with a reason, telling the body.
    // D2-07060
    pub fn stop_with(&mut self, host: &mut dyn MoveToHost, now: f64, error: u32) {
        let mut r = MovementReturn::default();
        r.set_error(error);
        self.stop(host, now, r);
    }

    /// Ends the move: the body stops asking for motion and turning, and hears how it ended.
    // D2-07053
    pub fn stop(&mut self, host: &mut dyn MoveToHost, now: f64, mut result: MovementReturn) {
        if !self.moving_to {
            return;
        }
        self.moving_to = false;
        self.persistent_target_reached = false;
        self.progress.reset(now);
        host.clear_target();
        host.do_motion(Vec3::ZERO);
        host.stop_turn_to();
        result.set_movement_parameters(&self.params);
        result.set_position(self.target_position);
        result.set_object_radius_and_height(self.target_radius, self.target_height);
        host.move_to_done(result);
        self.params.set_context(0);
        self.target_status = TargetStatus::Undefined;
        self.target_radius = 0.0;
        self.target_height = 0.0;
    }

    /// Ends a move under way as cancelled.
    // D2-07061
    pub fn interrupt(&mut self, host: &mut dyn MoveToHost, now: f64) {
        if self.moving_to {
            self.stop_with(host, now, errors::CANCELLED);
        }
    }

    /// A body that hovers rather than falls goes to its hover height over a place.
    // D2-07054
    pub fn translate_for_hovering(host: &dyn MoveToHost, params: &mut MovementParameters) {
        if !params.to_object() && !host.affected_by_gravity() {
            let mut v = params.offset;
            v.z += host.default_hover_height();
            params.set_offset(v);
        }
    }

    /// A new move: it replaces any under way, starts tracking its target, and starts walking and
    /// turning as asked; one asking for neither ends at once. `planned` moves need waypoints.
    // D2-07062
    pub fn move_to_event(
        &mut self,
        host: &mut dyn MoveToHost,
        now: f64,
        params: &MovementParameters,
        planned: bool,
    ) -> bool {
        if planned && !params.has_waypoints() {
            return false;
        }
        if self.moving_to {
            self.stop_with(host, now, errors::CANCELLED);
        }
        self.params = params.clone();
        Self::translate_for_hovering(host, &mut self.params);
        self.moving_to = true;
        self.target_status = TargetStatus::Undefined;
        self.persistent_target_reached = false;
        host.set_target(self.params.target_object(), 0.5, 1.0);
        if !params.moves_towards() && !params.turns() && !params.moves_away() {
            self.stop_with(host, now, 0);
            return true;
        }
        if params.moves_towards() && !self.start_move_to(host, now) {
            return false;
        }
        if params.turns() && !self.start_turn_to() {
            return false;
        }
        true
    }

    /// One tick's walking: notes the progress made since the last tick toward the target, follows
    /// a target object, and steers. Whether the body has arrived (or had no walking to do).
    // D2-07063
    pub fn update_move_to(
        &mut self,
        host: &mut dyn MoveToHost,
        now: f64,
        dt: f32,
        steer: &mut Steer,
    ) -> bool {
        if !self.params.moves_towards() {
            return true;
        }
        if self.path.is_none() {
            return false;
        }
        host.release_rails();
        if self.last_update_position.cell.0 != 0 {
            let here = host.position();
            let goal = if self.params.to_object() {
                host.target(self.params.target_object())
                    .map_or(self.target_position, |t| t.position)
            } else {
                self.path
                    .as_ref()
                    .map_or(self.target_position, |p| p.destination)
            };
            let mut dir = geom::offset(&here, &goal);
            let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
            dir = if len < EPS {
                Vec3::ZERO
            } else {
                let k = 1.0 / len;
                Vec3::new(dir.x * k, dir.y * k, dir.z * k)
            };
            let moved = geom::offset(&self.last_update_position, &here);
            let along = (moved.x * dir.x + moved.y * dir.y + moved.z * dir.z).max(0.0);
            if dt > 0.0 {
                self.progress.push(now, along / dt);
            }
        }
        self.last_update_position = host.position();
        self.last_update_time = now;
        if self.params.to_object() {
            match host.target(self.params.target_object()) {
                Some(t) => {
                    self.target_position = t.position;
                    self.target_radius = t.radius;
                    self.target_height = t.height;
                }
                None => match self.target_status {
                    TargetStatus::Ok => {}
                    TargetStatus::Undefined => return false,
                    TargetStatus::Gone(_) => {
                        self.stop_with(host, now, errors::TARGET_GONE);
                        return false;
                    }
                },
            }
            let mut dest = self.target_position;
            geom::add_local_offset(&mut dest, self.params.offset);
            if let Some(path) = self.path.as_mut() {
                path.set_destination(dest, self.target_radius, self.target_height);
            }
        }
        let Some(path) = self.path.as_ref() else {
            return false;
        };
        let (step, motion, heading) = path.update(&host.position());
        steer.motion = motion;
        steer.heading = heading;
        steer.instant = false;
        match step {
            Step::Continue => {
                let v = host.local_desired(motion);
                self.progress
                    .check_progress(now, (v.x * v.x + v.y * v.y + v.z * v.z).sqrt());
                false
            }
            Step::Success => true,
            Step::Failure => false,
        }
    }

    /// One tick's turning: toward a target object, or the asked heading. Whether the turn is done
    /// (or there was none to make).
    // D2-07064
    pub fn update_turn_to(
        &mut self,
        host: &mut dyn MoveToHost,
        now: f64,
        steer: &mut Steer,
    ) -> bool {
        if !self.params.turns() {
            return true;
        }
        if self.turn.is_none() {
            return false;
        }
        if self.params.to_object() {
            match host.target(self.params.target_object()) {
                Some(t) => {
                    self.target_position = t.position;
                    self.target_height = t.height;
                    self.target_radius = t.radius;
                }
                None => match self.target_status {
                    TargetStatus::Ok => {
                        self.target_radius = 1.0;
                        self.target_height = 1.0;
                    }
                    TargetStatus::Undefined => return false,
                    TargetStatus::Gone(_) => {
                        self.stop_with(host, now, errors::TARGET_GONE);
                        return false;
                    }
                },
            }
            let here = host.position();
            let h = if geom::distance(&here, &self.target_position) >= EPS {
                geom::heading_to(&here, &self.target_position)
            } else {
                host.heading()
            };
            if let Some(turn) = self.turn.as_mut() {
                turn.set_desired_heading(h);
            }
        }
        let Some(turn) = self.turn.as_ref() else {
            return false;
        };
        let (step, h) = turn.update(host.heading());
        steer.heading = h;
        steer.instant = false;
        match step {
            Step::Continue => false,
            Step::Success => true,
            Step::Failure => {
                if !self.params.persistent() {
                    host.snap_heading(h);
                }
                true
            }
        }
    }

    /// One tick of a move: walking, then turning; when both are done and waypoints remain, the
    /// next starts.
    // D2-07065
    pub fn update(&mut self, host: &mut dyn MoveToHost, now: f64, dt: f32, steer: &mut Steer) {
        if self.moving_to
            && self.update_move_to(host, now, dt, steer)
            && self.update_turn_to(host, now, steer)
            && self.params.has_waypoints()
        {
            self.start_move_to(host, now);
        }
    }
}

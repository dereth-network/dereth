//! `PositionManager` — interpolation, sticky and constraint.
//!
//! The three sub-managers are all optional; the position manager forwards to whichever exist and
//! is called from the internal position update.

use dereth_primitives::{Frame, ObjectId, Position, Quat, Vec3};

use crate::globals;
use crate::math::{self, V3};

/// `InterpolationNode`'s discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Position = 1,
    Stop = 2,
    Velocity = 3,
}

/// One queued node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterpolationNode {
    pub kind: NodeKind,
    pub pos: Position,
    pub velocity: Vec3,
}

/// `InterpolationManager`.
#[derive(Debug, Clone)]
pub struct InterpolationManager {
    pub position_queue: Vec<InterpolationNode>,
    pub keep_heading: bool,
    pub frame_counter: u32,
    pub original_distance: f32,
    pub progress_quantum: f64,
    pub node_fail_counter: u32,
    pub blipto_position: Option<Position>,
    /// The client's own "use adjusted speed" flag, which the interpolation pass tests to choose
    /// between the adjusted maximum speed and the plain one. The
    /// client initialises it to **1** and nothing writes it, which ACE spells
    /// `MotionInterp.UseAdjustedSpeed = true`. It is the flag the world's internal position
    /// update passes to [`dereth_primitives::MotionSource::motion_max_speed`].
    pub use_adjusted_speed: bool,
    /// The motion interpreter's max speed, refreshed from the object's own `MotionSource` before
    /// every [`Self::adjust_offset`] exactly as the client's motion-interpreter lookup is. `None` is the
    /// `NULL` arm, which is what drives the `7.5 m/s` fallback.
    pub max_speed: Option<f32>,
}

impl Default for InterpolationManager {
    fn default() -> Self {
        Self {
            position_queue: Vec::new(),
            keep_heading: false,
            frame_counter: 0,
            original_distance: LARGE_DISTANCE,
            progress_quantum: 0.0,
            node_fail_counter: 0,
            blipto_position: None,
            use_adjusted_speed: true,
            max_speed: None,
        }
    }
}

/// The queue length to which interpolation trims its pending nodes.
pub const MAX_QUEUE: usize = 20;
/// The "close enough, nothing to do" distance.
pub const CLOSE_ENOUGH: f32 = 0.05;
/// The fallback interpolation speed when there is no motion interpreter.
pub const FALLBACK_SPEED: f32 = 7.5;
/// `original_distance`'s "no node has ever been walked" seed — the float loaded by the
/// constructor and restored whenever the queue empties.
/// ACE spells it `InterpolationManager.LargeDistance`.
///
/// **It is not zero, and the difference is the whole walk.** `adjust_offset`'s progress test on
/// the fifth sub-step is `original_distance` minus the distance to the node; seeded with `0.0`
/// that is negative on the first window, so every node would be declared a failure after five
/// sub-steps — at 7.5 m/s and a 1/30 s quantum, 1.25 m into the walk.
pub const LARGE_DISTANCE: f32 = 999_999.0;
/// The number of consecutive failed nodes that arms the blip path.
pub const NODE_FAIL_LIMIT: u32 = 4;

/// What `InterpolateTo` decided to do, so the caller (and a test) can see which arm was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpolateDecision {
    /// The target was further than the autonomy blip distance: the node is queued **and** the
    /// fail counter is armed, so `UseTime` will teleport rather than interpolate.
    Blip,
    /// The target is within `0.05` m: nothing to do, and interpolation stops.
    AlreadyThere,
    /// A node was queued normally.
    Queued,
}

/// What asked the caller to do this sub-step.
///
/// The client's own position-manager time step calls the interpolation and simple-position
/// operations on the object to which it holds a back-pointer; this
/// manager holds no world, so the two are returned to the world's position time step
/// and run there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterpolationStep {
    /// Nothing for the caller. A **position** node is walked by the position step, not here: the
    /// node-kind test sends `NodeKind::Position` straight to the end.
    Idle,
    /// A velocity node: `set_velocity(node.velocity, 1)`, then `NodeCompleted(1)`.
    /// The node has already been taken off this queue.
    SetVelocity(Vec3),
    /// The blip: `SetPositionSimple(pos, 1)`. On success the caller
    /// applies `velocity` when it is present and then stops interpolating;
    /// on failure it does **neither** and the queue
    /// is retried next sub-step, which is what the client's early return does.
    Blip {
        /// Where the body is put.
        pos: Position,
        /// The tail node's velocity, only for the arm that walked the queue back to an earlier
        /// position node.
        velocity: Option<Vec3>,
    },
}

impl InterpolationManager {
    /// Begin interpolating to a position.
    ///
    /// `blip_distance` is (outdoors 100 m, indoors 20 m,
    /// 25 m for the player).
    pub fn interpolate_to(
        &mut self,
        current: &Position,
        target: &Position,
        keep_heading: bool,
        blip_distance: f32,
    ) -> InterpolateDecision {
        // 1. The reference is the tail node's position when the tail is a position node.
        let reference = match self.position_queue.last() {
            Some(n) if n.kind == NodeKind::Position => n.pos,
            _ => *current,
        };

        // 2. Further than the blip distance: queue and arm the blip path.
        if math::distance(&reference, target) > blip_distance {
            self.position_queue.push(InterpolationNode {
                kind: NodeKind::Position,
                pos: *target,
                velocity: Vec3::ZERO,
            });
            self.node_fail_counter = NODE_FAIL_LIMIT;
            return InterpolateDecision::Blip;
        }

        // 3. Close enough that nothing needs doing.
        if math::distance(current, target) < CLOSE_ENOUGH {
            self.stop_interpolating();
            return InterpolateDecision::AlreadyThere;
        }

        // 4. Collapse redundant tail nodes.
        while let Some(n) = self.position_queue.last() {
            if n.kind == NodeKind::Position && math::distance(&n.pos, target) < CLOSE_ENOUGH {
                self.position_queue.pop();
            } else {
                break;
            }
        }

        // 5. Trim from the head to fewer than MAX_QUEUE nodes.
        while self.position_queue.len() >= MAX_QUEUE {
            self.position_queue.remove(0);
        }

        // 6. Push, substituting the current heading when keep_heading is set.
        let mut pos = *target;
        if keep_heading {
            pos.frame.rotation = current.frame.rotation;
        }
        self.keep_heading = keep_heading;
        self.position_queue.push(InterpolationNode {
            kind: NodeKind::Position,
            pos,
            velocity: Vec3::ZERO,
        });
        InterpolateDecision::Queued
    }

    /// Stop interpolating.
    ///
    /// The tail:
    ///
    /// ```text
    ///   original_distance := 999999.0
    ///   frame_counter     := 0
    ///   progress_quantum  := 0
    ///   node_fail_counter := 0
    ///   original_distance := 999999.0
    /// ```
    ///
    /// The last two lines matter: the fail counter is cleared here and nowhere else on the
    /// success path, and `original_distance` is reset to [`LARGE_DISTANCE`], not to `0.0`.
    pub fn stop_interpolating(&mut self) {
        self.position_queue.clear();
        self.frame_counter = 0;
        self.progress_quantum = 0.0;
        self.node_fail_counter = 0;
        self.original_distance = LARGE_DISTANCE;
    }

    /// Whether a node is still queued.
    #[must_use]
    pub fn is_interpolating(&self) -> bool {
        !self.position_queue.is_empty()
    }

    /// The per-step motion toward the head
    /// node.
    ///
    /// `in_contact` is `transient_state & CONTACT_TS`: the client interpolates only while the
    /// object is touching the ground.
    pub fn adjust_offset(
        &mut self,
        current: &Position,
        in_contact: bool,
        quantum: f64,
        out: &mut Frame,
        sticky: bool,
    ) {
        let Some(head) = self.position_queue.first().copied() else {
            return;
        };
        if !in_contact {
            return;
        }
        if head.kind != NodeKind::Position {
            return;
        }
        let d = math::distance(current, &head.pos);
        if d < CLOSE_ENOUGH {
            self.node_completed(current, true);
            return;
        }

        let mut speed = 0.0_f32;
        if let Some(s) = self.max_speed {
            speed = s * 2.0;
        }
        if speed < globals::EPSILON {
            speed = FALLBACK_SPEED;
        }

        self.frame_counter += 1;
        self.progress_quantum += quantum;
        if self.frame_counter > 4 {
            let progress = self.original_distance - d;
            #[allow(clippy::cast_possible_truncation)]
            let rate = if self.progress_quantum > 0.0 {
                (f64::from(progress) / self.progress_quantum) as f32 / speed
            } else {
                0.0
            };
            if !sticky && (progress < globals::EPSILON || rate < 0.3) {
                if d >= 0.2 {
                    self.node_fail_counter += 1;
                    self.node_completed(current, false);
                } else {
                    self.node_completed(current, true);
                }
                return;
            }
            self.frame_counter = 0;
            self.progress_quantum = 0.0;
            self.original_distance = d;
        }

        // The local-space offset frame to the target,
        // rotation included. See [`subtract2`].
        let mut delta = subtract2(&head.pos, current);
        #[allow(clippy::cast_possible_truncation)]
        let step = speed * quantum as f32;
        let mag = delta.origin.mag2().sqrt();
        if mag < CLOSE_ENOUGH {
            // NodeCompleted(1) and then the arm **falls through** to the scale and
            // the write. It is not a `return`; the client really does apply the last sliver of
            // the node it has just completed.
            self.node_completed(current, true);
        }
        if step < mag {
            delta.origin = delta.origin.mul(step / mag);
        }
        if self.keep_heading {
            // Set `delta` to a pure zero-degree yaw, which is
            // the identity quaternion, so `combine` leaves the body's own heading alone.
            delta.rotation = Quat::IDENTITY;
        }
        *out = delta;
    }

    /// One interpolation node completed.
    ///
    /// ```text
    /// frame_counter = 0; progress_quantum = 0;
    /// pop the head node;
    /// if (new head != NULL) {
    ///     if (new head->kind == Position)
    ///         original_distance = distance(obj->position, new head->pos);
    ///     else if (!success) blipto_position = old head->pos;
    /// } else {
    ///     original_distance = 999999.0;
    ///     if (success) StopInterpolating();
    ///     else blipto_position = old head->pos;
    /// }
    /// ```
    ///
    /// Popping the node, zeroing `original_distance` and clearing `node_fail_counter` on success
    /// would be wrong on all three counts: the fail counter is cleared only by `StopInterpolating`,
    /// `original_distance` is re-seeded from the *next* node (or to [`LARGE_DISTANCE`]), and
    /// `blipto_position` is where a failed node is remembered.
    fn node_completed(&mut self, current: &Position, success: bool) {
        self.frame_counter = 0;
        self.progress_quantum = 0.0;
        let head = self.position_queue.first().copied();
        if let Some(next) = self.position_queue.get(1).copied() {
            if next.kind == NodeKind::Position {
                self.original_distance = math::distance(current, &next.pos);
            } else if !success {
                let Some(head) = head else { return };
                self.blipto_position = Some(head.pos);
            }
        } else {
            self.original_distance = LARGE_DISTANCE;
            if success {
                self.stop_interpolating();
            } else {
                let Some(head) = head else { return };
                self.blipto_position = Some(head.pos);
            }
        }
        if !self.position_queue.is_empty() {
            self.position_queue.remove(0);
        }
    }

    /// The sub-step half that is **not** the walk.
    ///
    /// ```text
    /// if (obj == NULL) return;
    /// if (node_fail_counter > 3 || head == NULL) {
    ///     if (node_fail_counter <= 0) return;
    ///     SetPositionSimple(target, 1); StopInterpolating();
    /// }
    /// switch (head->kind) {
    ///     case Velocity: set_velocity(head->velocity, 1); NodeCompleted(1); break;
    ///     case Stop:     NodeCompleted(1); break;
    ///     case Position: break;                             ; adjust_offset owns it
    /// }
    /// ```
    ///
    /// `target` is the tail node's position, or — when the tail is a stop/velocity node — the last
    /// *position* node before it with the tail's velocity applied after the move, or
    /// `blipto_position`.
    pub fn use_time(&mut self, current: &Position) -> InterpolationStep {
        if self.node_fail_counter >= NODE_FAIL_LIMIT || self.position_queue.is_empty() {
            if self.node_fail_counter == 0 {
                return InterpolationStep::Idle;
            }
            let blip = |pos: Option<Position>| {
                pos.map_or(InterpolationStep::Idle, |pos| InterpolationStep::Blip {
                    pos,
                    velocity: None,
                })
            };
            let Some(tail) = self.position_queue.last().copied() else {
                return blip(self.blipto_position);
            };
            if tail.kind == NodeKind::Position {
                return InterpolationStep::Blip {
                    pos: tail.pos,
                    velocity: None,
                };
            }
            return self
                .position_queue
                .iter()
                .rev()
                .find(|n| n.kind == NodeKind::Position)
                .map_or_else(
                    || blip(self.blipto_position),
                    |n| InterpolationStep::Blip {
                        pos: n.pos,
                        velocity: Some(tail.velocity),
                    },
                );
        }
        let head = self.position_queue[0];
        match head.kind {
            NodeKind::Velocity => {
                self.node_completed(current, true);
                InterpolationStep::SetVelocity(head.velocity)
            }
            NodeKind::Stop => {
                self.node_completed(current, true);
                InterpolationStep::Idle
            }
            NodeKind::Position => InterpolationStep::Idle,
        }
    }

    /// Return the correction manager's decision: when the fail counter has reached
    /// its limit the object **snaps** instead of interpolating. Returns the position to snap to.
    #[must_use]
    pub fn snap_target(&self) -> Option<Position> {
        if self.node_fail_counter < NODE_FAIL_LIMIT {
            return None;
        }
        // The tail node's position, or the last *position* node before a stop/velocity tail.
        self.position_queue
            .iter()
            .rev()
            .find(|n| n.kind == NodeKind::Position)
            .map(|n| n.pos)
            .or(self.blipto_position)
    }
}

/// `target` expressed as an offset frame in `current`'s own
/// local frame, so combining `current.frame` with `delta` produces `target` again.
///
/// Two halves:
///
/// * the **origin** is transformed into the current frame, which crosses landblocks correctly;
/// * the **rotation** is the relative quaternion, whose terms are exactly
///   `conj(current.q) * target.q`, normalized with restore-on-invalid assignment rather than a
///   bare store.
///
/// The rotation is not `Quat::IDENTITY`: that is a zero-degree heading — the `keep_heading`
/// answer — and returned for every node it would leave a body walked onto a node with whatever
/// heading it was created with.
#[must_use]
fn subtract2(target: &Position, current: &Position) -> Frame {
    let mut out = Frame::new(
        math::localtolocal(current, target, Vec3::ZERO),
        Quat::IDENTITY,
    );
    let (a, b) = (target.frame.rotation, current.frame.rotation);
    math::set_rotate(
        &mut out,
        a.w * b.w + a.x * b.x + a.y * b.y + a.z * b.z,
        a.x * b.w - a.w * b.x - a.z * b.y + a.y * b.z,
        a.y * b.w - a.w * b.y - a.x * b.z + a.z * b.x,
        a.z * b.w - a.w * b.z - a.y * b.x + a.x * b.y,
    );
    out
}

/// `StickyManager`.
#[derive(Debug, Clone, Default)]
pub struct StickyManager {
    pub target_id: Option<ObjectId>,
    pub target_radius: f32,
    pub target_position: Option<Position>,
    pub initialized: bool,
    pub sticky_timeout_time: f64,
}

impl StickyManager {
    /// Pull the object toward the target position,
    /// stopping at the target radius.
    pub fn adjust_offset(&self, current: &Position, out: &mut Frame) {
        let Some(target) = self.target_position else {
            return;
        };
        let d = math::get_offset(current, &target);
        let dist = d.mag2().sqrt();
        if dist <= self.target_radius {
            return;
        }
        let pull = d.mul((dist - self.target_radius) / dist);
        out.origin = out.origin.add(math::globaltolocal(
            &Frame::new(Vec3::ZERO, current.frame.rotation),
            pull,
        ));
    }

    /// Stick the position manager to a target.
    pub fn stick_to(&mut self, id: ObjectId, radius: f32) {
        self.target_id = Some(id);
        self.target_radius = radius;
        self.initialized = true;
    }

    /// Unstick from the object.
    pub fn unstick(&mut self) {
        *self = Self::default();
    }
}

/// `ConstraintManager`.
#[derive(Debug, Clone, Default)]
pub struct ConstraintManager {
    pub is_constrained: bool,
    pub constraint_pos_offset: f32,
    pub constraint_pos: Option<Position>,
    /// Outdoors `10`, indoors `5`.
    pub constraint_distance_start: f32,
    /// Outdoors `50`, indoors `20`.
    pub constraint_distance_max: f32,
}

impl ConstraintManager {
    /// Constrain to a position.
    pub fn constrain_to(&mut self, pos: Position, start: f32, max: f32) {
        self.is_constrained = true;
        self.constraint_pos = Some(pos);
        self.constraint_distance_start = start;
        self.constraint_distance_max = max;
    }

    /// Whether the object is fully constrained.
    #[must_use]
    pub fn is_fully_constrained(&self, current: &Position) -> bool {
        let Some(p) = self.constraint_pos else {
            return false;
        };
        self.is_constrained && math::distance(current, &p) >= self.constraint_distance_max
    }

    /// The constraint manager's offset adjustment — beyond the start distance the object is pulled
    /// back, linearly up to the maximum distance where it is held completely.
    pub fn adjust_offset(&self, current: &Position, out: &mut Frame) {
        if !self.is_constrained {
            return;
        }
        let Some(target) = self.constraint_pos else {
            return;
        };
        let d = math::get_offset(current, &target);
        let dist = d.mag2().sqrt();
        if dist <= self.constraint_distance_start {
            return;
        }
        let span = self.constraint_distance_max - self.constraint_distance_start;
        let frac = if span > 0.0 {
            ((dist - self.constraint_distance_start) / span).min(1.0)
        } else {
            1.0
        };
        let pull = d.mul(frac * (dist - self.constraint_distance_start) / dist);
        out.origin = out.origin.add(math::globaltolocal(
            &Frame::new(Vec3::ZERO, current.frame.rotation),
            pull,
        ));
    }

    pub fn unconstrain(&mut self) {
        *self = Self::default();
    }
}

/// `PositionManager` — a thin holder for the three optional sub-managers.
#[derive(Debug, Clone, Default)]
pub struct PositionManager {
    pub interpolation: Option<InterpolationManager>,
    pub sticky: Option<StickyManager>,
    pub constraint: Option<ConstraintManager>,
}

impl PositionManager {
    /// Adjust the frame offset.
    pub fn adjust_offset(
        &mut self,
        current: &Position,
        in_contact: bool,
        quantum: f64,
        out: &mut Frame,
    ) {
        let sticky_active = self.sticky.as_ref().is_some_and(|s| s.target_id.is_some());
        if let Some(i) = self.interpolation.as_mut() {
            i.adjust_offset(current, in_contact, quantum, out, sticky_active);
        }
        if let Some(s) = self.sticky.as_ref() {
            s.adjust_offset(current, out);
        }
        if let Some(c) = self.constraint.as_ref() {
            c.adjust_offset(current, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::CellId;

    // Oracle: the recovered physics-object behavior section "The position
    // manager", which reproduces the interpolation adjustment and time-step behavior, plus the
    // manager-distance table in the design contract.

    fn at(x: f32, y: f32, z: f32) -> Position {
        Position::new(
            CellId(0xA9B4_0001),
            Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
        )
    }

    #[test]
    fn the_documented_distances_are_the_ones_the_globals_carry() {
        // The position manager's documented distances: constraint, autonomy blip and active radius.
        assert_eq!(globals::CONSTRAINT_START_OUTDOORS, 10.0);
        assert_eq!(globals::CONSTRAINT_MAX_OUTDOORS, 50.0);
        assert_eq!(globals::CONSTRAINT_START_INDOORS, 5.0);
        assert_eq!(globals::CONSTRAINT_MAX_INDOORS, 20.0);
        assert_eq!(globals::AUTONOMY_BLIP_OUTDOORS, 100.0);
        assert_eq!(globals::AUTONOMY_BLIP_INDOORS, 20.0);
        assert_eq!(globals::AUTONOMY_BLIP_INDOORS_PLAYER, 25.0);
        assert_eq!(
            globals::ACTIVE_RADIUS,
            96.0,
            "the 96 m interpolate-vs-snap rule"
        );
    }

    #[test]
    fn a_target_beyond_the_blip_distance_arms_the_snap_path() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        let far = at(150.0, 0.0, 0.0);
        assert_eq!(
            m.interpolate_to(&here, &far, false, globals::AUTONOMY_BLIP_OUTDOORS),
            InterpolateDecision::Blip
        );
        assert_eq!(m.node_fail_counter, NODE_FAIL_LIMIT);
        assert_eq!(
            m.snap_target(),
            Some(far),
            "UseTime snaps rather than interpolating"
        );
    }

    #[test]
    fn a_target_inside_the_blip_distance_is_queued_and_interpolated() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        let there = at(50.0, 0.0, 0.0);
        assert_eq!(
            m.interpolate_to(&here, &there, false, globals::AUTONOMY_BLIP_OUTDOORS),
            InterpolateDecision::Queued
        );
        assert_eq!(m.position_queue.len(), 1);
        assert_eq!(
            m.snap_target(),
            None,
            "no snap while the fail counter is low"
        );
    }

    #[test]
    fn the_indoor_blip_distance_is_the_one_that_applies_indoors() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        let there = at(30.0, 0.0, 0.0);
        // 30 m outdoors is well inside 100 m...
        assert_eq!(
            m.interpolate_to(&here, &there, false, globals::AUTONOMY_BLIP_OUTDOORS),
            InterpolateDecision::Queued
        );
        // ..and outside the 20 m indoor distance.
        let mut m = InterpolationManager::default();
        assert_eq!(
            m.interpolate_to(&here, &there, false, globals::AUTONOMY_BLIP_INDOORS),
            InterpolateDecision::Blip
        );
        // The player gets 25 m indoors, which 30 m still exceeds.
        let mut m = InterpolationManager::default();
        assert_eq!(
            m.interpolate_to(&here, &there, false, globals::AUTONOMY_BLIP_INDOORS_PLAYER),
            InterpolateDecision::Blip
        );
    }

    #[test]
    fn a_target_within_five_centimetres_stops_interpolation_outright() {
        let mut m = InterpolationManager::default();
        m.position_queue.push(InterpolationNode {
            kind: NodeKind::Position,
            pos: at(1.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
        });
        let here = at(0.0, 0.0, 0.0);
        assert_eq!(
            m.interpolate_to(&here, &at(0.04, 0.0, 0.0), false, 100.0),
            InterpolateDecision::AlreadyThere
        );
        assert!(
            m.position_queue.is_empty(),
            "StopInterpolating clears the queue"
        );
    }

    #[test]
    fn redundant_tail_nodes_are_collapsed_and_the_queue_is_trimmed_to_under_twenty() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        // Thirty distinct targets, each 1 m further out.
        for i in 1..=30 {
            #[allow(clippy::cast_precision_loss)]
            let target = at(i as f32, 0.0, 0.0);
            m.interpolate_to(&here, &target, false, 100.0);
        }
        // The trim happens *before* the push, so the queue settles at exactly MAX_QUEUE.
        assert_eq!(
            m.position_queue.len(),
            MAX_QUEUE,
            "{}",
            m.position_queue.len()
        );
        // A repeat of the tail target collapses rather than growing the queue.
        let before = m.position_queue.len();
        let tail = m.position_queue.last().expect("non-empty").pos;
        m.interpolate_to(&here, &tail, false, 100.0);
        assert_eq!(
            m.position_queue.len(),
            before,
            "the redundant node collapsed"
        );
    }

    #[test]
    fn interpolation_only_runs_while_the_object_is_in_contact() {
        let mut m = InterpolationManager::default();
        m.position_queue.push(InterpolationNode {
            kind: NodeKind::Position,
            pos: at(10.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
        });
        let here = at(0.0, 0.0, 0.0);
        let mut out = Frame::default();
        m.adjust_offset(&here, false, 0.1, &mut out, false);
        assert_eq!(
            out,
            Frame::default(),
            "airborne objects are not interpolated"
        );
        m.adjust_offset(&here, true, 0.1, &mut out, false);
        assert_ne!(out, Frame::default(), "in contact, the step is produced");
    }

    #[test]
    fn the_fallback_speed_is_used_when_there_is_no_motion_interpreter() {
        let mut m = InterpolationManager::default();
        m.position_queue.push(InterpolationNode {
            kind: NodeKind::Position,
            pos: at(100.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
        });
        let here = at(0.0, 0.0, 0.0);
        let mut out = Frame::default();
        m.adjust_offset(&here, true, 1.0, &mut out, false);
        // One second at 7.5 m/s.
        assert!(
            (out.origin.mag2().sqrt() - FALLBACK_SPEED).abs() < 1e-3,
            "{out:?}"
        );
        // With a motion interpreter the speed is doubled.
        let mut m = InterpolationManager {
            max_speed: Some(3.0),
            ..InterpolationManager::default()
        };
        m.position_queue.push(InterpolationNode {
            kind: NodeKind::Position,
            pos: at(100.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
        });
        let mut out = Frame::default();
        m.adjust_offset(&here, true, 1.0, &mut out, false);
        assert!((out.origin.mag2().sqrt() - 6.0).abs() < 1e-3, "{out:?}");
    }

    /// The adjusted speed flag is set because the image initialises it to one.
    #[test]
    fn the_adjusted_speed_flag_is_set_because_the_image_initialises_it_to_one() {
        assert!(
            InterpolationManager::default().use_adjusted_speed,
            "the client's use-adjusted-speed flag is 1"
        );
    }

    #[test]
    fn a_node_that_makes_no_progress_is_abandoned_on_the_second_five_step_window() {
        let mut m = InterpolationManager::default();
        assert_eq!(
            m.original_distance, LARGE_DISTANCE,
            "the constructor seeds original_distance from the client's own large distance"
        );
        m.position_queue.push(InterpolationNode {
            kind: NodeKind::Position,
            pos: at(10.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
        });
        let here = at(0.0, 0.0, 0.0);
        let mut out = Frame::default();
        for _ in 0..5 {
            m.adjust_offset(&here, true, 0.1, &mut out, false);
        }
        assert_eq!(
            m.position_queue.len(),
            1,
            "the first window only re-seeds: {m:?}"
        );
        assert_eq!(m.node_fail_counter, 0);
        assert!(
            (m.original_distance - 10.0).abs() < 1e-3,
            "{}",
            m.original_distance
        );
        for _ in 0..5 {
            m.adjust_offset(&here, true, 0.1, &mut out, false);
        }
        assert!(m.position_queue.is_empty(), "the node was abandoned");
        assert_eq!(m.node_fail_counter, 1);
        assert_eq!(
            m.blipto_position,
            Some(at(10.0, 0.0, 0.0)),
            "NodeCompleted(0) remembers the node it gave up on, "
        );
    }

    #[test]
    fn a_completed_final_node_stops_interpolating_and_reseeds_the_large_distance() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        m.node_fail_counter = 2;
        m.interpolate_to(&here, &at(0.01, 0.0, 0.0), false, 100.0);
        // Within 0.05 m: InterpolateTo itself takes StopInterpolating.
        assert!(!m.is_interpolating());
        assert_eq!(
            m.node_fail_counter, 0,
            "stopping the interpolation clears it"
        );
        assert_eq!(m.original_distance, LARGE_DISTANCE);
    }

    #[test]
    fn use_time_blips_once_the_fail_counter_is_armed_and_is_idle_before_that() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        let far = at(150.0, 0.0, 0.0);
        assert_eq!(
            m.use_time(&here),
            InterpolationStep::Idle,
            "an empty queue asks for nothing"
        );
        m.interpolate_to(
            &here,
            &at(5.0, 0.0, 0.0),
            false,
            globals::AUTONOMY_BLIP_OUTDOORS,
        );
        assert_eq!(
            m.use_time(&here),
            InterpolationStep::Idle,
            "a position node is walked by adjust_offset, not by UseTime"
        );
        let mut m = InterpolationManager::default();
        assert_eq!(
            m.interpolate_to(&here, &far, false, globals::AUTONOMY_BLIP_OUTDOORS),
            InterpolateDecision::Blip
        );
        assert_eq!(
            m.use_time(&here),
            InterpolationStep::Blip {
                pos: far,
                velocity: None
            },
            "beyond the autonomy blip distance UseTime runs SetPositionSimple"
        );
    }

    #[test]
    fn four_failed_nodes_arm_the_snap() {
        let mut m = InterpolationManager::default();
        let here = at(0.0, 0.0, 0.0);
        for _ in 0..NODE_FAIL_LIMIT {
            m.position_queue.push(InterpolationNode {
                kind: NodeKind::Position,
                pos: at(10.0, 0.0, 0.0),
                velocity: Vec3::ZERO,
            });
            let mut out = Frame::default();
            for _ in 0..10 {
                m.adjust_offset(&here, true, 0.1, &mut out, false);
            }
        }
        assert_eq!(m.node_fail_counter, NODE_FAIL_LIMIT);
        m.blipto_position = Some(at(10.0, 0.0, 0.0));
        assert_eq!(m.snap_target(), Some(at(10.0, 0.0, 0.0)));
    }

    #[test]
    fn a_sticky_object_is_pulled_to_its_target_radius_and_no_further() {
        let mut s = StickyManager::default();
        s.stick_to(ObjectId(7), 2.0);
        s.target_position = Some(at(10.0, 0.0, 0.0));
        let mut out = Frame::default();
        s.adjust_offset(&at(0.0, 0.0, 0.0), &mut out);
        assert!(
            (out.origin.x - 8.0).abs() < 1e-4,
            "pulled to within the radius: {out:?}"
        );
        // Already inside the radius: no pull.
        let mut out = Frame::default();
        s.adjust_offset(&at(9.0, 0.0, 0.0), &mut out);
        assert_eq!(out.origin, Vec3::ZERO);
    }

    #[test]
    fn the_constraint_does_nothing_inside_the_start_distance_and_holds_at_the_maximum() {
        let mut c = ConstraintManager::default();
        c.constrain_to(
            at(0.0, 0.0, 0.0),
            globals::CONSTRAINT_START_OUTDOORS,
            globals::CONSTRAINT_MAX_OUTDOORS,
        );
        let mut out = Frame::default();
        c.adjust_offset(&at(5.0, 0.0, 0.0), &mut out);
        assert_eq!(
            out.origin,
            Vec3::ZERO,
            "inside 10 m the constraint is inert"
        );

        let mut out = Frame::default();
        c.adjust_offset(&at(30.0, 0.0, 0.0), &mut out);
        assert!(out.origin.x < 0.0, "beyond 10 m it pulls back: {out:?}");

        assert!(!c.is_fully_constrained(&at(30.0, 0.0, 0.0)));
        assert!(
            c.is_fully_constrained(&at(60.0, 0.0, 0.0)),
            "at 50 m it is fully constrained"
        );
    }

    #[test]
    fn the_indoor_constraint_distances_are_five_and_twenty() {
        let mut c = ConstraintManager::default();
        c.constrain_to(
            at(0.0, 0.0, 0.0),
            globals::CONSTRAINT_START_INDOORS,
            globals::CONSTRAINT_MAX_INDOORS,
        );
        let mut out = Frame::default();
        c.adjust_offset(&at(4.0, 0.0, 0.0), &mut out);
        assert_eq!(out.origin, Vec3::ZERO);
        let mut out = Frame::default();
        c.adjust_offset(&at(8.0, 0.0, 0.0), &mut out);
        assert!(out.origin.x < 0.0);
        assert!(c.is_fully_constrained(&at(25.0, 0.0, 0.0)));
    }

    #[test]
    fn the_position_manager_forwards_to_every_sub_manager_it_has() {
        let mut pm = PositionManager {
            interpolation: Some(InterpolationManager::default()),
            sticky: Some(StickyManager::default()),
            constraint: Some(ConstraintManager::default()),
        };
        let mut out = Frame::default();
        // Nothing configured: no movement, and no panic from any of the three.
        pm.adjust_offset(&at(0.0, 0.0, 0.0), true, 0.1, &mut out);
        assert_eq!(out, Frame::default());
    }
}

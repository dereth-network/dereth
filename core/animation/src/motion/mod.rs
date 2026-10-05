//! [`MovementManager`], [`MovementParameters`], the motion interpreter,
//! [`MoveToManager`] and [`StickyManager`].
//!
//! Two corrections to ACE are load-bearing here and both are in `docs/CORRECTIONS.md`:
//! the default `MovementParameters` bitfield is **`0x1EE0F`**, so `can_charge` is **clear**, and
//! the walk/run threshold is **15.0**, not 1.0. ACE has both wrong.

pub mod interp;
pub mod moveto;
mod owner;
pub mod sticky;

use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};

use crate::command::MotionCommand;

pub use interp::{
    get_jump_height, get_run_rate, jump_stamina_cost, load_mod, motion_allows_jump,
    power_bar_level, ActionNode, InterpretedMotionState, MotionInterp, RawMotionState,
    MIN_JUMP_EXTENT,
};
pub use moveto::{
    heading_diff, heading_greater, MoveToManager, MoveToRequest, MovementNode, TargetInfo,
};
pub use sticky::StickyManager;

// ---------------------------------------------------------------------------------------------
// HoldKey
// ---------------------------------------------------------------------------------------------

/// `HoldKey`. **`Invalid` is not a third mode**: it means "use the interpreter's persistent
/// `raw_state.current_holdkey`", which `adjust_motion` substitutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HoldKey {
    #[default]
    Invalid = 0,
    None = 1,
    Run = 2,
}

// ---------------------------------------------------------------------------------------------
// MovementParameters
// ---------------------------------------------------------------------------------------------

/// The 18 flag bits of `MovementParameters`.
pub mod flags {
    pub const CAN_WALK: u32 = 0x0_0001;
    pub const CAN_RUN: u32 = 0x0_0002;
    pub const CAN_SIDESTEP: u32 = 0x0_0004;
    pub const CAN_WALK_BACKWARDS: u32 = 0x0_0008;
    /// Read *indirectly*, in `get_command`'s hold-key test — do not optimise it away.
    pub const CAN_CHARGE: u32 = 0x0_0010;
    pub const FAIL_WALK: u32 = 0x0_0020;
    pub const USE_FINAL_HEADING: u32 = 0x0_0040;
    pub const STICKY: u32 = 0x0_0080;
    pub const MOVE_AWAY: u32 = 0x0_0100;
    pub const MOVE_TOWARDS: u32 = 0x0_0200;
    pub const USE_SPHERES: u32 = 0x0_0400;
    pub const SET_HOLD_KEY: u32 = 0x0_0800;
    pub const AUTONOMOUS: u32 = 0x0_1000;
    pub const MODIFY_RAW_STATE: u32 = 0x0_2000;
    pub const MODIFY_INTERPRETED_STATE: u32 = 0x0_4000;
    pub const CANCEL_MOVETO: u32 = 0x0_8000;
    pub const STOP_COMPLETELY: u32 = 0x1_0000;
    pub const DISABLE_JUMP_DURING_LINK: u32 = 0x2_0000;
}

/// The default bitfield: `0x1EE0F`.
///
/// `can_walk | can_run | can_sidestep | can_walk_backwards | move_towards | use_spheres |
/// set_hold_key | modify_raw_state | modify_interpreted_state | cancel_moveto | stop_completely`.
/// Note what is **clear**: `can_charge`, `fail_walk`, `use_final_heading`, `sticky`, `move_away`,
/// `autonomous`, `disable_jump_during_link`.
pub const DEFAULT_FLAGS: u32 = 0x1_EE0F;

/// The default distance to an object.
pub const DEFAULT_DISTANCE_TO_OBJECT: f32 = 0.6;
/// The default walk/run threshold. **15.0**, not ACE's 1.0.
pub const DEFAULT_WALK_RUN_THRESHOLD: f32 = 15.0;
/// The default fail distance, `FLT_MAX`.
pub const DEFAULT_FAIL_DISTANCE: f32 = f32::MAX;

/// `MovementParameters` — the client stores a 0x28-byte payload after its shared packed-object base.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementParameters {
    pub flags: u32,
    pub distance_to_object: f32,
    pub min_distance: f32,
    pub desired_heading: f32,
    pub speed: f32,
    pub fail_distance: f32,
    pub walk_run_threshold: f32,
    pub context_id: u32,
    pub hold_key_to_apply: HoldKey,
    pub action_stamp: u32,
}

impl Default for MovementParameters {
    /// The client's own constructor values.
    fn default() -> Self {
        Self {
            flags: DEFAULT_FLAGS,
            distance_to_object: DEFAULT_DISTANCE_TO_OBJECT,
            min_distance: 0.0,
            desired_heading: 0.0,
            speed: 1.0,
            fail_distance: DEFAULT_FAIL_DISTANCE,
            walk_run_threshold: DEFAULT_WALK_RUN_THRESHOLD,
            context_id: 0,
            hold_key_to_apply: HoldKey::Invalid,
            action_stamp: 0,
        }
    }
}

impl MovementParameters {
    #[must_use]
    pub const fn has(&self, mask: u32) -> bool {
        self.flags & mask != 0
    }

    /// Which forward command a move-to should issue
    /// and whether we are moving away.
    ///
    /// The hold-key test is transcribed literally, including `can_charge` (`0x10`): **a charging
    /// move always runs**. That bit is otherwise never read, which is why it must not be
    /// mistaken for dead.
    #[must_use]
    pub fn get_command(&self, dist: f32) -> (MotionCommand, HoldKey, bool) {
        let (cmd, moving_away) = if self.has(flags::MOVE_AWAY) && self.has(flags::MOVE_TOWARDS) {
            self.towards_and_away(dist)
        } else if self.has(flags::MOVE_AWAY) {
            if dist < self.min_distance {
                (MotionCommand::WALK_FORWARD, true)
            } else {
                (MotionCommand::NONE, false)
            }
        } else if dist > self.distance_to_object {
            (MotionCommand::WALK_FORWARD, false)
        } else {
            (MotionCommand::NONE, false)
        };

        let hold = if !self.has(flags::CAN_CHARGE)
            && (!self.has(flags::CAN_RUN)
                || (self.has(flags::CAN_WALK)
                    && dist - self.distance_to_object <= self.walk_run_threshold))
        {
            HoldKey::None
        } else {
            HoldKey::Run
        };
        (cmd, hold, moving_away)
    }

    /// The towards-and-away classification.
    #[must_use]
    pub fn towards_and_away(&self, dist: f32) -> (MotionCommand, bool) {
        if dist > self.distance_to_object {
            (MotionCommand::WALK_FORWARD, false)
        } else if dist - self.min_distance < dereth_primitives::num::consts::EPSILON {
            (MotionCommand::WALK_BACKWARDS, true)
        } else {
            (MotionCommand::NONE, false)
        }
    }

    /// The offset to add to the bearing to the target.
    ///
    /// Moving forwards towards the target, or backwards away from it, means 0°; **every other
    /// combination is 180°**, which is what makes an object backing away face the thing it is
    /// backing away from.
    #[must_use]
    pub fn get_desired_heading(cmd: MotionCommand, moving_away: bool) -> f32 {
        let forwards = cmd == MotionCommand::RUN_FORWARD || cmd == MotionCommand::WALK_FORWARD;
        if forwards && !moving_away {
            return 0.0;
        }
        if cmd == MotionCommand::WALK_BACKWARDS && moving_away {
            return 0.0;
        }
        180.0
    }
}

// ---------------------------------------------------------------------------------------------
// The environment and the effects
// ---------------------------------------------------------------------------------------------

/// The physics facts the movement layer reads. The client reaches through its physics and
/// game objects for all of them; this crate takes them as data, refreshed once per step, so that
/// the whole movement layer is a pure function of its inputs and testable without a physics object.
#[derive(Debug, Clone, Copy)]
pub struct MotionEnv {
    /// Whether the object is on the ground -- `Contact && OnWalkable`.
    pub on_ground: bool,
    /// `transient_state & 1` (Contact) alone. Move-to timing gates on this.
    pub contact: bool,
    pub in_cell: bool,
    /// The game object's creature inquiry. With no game object the client treats it as a creature in
    /// `jump_is_allowed` and as a non-creature in `adjust_motion`; [`MotionEnv::has_weenie`]
    /// separates the two.
    pub is_creature: bool,
    pub has_weenie: bool,
    /// `state & 0x400` — gravity-affected.
    pub gravity_affected: bool,
    pub fully_constrained: bool,
    /// Run rate supplied by the weenie. `None` falls back to the movement command's default rate.
    pub run_rate: Option<f32>,
    /// `burden / capacity`.
    pub load: f32,
    pub jump_skill: i32,
    /// The jump height's divisor: 1 at the end of retail, the world's own where its rules say.
    pub jump_scale: f32,
    /// The game object's actual can-jump inquiry. `None` is the isolated arithmetic adapter (`load <2`);
    /// an attached client supplies `Some(false)` when its player description is unavailable.
    pub jump_permission: Option<bool>,
    /// InqJumpVelocity can fail independently of CanJump (missing stamina/skill inquiry).
    /// Failure returns zero vertical velocity, not a successful skill-zero minimum jump.
    pub jump_velocity_available: bool,
    pub is_interpolating: bool,
    /// The object's world position, for [`MoveToManager`].
    pub position: Position,
    /// Integrated velocity.
    ///
    /// Read by the movement fallback, which multiplies the object's frame matrix by this field,
    /// and not by [`Self::cached_velocity`].
    pub velocity: Vec3,
    /// **Achieved** offset over the last quantum, which physics writes and zeroes when the object
    /// did not move through collision).
    ///
    /// **This cached value, not [`Self::velocity`], is returned to movement callers.** The
    /// integrated velocity and the cached velocity are different fields and the
    /// difference is load-bearing: a creature walking on the ground has an integrated velocity of
    /// zero throughout (the acceleration pass zeroes acceleration for a body in walkable contact,
    /// and its translation comes from the animation's root motion), while `cached_velocity`
    /// carries the metres per second it actually covered. The move-to manager's third block gates
    /// on `|get_velocity()| > 0.1`, so sourcing it from the integrated velocity makes that whole
    /// block unreachable for every walking creature.
    pub cached_velocity: Vec3,
    /// The extrapolation lead the object's
    /// target subscription currently holds, or `0.0` when it holds no target.
    ///
    /// The target-quantum query follows the subscription to its [`TargetInfo`], reads the
    /// quantum double, and falls back to zero when there is no target.
    ///
    /// The node-advance path is the only reader in the movement layer, and it uses this value to
    /// decide whether the target quantum differs enough to update.
    pub target_quantum: f32,
    pub cur_time: LocalTime,
    /// The object's own collision radius and height, for the `use_spheres` distance.
    pub radius: f32,
    pub height: f32,
    /// Physics-object id. Move-to-object and stick-to processing compare the target's
    /// top-level id against it, and take the
    /// clean-up arm rather than the tracking arm when they match. Zero for an object with no
    /// server id yet, which never equals a real top-level id.
    pub object_id: ObjectId,
}

impl Default for MotionEnv {
    fn default() -> Self {
        Self {
            on_ground: true,
            contact: true,
            in_cell: true,
            is_creature: true,
            has_weenie: true,
            gravity_affected: true,
            fully_constrained: false,
            run_rate: None,
            load: 0.0,
            jump_skill: 0,
            jump_scale: 1.0,
            jump_permission: None,
            jump_velocity_available: true,
            is_interpolating: false,
            position: Position::new(
                dereth_primitives::CellId(0),
                dereth_primitives::Frame::default(),
            ),
            velocity: Vec3::ZERO,
            cached_velocity: Vec3::ZERO,
            target_quantum: 0.0,
            cur_time: LocalTime(0.0),
            radius: 0.5,
            height: 1.0,
            object_id: ObjectId(0),
        }
    }
}

/// Calls across the animation/physics boundary. Isolated adapters record all calls;
/// production owners resolve movement callbacks and subscription changes synchronously.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotionEffect {
    /// Cancel the object's move-to operation.
    CancelMoveTo,
    /// Clear the object's on-walkable flag — how a jump leaves the ground.
    SetOnWalkable(bool),
    /// Set local velocity `v` with the jump flag true — the jump impulse.
    SetLocalVelocity(Vec3),
    /// Unstick the object from its support.
    UnstickFromObject,
    /// Stick the position manager to a target object.
    StickTo {
        id: ObjectId,
        radius: f32,
        height: f32,
    },
    /// Set heading `h` with the immediate flag true — the exact snap at the end of a turn.
    SetHeading(f32),
    /// Set target id, context, radius and quantum.
    ///
    /// Both producers pass **radius 0.5, quantum 0.0** -- the move-to and turn-to object
    /// entries and the sticky manager's stick-to. The radius
    /// is how far the target may drift before the target manager sends an update; the quantum is
    /// how far ahead to extrapolate, and `HandleMoveToPosition` re-sets it every tick from the
    /// closing speed. They are two separate fields.
    SetTarget {
        id: ObjectId,
        radius: f32,
        quantum: f32,
    },
    ClearTarget,
    SetTargetQuantum(f32),
    /// Cancel move-to and report the failure code to the object.
    MoveToFailed(u32),
}

/// Existing host subscription facts, now owned by the same driver as the callbacks that
/// create or clear them. This is not the complete retail target-subscription implementation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionTarget {
    pub id: ObjectId,
    pub radius: f32,
    pub quantum: f32,
    /// `TargetInfo::last_update_time`: when an update last **arrived**.
    /// The receiver stamps the current time.
    pub last_update: f64,
    /// The target-side subscription record's last-sent position,
    /// which retail keeps in the target's voyeur table and which is
    /// therefore new for every `SetTarget`. `None` until the first subscription update went out.
    pub last_sent: Option<Position>,
    /// The target-side update time as this registration sees it: when the
    /// 0.5 s tick last ran, whether or not it sent anything.
    pub tick_time: f64,
}

pub trait MotionEffects: std::fmt::Debug {
    fn push(&mut self, effect: MotionEffect);
}

impl MotionEffects for Vec<MotionEffect> {
    fn push(&mut self, effect: MotionEffect) {
        Vec::push(self, effect);
    }
}

/// Publish plain subscription changes at their source call, before the next continuation.
/// Remaining external physics effects retain their existing ordered owner queue.
#[derive(Debug)]
pub struct TargetedEffects<'a> {
    pub target: &'a mut Option<MotionTarget>,
    pub pending: &'a mut Vec<MotionEffect>,
}

impl MotionEffects for TargetedEffects<'_> {
    fn push(&mut self, effect: MotionEffect) {
        match effect {
            MotionEffect::SetTarget {
                id,
                radius,
                quantum,
            } => {
                *self.target = Some(MotionTarget {
                    id,
                    radius,
                    quantum,
                    last_update: f64::NEG_INFINITY,
                    last_sent: None,
                    tick_time: f64::NEG_INFINITY,
                });
            }
            MotionEffect::ClearTarget => *self.target = None,
            MotionEffect::SetTargetQuantum(quantum) => {
                if let Some(target) = self.target.as_mut() {
                    target.quantum = quantum;
                }
            }
            other => self.pending.push(other),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// MovementManager
// ---------------------------------------------------------------------------------------------

/// Coordinates the move-to and sticky workers. The client creates each worker
/// lazily; here both are always present because they cost nothing.
#[derive(Debug, Default, Clone)]
pub struct MovementManager {
    pub interp: MotionInterp,
    pub moveto: MoveToManager,
    pub sticky: StickyManager,
}

impl MovementManager {
    fn owner(&mut self) -> owner::MotionOwner<'_> {
        owner::MotionOwner {
            interp: &mut self.interp,
            moveto: &mut self.moveto,
            sticky: &mut self.sticky,
            owns_moveto: true,
            owns_sticky: true,
        }
    }

    /// the movement manager's tick; the position-manager phase is separate.
    pub fn use_movement_time(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().moveto_use_time(ctx);
    }

    /// the position manager's tick, after the part array's movement handling.
    pub fn use_position_time(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().position_use_time(ctx);
    }

    pub fn drain_completed_motions(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().drain_completed_motions(ctx);
    }

    pub fn drain_use_time(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().drain_use_time(ctx);
    }

    pub fn drain_animation_done(&mut self, success: bool, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().drain_animation_done(success, ctx);
    }

    pub fn remove_link_animations(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().remove_link_animations(ctx);
    }

    pub fn do_interpreted_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut interp::MotionCtx<'_>,
    ) -> u32 {
        self.owner().do_interpreted_motion(cmd, params, ctx)
    }

    pub fn enter_default_state(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().enter_default_state(ctx);
    }

    pub fn apply_raw_movement(
        &mut self,
        link: bool,
        longjump_hint: bool,
        ctx: &mut interp::MotionCtx<'_>,
    ) {
        self.owner().apply_raw_movement(link, longjump_hint, ctx);
    }

    pub fn handle_exit_world(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().handle_exit_world(ctx);
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Combined isolated-adapter tick. Production MotionDriver uses the separate movement
    /// and position phases, with part-array movement between them as in retail.
    pub fn use_time(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        let mut owner = self.owner();
        owner.moveto_use_time(ctx);
        owner.position_use_time(ctx);
    }

    /// The movement manager's perform-movement step, which drives the move-to manager's own.
    pub fn perform_movement(
        &mut self,
        req: &moveto::MoveToRequest,
        params: &MovementParameters,
        ctx: &mut interp::MotionCtx<'_>,
    ) {
        self.owner().moveto_perform_movement(req, params, ctx);
    }

    /// Forward cancellation to the move-to manager.
    /// The entry point does nothing else.
    ///
    /// Its one interesting caller is received-movement processing, which passes **`0x36`** as its
    /// *first* statement —
    /// so every movement buffer, of every arm, cancels whatever move-to was in flight before it
    /// is even read. Move-to cancellation returns immediately when none is active.
    pub fn cancel_move_to(&mut self, err: u32, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().cancel_move_to(err, ctx);
    }

    /// Unstick from the sticky target and stop completely, resolving the physics object's
    /// cancel-move-to callback through the owner. State reset and ClearTarget precede cancellation,
    /// and no late CancelMoveTo effect may survive to cancel a replacement installed by the caller.
    pub fn unstick_from_object(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().unstick_from_object(ctx);
    }

    /// finish the old stick's clear/cancel callbacks
    /// before recording the replacement and publishing its SetTarget subscription.
    pub fn stick_to_object(
        &mut self,
        target: ObjectId,
        radius: f32,
        now: f64,
        ctx: &mut interp::MotionCtx<'_>,
    ) {
        self.owner().stick_to_object(target, radius, 0.0, now, ctx);
    }

    /// The position manager's half of target-update handling. Failed updates
    /// for the current sticky target cancel the move-to operation before returning.
    pub fn handle_sticky_update_target(
        &mut self,
        info: &moveto::TargetInfo,
        ctx: &mut interp::MotionCtx<'_>,
    ) {
        self.owner().handle_sticky_update_target(info, ctx);
    }

    /// Received-movement processing's unconditional prefix, before style
    /// dispatch and before every body arm. Even an unchanged style must end an old stick.
    pub fn prepare_received_movement(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        let mut owner = self.owner();
        owner.cancel_move_to(interp::ACTION_CANCELLED, ctx);
        owner.unstick_from_object(ctx);
    }

    /// Stopping the object completely reaches the motion interpreter's stop.
    /// Its leading cancel_moveto call is synchronous. Hosts owning both managers must finish
    /// cancellation before resetting/reapplying raw input; a deferred CancelMoveTo can otherwise
    /// be dropped or cancel the replacement command after it has already started.
    pub fn stop_completely(&mut self, ctx: &mut interp::MotionCtx<'_>) -> u32 {
        self.owner().stop_completely(ctx)
    }

    /// Run motion with the physics object's cancel-move-to callback
    /// resolved synchronously through its owner, before hold-key, adjustment or refusal checks.
    pub fn do_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut interp::MotionCtx<'_>,
    ) -> u32 {
        self.owner().do_motion(cmd, params, ctx)
    }

    /// Stop a motion, including cancellation before a failed stop.
    /// Clearing CANCEL_MOVETO preserves the existing approach exactly as the retail flag does.
    pub fn stop_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut interp::MotionCtx<'_>,
    ) -> u32 {
        self.owner().stop_motion(cmd, params, ctx)
    }

    /// Cancels the old move before checking whether a jump is allowed, even on refusal.
    pub fn jump(&mut self, extent: f32, ctx: &mut interp::MotionCtx<'_>) -> u32 {
        self.owner().jump(extent, ctx)
    }

    /// Replacing the interpreted movement writes raw style, cancels the
    /// old approach, and only then reads the old jump restriction and installs replacement state.
    /// No deferred CancelMoveTo may remain to reset that replacement or a later approach.
    pub fn move_to_interpreted_state(
        &mut self,
        state: &InterpretedMotionState,
        is_the_player: bool,
        ctx: &mut interp::MotionCtx<'_>,
    ) {
        self.owner()
            .move_to_interpreted_state(state, is_the_player, ctx);
    }

    /// The movement manager's update-target handler.
    pub fn handle_update_target(
        &mut self,
        info: &moveto::TargetInfo,
        ctx: &mut interp::MotionCtx<'_>,
    ) {
        self.owner().moveto_handle_update_target(info, ctx);
    }

    /// The movement manager's ground-contact notification -- the motion interpreter's
    /// **then** the move-to manager's.
    pub fn hit_ground(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        let mut owner = self.owner();
        owner.hit_ground(ctx);
        owner.moveto_hit_ground(ctx);
    }

    /// [`MoveToManager`] contributes no non-trivial leave-ground behavior,
    /// so only the interpreter's handler runs.
    pub fn leave_ground(&mut self, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().leave_ground(ctx);
    }

    /// The id is ignored, as in the client.
    pub fn motion_done(&mut self, success: bool, ctx: &mut interp::MotionCtx<'_>) {
        self.owner().motion_done(success, ctx);
    }

    /// Whether a `MoveTo` is in flight.
    #[must_use]
    pub fn is_moving_to(&self) -> bool {
        self.moveto.is_moving_to()
    }

    /// Whether any motion is still pending.
    #[must_use]
    pub fn motions_pending(&self) -> bool {
        self.interp.motions_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ORACLE: the recovered movement-manager behavior
    /// section 2 and `docs/CORRECTIONS.md` — ACE sets `CanCharge = true` and a threshold of
    /// 1.0; the client does neither.
    #[test]
    fn the_default_parameters_are_the_clients_not_aces() {
        let p = MovementParameters::default();
        assert_eq!(p.flags, 0x1_EE0F);
        assert_eq!(p.walk_run_threshold, 15.0);
        assert!(
            !p.has(flags::CAN_CHARGE),
            "can_charge is CLEAR in the client"
        );
        assert!(p.has(flags::CAN_WALK));
        assert!(p.has(flags::CAN_RUN));
        assert!(p.has(flags::CAN_SIDESTEP));
        assert!(p.has(flags::CAN_WALK_BACKWARDS));
        assert!(p.has(flags::MOVE_TOWARDS));
        assert!(!p.has(flags::MOVE_AWAY));
        assert!(p.has(flags::USE_SPHERES));
        assert!(p.has(flags::SET_HOLD_KEY));
        assert!(p.has(flags::MODIFY_RAW_STATE));
        assert!(p.has(flags::MODIFY_INTERPRETED_STATE));
        assert!(p.has(flags::CANCEL_MOVETO));
        assert!(p.has(flags::STOP_COMPLETELY));
        assert!(!p.has(flags::AUTONOMOUS));
        assert!(!p.has(flags::STICKY));
        assert!(!p.has(flags::USE_FINAL_HEADING));
        assert!(!p.has(flags::FAIL_WALK));
        assert!(!p.has(flags::DISABLE_JUMP_DURING_LINK));
        assert_eq!(p.distance_to_object, 0.6);
        assert_eq!(p.fail_distance, f32::MAX);
        assert_eq!(p.speed, 1.0);
        assert_eq!(p.hold_key_to_apply, HoldKey::Invalid);
    }

    /// `get_command` with the defaults: walk when further than `distance_to_object`, nothing when
    /// closer, and the hold key flips to `Run` past the walk/run threshold.
    #[test]
    fn get_command_reproduces_its_table() {
        let p = MovementParameters::default();
        assert_eq!(p.get_command(0.5).0, MotionCommand::NONE);
        let (cmd, hold, away) = p.get_command(2.0);
        assert_eq!(cmd, MotionCommand::WALK_FORWARD);
        assert_eq!(hold, HoldKey::None, "2.0 - 0.6 <= 15.0");
        assert!(!away);
        let (_, hold, _) = p.get_command(100.0);
        assert_eq!(hold, HoldKey::Run, "100 - 0.6 > 15.0");

        // A charging move always runs, however close it is.
        let p = MovementParameters {
            flags: DEFAULT_FLAGS | flags::CAN_CHARGE,
            ..p
        };
        assert_eq!(p.get_command(2.0).1, HoldKey::Run);

        // `can_walk` clear means "always run".
        let p = MovementParameters {
            flags: DEFAULT_FLAGS & !flags::CAN_WALK,
            ..MovementParameters::default()
        };
        assert_eq!(p.get_command(2.0).1, HoldKey::Run);
    }

    /// `move_away` alone only moves when *inside* `min_distance`, and `towards_and_away` has three
    /// outcomes.
    #[test]
    fn the_move_away_arms_are_the_documented_ones() {
        let p = MovementParameters {
            flags: (DEFAULT_FLAGS & !flags::MOVE_TOWARDS) | flags::MOVE_AWAY,
            min_distance: 5.0,
            ..MovementParameters::default()
        };
        let (cmd, _, away) = p.get_command(1.0);
        assert_eq!(cmd, MotionCommand::WALK_FORWARD);
        assert!(away);
        assert_eq!(p.get_command(9.0).0, MotionCommand::NONE);

        let p = MovementParameters {
            flags: DEFAULT_FLAGS | flags::MOVE_AWAY,
            min_distance: 5.0,
            distance_to_object: 6.0,
            ..MovementParameters::default()
        };
        assert_eq!(
            p.towards_and_away(9.0),
            (MotionCommand::WALK_FORWARD, false)
        );
        assert_eq!(
            p.towards_and_away(5.0),
            (MotionCommand::WALK_BACKWARDS, true)
        );
        assert_eq!(p.towards_and_away(5.5), (MotionCommand::NONE, false));
    }

    /// `get_desired_heading`'s table: 0° for forwards-towards and backwards-away, 180° otherwise.
    #[test]
    fn get_desired_heading_reproduces_its_table() {
        use MotionCommand as M;
        assert_eq!(
            MovementParameters::get_desired_heading(M::RUN_FORWARD, false),
            0.0
        );
        assert_eq!(
            MovementParameters::get_desired_heading(M::WALK_FORWARD, false),
            0.0
        );
        assert_eq!(
            MovementParameters::get_desired_heading(M::WALK_BACKWARDS, true),
            0.0
        );
        assert_eq!(
            MovementParameters::get_desired_heading(M::RUN_FORWARD, true),
            180.0
        );
        assert_eq!(
            MovementParameters::get_desired_heading(M::WALK_BACKWARDS, false),
            180.0
        );
        assert_eq!(
            MovementParameters::get_desired_heading(M::NONE, false),
            180.0
        );
    }
}

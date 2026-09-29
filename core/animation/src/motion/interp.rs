//! Motion interpolation: the raw and interpreted states, `adjust_motion`,
//! `apply_run_to_command`, the run rate and the jump.
//!
//! **Keep the raw/interpreted split.** It is not redundancy: the raw state is what is serialised to
//! the server, the interpreted state is what is serialised to other clients, and `adjust_motion` is
//! the only bridge. The raw state deliberately never stores `RunForward` — running is expressed by
//! the hold key and materialised in the interpreted state.

use std::collections::VecDeque;

use dereth_primitives::num::consts::EPSILON;
use dereth_primitives::Vec3;

use crate::command::MotionCommand;
use crate::data::AnimAssets;
use crate::hooks::AnimEvent;
use crate::seq::Sequence;
use crate::table::{MotionTableManager, MovementType};

use super::{flags, HoldKey, MotionEnv, MovementParameters};

// ---------------------------------------------------------------------------------------------
// Error codes (`WeenieError`, in the client's numbering)
// ---------------------------------------------------------------------------------------------

pub const NO_PHYSICS_OBJECT: u32 = 8;
pub const NO_MOTION_INTERPRETER: u32 = 0xB;
pub const YOU_CANT_JUMP_WHILE_IN_THE_AIR: u32 = 0x24;
pub const ACTION_CANCELLED: u32 = 0x36;
pub const OBJECT_GONE: u32 = 0x37;
pub const NO_OBJECT: u32 = 0x38;
pub const YOU_CHARGED_TOO_FAR: u32 = 0x3D;
pub const CANT_CROUCH_IN_COMBAT: u32 = 0x3F;
pub const CANT_SIT_IN_COMBAT: u32 = 0x40;
pub const CANT_LIE_DOWN_IN_COMBAT: u32 = 0x41;
pub const CANT_CHAT_EMOTE_IN_COMBAT: u32 = 0x42;
pub const TOO_MANY_ACTIONS: u32 = 0x45;
pub const GENERAL_MOVEMENT_FAILURE: u32 = 0x47;
pub const YOU_CANT_JUMP_FROM_THIS_POSITION: u32 = 0x48;
pub const CANT_JUMP_LOADED_DOWN: u32 = 0x49;

/// The floor *and* the threshold, so the whole power-bar rule is
/// exactly `max(level, 0.001)`.
pub const MIN_JUMP_EXTENT: f32 = 0.001;

// ---------------------------------------------------------------------------------------------
// The speed constants
// ---------------------------------------------------------------------------------------------

/// Walk forward, m/s per unit of `forward_speed`.
pub const WALK_SPEED: f32 = 3.12;
/// Run forward, m/s per unit of `forward_speed`.
pub const RUN_SPEED: f32 = 4.0;
/// Sidestep, m/s per unit of `sidestep_speed`.
pub const SIDESTEP_SPEED: f32 = 1.25;
/// The multiplier `adjust_motion` applies to a backwards walk.
pub const BACKWARDS_MULTIPLIER: f32 = -0.65;
/// The multiplier `adjust_motion` applies to **both** sidestep directions.
pub const SIDESTEP_MULTIPLIER: f32 = 1.248;
/// The multiplier `apply_run_to_command` applies to a turn while running.
pub const TURN_RUN_MULTIPLIER: f32 = 1.5;
/// The clamp `apply_run_to_command` applies to a sidestep while running.
pub const SIDESTEP_RUN_CLAMP: f32 = 3.0;

// ---------------------------------------------------------------------------------------------
// The gameplay formulas
// ---------------------------------------------------------------------------------------------

/// The movement formulas live in `dereth-rules` (one implementation for the motion interpreter,
/// the gameplay rules and the server); re-exported here at their old paths.
pub use dereth_rules::burden::load_mod;
pub use dereth_rules::movement::{get_jump_height, get_run_rate, jump_stamina_cost, jump_velocity};

/// Returns the timed power-bar level plus the `MIN_JUMP_EXTENT` floor.
///
/// The bar is linear in time with `T = 1.0 s`, or **0.8 s** in `DualWieldCombat`. Since the floor
/// and the threshold for using the measured level are the same number, the whole thing is exactly
/// `max(level, 0.001)`: an instantaneous tap gives 0.001, not 0.
#[must_use]
pub fn power_bar_level(held_seconds: f32, dual_wield: bool) -> f32 {
    let t = if dual_wield { 0.8 } else { 1.0 };
    let level = (held_seconds / t).clamp(0.0, 1.0);
    level.max(MIN_JUMP_EXTENT)
}

/// Returns `0x48` when the command is one of the ranges
/// below, else 0.
///
/// The final retail client's list, which the rebuild follows: its renumbered command table puts
/// the Purple power-ups at `0x1000012B..=0x10000134`, and it refuses a jump from `Sanctuary` and
/// from `AI_TelegraphCast` as well, which the 2013 client did not.
#[must_use]
pub fn motion_allows_jump(cmd: MotionCommand) -> u32 {
    let c = cmd.0;
    let blocked = (0x1000_006F..=0x1000_0078).contains(&c)      // MagicPowerUp01..10
        || (0x1000_012B..=0x1000_0134).contains(&c)             // MagicPowerUp01..10 Purple
        || c == MotionCommand::SANCTUARY.0                      // Sanctuary
        || c == MotionCommand::AI_TELEGRAPH_CAST.0              // AI_TelegraphCast
        || c == 0x4000_0008                                     // Fallen
        || (0x4000_0016..=0x4000_0018).contains(&c)             // Reload, Unload, Pickup
        || (0x4000_001E..=0x4000_0039).contains(&c)             // Aim* and Magic* casting
        || (0x4100_0012..=0x4100_0014).contains(&c); // Crouch, Sitting, Sleeping
    if blocked {
        YOU_CANT_JUMP_FROM_THIS_POSITION
    } else {
        0
    }
}

/// The forward commands from which a jump cannot even be charged: `Fallen`, `Crouch`, `Sitting`,
/// `Sleeping`, and -- in the final retail client, which the rebuild follows -- `Sanctuary`.
/// Shared by the charge and by the permission test, which apply the same list.
fn forward_refuses_a_charge(fc: MotionCommand) -> bool {
    fc == MotionCommand::FALLEN
        || fc == MotionCommand::SANCTUARY
        || fc == MotionCommand::CROUCH
        || fc == MotionCommand::SITTING
        || fc == MotionCommand::SLEEPING
}

// ---------------------------------------------------------------------------------------------
// The two motion states
// ---------------------------------------------------------------------------------------------

/// `ActionNode { action, speed, stamp, autonomous }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionNode {
    pub action: MotionCommand,
    pub speed: f32,
    pub stamp: u32,
    pub autonomous: bool,
}

/// `RawMotionState` — what the input layer asked for, including the hold keys.
#[derive(Debug, Clone, PartialEq)]
pub struct RawMotionState {
    pub current_holdkey: HoldKey,
    pub current_style: MotionCommand,
    pub forward_command: MotionCommand,
    pub forward_holdkey: HoldKey,
    pub forward_speed: f32,
    pub sidestep_command: MotionCommand,
    pub sidestep_holdkey: HoldKey,
    pub sidestep_speed: f32,
    pub turn_command: MotionCommand,
    pub turn_holdkey: HoldKey,
    pub turn_speed: f32,
    pub actions: VecDeque<ActionNode>,
}

impl Default for RawMotionState {
    /// The client's own constructor values.
    fn default() -> Self {
        Self {
            current_holdkey: HoldKey::None,
            current_style: MotionCommand::NON_COMBAT,
            forward_command: MotionCommand::READY,
            forward_holdkey: HoldKey::Invalid,
            forward_speed: 1.0,
            sidestep_command: MotionCommand::NONE,
            sidestep_holdkey: HoldKey::Invalid,
            sidestep_speed: 1.0,
            turn_command: MotionCommand::NONE,
            turn_holdkey: HoldKey::Invalid,
            turn_speed: 1.0,
            actions: VecDeque::new(),
        }
    }
}

fn axis_holdkey(p: &MovementParameters) -> HoldKey {
    // `if (flags & set_hold_key) holdkey = HoldKey::Invalid else holdkey = hold_key_to_apply`.
    if p.has(flags::SET_HOLD_KEY) {
        HoldKey::Invalid
    } else {
        p.hold_key_to_apply
    }
}

impl RawMotionState {
    /// Apply a motion to the raw state.
    ///
    /// Note the deliberate exclusion of `RunForward`: the raw state only ever stores
    /// `WalkForward`/`WalkBackwards`, and running lives in the hold key.
    pub fn apply_motion(&mut self, cmd: MotionCommand, p: &MovementParameters) {
        match cmd {
            MotionCommand::TURN_RIGHT | MotionCommand::TURN_LEFT => {
                self.turn_command = cmd;
                self.turn_holdkey = axis_holdkey(p);
                self.turn_speed = p.speed;
            }
            MotionCommand::SIDE_STEP_RIGHT | MotionCommand::SIDE_STEP_LEFT => {
                self.sidestep_command = cmd;
                self.sidestep_holdkey = axis_holdkey(p);
                self.sidestep_speed = p.speed;
            }
            _ if cmd.is_substate() && cmd != MotionCommand::RUN_FORWARD => {
                self.forward_command = cmd;
                self.forward_holdkey = axis_holdkey(p);
                self.forward_speed = p.speed;
            }
            _ if cmd.is_style() => {
                if self.current_style != cmd {
                    self.forward_command = MotionCommand::READY;
                    self.current_style = cmd;
                }
            }
            _ if cmd.is_action() => {
                self.actions.push_back(ActionNode {
                    action: cmd,
                    speed: p.speed,
                    stamp: p.action_stamp,
                    autonomous: p.has(flags::AUTONOMOUS),
                });
            }
            _ => {}
        }
    }

    /// Remove a motion from the raw state.
    pub fn remove_motion(&mut self, cmd: MotionCommand) {
        match cmd {
            MotionCommand::TURN_RIGHT | MotionCommand::TURN_LEFT => {
                self.turn_command = MotionCommand::NONE;
            }
            MotionCommand::SIDE_STEP_RIGHT | MotionCommand::SIDE_STEP_LEFT => {
                self.sidestep_command = MotionCommand::NONE;
            }
            _ => {
                if self.forward_command == cmd {
                    self.forward_command = MotionCommand::READY;
                    self.forward_speed = 1.0;
                } else if self.current_style == cmd {
                    self.current_style = MotionCommand::NON_COMBAT;
                }
            }
        }
    }

    pub fn remove_action(&mut self) {
        self.actions.pop_front();
    }
}

/// `InterpretedMotionState` — what the object is actually doing. Same shape minus the hold keys.
#[derive(Debug, Clone, PartialEq)]
pub struct InterpretedMotionState {
    pub current_style: MotionCommand,
    pub forward_command: MotionCommand,
    pub forward_speed: f32,
    pub sidestep_command: MotionCommand,
    pub sidestep_speed: f32,
    pub turn_command: MotionCommand,
    pub turn_speed: f32,
    pub actions: VecDeque<ActionNode>,
}

impl Default for InterpretedMotionState {
    /// The client's own constructor values.
    fn default() -> Self {
        Self {
            current_style: MotionCommand::NON_COMBAT,
            forward_command: MotionCommand::READY,
            forward_speed: 1.0,
            sidestep_command: MotionCommand::NONE,
            sidestep_speed: 1.0,
            turn_command: MotionCommand::NONE,
            turn_speed: 1.0,
            actions: VecDeque::new(),
        }
    }
}

impl InterpretedMotionState {
    /// Apply a motion to the interpreted state.
    ///
    /// Only the canonical `TurnRight`/`SideStepRight` arrive here: the left variants have already
    /// been folded to right-with-negative-speed by `adjust_motion`.
    pub fn apply_motion(&mut self, cmd: MotionCommand, p: &MovementParameters) {
        match cmd {
            MotionCommand::TURN_RIGHT => {
                self.turn_command = cmd;
                self.turn_speed = p.speed;
            }
            MotionCommand::SIDE_STEP_RIGHT => {
                self.sidestep_command = cmd;
                self.sidestep_speed = p.speed;
            }
            _ if cmd.is_substate() => {
                self.forward_command = cmd;
                self.forward_speed = p.speed;
            }
            _ if cmd.is_style() => {
                if self.current_style != cmd {
                    self.forward_command = MotionCommand::READY;
                    self.current_style = cmd;
                }
            }
            _ if cmd.is_action() => {
                self.actions.push_back(ActionNode {
                    action: cmd,
                    speed: p.speed,
                    stamp: p.action_stamp,
                    autonomous: p.has(flags::AUTONOMOUS),
                });
            }
            _ => {}
        }
    }

    /// Remove a motion from the interpreted state.
    pub fn remove_motion(&mut self, cmd: MotionCommand) {
        match cmd {
            MotionCommand::TURN_RIGHT | MotionCommand::TURN_LEFT => {
                self.turn_command = MotionCommand::NONE;
            }
            MotionCommand::SIDE_STEP_RIGHT | MotionCommand::SIDE_STEP_LEFT => {
                self.sidestep_command = MotionCommand::NONE;
            }
            _ => {
                if self.forward_command == cmd {
                    self.forward_command = MotionCommand::READY;
                    self.forward_speed = 1.0;
                } else if self.current_style == cmd {
                    self.current_style = MotionCommand::NON_COMBAT;
                }
            }
        }
    }

    /// The seven scalars, **not** the
    /// action list. That is what lets `move_to_interpreted_state` replay queued actions rather than
    /// replacing them.
    pub fn copy_movement_from(&mut self, o: &Self) {
        self.current_style = o.current_style;
        self.forward_command = o.forward_command;
        self.forward_speed = o.forward_speed;
        self.sidestep_command = o.sidestep_command;
        self.sidestep_speed = o.sidestep_speed;
        self.turn_command = o.turn_command;
        self.turn_speed = o.turn_speed;
    }

    pub fn remove_action(&mut self) {
        self.actions.pop_front();
    }

    #[must_use]
    pub fn num_actions(&self) -> usize {
        self.actions.len()
    }
}

// ---------------------------------------------------------------------------------------------
// Motion interpolation
// ---------------------------------------------------------------------------------------------

/// `MotionNode { context_id, motion, jump_error_code }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionNode {
    pub context_id: u32,
    pub motion: MotionCommand,
    pub jump_error_code: u32,
}

/// Everything a movement call needs from the rest of the object, bundled so the signatures stay
/// readable.
pub struct MotionCtx<'a> {
    pub mgr: &'a mut MotionTableManager,
    pub seq: &'a mut Sequence,
    pub assets: &'a dyn AnimAssets,
    pub env: &'a MotionEnv,
    pub effects: &'a mut dyn super::MotionEffects,
    pub events: &'a mut Vec<AnimEvent>,
}

impl std::fmt::Debug for MotionCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `assets` is a `dyn` trait object with no `Debug` bound, and adding one would force every
        // asset cache in the project to implement it. The rest is what a failing test wants to see.
        f.debug_struct("MotionCtx")
            .field("pending_motions", &self.mgr.pending_len())
            .field("nodes", &self.seq.nodes().len())
            .field("env", &self.env)
            .field("effects", &self.effects)
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

/// The original client's motion-interpolation state occupies 0x88 bytes.
#[derive(Debug, Clone)]
pub struct MotionInterp {
    pub initted: bool,
    pub raw_state: RawMotionState,
    pub interpreted_state: InterpretedMotionState,
    /// The divisor used by `get_adjusted_max_speed`.
    pub current_speed_factor: f32,
    pub standing_longjump: bool,
    pub jump_extent: f32,
    pub server_action_stamp: u32,
    /// The last known run rate, used when the weenie cannot supply one.
    pub my_run_rate: f32,
    pub pending_motions: VecDeque<MotionNode>,
}

impl Default for MotionInterp {
    /// The motion interpreter's own construction.
    fn default() -> Self {
        Self {
            initted: false,
            raw_state: RawMotionState::default(),
            interpreted_state: InterpretedMotionState::default(),
            current_speed_factor: 1.0,
            standing_longjump: false,
            jump_extent: 0.0,
            server_action_stamp: 0,
            my_run_rate: 1.0,
            pending_motions: VecDeque::new(),
        }
    }
}

impl MotionInterp {
    pub(super) fn with_owner<R>(
        &mut self,
        f: impl FnOnce(&mut super::owner::MotionOwner<'_>) -> R,
    ) -> R {
        let mut moveto = super::MoveToManager::new();
        let mut sticky = super::StickyManager::new();
        f(&mut super::owner::MotionOwner {
            interp: self,
            moveto: &mut moveto,
            sticky: &mut sticky,
            owns_moveto: false,
            owns_sticky: false,
        })
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The run rate this object moves at: the weenie's, or the cached `my_run_rate`.
    #[must_use]
    pub fn run_rate(&self, env: &MotionEnv) -> f32 {
        if env.has_weenie {
            env.run_rate.unwrap_or(self.my_run_rate)
        } else {
            1.0
        }
    }

    /// Apply the run flag to a command.
    ///
    /// Walking *backwards* — a negative speed after the `−0.65` — stays `WalkForward`, because the
    /// promotion to `RunForward` only happens when the speed is positive. You never get a "run
    /// backwards" animation.
    pub fn apply_run_to_command(&self, cmd: &mut MotionCommand, speed: &mut f32, env: &MotionEnv) {
        let rate = self.run_rate(env);
        match *cmd {
            MotionCommand::WALK_FORWARD => {
                if *speed > 0.0 {
                    *cmd = MotionCommand::RUN_FORWARD;
                }
                *speed *= rate;
            }
            MotionCommand::TURN_RIGHT => {
                *speed *= TURN_RUN_MULTIPLIER;
            }
            MotionCommand::SIDE_STEP_RIGHT => {
                *speed *= rate;
                if speed.abs() > SIDESTEP_RUN_CLAMP {
                    *speed = SIDESTEP_RUN_CLAMP.copysign(*speed);
                }
            }
            _ => {}
        }
    }

    /// The raw→interpreted transformation, per axis.
    ///
    /// The multipliers are **order-dependent**: `−0.65` / `−1.0` / `1.248` first,
    /// then the hold-key substitution, then `apply_run_to_command`'s run rate, `1.5` and the 3.0
    /// clamp. Out of order changes diagonal running speed noticeably.
    ///
    /// Note `SideStepLeft` **falls through** into the `SideStepRight` case, so `1.248` applies to
    /// both directions.
    pub fn adjust_motion(
        &self,
        cmd: &mut MotionCommand,
        speed: &mut f32,
        mut hold: HoldKey,
        env: &MotionEnv,
    ) {
        if env.has_weenie && !env.is_creature {
            return; // non-creatures are not adjusted
        }
        match *cmd {
            MotionCommand::WALK_BACKWARDS => {
                *cmd = MotionCommand::WALK_FORWARD;
                *speed *= BACKWARDS_MULTIPLIER;
            }
            MotionCommand::TURN_LEFT => {
                *cmd = MotionCommand::TURN_RIGHT;
                *speed *= -1.0;
            }
            MotionCommand::SIDE_STEP_LEFT => {
                *cmd = MotionCommand::SIDE_STEP_RIGHT;
                *speed *= -1.0;
                *speed *= SIDESTEP_MULTIPLIER;
            }
            MotionCommand::SIDE_STEP_RIGHT => {
                *speed *= SIDESTEP_MULTIPLIER;
            }
            MotionCommand::RUN_FORWARD => return, // already interpreted, no hold-key logic
            _ => {}
        }
        if hold == HoldKey::Invalid {
            hold = self.raw_state.current_holdkey;
        }
        if hold == HoldKey::Run {
            self.apply_run_to_command(cmd, speed, env);
        }
    }

    /// `run_rate × 4.0`, or 4.0 with no weenie.
    #[must_use]
    pub fn get_max_speed(&self, env: &MotionEnv) -> f32 {
        self.run_rate(env) * RUN_SPEED
    }

    /// The adjusted maximum speed.
    #[must_use]
    pub fn get_adjusted_max_speed(&self, env: &MotionEnv) -> f32 {
        if self.interpreted_state.forward_command == MotionCommand::RUN_FORWARD {
            (self.interpreted_state.forward_speed / self.current_speed_factor) * RUN_SPEED
        } else {
            self.get_max_speed(env)
        }
    }

    /// The object-local velocity the motion layer
    /// wants, clamped to `run_rate × 4` while preserving direction.
    ///
    /// The forward clamp is exactly the running speed, so it never reduces a pure forward run: it
    /// only limits the diagonal of a run plus a sidestep.
    #[must_use]
    pub fn state_velocity(&self, env: &MotionEnv) -> Vec3 {
        use crate::frame::V3;
        let s = &self.interpreted_state;
        let x = if s.sidestep_command == MotionCommand::SIDE_STEP_RIGHT {
            s.sidestep_speed * SIDESTEP_SPEED
        } else {
            0.0
        };
        let y = if s.forward_command == MotionCommand::WALK_FORWARD {
            s.forward_speed * WALK_SPEED
        } else if s.forward_command == MotionCommand::RUN_FORWARD {
            s.forward_speed * RUN_SPEED
        } else {
            0.0
        };
        let mut v = Vec3::new(x, y, 0.0);
        let maxv = self.get_max_speed(env);
        let mag = v.mag2().sqrt();
        if mag > maxv {
            v = v.mul(maxv / mag);
        }
        v
    }

    /// A gravity-affected creature that is
    /// airborne may only turn, die or fall.
    #[must_use]
    pub fn contact_allows_move(cmd: MotionCommand, env: &MotionEnv) -> bool {
        if cmd == MotionCommand::FALLING
            || cmd == MotionCommand::DEAD
            || cmd == MotionCommand::TURN_RIGHT
            || cmd == MotionCommand::TURN_LEFT
        {
            return true;
        }
        if env.has_weenie && !env.is_creature {
            return true;
        }
        !env.gravity_affected || env.on_ground
    }

    /// Whether the interpreter considers the object standing still.
    #[must_use]
    pub fn is_standing_still(&self, env: &MotionEnv) -> bool {
        env.on_ground
            && self.interpreted_state.forward_command == MotionCommand::READY
            && self.interpreted_state.sidestep_command == MotionCommand::NONE
            && self.interpreted_state.turn_command == MotionCommand::NONE
    }

    /// Queue one motion.
    pub(super) fn add_to_queue(
        &mut self,
        context_id: u32,
        motion: MotionCommand,
        jump_error_code: u32,
    ) {
        self.pending_motions.push_back(MotionNode {
            context_id,
            motion,
            jump_error_code,
        });
    }

    /// Whether any motion is still pending.
    #[must_use]
    pub fn motions_pending(&self) -> bool {
        !self.pending_motions.is_empty()
    }

    /// Pop the head; an action also unsticks and drops the
    /// action from **both** states.
    pub fn motion_done(&mut self, _success: bool, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.motion_done(_success, ctx))
    }

    /// Drains completed motions together with their motion-done tail.
    ///
    /// The client's loop is six statements and the last two are the ones that are easy to
    /// lose:
    ///
    /// ```text
    ///   if (num_anims != 0) exit
    ///   test the action-class bit
    ///   remove the action-state head
    ///   report the motion done with success true
    /// ```
    ///
    /// That reaches [`Self::motion_done`] through the movement manager,
    /// and that is the **only** thing that pops `pending_motions`. The manager and
    /// the interpreter keep two parallel queues here because the client keeps them on two
    /// different objects; a caller that drains one and not the other leaks a node **for ever**,
    /// and `motions_pending()` then prevents movement retakes and keeps completion delivery shut
    /// for the rest of the session.
    ///
    /// **This method exists so that an event-only drain cannot be spelled.** A convention of
    /// "route the events" is too easy to forget at each call site, so it is enforced here:
    /// the MotionTableManager event-only drains are crate-private test adapters.
    /// This isolated adapter and the full MovementManager entry point reach one MotionOwner
    /// continuation; a production host must use the full owner to resolve reentrant callbacks.
    pub fn drain_completed_motions(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.drain_completed_motions(ctx))
    }

    /// The part array's movement handling reaches the motion-table manager's tick, tail
    /// included. That tick's body is byte-identical to the animation-done path; it
    /// is kept as its own entry point because the client keeps two, and because the per-frame
    /// drain and the post-`PerformMovement` drain are different events to a reader of a trace.
    pub fn drain_use_time(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.drain_use_time(ctx))
    }

    /// The part array's animation-done step reaches the motion-table manager's own,
    /// with the same `MotionDone` tail as [`Self::drain_completed_motions`].
    pub fn drain_animation_done(&mut self, success: bool, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.drain_animation_done(success, ctx))
    }

    /// Removes link animations synchronously.
    ///
    /// The chain's first hop is a **tail call**, which is easy to miss:
    ///
    /// ```text
    ///   load the physics object's part array
    ///   if there is no part array, return
    ///   tail jump to part-array link-animation removal
    /// ..which walks &part_array->sequence
    ///   remove link animations through the motion-table manager
    ///   remove all link animations from the sequence
    /// ..and the animation-done step runs once per
    ///   pending node until the list empties
    /// ```
    ///
    /// **Seven call sites.** The link-animation removal entry point is reached from exactly
    /// **seven** places in retail, all in the motion interpreter: the stop-completely path, two
    /// sites in the interpreted-motion entry, the interpreted-motion stop, the interpreted-movement
    /// apply, and two more. It is the one thing in the client that empties a jammed motion queue.
    ///
    /// It is a **call**, not a deferred effect, and the difference is load-bearing: `HitGround`
    /// runs it *before* `apply_current_movement`, so a deferred version would wipe the motions
    /// `apply_current_movement` had just queued.
    pub fn remove_link_animations(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.remove_link_animations(ctx))
    }

    /// Leaving the world.
    pub fn handle_exit_world(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.handle_exit_world(ctx))
    }

    /// The raw entry point.
    ///
    /// The **original, unadjusted** command goes into the raw state while the **adjusted** command
    /// goes into the interpreted state. The action-queue cap is six (`> 5` rejects).
    /// This isolated-interpreter adapter records the external cancellation call. A host owning
    /// a move-to manager must use the combined movement-manager entry point so cancellation
    /// finishes first.
    pub fn do_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        self.with_owner(|owner| owner.do_motion(cmd, params, ctx))
    }

    /// Perform one interpreted motion.
    pub fn do_interpreted_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        self.with_owner(|owner| owner.do_interpreted_motion(cmd, params, ctx))
    }

    /// Stop one interpreted motion.
    pub fn stop_interpreted_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        self.with_owner(|owner| owner.stop_interpreted_motion(cmd, params, ctx))
    }

    /// Isolated effect adapter; hosts owning both
    /// managers use the combined stop-motion path for the synchronous leading cancellation.
    pub fn stop_motion(
        &mut self,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        self.with_owner(|owner| owner.stop_motion(cmd, params, ctx))
    }

    /// Stop completely.
    pub fn stop_completely(&mut self, ctx: &mut MotionCtx<'_>) -> u32 {
        self.with_owner(|owner| owner.stop_completely(ctx))
    }

    /// Enter the default state.
    pub fn enter_default_state(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.enter_default_state(ctx))
    }

    /// Set the hold key -- ignores [`HoldKey::Invalid`].
    pub fn set_hold_key(&mut self, key: HoldKey) {
        if key == HoldKey::Invalid {
            return;
        }
        self.raw_state.current_holdkey = key;
    }

    /// Sets hold-run with the client's gate and tail.
    ///
    /// The gate compares `(on == 0)` with `(current_holdkey != HoldKey::Run)`. If they are equal it
    /// returns without writing; otherwise it updates the hold key and applies current movement with
    /// the supplied link flag and no long-jump hint.
    ///
    /// **The gate reads `current_holdkey` itself**, not *the last input word the caller applied*,
    /// which is a different question. See `Character::apply_input`.
    ///
    /// Note `a == b` when `on` is false and the key is `Invalid`: retail leaves `Invalid` alone
    /// rather than writing `None` over it, because `Invalid` already answers "not Run" to
    /// everything that reads the bit.
    ///
    /// Returns whether the bit moved — i.e. whether the caller owes the `apply_current_movement`
    /// tail. The tail is the caller's here and not this crate's, because the raw/interpreted
    /// choice reads two facts (whether this is the player, `movement_is_autonomous`) that live
    /// above this crate; see [`Self::apply_raw_movement`].
    #[must_use]
    pub fn set_hold_run(&mut self, on: bool) -> bool {
        if !on == (self.raw_state.current_holdkey != HoldKey::Run) {
            return false;
        }
        self.raw_state.current_holdkey = if on { HoldKey::Run } else { HoldKey::None };
        true
    }

    /// The ground-contact notification.
    pub fn hit_ground(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.hit_ground(ctx))
    }

    /// This is where the jump impulse reaches the
    /// integrator.
    pub fn leave_ground(&mut self, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.leave_ground(ctx))
    }

    /// Returns the launch speed `sqrt(height × 19.6)`, i.e. `v = sqrt(2gh)`
    /// with g = 9.8. A non-weenie object jumps at a flat 10 m/s.
    #[must_use]
    pub fn get_jump_v_z(&self, env: &MotionEnv) -> f32 {
        let mut e = self.jump_extent;
        if e < EPSILON {
            return 0.0;
        }
        if e > 1.0 {
            e = 1.0;
        }
        if !env.has_weenie {
            return 10.0;
        }
        if !env.jump_velocity_available {
            return 0.0;
        }
        jump_velocity(env.load, env.jump_skill, e, 1.0)
    }

    /// The velocity the object leaves the ground with.
    #[must_use]
    pub fn get_leave_ground_velocity(&self, env: &MotionEnv) -> Vec3 {
        let mut v = self.state_velocity(env);
        v.z = self.get_jump_v_z(env);
        if v.x.abs() < EPSILON && v.y.abs() < EPSILON && v.z.abs() < EPSILON {
            // Keep whatever drift we had, expressed in the object's local frame.
            let m = crate::frame::l2g(env.position.frame.rotation);
            return crate::frame::globaltolocalvec(m, env.velocity);
        }
        v
    }

    /// Charge a jump.
    pub fn charge_jump(&mut self, extent: f32, env: &MotionEnv) -> u32 {
        if env.has_weenie && !env.jump_permission.unwrap_or(env.load < 2.0) {
            // The client's own jump permission is simply `load < 2.0`.
            let _ = extent;
            return CANT_JUMP_LOADED_DOWN;
        }
        let fc = self.interpreted_state.forward_command;
        if forward_refuses_a_charge(fc) {
            return YOU_CANT_JUMP_FROM_THIS_POSITION;
        }
        if env.on_ground
            && fc == MotionCommand::READY
            && self.interpreted_state.sidestep_command == MotionCommand::NONE
            && self.interpreted_state.turn_command == MotionCommand::NONE
        {
            self.standing_longjump = true;
        }
        0
    }

    /// Whether a jump is allowed.
    #[must_use]
    pub fn jump_is_allowed(&self, env: &MotionEnv) -> u32 {
        if (!env.has_weenie || env.is_creature) && env.gravity_affected && !env.on_ground {
            return YOU_CANT_JUMP_WHILE_IN_THE_AIR;
        }
        if env.fully_constrained {
            return GENERAL_MOVEMENT_FAILURE;
        }
        if let Some(head) = self.pending_motions.front() {
            if head.jump_error_code != 0 {
                return head.jump_error_code;
            }
        }
        // The charge permission.
        if env.has_weenie && !env.jump_permission.unwrap_or(env.load < 2.0) {
            return CANT_JUMP_LOADED_DOWN;
        }
        let fc = self.interpreted_state.forward_command;
        if forward_refuses_a_charge(fc) {
            return YOU_CANT_JUMP_FROM_THIS_POSITION;
        }
        motion_allows_jump(fc)
    }

    /// On success, `set_on_walkable(FALSE)`, which makes the
    /// physics call back into `LeaveGround` and turn `jump_extent` into a velocity.
    /// The combined movement-manager path completes cancellation before testing
    /// permission, including the refused-jump path.
    pub fn jump(&mut self, extent: f32, ctx: &mut MotionCtx<'_>) -> u32 {
        self.with_owner(|owner| owner.jump(extent, ctx))
    }

    /// Apply the interpreted movement.
    ///
    /// The order is **style → forward (or falling / longjump) → sidestep → turn** and it is
    /// observable: each call queues its own transition animations.
    pub fn apply_interpreted_movement(
        &mut self,
        link: bool,
        longjump_hint: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(|owner| owner.apply_interpreted_movement(link, longjump_hint, ctx))
    }

    /// Copy the raw state into the interpreted
    /// state verbatim, run `adjust_motion` on each of the three axes with that axis's raw hold key,
    /// then apply.
    pub fn apply_raw_movement(&mut self, link: bool, longjump_hint: bool, ctx: &mut MotionCtx<'_>) {
        self.with_owner(|owner| owner.apply_raw_movement(link, longjump_hint, ctx))
    }

    /// Apply the current movement.
    ///
    /// `autonomous` is the object's own `last_move_was_autonomous` flag -- combined with
    /// `last_move_was_autonomous` flag — combined with "this is the player".
    pub fn apply_current_movement(
        &mut self,
        link: bool,
        longjump_hint: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(|owner| owner.apply_current_movement(link, longjump_hint, ctx))
    }

    /// The heart of remote-object movement
    /// and of server-forced player movement.
    ///
    /// Two rules matter: the **15-bit wrapping** stamp comparison, and the fact that the player
    /// never replays his own autonomous actions echoed back by the server.
    /// Isolated effect adapter; hosts owning both managers use the combined movement-manager path
    /// to cancel before copying replacement motion.
    pub fn move_to_interpreted_state(
        &mut self,
        state: &InterpretedMotionState,
        is_the_player: bool,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(|owner| owner.move_to_interpreted_state(state, is_the_player, ctx))
    }

    /// Perform the movement -- dispatch, then **always**
    /// `CheckForCompletedMotions`, **tail included**.
    ///
    /// The tail is not `ctx.mgr.check_for_completed_motions(ctx.events)`, which would drain the
    /// manager's queue and keep the events without handing one to the driver's completion route.
    /// Keeping an event is not routing it — see
    /// [`Self::drain_completed_motions`].
    pub fn perform_movement(
        &mut self,
        kind: MovementType,
        cmd: MotionCommand,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) -> u32 {
        self.with_owner(|owner| owner.perform_interpreter_movement(kind, cmd, params, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::NoAssets;

    fn env() -> MotionEnv {
        MotionEnv::default()
    }

    fn loaded(interp: &mut MotionInterp, mgr: &mut MotionTableManager, seq: &mut Sequence) {
        let action = MotionCommand::DOUBLE_SLASH_HIGH;
        assert!(
            action.is_action(),
            "the premise: {action:?} is action-class"
        );
        for (m, n) in [(MotionCommand::READY, 0u32), (action, 1)] {
            mgr.add_to_queue(m, n, seq);
            interp.pending_motions.push_back(MotionNode {
                context_id: 0,
                motion: m,
                jump_error_code: 0,
            });
        }
        interp.interpreted_state.actions.push_back(ActionNode {
            action,
            speed: 1.0,
            stamp: 0,
            autonomous: false,
        });
    }

    fn creature_env() -> MotionEnv {
        MotionEnv {
            is_creature: true,
            gravity_affected: true,
            in_cell: true,
            ..MotionEnv::default()
        }
    }

    /// The leave-ground call, on its own.
    #[test]
    fn leaving_the_ground_empties_the_ledger_and_routes_what_it_drained() {
        let (mut interp, mut mgr, mut seq) = (
            MotionInterp::new(),
            MotionTableManager::new(None),
            Sequence::new(),
        );
        loaded(&mut interp, &mut mgr, &mut seq);
        let (e, mut effects, mut events) = (creature_env(), Vec::new(), Vec::new());
        let mut ctx = MotionCtx {
            mgr: &mut mgr,
            seq: &mut seq,
            assets: &NoAssets,
            env: &e,
            effects: &mut effects,
            events: &mut events,
        };
        assert_eq!(
            interp.pending_motions.len(),
            2,
            "the premise: the ledger is full"
        );
        interp.leave_ground(&mut ctx);
        assert_eq!(interp.pending_motions.len(), 0, "the pending-motion ledger");
        assert_eq!(mgr.pending().len(), 0, "MotionTableManager's ledger");
        assert_eq!(
            interp.interpreted_state.actions.len(),
            0,
            "and the action left the interpreted state, which only motion completion does -- so \
              the drained nodes were routed rather than dropped"
        );
    }

    /// The hit-ground call, on its own. Same body, same
    /// ledger, the other edge.
    #[test]
    fn hitting_the_ground_empties_the_ledger_and_routes_what_it_drained() {
        let (mut interp, mut mgr, mut seq) = (
            MotionInterp::new(),
            MotionTableManager::new(None),
            Sequence::new(),
        );
        loaded(&mut interp, &mut mgr, &mut seq);
        let (e, mut effects, mut events) = (creature_env(), Vec::new(), Vec::new());
        let mut ctx = MotionCtx {
            mgr: &mut mgr,
            seq: &mut seq,
            assets: &NoAssets,
            env: &e,
            effects: &mut effects,
            events: &mut events,
        };
        assert_eq!(
            interp.pending_motions.len(),
            2,
            "the premise: the ledger is full"
        );
        interp.hit_ground(&mut ctx);
        assert_eq!(interp.pending_motions.len(), 0, "the pending-motion ledger");
        assert_eq!(mgr.pending().len(), 0, "MotionTableManager's ledger");
        assert_eq!(
            interp.interpreted_state.actions.len(),
            0,
            "and the action was routed away"
        );
    }

    /// The control for both: a non-creature is not gravity-affected, so neither callback does
    /// anything at all. Without this, the two tests above would pass on a `remove_link_animations`
    /// that fired unconditionally, which is a different function from the client's.
    #[test]
    fn a_non_creature_ground_edge_touches_neither_ledger() {
        let (mut interp, mut mgr, mut seq) = (
            MotionInterp::new(),
            MotionTableManager::new(None),
            Sequence::new(),
        );
        loaded(&mut interp, &mut mgr, &mut seq);
        let e = MotionEnv {
            is_creature: false,
            gravity_affected: true,
            ..MotionEnv::default()
        };
        let (mut effects, mut events) = (Vec::new(), Vec::new());
        let mut ctx = MotionCtx {
            mgr: &mut mgr,
            seq: &mut seq,
            assets: &NoAssets,
            env: &e,
            effects: &mut effects,
            events: &mut events,
        };
        interp.hit_ground(&mut ctx);
        interp.leave_ground(&mut ctx);
        // The weenie-backed predicate's two tests, read as flag tests rather than as an expression:
        // a query on the weenie (the creature question), and a test of the physics object's state
        // word -- bit `0x400`, gravity. Either failing skips the rest and the callback does
        // nothing at all.
        assert_eq!(
            interp.pending_motions.len(),
            2,
            "both tests refused the callback"
        );
        assert_eq!(
            mgr.pending().len(),
            2,
            "and the manager's ledger is untouched"
        );
        assert_eq!(
            interp.interpreted_state.actions.len(),
            1,
            "and the action is still queued"
        );
    }

    fn with_ctx<R>(f: impl FnOnce(&mut MotionCtx<'_>) -> R) -> R {
        let mut mgr = MotionTableManager::new(None);
        let mut seq = Sequence::new();
        let e = env();
        let mut effects = Vec::new();
        let mut events = Vec::new();
        let mut ctx = MotionCtx {
            mgr: &mut mgr,
            seq: &mut seq,
            assets: &NoAssets,
            env: &e,
            effects: &mut effects,
            events: &mut events,
        };
        f(&mut ctx)
    }

    /// ORACLE: the worked table in
    /// the recovered movement-manager behavior, "The run
    /// rate", each row of which was evaluated from the expression. Contract 12.4.
    #[test]
    fn the_run_rate_table_reproduces_including_the_800_discontinuity() {
        let about = |got: f32, want: f32| {
            assert!((got - want).abs() < 5e-6, "{got} != {want}");
        };
        about(get_run_rate(0.0, 0, 1.0), 1.0);
        about(get_run_rate(0.0, 100, 1.0), 1.916_666_7);
        about(get_run_rate(0.0, 200, 1.0), 2.375);
        about(get_run_rate(0.0, 300, 1.0), 2.65);
        about(get_run_rate(0.0, 600, 1.0), 3.0625);
        about(get_run_rate(0.0, 799, 1.0), 3.199_449);
        about(get_run_rate(0.0, 800, 1.0), 4.5);
        about(get_run_rate(0.0, 801, 1.0), 3.200_549);
        about(get_run_rate(0.0, 9999, 1.0), 3.696_073_3);

        // The world speeds those rates imply.
        about(get_run_rate(0.0, 799, 1.0) * 4.0, 12.797_796);
        about(get_run_rate(0.0, 800, 1.0) * 4.0, 18.0);
        about(get_run_rate(0.0, 801, 1.0) * 4.0, 12.802_196);
    }

    /// The discontinuity itself, stated as the property: the curve is not monotonic, 800 is about
    /// 41 % faster than 801, and the general expression at 800 would have given 3.2.
    #[test]
    fn skill_800_is_not_on_its_own_curve() {
        let r799 = get_run_rate(0.0, 799, 1.0);
        let r800 = get_run_rate(0.0, 800, 1.0);
        let r801 = get_run_rate(0.0, 801, 1.0);
        assert!(r800 > r799 && r800 > r801, "non-monotonic");
        let ratio = r800 / r801;
        assert!((ratio - 1.406_0).abs() < 0.001, "{ratio}");
        // What the curve itself gives at 800.
        let on_curve = ((800.0_f32 / 1000.0) * 11.0 + 4.0) / 4.0;
        assert!((on_curve - 3.2).abs() < 1e-6);
        assert!(
            (r800 - on_curve).abs() > 1.0,
            "the early return really does substitute 4.5"
        );
    }

    /// `LoadMod` multiplies the skill term but not the `+ 4.0`, so the rate never falls below 1.0.
    #[test]
    fn encumbrance_can_never_drive_the_run_rate_below_one() {
        assert_eq!(load_mod(0.0), 1.0);
        assert_eq!(load_mod(0.999), 1.0);
        assert_eq!(load_mod(1.5), 0.5);
        assert_eq!(load_mod(2.0), 0.0);
        assert_eq!(load_mod(3.0), 0.0);
        let r = get_run_rate(1.5, 300, 1.0);
        assert!((r - 1.825).abs() < 1e-5, "{r}");
        for skill in [0, 100, 300, 801, 9999] {
            assert_eq!(
                get_run_rate(2.0, skill, 1.0),
                1.0,
                "skill {skill} fully encumbered"
            );
        }
        // Skill 800 is the exception to the exception: its early return fires before the movement
        // modifier is applied, so a fully encumbered character at exactly 800 still runs at 4.5.
        assert_eq!(get_run_rate(2.0, 800, 1.0), 4.5);
    }

    /// `GetJumpHeight`'s worked table, and the 0.35 m floor applied **last** (contract 5.9).
    #[test]
    fn the_jump_height_table_reproduces_and_the_floor_is_applied_last() {
        let about = |got: f32, want: f32| assert!((got - want).abs() < 1e-4, "{got} != {want}");
        about(get_jump_height(0.0, 0, 1.0, 1.0), 0.35);
        about(get_jump_height(0.0, 0, 0.5, 1.0), 0.35);
        about(get_jump_height(0.0, 100, 1.0, 1.0), 1.635_714_3);
        about(get_jump_height(0.0, 300, 1.0, 1.0), 4.2125);
        about(get_jump_height(0.0, 300, 0.5, 1.0), 2.106_25);
        // The floor bites *after* the extent multiply: a feather touch at skill 300 still reaches
        // exactly 0.35, where an "extent applied after the floor" reading would give 0.00035.
        about(get_jump_height(0.0, 300, 0.001, 1.0), 0.35);
        about(get_jump_height(0.0, 1000, 1.0, 1.0), 9.702_174);
        // vz = sqrt(h * 19.6).
        about(
            (get_jump_height(0.0, 300, 1.0, 1.0) * 19.6).sqrt(),
            9.086_528,
        );
        about((0.35_f32 * 19.6).sqrt(), 2.619_160_2);
    }

    /// `JumpStaminaCost` and the power bar.
    #[test]
    fn the_jump_stamina_and_power_bar_formulas_reproduce() {
        assert_eq!(jump_stamina_cost(0.0, 1.0, false), 6);
        assert_eq!(jump_stamina_cost(0.0, 0.5, false), 4);
        assert_eq!(jump_stamina_cost(1.0, 1.0, false), 14);
        assert_eq!(power_bar_level(0.4, false), 0.4);
        assert!(
            (power_bar_level(0.4, true) - 0.5).abs() < 1e-6,
            "0.4 / 0.8 in DualWield"
        );
        assert_eq!(power_bar_level(2.0, false), 1.0);
        assert_eq!(
            power_bar_level(0.0, false),
            MIN_JUMP_EXTENT,
            "a tap is 0.001, not 0"
        );
    }

    /// `adjust_motion`'s multipliers, in order. Contract 5.8.
    #[test]
    fn adjust_motion_applies_its_multipliers_in_the_documented_order() {
        let mi = MotionInterp::new();
        let e = env();
        // Walking backwards becomes WalkForward at -0.65 and stays WalkForward under the run key.
        let mut cmd = MotionCommand::WALK_BACKWARDS;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::None, &e);
        assert_eq!(cmd, MotionCommand::WALK_FORWARD);
        assert!((speed - -0.65).abs() < 1e-6);

        let mut cmd = MotionCommand::WALK_BACKWARDS;
        let mut speed = 1.0;
        let e2 = MotionEnv {
            run_rate: Some(2.0),
            ..e
        };
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e2);
        assert_eq!(
            cmd,
            MotionCommand::WALK_FORWARD,
            "never RunForward: the speed is negative"
        );
        assert!((speed - -1.3).abs() < 1e-6, "-0.65 then the run rate");

        // TurnLeft folds to TurnRight with -1, then x1.5 under the run key.
        let mut cmd = MotionCommand::TURN_LEFT;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e2);
        assert_eq!(cmd, MotionCommand::TURN_RIGHT);
        assert!((speed - -1.5).abs() < 1e-6);

        // Both sidestep directions get 1.248; the left one also gets -1 first.
        let mut cmd = MotionCommand::SIDE_STEP_RIGHT;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::None, &e);
        assert!((speed - 1.248).abs() < 1e-6);
        let mut cmd = MotionCommand::SIDE_STEP_LEFT;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::None, &e);
        assert_eq!(cmd, MotionCommand::SIDE_STEP_RIGHT);
        assert!((speed - -1.248).abs() < 1e-6);

        // WalkForward under the run key becomes RunForward at the run rate.
        let mut cmd = MotionCommand::WALK_FORWARD;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e2);
        assert_eq!(cmd, MotionCommand::RUN_FORWARD);
        assert!((speed - 2.0).abs() < 1e-6);

        // RunForward returns immediately: no hold-key logic at all.
        let mut cmd = MotionCommand::RUN_FORWARD;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e2);
        assert_eq!(speed, 1.0);
    }

    /// The sidestep clamp bites at `run_rate = 3 / 1.248 = 2.40385`.
    #[test]
    fn the_sidestep_run_clamp_bites_at_the_documented_rate() {
        let mi = MotionInterp::new();
        let e = MotionEnv {
            run_rate: Some(2.0),
            ..env()
        };
        let mut cmd = MotionCommand::SIDE_STEP_RIGHT;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e);
        assert!((speed - 2.496).abs() < 1e-5, "1.248 * 2.0");

        let e = MotionEnv {
            run_rate: Some(2.404),
            ..env()
        };
        let mut cmd = MotionCommand::SIDE_STEP_RIGHT;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e);
        assert_eq!(speed, 3.0, "clamped");
        // The clamp keeps the sign.
        let mut cmd = MotionCommand::SIDE_STEP_LEFT;
        let mut speed = 1.0;
        mi.adjust_motion(&mut cmd, &mut speed, HoldKey::Run, &e);
        assert_eq!(speed, -3.0);
    }

    /// The world-speed table: `adjust_motion` then `get_state_velocity`.
    #[test]
    fn the_world_speed_table_reproduces() {
        let mut mi = MotionInterp::new();
        let e = env();
        let about = |got: f32, want: f32| assert!((got - want).abs() < 1e-4, "{got} != {want}");

        mi.interpreted_state.forward_command = MotionCommand::WALK_FORWARD;
        mi.interpreted_state.forward_speed = 1.0;
        about(mi.state_velocity(&e).y, 3.12);

        mi.interpreted_state.forward_speed = -0.65;
        about(mi.state_velocity(&e).y, -2.028);

        mi.interpreted_state.forward_command = MotionCommand::RUN_FORWARD;
        mi.interpreted_state.forward_speed = 1.0;
        about(mi.state_velocity(&e).y, 4.0);

        let e265 = MotionEnv {
            run_rate: Some(2.65),
            ..e
        };
        mi.interpreted_state.forward_speed = 2.65;
        about(mi.state_velocity(&e265).y, 10.6);

        let emax = MotionEnv {
            run_rate: Some(3.696_073_3),
            ..e
        };
        mi.interpreted_state.forward_speed = 3.696_073_3;
        about(mi.state_velocity(&emax).y, 14.784_293);

        // Sidestep while walking is exactly half a walking step forward.
        let mut mi = MotionInterp::new();
        mi.interpreted_state.sidestep_command = MotionCommand::SIDE_STEP_RIGHT;
        mi.interpreted_state.sidestep_speed = 1.248;
        about(mi.state_velocity(&e).x, 1.56);
        about(1.56 * 2.0, 3.12);

        // Sidestep while running, rate 2.0.
        mi.interpreted_state.sidestep_speed = 2.496;
        let e2 = MotionEnv {
            run_rate: Some(2.0),
            ..e
        };
        about(mi.state_velocity(&e2).x, 3.12);
        // Clamped at 3.0 -> 3.75 m/s for every rate at or above 2.40385.
        mi.interpreted_state.sidestep_speed = 3.0;
        let e3 = MotionEnv {
            run_rate: Some(3.0),
            ..e
        };
        about(mi.state_velocity(&e3).x, 3.75);
    }

    /// `motion_allows_jump`'s ranges.
    #[test]
    fn motion_allows_jump_blocks_exactly_the_documented_ranges() {
        let blocked = [
            MotionCommand::MAGIC_POWER_UP01,
            MotionCommand::MAGIC_POWER_UP10,
            MotionCommand::MAGIC_POWER_UP01_PURPLE,
            MotionCommand::MAGIC_POWER_UP10_PURPLE,
            MotionCommand::FALLEN,
            MotionCommand::RELOAD,
            MotionCommand::UNLOAD,
            MotionCommand::PICKUP,
            MotionCommand::AIM_LEVEL,
            MotionCommand::MAGIC_PRAY,
            MotionCommand::CROUCH,
            MotionCommand::SITTING,
            MotionCommand::SLEEPING,
            MotionCommand::SANCTUARY,
            MotionCommand::AI_TELEGRAPH_CAST,
        ];
        for c in blocked {
            assert_eq!(motion_allows_jump(c), 0x48, "{c:?} should block a jump");
        }
        // The Purple power-ups by value, and their neighbours, which the final client does not
        // block: the triple thrusts below and `Helper` above.
        assert_eq!(MotionCommand::MAGIC_POWER_UP01_PURPLE.0, 0x1000_012B);
        assert_eq!(MotionCommand::MAGIC_POWER_UP10_PURPLE.0, 0x1000_0134);
        for c in [MotionCommand::TRIPLE_THRUST_HIGH, MotionCommand::HELPER] {
            assert_eq!(motion_allows_jump(c), 0, "{c:?} should allow a jump");
        }
        for c in [
            MotionCommand::READY,
            MotionCommand::WALK_FORWARD,
            MotionCommand::RUN_FORWARD,
            MotionCommand::TURN_RIGHT,
            MotionCommand::CHEER,
        ] {
            assert_eq!(motion_allows_jump(c), 0, "{c:?} should allow a jump");
        }
    }

    /// The raw state never stores `RunForward`, and a style change resets the forward command.
    #[test]
    fn the_raw_state_never_stores_run_forward() {
        let mut s = RawMotionState::default();
        let p = MovementParameters::default();
        s.apply_motion(MotionCommand::RUN_FORWARD, &p);
        assert_eq!(
            s.forward_command,
            MotionCommand::READY,
            "RunForward is excluded"
        );
        s.apply_motion(MotionCommand::WALK_FORWARD, &p);
        assert_eq!(s.forward_command, MotionCommand::WALK_FORWARD);
        s.apply_motion(MotionCommand::SWORD_COMBAT, &p);
        assert_eq!(s.current_style, MotionCommand::SWORD_COMBAT);
        assert_eq!(
            s.forward_command,
            MotionCommand::READY,
            "a style change resets the axis"
        );
        s.remove_motion(MotionCommand::SWORD_COMBAT);
        assert_eq!(
            s.current_style,
            MotionCommand::NON_COMBAT,
            "falls back to NonCombat"
        );
    }

    /// `copy_movement_from` copies the seven scalars and **not** the action list.
    #[test]
    fn copy_movement_from_leaves_the_action_list_alone() {
        let mut a = InterpretedMotionState::default();
        a.actions.push_back(ActionNode {
            action: MotionCommand::CHEER,
            speed: 1.0,
            stamp: 1,
            autonomous: false,
        });
        let b = InterpretedMotionState {
            forward_command: MotionCommand::RUN_FORWARD,
            forward_speed: 2.0,
            ..InterpretedMotionState::default()
        };
        a.copy_movement_from(&b);
        assert_eq!(a.forward_command, MotionCommand::RUN_FORWARD);
        assert_eq!(a.forward_speed, 2.0);
        assert_eq!(a.actions.len(), 1, "the queued action survives");
    }

    /// `DoMotion`'s combat refusals and the six-action cap.
    #[test]
    fn combat_refusals_and_the_action_cap() {
        let mut mi = MotionInterp::new();
        mi.interpreted_state.current_style = MotionCommand::SWORD_COMBAT;
        let p = MovementParameters::default();
        with_ctx(|ctx| {
            assert_eq!(
                mi.do_motion(MotionCommand::CROUCH, &p, ctx),
                CANT_CROUCH_IN_COMBAT
            );
            assert_eq!(
                mi.do_motion(MotionCommand::SITTING, &p, ctx),
                CANT_SIT_IN_COMBAT
            );
            assert_eq!(
                mi.do_motion(MotionCommand::SLEEPING, &p, ctx),
                CANT_LIE_DOWN_IN_COMBAT
            );
            assert_eq!(
                mi.do_motion(MotionCommand::CHEER, &p, ctx),
                CANT_CHAT_EMOTE_IN_COMBAT
            );
        });

        let mut mi = MotionInterp::new();
        for _ in 0..6 {
            mi.interpreted_state.actions.push_back(ActionNode {
                action: MotionCommand::HOP,
                speed: 1.0,
                stamp: 0,
                autonomous: false,
            });
        }
        with_ctx(|ctx| {
            assert_eq!(mi.do_motion(MotionCommand::HOP, &p, ctx), TOO_MANY_ACTIONS);
        });
    }

    /// `contact_allows_move`: a gravity-affected creature in the air may only turn, die or fall.
    #[test]
    fn contact_allows_move_only_lets_an_airborne_creature_turn_die_or_fall() {
        let air = MotionEnv {
            on_ground: false,
            ..env()
        };
        assert!(MotionInterp::contact_allows_move(
            MotionCommand::TURN_LEFT,
            &air
        ));
        assert!(MotionInterp::contact_allows_move(MotionCommand::DEAD, &air));
        assert!(MotionInterp::contact_allows_move(
            MotionCommand::FALLING,
            &air
        ));
        assert!(!MotionInterp::contact_allows_move(
            MotionCommand::WALK_FORWARD,
            &air
        ));
        // Not gravity-affected: anything goes.
        let floaty = MotionEnv {
            gravity_affected: false,
            ..air
        };
        assert!(MotionInterp::contact_allows_move(
            MotionCommand::WALK_FORWARD,
            &floaty
        ));
        // A non-creature is never restricted.
        let prop = MotionEnv {
            is_creature: false,
            ..air
        };
        assert!(MotionInterp::contact_allows_move(
            MotionCommand::WALK_FORWARD,
            &prop
        ));
    }

    /// The jump gates, in the order `jump_is_allowed` applies them.
    #[test]
    fn the_jump_gates_fire_in_order() {
        let mi = MotionInterp::new();
        let air = MotionEnv {
            on_ground: false,
            ..env()
        };
        assert_eq!(mi.jump_is_allowed(&air), YOU_CANT_JUMP_WHILE_IN_THE_AIR);
        let stuck = MotionEnv {
            fully_constrained: true,
            ..env()
        };
        assert_eq!(mi.jump_is_allowed(&stuck), GENERAL_MOVEMENT_FAILURE);
        let heavy = MotionEnv { load: 2.5, ..env() };
        assert_eq!(mi.jump_is_allowed(&heavy), CANT_JUMP_LOADED_DOWN);
        let mut sitting = MotionInterp::new();
        sitting.interpreted_state.forward_command = MotionCommand::SITTING;
        assert_eq!(
            sitting.jump_is_allowed(&env()),
            YOU_CANT_JUMP_FROM_THIS_POSITION
        );
        assert_eq!(mi.jump_is_allowed(&env()), 0);
    }

    /// A standing long jump latches only from a complete standstill.
    #[test]
    fn charge_jump_latches_the_standing_long_jump_only_from_a_standstill() {
        let mut mi = MotionInterp::new();
        assert_eq!(mi.charge_jump(1.0, &env()), 0);
        assert!(mi.standing_longjump);

        let mut mi = MotionInterp::new();
        mi.interpreted_state.turn_command = MotionCommand::TURN_RIGHT;
        assert_eq!(mi.charge_jump(1.0, &env()), 0);
        assert!(!mi.standing_longjump, "turning is not a standstill");

        let mut mi = MotionInterp::new();
        mi.interpreted_state.forward_command = MotionCommand::SITTING;
        assert_eq!(
            mi.charge_jump(1.0, &env()),
            YOU_CANT_JUMP_FROM_THIS_POSITION
        );
    }

    /// `Sanctuary` refuses the charge and the jump; `AI_TelegraphCast` refuses only the jump.
    #[test]
    fn sanctuary_refuses_a_charge_and_the_telegraph_only_a_jump() {
        let mut mi = MotionInterp::new();
        mi.interpreted_state.forward_command = MotionCommand::SANCTUARY;
        assert_eq!(
            mi.charge_jump(1.0, &env()),
            YOU_CANT_JUMP_FROM_THIS_POSITION
        );
        assert!(!mi.standing_longjump);
        assert_eq!(mi.jump_is_allowed(&env()), YOU_CANT_JUMP_FROM_THIS_POSITION);

        let mut mi = MotionInterp::new();
        mi.interpreted_state.forward_command = MotionCommand::AI_TELEGRAPH_CAST;
        assert_eq!(
            mi.charge_jump(1.0, &env()),
            0,
            "the charge list does not name it"
        );
        assert_eq!(
            mi.jump_is_allowed(&env()),
            YOU_CANT_JUMP_FROM_THIS_POSITION,
            "the jump list does"
        );
    }

    /// The jump velocity: `sqrt(2 g h)` with g = 9.8, and 10 m/s flat for a non-weenie object.
    #[test]
    fn the_jump_velocity_is_sqrt_two_g_h() {
        let mut mi = MotionInterp::new();
        mi.jump_extent = 1.0;
        let e = MotionEnv {
            jump_skill: 300,
            ..env()
        };
        let vz = mi.get_jump_v_z(&e);
        assert!((vz - 9.086_528).abs() < 1e-4, "{vz}");
        // Below the epsilon: no jump at all.
        mi.jump_extent = 0.0001;
        assert_eq!(mi.get_jump_v_z(&e), 0.0);
        // No weenie: a flat 10 m/s.
        mi.jump_extent = 1.0;
        let e = MotionEnv {
            has_weenie: false,
            ..e
        };
        assert_eq!(mi.get_jump_v_z(&e), 10.0);
    }

    /// The 15-bit wrapping stamp comparison in `move_to_interpreted_state`, in isolation.
    #[test]
    fn the_action_stamp_comparison_wraps_at_fifteen_bits() {
        let newer = |a: u32, b: u32| {
            let (a, b) = (a & 0x7FFF, b & 0x7FFF);
            let d = a.abs_diff(b);
            if d < 0x4000 {
                b < a
            } else {
                a < b
            }
        };
        assert!(newer(5, 4));
        assert!(!newer(4, 5));
        // Across the wrap: 1 is newer than 0x7FFE.
        assert!(newer(1, 0x7FFE));
        assert!(!newer(0x7FFE, 1));
    }
}

//! `MoveToManager`: the node machine that drives an object toward a position, an object
//! or a heading by issuing ordinary motion commands.
//!
//! **Move-to always drives the *interpreted* state**, never the raw one — which is why a
//! server-driven move-to does not stomp on the player's key state.
//!
//! The 20°/340° dead-band on the corrective turn is what makes AC's move-to look like a shallow arc
//! rather than a stepped path.
//!
//! The object half is here too: the move-to and turn-to object entries, their internal forms
//! (**with the range test**, which is what stops a use on something already in reach from
//! walking), the clean-up-and-call-weenie step and the notification back. Without it,
//! `handle_update_target` would push a turn and a walk unconditionally.

use std::collections::VecDeque;

use dereth_primitives::num::consts::EPSILON;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, ObjectId, Position, Vec3};

use crate::command::MotionCommand;
use crate::frame::V3;

use super::interp::MotionCtx;
#[cfg(test)]
use super::interp::MotionInterp;
use super::{flags, MovementParameters};
use crate::table::MovementType;

/// 192 m per landblock, and **z is always 0**.
///
/// Shared position arithmetic keeps movement and collision distances in the same block space.
#[must_use]
pub fn block_offset(from: CellId, to: CellId) -> Vec3 {
    dereth_primitives::position::block_offset(from, to)
}

/// The offset between two positions.
#[must_use]
pub fn get_offset(from: &Position, to: &Position) -> Vec3 {
    dereth_primitives::position::get_offset(from, to)
}

/// The distance between two positions.
#[must_use]
pub fn distance(a: &Position, b: &Position) -> f32 {
    dereth_primitives::position::distance(a, b)
}

/// The horizontal term is the **3-D** centre distance.
#[must_use]
pub fn cylinder_distance(r1: f32, h1: f32, p1: &Position, r2: f32, h2: f32, p2: &Position) -> f32 {
    dereth_primitives::position::cylinder_distance(r1, h1, p1, r2, h2, p2)
}

/// The bearing from `from` to `to`, in degrees, with the same
/// `450 −` convention as heading extraction. A degenerate offset gives 0.
#[must_use]
pub fn position_heading(from: &Position, to: &Position) -> f32 {
    let mut d = get_offset(from, to);
    d.z = 0.0;
    if d.normalize_check_small() {
        return 0.0;
    }
    let deg = 450.0 - math::atan2(f64::from(d.y), f64::from(d.x)) * (180.0 / std::f64::consts::PI);
    #[allow(clippy::cast_possible_truncation)]
    {
        (deg % 360.0) as f32
    }
}

/// The heading difference -- always in `[0, 360)`.
#[must_use]
pub fn heading_diff(a: f32, b: f32, cmd: MotionCommand) -> f32 {
    let mut d = a - b;
    if d.abs() < EPSILON {
        d = 0.0;
    }
    if d < -EPSILON {
        d += 360.0;
    }
    if d > EPSILON && cmd != MotionCommand::TURN_RIGHT {
        d = 360.0 - d;
    }
    d
}

/// Answers "have we passed `b` turning in direction `cmd`?".
#[must_use]
pub fn heading_greater(a: f32, b: f32, cmd: MotionCommand) -> bool {
    let r = if (a - b).abs() <= 180.0 {
        a <= b
    } else {
        b <= a
    };
    let mut res = !r;
    if cmd != MotionCommand::TURN_RIGHT {
        res = !res;
    }
    res
}

/// Set target `id` with context 0, radius 0.5 and quantum 0.0 -- the drift radius that every
/// move-to and turn-to request asks its target manager for. Both entry points pass this literal,
/// and ACE's port
/// (`MoveToManager.cs`) carries the same two constants: `0.5f` radius, `0.0f` quantum.
pub const TARGET_RADIUS: f32 = 0.5;

/// The four `MovementStruct` request forms that movement dispatch consumes.
///
/// The three object-shaped arms carry the facts that the movement host resolves
/// off the **object table** before it builds the struct: the top-level (unparented) id, and the
/// target's radius and height. The application owns the object table, so it
/// resolves them and hands them here; this crate never looks an object up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveToRequest {
    /// `MovementTypes::MoveToObject` (6).
    MoveToObject {
        object_id: ObjectId,
        top_level_id: ObjectId,
        radius: f32,
        height: f32,
    },
    /// `MovementTypes::MoveToPosition` (7).
    MoveToPosition { pos: Position },
    /// `MovementTypes::TurnToObject` (8).
    TurnToObject {
        object_id: ObjectId,
        top_level_id: ObjectId,
    },
    /// `MovementTypes::TurnToHeading` (9).
    TurnToHeading,
}

/// `MovementNode { type, heading }` — only `MoveToPosition (7)` and `TurnToHeading (9)` are ever
/// used as node types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementNode {
    pub kind: MovementType,
    pub heading: f32,
}

/// The target information `TargetManager` feeds a move-to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetInfo {
    pub object_id: ObjectId,
    pub ok: bool,
    pub target_position: Position,
    pub interpolated_position: Position,
}

/// The target's position, velocity and status, resolved by the host from its object
/// table once per frame for the object this driver watches.
/// `MotionDriver::handle_targetting` consumes them once per physics sub-step.
///
/// `ok == false` is `TargetStatus != Ok`: the object has left the world, which the target
/// manager reports to its voyeurs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetSnapshot {
    pub object_id: ObjectId,
    pub ok: bool,
    pub position: Position,
    /// The target's **`cached_velocity`**, not
    /// its integrated velocity: a walking creature's integrator velocity is zero in walkable
    /// contact, and a blocked object's cached velocity is zero, so this is the one that leads a
    /// moving target and stops leading a stopped one.
    pub velocity: Vec3,
}

/// Target updates use the simulation-time gate `now - last_update_time >= 0.5`.
/// This repeats `dereth_physics::globals::TARGET_TICK` because the animation crate
/// does not depend on the physics crate.
pub const TARGET_TICK: f64 = 0.5;

/// The state machine for approaching a position or object and turning toward a target.
#[derive(Debug, Clone)]
pub struct MoveToManager {
    pub movement_type: MovementType,
    pub sought_position: Position,
    pub current_target_position: Position,
    pub starting_position: Position,
    pub movement_params: MovementParameters,
    pub previous_heading: f32,
    pub previous_distance: f32,
    pub previous_distance_time: f64,
    pub original_distance: f32,
    pub original_distance_time: f64,
    /// Incremented and reset, and **read by nothing** — dead state in the client and in ACE.
    /// Kept because its resets are how a test sees progress being made.
    pub fail_progress_count: u32,
    pub sought_object_id: ObjectId,
    pub top_level_object_id: ObjectId,
    pub sought_object_radius: f32,
    pub sought_object_height: f32,
    pub current_command: MotionCommand,
    pub aux_command: MotionCommand,
    pub moving_away: bool,
    pub initialized: bool,
    pub pending_actions: VecDeque<MovementNode>,
    /// A host option, **never set by this client**: when true, beginning a turn-to-heading node
    /// does not wait for the body's pending motions to drain, and the turn starts at once.
    ///
    /// The client always waits (the default, `false`), and nothing in the client path writes the
    /// field, so its behaviour is unchanged. A host that drives this manager for bodies it
    /// simulates (a server turning a caster that has just finished another action) sets it around
    /// the request and clears it afterwards. The manager never resets it.
    pub always_turn: bool,
}

fn null_position() -> Position {
    Position::new(CellId(0), dereth_primitives::Frame::default())
}

impl Default for MoveToManager {
    fn default() -> Self {
        Self {
            movement_type: MovementType::Invalid,
            sought_position: null_position(),
            current_target_position: null_position(),
            starting_position: null_position(),
            movement_params: MovementParameters::default(),
            previous_heading: 0.0,
            previous_distance: f32::MAX,
            previous_distance_time: 0.0,
            original_distance: f32::MAX,
            original_distance_time: 0.0,
            fail_progress_count: 0,
            sought_object_id: ObjectId(0),
            top_level_object_id: ObjectId(0),
            sought_object_radius: 0.0,
            sought_object_height: 0.0,
            current_command: MotionCommand::NONE,
            aux_command: MotionCommand::NONE,
            moving_away: false,
            initialized: false,
            pending_actions: VecDeque::new(),
            always_turn: false,
        }
    }
}

impl MoveToManager {
    /// **The isolated adapters below are a SECOND DOOR, and they are shut.**
    ///
    /// The production path into this machine is
    /// [`crate::motion::MovementManager`], which owns all three workers
    /// (`owns_moveto: true, owns_sticky: true`) and resolves every callback synchronously. These
    /// inherent methods fabricate a **throwaway** [`super::StickyManager`] with
    /// `owns_sticky: false`, so a sticky hand-off taken through them escapes as a
    /// [`super::MotionEffect::StickTo`] against a manager that is dropped on return — and
    /// `WorldScene::apply_object_effects` and `Character::apply_effect_list` both declare that
    /// variant `unreachable!()`. Code that wired one of these would get a panic, not a stick.
    ///
    /// They have **no callers outside this file's own tests**, so rather than leave a wrong door
    /// open they are `#[cfg(test)]`:
    /// the tests below keep them, nothing else can reach them, and the only public way in stays
    /// `MovementManager`. The matching isolated motion-interpreter adapters in `interp.rs`
    /// are not gated this way.
    #[cfg(test)]
    fn with_owner<R>(
        &mut self,
        interp: &mut MotionInterp,
        f: impl FnOnce(&mut super::owner::MotionOwner<'_>) -> R,
    ) -> R {
        let mut sticky = super::StickyManager::new();
        f(&mut super::owner::MotionOwner {
            interp,
            moveto: self,
            sticky: &mut sticky,
            owns_moveto: true,
            owns_sticky: false,
        })
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_moving_to(&self) -> bool {
        self.movement_type != MovementType::Invalid
    }

    /// Initialise the manager's local variables.
    pub(super) fn initialize_local_variables(&mut self, now: f64) {
        let params = MovementParameters::default();
        *self = Self {
            previous_distance_time: now,
            original_distance_time: now,
            movement_params: params,
            ..Self::default()
        };
    }

    pub(super) fn default_params(&self) -> MovementParameters {
        MovementParameters {
            speed: self.movement_params.speed,
            hold_key_to_apply: self.movement_params.hold_key_to_apply,
            // `p.flags &= ~0x8000` — do not cancel ourselves.
            flags: MovementParameters::default().flags & !flags::CANCEL_MOVETO,
            ..MovementParameters::default()
        }
    }

    /// The current distance to the target.
    #[must_use]
    pub fn current_distance(&self, ctx: &MotionCtx<'_>) -> f32 {
        if self.movement_params.has(flags::USE_SPHERES) {
            cylinder_distance(
                ctx.env.radius,
                ctx.env.height,
                &ctx.env.position,
                self.sought_object_radius,
                self.sought_object_height,
                &self.current_target_position,
            )
        } else {
            distance(&ctx.env.position, &self.current_target_position)
        }
    }

    /// Move to a position.
    #[cfg(test)]
    pub fn move_to_position(
        &mut self,
        interp: &mut MotionInterp,
        pos: Position,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(interp, |owner| {
            owner.moveto_move_to_position(pos, params, ctx)
        })
    }

    /// The one door a server movement buffer comes
    /// through.
    ///
    /// The two lines before the switch are not decoration: **every move-to cancels the one in
    /// flight** with `ACTION_CANCELLED (0x36)` and unsticks, so a second use while walking
    /// restarts the approach instead of queueing behind it.
    #[cfg(test)]
    pub fn perform_movement(
        &mut self,
        req: &MoveToRequest,
        params: &MovementParameters,
        interp: &mut MotionInterp,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(interp, |owner| {
            owner.moveto_perform_movement(req, params, ctx)
        })
    }

    /// **The entry point the whole approach hangs on.**
    ///
    /// It starts **no motion**. It records the target and asks the physics object's target manager
    /// to watch it; the walk begins only when the first `TargetInfo` arrives at
    /// [`Self::handle_update_target`], which is why an approach to an object the client does not
    /// know about never starts at all.
    ///
    /// Note what it does **not** do, against [`Self::move_to_position`] which sits beside it: it
    /// does not clear [`flags::STICKY`], because sticking to the thing you walked to is exactly
    /// what a sticky move-to is for.
    ///
    /// The self-target arm (`top_level_id` == my own id) is the client's guard against a move-to
    /// aimed at yourself: it cleans up and stops rather than tracking, and no target is ever set,
    /// which is what makes the matching arm in `HandleUpdateTarget` unreachable in practice.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    pub fn move_to_object(
        &mut self,
        object_id: ObjectId,
        top_level_id: ObjectId,
        radius: f32,
        height: f32,
        params: &MovementParameters,
        interp: &mut MotionInterp,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(interp, |owner| {
            owner.moveto_move_to_object(object_id, top_level_id, radius, height, params, ctx)
        })
    }

    /// Turn to an object.
    ///
    /// The corpus carries **151** of these against **22** move-tos, because the server turns you to
    /// face what you used far more often than it walks you to it.
    ///
    /// `StopCompletely` here is conditional on the parameter bit, unlike
    /// [`Self::move_to_object`]'s unconditional one; and the heading it stows on
    /// `current_target_position` is **overwritten** by `turn_to_object_internal` before anything
    /// reads it. That is not a transcription slip: ACE's `MoveToManager.TurnToObject` does the
    /// same, off the same binary.
    #[cfg(test)]
    pub fn turn_to_object(
        &mut self,
        object_id: ObjectId,
        top_level_id: ObjectId,
        params: &MovementParameters,
        interp: &mut MotionInterp,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(interp, |owner| {
            owner.moveto_turn_to_object(object_id, top_level_id, params, ctx)
        })
    }

    /// Turn to a heading.
    #[cfg(test)]
    pub fn turn_to_heading(
        &mut self,
        interp: &mut MotionInterp,
        params: &MovementParameters,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(interp, |owner| owner.moveto_turn_to_heading(params, ctx))
    }

    /// Begin the next node.
    #[cfg(test)]
    fn begin_next_node(&mut self, interp: &mut MotionInterp, ctx: &mut MotionCtx<'_>) {
        self.with_owner(interp, |owner| owner.moveto_begin_next_node(ctx))
    }

    /// The per-frame tick.
    ///
    /// **No ticking while airborne** (`transient_state & 1`), and no ticking until the first
    /// `TargetInfo` has arrived for an object-targeted move.
    #[cfg(test)]
    pub fn use_time(&mut self, interp: &mut MotionInterp, ctx: &mut MotionCtx<'_>) {
        self.with_owner(interp, |owner| owner.moveto_use_time(ctx))
    }

    /// A one-second grace window and a 0.25 m/s
    /// minimum closing rate, checked against **both** the recent and the overall window.
    pub(super) fn check_progress_made(&mut self, dist: f32, now: f64) -> bool {
        let dt = now - self.previous_distance_time;
        if dt <= 1.0 {
            return true;
        }
        let delta = if self.moving_away {
            dist - self.previous_distance
        } else {
            self.previous_distance - dist
        };
        #[allow(clippy::cast_possible_truncation)]
        let dt32 = dt as f32;
        if delta / dt32 >= 0.25 {
            self.previous_distance = dist;
            self.previous_distance_time = now;
            let delta2 = if self.moving_away {
                dist - self.original_distance
            } else {
                self.original_distance - dist
            };
            #[allow(clippy::cast_possible_truncation)]
            let odt = (now - self.original_distance_time) as f32;
            if delta2 / odt >= 0.25 {
                return true;
            }
        }
        false
    }

    /// Handle a target update.
    ///
    /// Note the asymmetry: the **first** update reports `0x38 NoObject`, later ones
    /// `0x37 ObjectGone`.
    #[cfg(test)]
    pub fn handle_update_target(
        &mut self,
        info: &TargetInfo,
        interp: &mut MotionInterp,
        ctx: &mut MotionCtx<'_>,
    ) {
        self.with_owner(interp, |owner| owner.moveto_handle_update_target(info, ctx))
    }

    /// How a move-to resumes after a jump or a fall.
    #[cfg(test)]
    pub fn hit_ground(&mut self, interp: &mut MotionInterp, ctx: &mut MotionCtx<'_>) {
        self.with_owner(interp, |owner| owner.moveto_hit_ground(ctx))
    }

    /// Only when a move is actually in progress.
    #[cfg(test)]
    pub fn cancel_move_to(&mut self, err: u32, interp: &mut MotionInterp, ctx: &mut MotionCtx<'_>) {
        self.with_owner(interp, |owner| owner.moveto_cancel_move_to(err, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::set_heading;
    use crate::motion::MotionEffect;
    use crate::motion::MotionEnv;
    use crate::table::MotionTableManager;
    use dereth_primitives::Frame;

    fn pos(x: f32, y: f32, heading: f32) -> Position {
        let mut f = Frame::new(Vec3::new(x, y, 0.0), dereth_primitives::Quat::IDENTITY);
        set_heading(&mut f, heading);
        Position::new(CellId(0xA9B4_0001), f)
    }

    /// A minimal but *real* motion table: `NonCombat` with a `Ready` default, cycles for `Ready`,
    /// `WalkForward` and `TurnRight`, and links between them. The node machine drives ordinary
    /// motion commands, so without a table every `_DoMotion` returns `NoAnimationTable (7)` and
    /// the move-to cancels itself before it starts.
    fn table() -> (
        std::sync::Arc<crate::data::MotionTableData>,
        crate::data::MapAssets,
    ) {
        use crate::data::{AnimData, AnimationData, MapAssets, MotionData, MotionTableData};
        use crate::table::MotionTable;
        use dereth_primitives::DataId;

        let mut assets = MapAssets::default();
        for i in 1..=8u32 {
            assets.animations.insert(
                0x0300_0000 | i,
                std::sync::Arc::new(AnimationData {
                    num_frames: 2,
                    num_parts: 1,
                    pos_frames: None,
                    part_frames: vec![crate::data::AnimFrame::default(); 2],
                    has_hooks: false,
                }),
            );
        }
        let one = |i: u32| MotionData {
            anims: vec![AnimData {
                anim_id: DataId(0x0300_0000 | i),
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            }],
            ..MotionData::default()
        };
        let style = MotionCommand::NON_COMBAT;
        let mut d = MotionTableData {
            default_style: style,
            ..MotionTableData::default()
        };
        d.style_defaults.insert(style.0, MotionCommand::READY);
        d.cycles
            .insert(MotionTable::key(style, MotionCommand::READY), one(1));
        d.cycles
            .insert(MotionTable::key(style, MotionCommand::WALK_FORWARD), one(2));
        d.cycles
            .insert(MotionTable::key(style, MotionCommand::RUN_FORWARD), one(3));
        d.cycles
            .insert(MotionTable::key(style, MotionCommand::TURN_RIGHT), one(4));
        let mut from_ready = std::collections::BTreeMap::new();
        from_ready.insert(MotionCommand::WALK_FORWARD.0, one(5));
        from_ready.insert(MotionCommand::RUN_FORWARD.0, one(5));
        from_ready.insert(MotionCommand::TURN_RIGHT.0, one(6));
        d.links
            .insert(MotionTable::key(style, MotionCommand::READY), from_ready);
        for from in [
            MotionCommand::WALK_FORWARD,
            MotionCommand::RUN_FORWARD,
            MotionCommand::TURN_RIGHT,
        ] {
            let mut back = std::collections::BTreeMap::new();
            back.insert(MotionCommand::READY.0, one(7));
            back.insert(MotionCommand::TURN_RIGHT.0, one(8));
            back.insert(MotionCommand::WALK_FORWARD.0, one(8));
            d.links.insert(MotionTable::key(style, from), back);
        }
        (std::sync::Arc::new(d), assets)
    }

    struct Harness {
        mgr: MotionTableManager,
        seq: crate::seq::Sequence,
        assets: crate::data::MapAssets,
        env: MotionEnv,
        effects: Vec<MotionEffect>,
        events: Vec<crate::hooks::AnimEvent>,
    }

    impl Harness {
        fn new() -> Self {
            let (table, assets) = table();
            let mut mgr = MotionTableManager::new(Some(table));
            let mut seq = crate::seq::Sequence::new();
            mgr.initialize_state(&mut seq, &assets);
            Self {
                mgr,
                seq,
                assets,
                env: MotionEnv {
                    position: pos(0.0, 0.0, 0.0),
                    ..MotionEnv::default()
                },
                effects: Vec::new(),
                events: Vec::new(),
            }
        }
        fn ctx(&mut self) -> MotionCtx<'_> {
            MotionCtx {
                mgr: &mut self.mgr,
                seq: &mut self.seq,
                assets: &self.assets,
                env: &self.env,
                effects: &mut self.effects,
                events: &mut self.events,
            }
        }
    }

    /// ORACLE: the recovered movement-manager behavior
    /// section 6, the heading difference and the heading comparison.
    #[test]
    fn heading_diff_is_always_in_zero_to_360_and_direction_aware() {
        // Turning right: the difference is taken as-is.
        assert_eq!(heading_diff(90.0, 0.0, MotionCommand::TURN_RIGHT), 90.0);
        assert_eq!(heading_diff(0.0, 90.0, MotionCommand::TURN_RIGHT), 270.0);
        // Turning left: the complement.
        assert_eq!(heading_diff(90.0, 0.0, MotionCommand::TURN_LEFT), 270.0);
        // Equal headings collapse to zero rather than to a tiny residue.
        assert_eq!(heading_diff(45.0, 45.0, MotionCommand::TURN_RIGHT), 0.0);
        assert_eq!(
            heading_diff(45.000_01, 45.0, MotionCommand::TURN_RIGHT),
            0.0
        );
        for (a, b) in [(0.0, 359.9), (359.9, 0.0), (180.0, 0.0), (0.0, 180.0)] {
            let d = heading_diff(a, b, MotionCommand::TURN_RIGHT);
            assert!((0.0..360.0).contains(&d), "{a} {b} -> {d}");
        }
    }

    /// `heading_greater` answers "have we passed b turning this way?".
    #[test]
    fn heading_greater_flips_with_the_turn_direction() {
        assert!(heading_greater(90.0, 45.0, MotionCommand::TURN_RIGHT));
        assert!(!heading_greater(30.0, 45.0, MotionCommand::TURN_RIGHT));
        assert!(heading_greater(30.0, 45.0, MotionCommand::TURN_LEFT));
        assert!(!heading_greater(90.0, 45.0, MotionCommand::TURN_LEFT));
    }

    /// The distance primitives cross a landblock boundary correctly: two objects one metre apart
    /// across a boundary have origins ~192 m apart.
    #[test]
    fn the_distance_primitives_go_through_the_block_offset() {
        let a = Position::new(
            CellId(0xA9B4_0001),
            Frame::new(
                Vec3::new(191.5, 0.0, 0.0),
                dereth_primitives::Quat::IDENTITY,
            ),
        );
        let b = Position::new(
            CellId(0xAAB4_0001),
            Frame::new(Vec3::new(0.5, 0.0, 0.0), dereth_primitives::Quat::IDENTITY),
        );
        assert!(
            (distance(&a, &b) - 1.0).abs() < 1e-3,
            "{}",
            distance(&a, &b)
        );
    }

    /// The node machine: a `MoveToPosition` builds a turn node then a move node, turns until the
    /// heading is reached, moves until it arrives, and then stops completely.
    #[test]
    fn the_node_machine_turns_then_moves_then_stops() {
        let mut h = Harness::new();
        let mut interp = MotionInterp::new();
        let mut m = MoveToManager::new();
        let target = pos(0.0, 10.0, 0.0);
        let params = MovementParameters {
            flags: MovementParameters::default().flags & !flags::USE_SPHERES,
            ..MovementParameters::default()
        };

        h.env.position = pos(0.0, 0.0, 90.0); // facing east, target is north
        m.move_to_position(&mut interp, target, &params, &mut h.ctx());
        assert_eq!(m.pending_actions.len(), 2);
        assert_eq!(m.pending_actions[0].kind, MovementType::TurnToHeading);
        assert_eq!(m.pending_actions[1].kind, MovementType::MoveToPosition);
        assert_eq!(
            m.current_command,
            MotionCommand::TURN_LEFT,
            "north is left of east"
        );
        interp.drain_use_time(&mut h.ctx());
        m.use_time(&mut interp, &mut h.ctx());
        assert_eq!(
            m.current_command,
            MotionCommand::TURN_LEFT,
            "and the tick keeps it turning"
        );

        // Arrive at the heading: the node pops and the move node begins.
        h.env.position = pos(0.0, 0.0, 0.0);
        m.use_time(&mut interp, &mut h.ctx());
        assert_eq!(m.pending_actions.len(), 1);
        assert_eq!(m.pending_actions[0].kind, MovementType::MoveToPosition);
        assert_eq!(m.current_command, MotionCommand::WALK_FORWARD);
        assert!(
            h.effects.contains(&MotionEffect::SetHeading(0.0)),
            "the heading snaps exactly"
        );

        // Arrive at the position: the queue empties and the object stops completely.
        h.env.position = pos(0.0, 9.9, 0.0);
        m.use_time(&mut interp, &mut h.ctx());
        assert!(m.pending_actions.is_empty());
        assert!(!m.is_moving_to(), "CleanUp reset the state");
        assert_eq!(
            interp.interpreted_state.forward_command,
            MotionCommand::READY
        );
        assert_eq!(h.seq.velocity, Vec3::ZERO);
    }

    /// The sticky hand-off fires on arrival when the `sticky` flag is set — and never for a bare
    /// position, because `MoveToPosition` clears the flag.
    #[test]
    fn the_sticky_handoff_fires_on_arrival() {
        let mut h = Harness::new();
        let mut interp = MotionInterp::new();
        let mut m = MoveToManager::new();
        // A move-to-object with the sticky flag: set the state up the way
        // `MoveToObject`/`HandleUpdateTarget` would.
        m.movement_type = MovementType::MoveToObject;
        m.movement_params = MovementParameters {
            flags: (MovementParameters::default().flags | flags::STICKY) & !flags::USE_SPHERES,
            ..MovementParameters::default()
        };
        m.top_level_object_id = ObjectId(0x5000_0001);
        m.sought_object_radius = 0.5;
        m.sought_object_height = 1.2;
        m.begin_next_node(&mut interp, &mut h.ctx());
        assert!(
            h.effects
                .iter()
                .any(|e| matches!(e, MotionEffect::StickTo { .. })),
            "an empty queue with the sticky flag hands off: {:?}",
            h.effects
        );

        // The same with the flag clear stops instead.
        let mut h = Harness::new();
        let mut m = MoveToManager::new();
        m.movement_type = MovementType::MoveToObject;
        m.begin_next_node(&mut interp, &mut h.ctx());
        assert!(!h
            .effects
            .iter()
            .any(|e| matches!(e, MotionEffect::StickTo { .. })));
        assert_eq!(
            interp.interpreted_state.forward_command,
            MotionCommand::READY
        );
        assert_eq!(h.seq.velocity, Vec3::ZERO);

        // A bare position can never be sticky.
        let mut h = Harness::new();
        let mut m = MoveToManager::new();
        let p = MovementParameters {
            flags: MovementParameters::default().flags | flags::STICKY,
            ..MovementParameters::default()
        };
        m.move_to_position(&mut interp, pos(0.0, 0.0, 0.0), &p, &mut h.ctx());
        assert!(!m.movement_params.has(flags::STICKY));
    }

    /// `CheckProgressMade`: a one-second grace window, then a 0.25 m/s minimum closing rate.
    #[test]
    fn check_progress_made_has_a_one_second_grace_window() {
        let mut m = MoveToManager::new();
        m.previous_distance = 10.0;
        m.original_distance = 10.0;
        m.previous_distance_time = 0.0;
        m.original_distance_time = 0.0;
        assert!(m.check_progress_made(10.0, 0.5), "inside the grace window");
        // Two seconds later having closed 1 m: 0.5 m/s, which passes.
        assert!(m.check_progress_made(9.0, 2.0));
        // Two more seconds having closed nothing: fails.
        assert!(!m.check_progress_made(9.0, 4.0));
    }

    /// `CancelMoveTo` reports the error, empties the queue and stops — and does nothing at all when
    /// no move is in progress.
    #[test]
    fn cancel_move_to_is_a_no_op_when_nothing_is_moving() {
        let mut h = Harness::new();
        let mut interp = MotionInterp::new();
        let mut m = MoveToManager::new();
        m.cancel_move_to(ACTION_CANCELLED_FOR_TEST, &mut interp, &mut h.ctx());
        assert!(h.effects.is_empty());

        m.movement_type = MovementType::MoveToPosition;
        m.pending_actions.push_back(MovementNode {
            kind: MovementType::MoveToPosition,
            heading: 0.0,
        });
        m.cancel_move_to(ACTION_CANCELLED_FOR_TEST, &mut interp, &mut h.ctx());
        assert!(h
            .effects
            .contains(&MotionEffect::MoveToFailed(ACTION_CANCELLED_FOR_TEST)));
        assert!(m.pending_actions.is_empty());
        assert!(!m.is_moving_to());
    }

    #[test]
    fn a_replacement_stops_the_old_motion_before_initializing_the_new_target() {
        let mut h = Harness::new();
        let mut interp = MotionInterp::new();
        let mut m = MoveToManager::new();
        let params = MovementParameters::default();
        let old = ObjectId(0x5000_0001);
        let new = ObjectId(0x5000_0002);
        m.move_to_object(old, old, 0.5, 1.0, &params, &mut interp, &mut h.ctx());
        h.effects.clear();
        interp.interpreted_state.forward_command = MotionCommand::WALK_FORWARD;
        interp.raw_state.forward_command = MotionCommand::WALK_FORWARD;
        m.move_to_object(new, new, 0.5, 1.0, &params, &mut interp, &mut h.ctx());
        assert_eq!(
            interp.interpreted_state.forward_command,
            MotionCommand::READY
        );
        assert_eq!(interp.raw_state.forward_command, MotionCommand::READY);
        assert_eq!(m.top_level_object_id, new);
        assert_eq!(m.movement_type, MovementType::MoveToObject);
        assert!(!m.initialized);
        assert_eq!(
            h.effects
                .iter()
                .filter(|e| matches!(e, MotionEffect::MoveToFailed(0x36)))
                .count(),
            1
        );
        assert!(
            !h.effects.contains(&MotionEffect::CancelMoveTo),
            "a delayed cancel would kill the replacement"
        );
        assert!(matches!(h.effects.last(), Some(MotionEffect::SetTarget { id, .. }) if *id == new));
    }

    /// Received movement must complete its leading cancel-move-to call before returning
    /// to a host that will reapply input. There must be no delayed cancellation left to kill it.
    #[test]
    fn manager_stop_cancels_once_before_returning_and_leaves_no_delayed_cancel() {
        let mut h = Harness::new();
        let mut movement = crate::motion::MovementManager::new();
        let target = ObjectId(0x5000_0001);
        movement.perform_movement(
            &crate::motion::MoveToRequest::MoveToObject {
                object_id: target,
                top_level_id: target,
                radius: 0.5,
                height: 1.0,
            },
            &MovementParameters::default(),
            &mut h.ctx(),
        );
        assert!(movement.is_moving_to());
        h.effects.clear();
        assert_eq!(movement.stop_completely(&mut h.ctx()), 0);
        assert!(!movement.is_moving_to());
        assert!(movement.moveto.pending_actions.is_empty());
        assert_eq!(
            movement.interp.raw_state.forward_command,
            MotionCommand::READY
        );
        assert_eq!(
            movement.interp.interpreted_state.forward_command,
            MotionCommand::READY
        );
        assert_eq!(
            h.effects
                .iter()
                .filter(|e| matches!(e, MotionEffect::MoveToFailed(0x36)))
                .count(),
            1
        );
        assert!(h.effects.contains(&MotionEffect::ClearTarget));
        assert!(!h.effects.contains(&MotionEffect::CancelMoveTo));

        h.effects.clear();
        movement.stop_completely(&mut h.ctx());
        assert!(
            !h.effects.iter().any(|e| matches!(
                e,
                MotionEffect::CancelMoveTo
                    | MotionEffect::MoveToFailed(_)
                    | MotionEffect::ClearTarget
            )),
            "stopping an already canceled approach must not report another cancellation"
        );
    }

    fn pending_manager(h: &mut Harness) -> crate::motion::MovementManager {
        let mut movement = crate::motion::MovementManager::new();
        let target = ObjectId(0x5000_0001);
        movement.perform_movement(
            &MoveToRequest::MoveToObject {
                object_id: target,
                top_level_id: target,
                radius: 0.5,
                height: 1.0,
            },
            &MovementParameters::default(),
            &mut h.ctx(),
        );
        assert!(movement.is_moving_to());
        h.effects.clear();
        movement
    }

    /// Constructed motion table, source-derived call ordering: the two motion entries inspect
    /// CANCEL_MOVETO before attempting (or refusing) the command. Failure does not undo cancel.
    #[test]
    fn manager_raw_cancellation_precedes_success_or_refusal_and_respects_the_flag() {
        for start in [false, true] {
            for cancel in [false, true] {
                for failure in [false, true] {
                    let mut h = Harness::new();
                    let mut m = pending_manager(&mut h);
                    let mut p = MovementParameters::default();
                    if !cancel {
                        p.flags &= !flags::CANCEL_MOVETO;
                    }
                    let (cmd, expected) = if failure && start {
                        m.interp.interpreted_state.current_style = MotionCommand::HAND_COMBAT;
                        (MotionCommand::CROUCH, 0x3f) // CANT_CROUCH_IN_COMBAT, before table work.
                    } else if failure {
                        h.mgr = MotionTableManager::new(None);
                        (MotionCommand::WALK_FORWARD, 7) // NO_ANIMATION_TABLE.
                    } else if !start {
                        (MotionCommand::READY, 0) // Stop the current substate, not an absent walk.
                    } else {
                        (MotionCommand::WALK_FORWARD, 0)
                    };
                    let before = p;
                    let error = if start {
                        m.do_motion(cmd, &p, &mut h.ctx())
                    } else {
                        m.stop_motion(cmd, &p, &mut h.ctx())
                    };
                    assert_eq!(
                        error, expected,
                        "start={start} cancel={cancel} failure={failure}"
                    );
                    assert_eq!(p, before, "adjust_motion must not alter caller parameters");
                    assert_eq!(m.is_moving_to(), !cancel);
                    assert_eq!(
                        h.effects
                            .iter()
                            .filter(|e| matches!(e, MotionEffect::MoveToFailed(0x36)))
                            .count(),
                        usize::from(cancel)
                    );
                    assert!(!h.effects.contains(&MotionEffect::CancelMoveTo));
                    if start && !failure {
                        assert_eq!(
                            m.interp.raw_state.forward_command,
                            MotionCommand::WALK_FORWARD
                        );
                        assert_eq!(
                            m.interp.interpreted_state.forward_command,
                            MotionCommand::WALK_FORWARD
                        );
                    }
                }
            }
        }
    }

    /// StopMotion's own cancellation may reset the old substate before the stop is attempted.
    /// Its resulting BAD_MOVEMENT_COMMAND is returned, not hidden or converted to success.
    #[test]
    fn manager_stop_returns_the_refusal_caused_by_its_own_leading_cancellation() {
        for cancel in [false, true] {
            let mut h = Harness::new();
            let mut m = pending_manager(&mut h);
            let mut p = MovementParameters::default();
            p.flags &= !flags::CANCEL_MOVETO;
            assert_eq!(
                m.do_motion(MotionCommand::WALK_FORWARD, &p, &mut h.ctx()),
                0
            );
            assert!(m.is_moving_to());
            if cancel {
                p.flags |= flags::CANCEL_MOVETO;
            }
            let error = m.stop_motion(MotionCommand::WALK_FORWARD, &p, &mut h.ctx());
            assert_eq!(error, if cancel { 0x43 } else { 0 });
            assert_eq!(m.is_moving_to(), !cancel);
            assert_eq!(
                m.interp.interpreted_state.forward_command,
                MotionCommand::READY
            );
            assert!(!h.effects.contains(&MotionEffect::CancelMoveTo));
        }
    }

    /// jump cancels before checking permission, including air/constrained refusal.
    #[test]
    fn manager_jump_cancels_once_before_permission_and_never_leaves_a_late_cancel() {
        for error in [0, 0x24, 0x47] {
            let mut h = Harness::new();
            let mut m = pending_manager(&mut h);
            h.env.on_ground = error != 0x24;
            h.env.fully_constrained = error == 0x47;
            m.interp.standing_longjump = true;
            assert_eq!(m.jump(0.6, &mut h.ctx()), error);
            assert!(!m.is_moving_to());
            assert_eq!(
                h.effects
                    .iter()
                    .filter(|e| matches!(e, MotionEffect::MoveToFailed(0x36)))
                    .count(),
                1
            );
            assert!(!h.effects.contains(&MotionEffect::CancelMoveTo));
            assert_eq!(
                h.effects.contains(&MotionEffect::SetOnWalkable(false)),
                error == 0
            );
            if error == 0 {
                assert_eq!(m.interp.jump_extent, 0.6);
            } else {
                assert!(!m.interp.standing_longjump);
            }
        }
    }

    /// Keep the isolated motion-interpreter APIs as explicit effect adapters. With no active approach,
    /// the owner facade and adapter have identical state/table/report behavior apart from the
    /// external CancelMoveTo notice which the owner has already completed. This is NOT an
    /// equivalence claim for executing an adapter's cancellation after replacement state.
    #[test]
    fn manager_cancellation_facades_match_isolated_adapters_when_no_approach_is_active() {
        for operation in 0..4 {
            let mut owner_h = Harness::new();
            let mut adapter_h = Harness::new();
            let mut owner = crate::motion::MovementManager::new();
            let mut adapter = MotionInterp::new();
            let p = MovementParameters::default();
            let state = crate::motion::InterpretedMotionState {
                forward_command: MotionCommand::WALK_FORWARD,
                forward_speed: 0.7,
                ..Default::default()
            };
            let (owned_result, adapter_result) = match operation {
                0 => (
                    owner.do_motion(MotionCommand::WALK_FORWARD, &p, &mut owner_h.ctx()),
                    adapter.do_motion(MotionCommand::WALK_FORWARD, &p, &mut adapter_h.ctx()),
                ),
                1 => (
                    owner.stop_motion(MotionCommand::WALK_FORWARD, &p, &mut owner_h.ctx()),
                    adapter.stop_motion(MotionCommand::WALK_FORWARD, &p, &mut adapter_h.ctx()),
                ),
                2 => (
                    owner.jump(0.7, &mut owner_h.ctx()),
                    adapter.jump(0.7, &mut adapter_h.ctx()),
                ),
                _ => {
                    owner.move_to_interpreted_state(&state, true, &mut owner_h.ctx());
                    adapter.move_to_interpreted_state(&state, true, &mut adapter_h.ctx());
                    (0, 0)
                }
            };
            assert_eq!(owned_result, adapter_result);
            assert_eq!(owner.interp.raw_state, adapter.raw_state);
            assert_eq!(owner.interp.interpreted_state, adapter.interpreted_state);
            assert_eq!(owner.interp.pending_motions, adapter.pending_motions);
            assert_eq!(owner.interp.jump_extent, adapter.jump_extent);
            assert_eq!(owner_h.seq.frame_number, adapter_h.seq.frame_number);
            let nodes = |seq: &crate::seq::Sequence| {
                seq.nodes
                    .iter()
                    .map(|n| (n.anim_id, n.framerate, n.low_frame, n.high_frame))
                    .collect::<Vec<_>>()
            };
            assert_eq!(nodes(&owner_h.seq), nodes(&adapter_h.seq));
            assert_eq!(owner_h.mgr.state, adapter_h.mgr.state);
            assert_eq!(owner_h.events, adapter_h.events);
            assert_eq!(adapter_h.effects.first(), Some(&MotionEffect::CancelMoveTo));
            assert_eq!(owner_h.effects, adapter_h.effects[1..]);
        }
    }

    /// `always_turn` (a host option; A14): a turn-to-heading node begun while a motion is still
    /// pending waits for it by default, which is the client's behaviour, and starts at once when
    /// the host has set the option. With nothing pending the two agree.
    #[test]
    fn always_turn_skips_only_the_wait_for_pending_motions() {
        let begin = |always_turn: bool, pending: bool| {
            let mut h = Harness::new();
            let mut interp = MotionInterp::new();
            let mut m = MoveToManager::new();
            h.env.position = pos(0.0, 0.0, 90.0); // facing east; the node asks for north
            m.movement_type = MovementType::TurnToHeading;
            m.pending_actions.push_back(MovementNode {
                kind: MovementType::TurnToHeading,
                heading: 0.0,
            });
            if pending {
                interp.add_to_queue(0, MotionCommand::WALK_FORWARD, 0);
            }
            m.always_turn = always_turn;
            m.begin_next_node(&mut interp, &mut h.ctx());
            m.current_command
        };
        assert!(
            !MoveToManager::new().always_turn,
            "the client default waits"
        );
        assert_eq!(
            begin(false, true),
            MotionCommand::NONE,
            "the client waits for the pending motion"
        );
        assert_eq!(
            begin(true, true),
            MotionCommand::TURN_LEFT,
            "always_turn starts the turn at once"
        );
        assert_eq!(begin(false, false), MotionCommand::TURN_LEFT);
        assert_eq!(begin(true, false), MotionCommand::TURN_LEFT);
    }

    const ACTION_CANCELLED_FOR_TEST: u32 = 0x36;
}

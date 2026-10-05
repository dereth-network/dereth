//! The embodied character.
//!
//! **This module wires; it does not implement.** Everything that decides how the body *behaves*
//! lives in the physics and animation crates and is called from here by name:
//!
//! | Decision | Whose |
//! |---|---|
//! | the 30 Hz gate, the 0.2 s sub-step ladder, the remainder rule | physics simulation |
//! | terrain collision, sliding, step-up and step-down, `FLOOR_Z` | object physics |
//! | the animation player, the motion table, the part array | `MotionDriver` |
//! | which animation a key press means, and what it does to the state machine | `MotionInterp` |
//! | the animation offset per sub-step, and the hooks it queues | `MotionSource::advance` |
//!
//! The two are joined by [`dereth_primitives::MotionSource`], which the animation crate
//! implements and the physics crate consumes. This crate supplies the three things neither owns:
//! a `LandSource` ([`dereth_world_data::land_source`]), an `AnimAssets` ([`dereth_world_data::anim_assets`]), and the
//! once-per-frame call order below.
//!
//! ## The frame
//!
//! ```text
//! refresh MotionEnv from physics      // on_ground, contact, position, velocity, radius, height
//! issue key changes as motions        // DoMotion / StopMotion
//! advance physics to now              //   -> advance()  -> the animation offset
//!                                     //   -> tick_movement(), hit_ground(), leave_ground()
//!                                     //   -> process_hooks()
//! apply the MotionEffects it raised   // set_local_velocity, set_heading, set_on_walkable
//! update the part array from world    // place every part for the renderer
//! ```
//!
//! `MotionEnv` is the animation crate's **copy** of facts the client reads live off the physics
//! object, and the copy is where this seam is easiest to get wrong. It is refreshed before the
//! step, because move-to and motion adjustment read it there — but the movement manager's
//! hit-ground callback fires from *inside* the step and also reads it, so
//! `SharedMotion::hit_ground` corrects the two facts that call site guarantees before forwarding.
//! Without that the movement layer sees an airborne object at the instant it lands.
//!
//! ## Why the driver is behind an `Rc<RefCell<…>>`
//!
//! `PhysicsObj::motion` is a `Box<dyn MotionSource>`, so once the driver is installed the
//! application can no longer reach the concrete `MotionDriver` — and it must, to refresh the env,
//! to issue motions and to read the part frames for rendering. `SharedMotion` is a second handle
//! onto the same driver. It is **not** a workaround for a missing API: the client has exactly the
//! same shape: the part array is reachable both from the physics update and from object drawing.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::motion::interp::MotionCtx;
use dereth_animation::motion::{MotionEffect, MoveToRequest};
use dereth_animation::{MotionCommand, MotionDriver, MovementParameters};
use dereth_assets::{Decode, Region, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::{PhysHandle, PhysicsWorld};
use dereth_primitives::{
    CellId, DataId, Frame, LandblockId, LocalTime, MotionSource, ObjectId, Position, Quat, Vec3,
};

use dereth_world_data::anim_assets::DatAnimAssets;
use {
    dereth_world_data::land_source::DatLandSource, dereth_world_data::land_source::LandSourceError,
};

/// The Aluvian male's setup from the chargen table: `setup = 0x02000001`.
///
/// The setup's own `parts` are already the male body's 34 gfxobjs, so the base `ObjDesc`'s
/// seventeen `AnimPartChange`s are a no-op over it; nothing here needs the chargen record at
/// runtime, and taking the ids from these constants rather than re-reading `0x0E000002` keeps the
/// character out of the character-generation path, which is where the record belongs.
pub const ALUVIAN_MALE_SETUP: DataId = DataId(0x0200_0001);

/// `motionTable = 0x09000001` from the same row. Setup `0x02000001` carries no default motion
/// table of its own — a body without one stands in its placement frame and never animates — so the
/// chargen table is where it comes from, exactly as the character-generation data supplies it at character
/// creation.
pub const ALUVIAN_MALE_MOTION_TABLE: DataId = DataId(0x0900_0001);

/// The sound table, `0x20000001`, from the same row of the same table.
///
/// Setup `0x02000001` carries no `default_stable_id` either, and a body without a sound table has
/// no footsteps: sound hooks resolve their `SoundType` against **the object's own** sound table,
/// and 145 of the 321 animations the Aluvian male's motion table can
/// reach carry one of those hooks — every walk and run cycle among them. Live, this arrives on the
/// player's create as the descriptor's `stable_id`; offline it comes from here, exactly as the
/// motion table above does.
pub const ALUVIAN_MALE_SOUND_TABLE: DataId = DataId(0x2000_0001);

/// `scaling = 100`, i.e. `scale = 1.0`. The chargen field is a percentage.
pub const ALUVIAN_MALE_SCALE: f32 = 1.0;

/// The extent the debug jump key uses. The client's own value is the power-bar level at the moment
/// the key is released, which the power bar supplies; see [`Character::jump`].
pub const FULL_JUMP_EXTENT: f32 = 1.0;

/// The object id the local body carries **before the server has said what it is**.
///
/// The name is the one the rest of this crate already greps for and is kept; read it as *the
/// offline id*. The id that matters is the one `0xF746 Login_CharacterSet` carries, and
/// [`Character::adopt_server_id`] takes that one the moment it arrives.
///
/// A body with no server id is a real state this build supports, not an error: `--no-connect`,
/// the offline viewer and every offline test stand a body on the ground before anything
/// has logged in, and `world.rs`'s `attach_character` runs before the connected path knows the
/// answer either. So the body needs *an* id, and the only requirement on it is that **no server
/// can ever send it for something else** — the physics world is keyed by object id and
/// a second insert replaces the first, so a clash silently unregisters one of the two bodies.
///
/// `0x60000000` meets that on both sides:
///
/// * it is inside the one object-id range the client itself tests —
///   `0x50000001 <= id <= 0x6FFFFFFF` is the ranged-talk handler's
///   "this sender is a player, render it clickable" test, so the local body still reads as a
///   player everywhere the
///   client asks the question;
/// * and it is outside every range a server allocates from. ACE hands players
///   `0x50000001..=0x5FFFFFFF` **from the bottom up**, world objects `0x70000000..=0x7FFFFFFF` and
///   generated objects `0x80000000..=0xFFFFFFFE`; `0x60000000` is reserved outside captured
///   player ids.
///
/// `0x50000001` would not do: it is precisely the **first** GUID ACE hands out, so the account's
/// own second character could carry it and collide with the local body.
pub const PLAYER_OBJECT_ID: ObjectId = ObjectId(0x6000_0000);

/// **Proof that the viewer block this frame's render space is expressed in has been chosen.**
/// The block-local render ordering constraint, made a thing the compiler checks.
///
/// # Why the constraint exists
///
/// In the client nothing a part placement writes is expressed relative to the viewer's block:
/// part placement writes the object's own **cell-local**
/// `position.frame`, and block drawing applies the block origin by
/// pushing a `Position` carrying the block's cell
/// id through the render position stack. Drawing may therefore
/// run in **any** order with respect to the placements.
///
/// This build folds that origin into the part frames instead (so the vertices stay
/// block-local): [`Character::render_frame`] and `crate::world::WorldScene::render_frame_of`
/// both add `(block - viewer) * BLOCK_LENGTH`. That creates an ordering constraint the client
/// does not have — **every writer of a drawn frame must run after the re-centre**. A violation
/// leaves the body's parts `(-192 * dx, -192 * dy, 0)` behind the camera, which is the player's
/// own body vanishing for one frame at every landblock crossing.
///
/// # Why it is a token and not a comment
///
/// Both writers that exist are correct by **placement**, held there by a comment and by a
/// landblock-crossing test that four mutations redden. Neither of those stops a *third* writer
/// landing on the wrong side; they only report it afterwards, and only if it happens to move a
/// pixel at one of the four crossings that test measures.
///
/// So this is a zero-sized capability rather than data. Its field is private to the module below,
/// which means **no code anywhere — in this crate or outside it — can construct one**; the only
/// producer is [`Character::set_viewer_block`], the act of choosing the block, and
/// `crate::world::WorldScene::recenter` is its only caller in a running client. A render-space
/// writer takes one by value, so it cannot be *called* before the binding that holds it exists.
/// Move `place_local_body` above `recenter()` in `WorldScene::update` and the frame loop no
/// longer compiles.
///
/// # What it does and does not enforce, exactly
///
/// **What it enforces is the *ordering*, and that is airtight**: the token is a local binding that
/// `crate::world::WorldScene::recenter` produces, so a writer called above that line has nothing
/// to pass and the frame loop does not compile.
///
/// **What it does not enforce is who may mint one.** A scene with no body has no `Character` to
/// re-anchor — the free camera owns its own position — and
/// `crate::world::WorldScene::advance_objects` still has to place the server's objects there, so
/// there is a second producer, [`RenderSpace::for_a_window_without_a_body`], which `recenter`'s
/// bodiless arm calls. It is `pub` because `WorldScene::recenter` is in `dereth-client` and this
/// module is not; a determined writer could mint a token instead of taking `recenter`'s. Saying
/// so is the point: a device that reads as total and is not is worse than one with a stated edge.
/// The runtime half of the constraint, the render-space integration test, is what watches that
/// seam, and the landblock-crossing test's four mutations still watch the placement itself.
pub use render_space::RenderSpace;

mod render_space {
    /// See the re-export above for what this is and why. The tuple field is private to this
    /// module, and this module has no constructor that is public beyond its parent, so
    /// `Character::set_viewer_block` is the only way anyone gets one.
    #[derive(Debug, Clone, Copy)]
    pub struct RenderSpace(());

    impl RenderSpace {
        /// The only constructor. `pub(super)` on purpose: `character.rs` can mint one and
        /// nothing else can, which is what makes the token mean something.
        pub(super) const fn chosen() -> Self {
            Self(())
        }

        /// The viewer block chosen for a scene with **no body**.
        ///
        /// `Character::set_viewer_block` is the ordinary producer, and it needs a `Character`.
        /// `WorldScene::recenter` has two arms that do not have one: a `--no-character` flycam
        /// scene, and the moment before the window has a block at all, where
        /// `WorldScene::render_frame_of` falls back to `cfg.landblock`. Both still choose the
        /// block the frame is drawn in, and `advance_objects` still places the server's objects,
        /// so both still need a token.
        ///
        /// `pub` because `WorldScene::recenter` lives in another crate; it is still the only
        /// caller. It takes the block it is
        /// asserting so that a caller has to say which one it chose.
        pub const fn for_a_window_without_a_body(_block: (i32, i32)) -> Self {
            Self(())
        }
    }
}

/// Anything that stops the character from being created.
#[derive(Debug, thiserror::Error)]
pub enum CharacterError {
    #[error("setup {0} is missing or will not decode")]
    NoSetup(DataId),
    #[error("motion table {0} is missing or will not decode")]
    NoMotionTable(DataId),
    #[error("the part array refused setup {0}")]
    NoPartArray(DataId),
    #[error("landblock {0:#06X} carries no physics geometry")]
    NoLandblock(u16),
    #[error(transparent)]
    Land(#[from] LandSourceError),
}

/// Which of the character's controls are held this frame.
///
/// **This is written only by the input seam.** Latching it straight out of the raw host key
/// event, upstream of input-event dispatch, would leave the `MAP_BLOCK_KEYBOARD` barrier
/// correct, registered and completely inert: the movement would never travel through the device
/// input at all, so there would be nothing for the barrier to break. The only writer is
/// [`MovementCommands`], which takes the command interpreter's decision — and an
/// `InputEvent` only exists because `fire::walk_input_maps` produced one.
///
/// **This is a *view* of the three command lists, not a substitute for them.** Six direction
/// bools alone cannot express "remove and resume the new head":
/// holding `A`, pressing `D` and releasing `D` would empty the turn slot instead of resuming the
/// turn to the left. [`MovementCommands`] holds the real
/// [`crate::actions::movement::CommandLists`] and this struct is recomputed from
/// `apply_current_movement`'s three heads after every command, which is also how the
/// client re-asserts movement (it does not trust the edge events).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterInput {
    pub forward: bool,
    pub back: bool,
    pub step_left: bool,
    pub step_right: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    /// The hold-run key passed to movement interpolation.
    pub run: bool,
    /// Rising edge only: a jump is an action, not a held state.
    pub jump: bool,
    /// The run lock (`0x30 Autorun`, `DIK_Q`).
    ///
    /// It is a *view* of [`crate::actions::movement::CommandLists::auto_run`] rather than a second
    /// copy: `apply_current_movement` puts `WalkForward` in the base slot while it is set, so
    /// [`Self::forward`] is already true and this is what tells the difference between "the player
    /// is holding `W`" and "the run lock is on". Without this slot `DIK_Q` — a key the shipped
    /// default keymap binds — would do nothing.
    pub auto_run: bool,
}

impl CharacterInput {
    /// The forward-slot command this input asks for, or `NONE`.
    ///
    /// **The run key does not change the command; it changes the hold key.**
    ///
    /// The command interpreter takes the command from the head of its own
    /// command list — `WalkForward` or `WalkBackwards`, never `RunForward` — and expresses the run
    /// as `hold_key_to_apply = HoldKey::None + (run != 0)`, i.e. `HoldKey::Run`. The substitution
    /// to `RunForward` happens one layer down, during motion adjustment, and only
    /// on the way to the **interpreted** state; the raw state keeps the walk.
    ///
    /// Applying a raw motion state makes that structural rather than conventional:
    /// its forward arm skips `0x44000007`, so `RunForward` is **excluded by
    /// name** and the raw state's `forward_command` is left at its `Ready (0x41000003)` default.
    /// Packing the raw state marks the field present only when it differs from
    /// `Ready`, so answering `RunForward` here would not report a *wrong* forward command — it
    /// would report **none at all**, in exactly the state a player is in most of the time.
    ///
    /// The corpus is unanimous over all **1,187** recorded `0xF61C` bodies: the forward command is
    /// `WalkForward` (662), `WalkBackwards` (11) or absent (514) and is **`RunForward` 0 times**,
    /// while `current_holdkey` is `HoldKey::Run` in **1,182**. `forward_holdkey` is absent in all
    /// 1,187, which is the apply-motion step's other arm: `MovementParameters`' default flags carry
    /// `SET_HOLD_KEY (0x800)`, so the per-axis hold key is stored as `HoldKey::Invalid` and only
    /// `current_holdkey` — written by `set_hold_run` — travels.
    ///
    /// What this costs on the server, read from ACE rather than assumed: with the default
    /// `client_movement_formula = false`, `Player_Tick.cs::OnMoveToState_ServerMethod` copies the
    /// raw state straight into its own `motion.RawState` and calls `apply_raw_movement`, and
    /// `Player_Networking.cs::BroadcastMovement` re-broadcasts it with an explicit
    /// `SetForwardCommand(MotionCommand.Ready)` when the forward flag is clear. So answering
    /// `RunForward` would leave a running player standing still both in the server's own physics
    /// copy and on every other player's screen.
    #[must_use]
    pub fn forward_command(self) -> MotionCommand {
        match (self.forward, self.back) {
            (true, false) => MotionCommand::WALK_FORWARD,
            (false, true) => MotionCommand::WALK_BACKWARDS,
            _ => MotionCommand::NONE,
        }
    }

    /// The sidestep-slot command.
    #[must_use]
    pub fn sidestep_command(self) -> MotionCommand {
        match (self.step_left, self.step_right) {
            (true, false) => MotionCommand::SIDE_STEP_LEFT,
            (false, true) => MotionCommand::SIDE_STEP_RIGHT,
            _ => MotionCommand::NONE,
        }
    }

    /// The turn-slot command.
    #[must_use]
    pub fn turn_command(self) -> MotionCommand {
        match (self.turn_left, self.turn_right) {
            (true, false) => MotionCommand::TURN_LEFT,
            (false, true) => MotionCommand::TURN_RIGHT,
            _ => MotionCommand::NONE,
        }
    }
}

/// **The movement command interpreter's three command lists, on this side of the input seam.**
///
/// The input decision must reach the interpreter, not `CharacterInput`'s six direction bools
/// directly: otherwise `0x30 Autorun` has nowhere to go and "remove and resume the new head"
/// cannot happen. [`crate::actions::movement::CommandLists`] is the interpreter; this struct is
/// its caller.
///
/// The order is the client's own:
///
/// ```text
/// input action                  -> MovementAction              (crate::actions::movement::on_action)
/// decoded motion               -> the command record          (the decode's own product)
/// handle_keyboard_command      -> the three lists             (CommandLists)
///                              -> move_player(cmd, start)
/// -> CharacterInput
/// ```
///
/// **`move_player` is what writes the slots, not `apply_current_movement`.** That distinction is
/// the whole of the transcription and it is easy to get backwards: `apply_current_movement`
/// re-asserts all three heads, but `use_time` only calls it while the server has control,
/// and `set_auto_run` calls it on a toggle. A build that derived the slots from it every time would
/// answer "still walking backwards" to *Stop Moving*, because the released-but-still-held key is
/// still on the substate list. [`Self::apply_current_movement`] is therefore reached only where the
/// client reaches it.
#[derive(Debug, Default)]
pub struct MovementCommands {
    /// Whether the command interpreter is enabled, initially true.
    ///
    /// The logoff request disables the interpreter, clearing it after issuing the final
    /// movement event. Player creation enables it again after
    /// accepting the new player id. [`crate::flags::StartsTrue`] preserves that non-zero
    /// constructor value under this struct's derived [`Default`].
    enabled: crate::flags::StartsTrue,
    /// The substate, turn and sidestep command lists, plus `auto_run`, `hold_run` and
    /// `hold_sidestep`.
    pub lists: crate::actions::movement::CommandLists,
    /// The *Run as Default Movement* character option: ordinal 10, bit `0x0400`.
    ///
    /// Hold-run processing **XORs** the physical key against this option rather than overriding
    /// it: `effective_run = hold_run != ui_toggles_run`.
    ///
    /// **The option is default-on**, in both independent statements of the default
    /// (the client's default-option true-list contains ordinal 10, and
    /// the default character-option word `0x50C4A54A` has bit `0x0400` set), so on a shipped
    /// character `effective_run = !hold_run`: **run is the default state and holding the key walks
    /// you**. The client's own name for the input action says the same — `0x00000032` is
    /// `MovementWalkMode`, not a run modifier.
    ///
    /// **This field must have a writer.** It is read by [`Self::env`] and by the `SetHoldRun` arm
    /// of [`Self::on_action`]; left unassigned, a correctly transcribed XOR is evaluated against a
    /// `false` that no character ever has, and the run polarity in a running client is inverted —
    /// an inversion by omission rather than by a backwards formula. The writer is
    /// [`crate::app::App::apply_input_actions`], which refreshes it from
    /// `dereth_client_model::PlayerOptions::toggle_run()` immediately before dispatching the frame's
    /// actions; the client re-reads the option on every `SetHoldRun`, and one
    /// refresh per frame ahead of every action that can reach `SetHoldRun` is the same answer.
    pub ui_toggles_run: bool,
    /// The command interpreter's autorun speed.
    pub autorun_speed: f32,
    /// How many commands reached the lists, and how many were handed back. A denominator: "the
    /// character did not move" and "nothing arrived" are the same observation without one.
    pub commands_handled: u64,
    /// `set_auto_run`'s message-window lines, waiting for the frame to
    /// put them on the scroll channel.
    ///
    /// In the client the send is synchronous, three instructions after the literal is built. Here
    /// the notice sink is the world's scroll receiver and this decision runs on a `&mut
    /// CharacterInput`, so the line is queued for [`crate::app::App::apply_input_actions`] to hand
    /// on in the same frame — the same one-borrow-at-a-time deferral `dereth_client_model::scroll`'s own
    /// header documents for every other producer in this build, and no longer than it.
    ///
    /// Empty is a real answer and not an absence: `set_auto_run`'s first act is
    /// `if ((param != 0) == (auto_run != 0)) return;`, so a toggle that changes nothing writes
    /// nothing. [`Self::auto_run_notices_sent`] is the denominator that tells the two apart.
    notices: Vec<&'static str>,
    /// How many display-string notice lines this interpreter has produced, ever.
    ///
    /// A counter and not a queue length, because [`Self::take_notices`] empties the queue: a test
    /// asserting "nothing was said" must be able to tell that from "something was said and
    /// delivered".
    pub auto_run_notices_sent: u64,
    /// Body-side calls owed to the caller,
    /// raised by the keyboard-command handler's unconditional slot-26 call.
    ///
    /// The same split [`Self::use_time`] already has: the flag lives here and the four body
    /// statements live on [`Character`], which this decision does not hold. `App` drains it in the
    /// same slot it drains [`Self::notices`], for the same one-borrow-at-a-time reason.
    control_retake_pending: bool,
    /// How many times a key press has taken control back from the server, ever. The denominator
    /// for the drain above: *nothing was owed* and *the drain never ran* are one observation
    /// without it.
    pub control_retakes_from_commands: u64,
    /// The move-player command's do-motion/stop-motion tail
    /// for the commands that are on **no** command list -- the four stances and the 87 emotes of
    /// the emote input-action table.
    ///
    /// `(motion command, start)`, in the order the keys arrived. The same one-borrow-at-a-time
    /// split [`Self::notices`] and [`Self::control_retake_pending`] already have: this decision
    /// runs on a `&mut CharacterInput` and needs the body, so the caller
    /// ([`crate::app::App::apply_input_actions`]) drains it in the same frame.
    transient_motions: Vec<(u32, bool)>,
    /// How many transient motions have been handed out, ever -- the denominator that tells
    /// *"no emote key was pressed"* from *"the drain never ran"*, which are the same reading of an
    /// empty queue.
    pub transient_motions_issued: u64,
    /// The last `(motion command, start)` this interpreter handed out, kept after the queue has
    /// been drained so a caller that holds no body can still say **which** motion was asked for.
    ///
    /// A count alone cannot: `Sleeping` and `Wave` are one increment each, and what matters for
    /// emotes is that the right *command* comes out of the emote input-action table.
    pub last_transient_motion: Option<(u32, bool)>,
    /// The **second** statement in new-forward-movement handling,
    /// owed to the caller: abort automatic attack.
    ///
    /// The command interpreter classifies an accepted press as *new forward
    /// movement* and calls the interpreter's new-forward-movement hook, which the client overrides.
    /// The override is two calls: `set_auto_run(false)`, which is
    /// [`crate::actions::movement::CommandLists::add_command`]'s own and has already run by the time
    /// this flag is written, and the automatic-attack abort, which needs the client combat system
    /// singleton. This projection holds a `&mut CharacterInput` and no combat system, so the edge
    /// is queued here for [`crate::app::App::apply_input_actions`] to spend against the world's
    /// automatic-attack abort in the same frame — the same one-borrow-at-a-time split
    /// [`Self::control_retake_pending`] and [`Self::transient_motions`] already have.
    ///
    /// If the boolean died in [`crate::actions::movement::CommandLists::bookkeep`], a mapped
    /// Backward key would walk the body backwards and leave the repeat attack armed.
    new_forward_movement: bool,
    /// How many new-forward-movement edges this interpreter has raised, ever — the denominator
    /// that tells *"no press raised one"* from *"the drain never ran"*, which are the same reading
    /// of a `false` flag. Same role as [`Self::control_retakes_from_commands`].
    pub new_forward_movements: u64,
}

impl MovementCommands {
    /// Whether the command interpreter is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled.0
    }

    /// Disable the command interpreter when logoff handling reaches this point.
    ///
    /// Returns whether the caller owes the body-side `apply_current_movement` and
    /// `send_movement_event`: those two calls are guarded by `autonomy_level != 0`, a player body,
    /// and `!controlled_by_server`; the body part of that guard belongs to `App`.
    ///
    /// `clear_all_commands` clears only the three lists. In particular, the client's disable
    /// does **not** clear `auto_run`; its one scalar store is `hold_sidestep`.
    pub fn disable(&mut self, out: &mut CharacterInput) -> bool {
        self.lists.clear_all_commands();
        out.run = self.lists.set_hold_run(false, self.ui_toggles_run);
        self.lists.hold_sidestep = false;
        let apply = self.lists.autonomy_level != 0 && !self.lists.controlled_by_server;
        if apply {
            self.apply_current_movement(out);
        }
        self.enabled.0 = false;
        apply
    }

    /// Enable the interpreter first, then re-apply the held-run key.
    pub fn enable(&mut self, out: &mut CharacterInput) {
        self.enabled.0 = true;
        out.run = self
            .lists
            .set_hold_run(self.lists.hold_run, self.ui_toggles_run);
    }

    /// The interpreter's environment for this call.
    fn env(&self) -> crate::actions::movement::CommandEnv {
        crate::actions::movement::CommandEnv {
            // The interpreter's own flag, which [`Self::lose_control_to_server`] sets when the
            // server sends the player a non-autonomous `0xF74C` -- a combat-stance change being
            // the typical case -- and [`Self::use_time`] clears again.
            controlled_by_server: self.lists.controlled_by_server,
            // **A surviving mutation: the code is right and the test is right.** Replacing this
            // with `false` changes nothing in this build, because the
            // arm that reads it has no producer. `crate::actions::movement::on_action` maps action
            // `0x32` (`MovementWalkMode`) straight to `MovementAction::SetHoldRun`, not to a
            // `SetMotion`, so `handle_keyboard_command`'s `command::HOLD_RUN` arm -- the only
            // reader of `CommandEnv::ui_toggles_run` -- can only be reached by a `SetMotion`
            // carrying `0x85000001`, which nothing in this build constructs (the emote hash is
            // `|_| None`). The live route is [`Self::on_action`]'s `M::SetHoldRun` arm below,
            // which passes the field directly and *is* covered.
            //
            // It is kept and its unfalsifiability written down rather than deleted, for the same
            // reason as `out.auto_run = self.lists.auto_run` further down: `env()` is this
            // interpreter's environment, and an environment that is only truthful on the arms
            // something happens to reach is not an environment. The moment a producer exists --
            // a text command, or a server-driven keyboard command after losing control to the
            // server -- it becomes load-bearing, and the polarity would otherwise be inverted
            // on exactly that path.
            ui_toggles_run: self.ui_toggles_run,
            speed: 1.0,
        }
    }

    /// Take the display-string notice lines this frame's commands raised.
    ///
    /// The caller is [`crate::app::App::apply_input_actions`], which hands each one to the world's
    /// scroll-notice receiver on channel
    /// [`dereth_client_model::scroll::LOCAL_ERROR_TYPE`] — the scroll surface, not a second route
    /// to the chat window.
    pub fn take_notices(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.notices)
    }

    /// Whether a key press has just taken control back from the server, so the
    /// caller owes [`Character::take_control_from_server`] — the body half of the retake.
    ///
    /// A **take** rather than a read, for the reason
    /// `crate::world::WorldScene::take_player_movement_applied` gives: the transfer is an edge,
    /// and two frames must not both act on one press.
    pub fn take_control_retake_pending(&mut self) -> bool {
        std::mem::take(&mut self.control_retake_pending)
    }

    /// Take the frame's *new forward movement* edge — whether the caller owes the automatic-attack
    /// abort that new-forward-movement handling performs.
    ///
    /// A **take** rather than a read, for the reason [`Self::take_control_retake_pending`] gives:
    /// the abort is an edge, and two frames must not both send a `CancelAttack` for one press.
    pub fn take_new_forward_movement(&mut self) -> bool {
        std::mem::take(&mut self.new_forward_movement)
    }

    /// Take the frame's list-less motion commands — `move_player`'s
    /// do-motion/stop-motion tail, for the caller to run against the body.
    ///
    /// A **take** rather than a read, for the reason [`Self::take_control_retake_pending`] gives:
    /// an emote is an edge and two frames must not both play it.
    pub fn take_transient_motions(&mut self) -> Vec<(u32, bool)> {
        self.transient_motions_issued += self.transient_motions.len() as u64;
        std::mem::take(&mut self.transient_motions)
    }

    /// One decision, applied to the lists and then to `out`.
    ///
    /// Returns whether the decision was **consumed**. Sending an action to listeners walks the
    /// whole input-handler list, and a handler that returns false passes
    /// the event on, so anything this build does not model has to be handed back rather than
    /// swallowed — that is what keeps the combat-mode toggle working.
    pub fn on_action(
        &mut self,
        a: crate::actions::movement::MovementAction,
        out: &mut CharacterInput,
    ) -> bool {
        use crate::actions::movement::{KeyboardCommand as K, MovementAction as M};
        // The keyboard-command handler checks it is active before its switch and returns true
        // when inactive. Preserve the global fall-through for actions this interpreter does not
        // own, but consume every movement action without changing state while disabled.
        if !self.is_enabled() {
            return !matches!(a, M::NotHandled);
        }
        match a {
            M::SetMotion(c) => {
                let env = self.env();
                // Keyboard-command handling takes control back from the server.
                // Doing so unconditionally clears autorun
                // and reads nothing back from it -- so the *flag across the call* is how this
                // side learns the transfer happened, and it is the same question
                // [`Self::use_time`] answers with a `bool` because it makes the call itself.
                let was_server_controlled = self.lists.controlled_by_server;
                let r = self.lists.handle_keyboard_command(&c, &env);
                if was_server_controlled && !self.lists.controlled_by_server {
                    // Taking control back has two interpreter-side tails: re-apply held-run state
                    // and apply the current command-list heads. The client runs them before the hold-key
                    // branch and `move_player`; here the lists have already been bookkept by the
                    // call above, so the projection is written from the new heads and `move_player`
                    // below then writes its own slot over the top. The two orders differ only in
                    // which statement writes a slot both agree on -- said out loud because the
                    // ordering *is* the transcription.
                    //
                    // **A surviving mutation, where the code and the test are both right.**
                    // Deleting the `apply_current_movement` below changes
                    // nothing measurable, because `CharacterInput` is a **view** of the lists that
                    // every other arm keeps current -- so it already equals what this call would
                    // write, and `move_player` below writes its own slot either way. What actually
                    // moves the body is `Character::take_control_from_server`'s `applied` reset,
                    // whose own mutation **reddens** (a press that retakes control must re-issue
                    // the key that was already held).
                    // The second tail is retained; it becomes load-bearing the
                    // moment a producer desynchronises the view.
                    out.run = self
                        .lists
                        .set_hold_run(self.lists.hold_run, self.ui_toggles_run);
                    self.apply_current_movement(out);
                    // Stopping the player and resetting `applied` live on
                    // `Character`, which this decision does not hold.
                    self.control_retake_pending = true;
                    self.control_retakes_from_commands += 1;
                }
                match r {
                    // `cmd & 0x08000000` and not AutoRun: the emote and stance commands, which
                    // the animation crate owns and have no slot here. Declined, not dropped.
                    K::Ignored => return false,
                    // `set_auto_run` with the send-event flag set ends in
                    // `apply_current_movement`, in **both** directions -- turning the run lock on
                    // starts the walk and turning it off stops it.
                    //
                    // `message` is the message-window line, and it is taken rather than dropped.
                    // The order is the client's: `set_auto_run` builds the literal and emits
                    // `(0x1a, text)` **before** its send-event-gated `apply_current_movement`
                    // tail. `message` and
                    // the tail share one condition in the client -- both are inside the
                    // state-changed branch -- so they are driven from the one `Option` here.
                    K::AutoRun { speed, message } => {
                        self.autorun_speed = speed;
                        if let Some(text) = message {
                            self.notices.push(text);
                            self.auto_run_notices_sent += 1;
                            self.apply_current_movement(out);
                        }
                    }
                    K::HoldRun { run } => out.run = run,
                    // `move_player(cmd, start)` -- the one call that moves the body.
                    //
                    // The move-player command ends with a do-motion (notification on) on the
                    // press and a stop-motion on the release for *any* command
                    // it is given; the six direction bools below are this build's shorthand for
                    // the three that belong to a list. A command on **no** list -- every one of
                    // the emote input-action table's 91, plus `Ready`, which has its own arm
                    // in `move_player` -- has no bool to be shorthand for, so it is queued for
                    // [`Self::take_transient_motions`] instead of being dropped.
                    K::Move {
                        command,
                        start,
                        new_forward,
                    } => {
                        Self::move_player(command, start, out);
                        // The new-forward-movement callback both clears autorun
                        // **and** aborts the automatic attack.
                        // The first half ran inside `handle_keyboard_command`; the
                        // second needs a combat system and is owed to `App`. Retail raises it
                        // *before* `move_player` -- the call is inside the bookkeeping -- and the
                        // two cannot observe each other here, because the abort reads combat
                        // state and `move_player` writes only motion slots.
                        if new_forward {
                            self.new_forward_movement = true;
                            self.new_forward_movements += 1;
                        }
                        use crate::actions::movement::{command as c, which_list, CommandList};
                        // `MoveTo` is `move_player`'s own early return, in **both** the press and
                        // the release legs; each compares against `0x2500003B` and returns.
                        // Nothing in this build constructs a `SetMotion` carrying it, so the guard
                        // is unreachable
                        // today and is here because the condition it is missing from would be
                        // invisible: an approach walk routed through the interpreter would
                        // otherwise be re-issued as a body motion.
                        if which_list(command) == CommandList::None
                            && command != c::READY
                            && command != c::MOVE_TO
                        {
                            self.transient_motions.push((command, start));
                            self.last_transient_motion = Some((command, start));
                        }
                    }
                    // Nothing was accepted, and nothing is issued. The lists may still have
                    // changed; the body keeps doing what it was doing, which is the client's
                    // behaviour and not an oversight.
                    // The
                    // command-removal step has just taken the released key off its list and the
                    // client issues nothing, because while `controlled_by_server` is set the
                    // *server* owns the body and the lists are held in reserve until
                    // `apply_current_movement` re-issues them at the retake. In this build
                    // `CharacterInput` is the only thing that drives the body -- there is no
                    // second, server-owned motion source -- so a view left reading `forward` after
                    // the key has left the list is a character who keeps running, which is exactly
                    // the measured stuck-running behavior. This re-derives the view from the lists the client would
                    // re-derive it from; it is `apply_current_movement` and not a fresh decision.
                    // If the retake never comes (three empty lists and no run lock is a state
                    // `use_time` refuses to retake from) it is the *only* thing that
                    // ever clears the slot.
                    K::NonAutonomous(_) => self.apply_current_movement(out),
                    K::HoldSidestep(_) | K::Refused => {}
                }
                // **A surviving mutation, where the code and the test are both right.** Deleting
                // this line changes nothing in this build. Every path that can move
                // `lists.auto_run` also runs `apply_current_movement`, which sets the same field:
                // the `K::AutoRun` arm above does it directly, and the *cancel* inside
                // `add_command`'s new-forward-movement handling can only fire for `WalkForward`,
                // `WalkBackwards`, `Ready` or an emote -- the first three of which the input
                // layer's has already turned into a `0x30` release
                // dispatched *before* them, and the fourth of which this build cannot produce
                // (`apply_input_actions` passes `|_| None` for the emote hash).
                //
                // So it is kept and its unfalsifiability is written down, rather than deleted
                // because no test misses it. The moment the emote hash is wired it becomes
                // load-bearing, and it is the projection's own invariant: `out` is a view of the
                // lists, and a view that is only refreshed on some arms is not a view.
                out.auto_run = self.lists.auto_run;
                self.commands_handled += 1;
                true
            }
            M::SetHoldRun(on) => {
                out.run = self.lists.set_hold_run(on, self.ui_toggles_run);
                self.commands_handled += 1;
                true
            }
            // Isolated/debug compatibility adapter. Production App intercepts both jump actions
            // before this projection and executes the real charge/release owner in crate::jump.
            M::CommenceJump => {
                out.jump = true;
                self.commands_handled += 1;
                true
            }
            M::DoJump => true,
            M::NotHandled => false,
        }
    }

    /// Apply the move-player command, projected onto the motion slot
    /// `which_list` picks for the command.
    ///
    /// One command touches **one** slot, which is why a turn cannot be left and right at once: the
    /// client's turn list holds one head and `move_player(TurnRight, 1)` replaces whatever the turn
    /// slot held. `Ready` is on no list and is the standing state — every slot out.
    fn move_player(command: u32, start: bool, out: &mut CharacterInput) {
        use crate::actions::movement::{command as c, which_list, CommandList as L};
        match which_list(command) {
            L::Substate => {
                (out.forward, out.back) = (
                    start && command == c::WALK_FORWARD,
                    start && command == c::WALK_BACKWARDS,
                );
            }
            L::Turn => {
                (out.turn_left, out.turn_right) = (
                    start && command == c::TURN_LEFT,
                    start && command == c::TURN_RIGHT,
                );
            }
            L::Sidestep => {
                (out.step_left, out.step_right) = (
                    start && command == c::SIDE_STEP_LEFT,
                    start && command == c::SIDE_STEP_RIGHT,
                );
            }
            // `Ready` (`0x2B Stop Moving`): the standing state. `run` and `jump` survive it, as
            // they do in the client -- `SetHoldRun` is its own entry point and `Ready` does not
            // touch it.
            //
            // **This arm is `Ready` alone, not all of `L::None`, and that is a fix, not a
            // restriction.** Command-list classification answers "no list" for every one of
            // the emote input-action table's 91 commands as well as for `Ready`
            // (`crate::actions::emote`'s own test asserts it), so with the hash wired a wider arm
            // would make *pressing `J` to wave* clear the movement slots -- a player running past
            // somebody would stop dead to wave at them. The client does no such thing: moving the
            // player touches no list at all and applies the supplied motion with a do-motion and
            // notification enabled, whatever command it was handed; for a `0x13……`
            // emote the bookkeeping step has already
            // declined to touch the lists too, because its own test is `(cmd & 0x40000000) &&
            // !(cmd & 0x04000000)`.
            //
            // So: `Ready` still empties the slots, and anything else on no list is handed to the
            // caller as a **transient motion** for the body, which is the other half of the
            // move-player command.
            L::None if command == c::READY => {
                let (run, jump, auto_run) = (out.run, out.jump, out.auto_run);
                *out = CharacterInput {
                    run,
                    jump,
                    auto_run,
                    ..CharacterInput::default()
                };
            }
            L::None => {}
        }
    }

    /// Apply the current command-list heads, projected onto the three
    /// motion slots the body reads.
    ///
    /// **It is reached only where the client reaches it**: `set_auto_run`'s tail, the focus-loss
    /// clear, and the per-frame retake, which is
    /// [`Self::use_time`]. That third caller is the one that makes a held key resume movement
    /// after a combat-stance change; it is gated on `controlled_by_server`, which
    /// [`Self::lose_control_to_server`] sets.
    ///
    /// `run` and `jump` are deliberately untouched: neither is list state.
    pub fn apply_current_movement(&self, out: &mut CharacterInput) {
        use crate::actions::movement::command;
        let m = self.lists.apply_current_movement();
        match m.base {
            Some(command::WALK_FORWARD) => (out.forward, out.back) = (true, false),
            Some(command::WALK_BACKWARDS) => (out.forward, out.back) = (false, true),
            // `Ready` is the standing state: the base slot released.
            Some(_) => (out.forward, out.back) = (false, false),
            // `transient_state` is held and nothing is re-applied. Nothing in this build sets it
            // (it takes a non-substate motion command that is not `Ready`, i.e. an emote), so this
            // arm has never been evaluated in a running client — said out loud rather than left to
            // read as a passing branch.
            None => {}
        }
        out.turn_left = m.turn == Some(command::TURN_LEFT);
        out.turn_right = m.turn == Some(command::TURN_RIGHT);
        out.step_left = m.sidestep == Some(command::SIDE_STEP_LEFT);
        out.step_right = m.sidestep == Some(command::SIDE_STEP_RIGHT);
        out.auto_run = self.lists.auto_run;
    }

    /// The player-teleported handler's `set_auto_run(false)` call, with the send-event flag set.
    /// The caller owes the body-side `apply_current_movement` only when the run lock changed,
    /// followed by the teleport handler's unconditional movement event.
    pub fn player_teleported(&mut self, out: &mut CharacterInput) -> bool {
        let Some(text) = self.lists.set_auto_run(false) else {
            return false;
        };
        self.notices.push(text);
        self.auto_run_notices_sent += 1;
        self.apply_current_movement(out);
        true
    }

    /// Losing control to the server: the *stop* edge.
    ///
    /// The producer is the player's `0xF74C` movement-update arm:
    ///
    /// Apply the object's movement buffer; if the movement setter returns nonzero,
    /// relinquish control to the server through the command interpreter.
    ///
    /// The movement setter returns **1 exactly when it applied the buffer to the
    /// player** (that is, when the weenie answers that it is the player),
    /// having first written `last_move_was_autonomous = autonomous`. A remote object's buffer
    /// returns 0 and never reaches this. The same arm exists for a `0xF619` carrying a movement
    /// buffer.
    ///
    /// Nothing here stops the body: the stop is the buffer itself, whose
    /// `InterpretedMotionState` carries `forward_command = Ready`, applied by movement unpacking's
    /// case 0. What *this* does is cancel the
    /// run lock and mark the interpreter server-controlled, which is what decides whether the
    /// character starts moving again afterwards. Assert the two separately: a test that only
    /// checks the stop passes with the resume dead.
    ///
    /// The returned text is `set_auto_run`'s message-window line, queued for
    /// [`crate::app::App::apply_input_actions`] exactly as the `K::AutoRun` arm queues it.
    /// **`apply_current_movement` deliberately does not run here** -- losing control to the
    /// server clears the send-event flag.
    /// `out.auto_run` is refreshed and **nothing else is**, which is the shape of the client's own
    /// call: `set_auto_run`'s `apply_current_movement` tail is gated on the send-event flag and
    /// losing control passes false, so the *movement* is not re-applied here -- only the run
    /// lock's own value changes, and `CharacterInput` is a **view** of the lists rather than a
    /// second copy of them (the invariant `on_action` already maintains at its own tail). Without
    /// this line `char_input().auto_run` would keep reading `true` after the lock had been
    /// cancelled, which is a stale view rather than a re-applied movement.
    pub fn lose_control_to_server(&mut self, out: &mut CharacterInput) {
        self.lose_control_to_server_with_finish(out, || {});
    }

    /// The client finishes a held jump right after cancelling the run lock (without sending an
    /// event), before the next operation.
    /// Isolated command-list callers may use the wrapper; App supplies its actual body/model.
    pub fn lose_control_to_server_with_finish(
        &mut self,
        out: &mut CharacterInput,
        finish_jump: impl FnOnce(),
    ) {
        if self.lists.autonomy_level == 0 {
            return;
        }
        if let Some(text) = self.lists.lose_control_to_server() {
            self.notices.push(text);
            self.auto_run_notices_sent += 1;
        }
        out.auto_run = self.lists.auto_run;
        finish_jump();
    }

    /// Resume local command control during the per-frame update.
    ///
    /// `motions_pending` reports pending motions in the body's movement manager;
    /// `is_moving_to` reports its move-to state. They are the delay: the retake waits for the
    /// stance-change animation to drain, which is why the stop is not instantaneous.
    ///
    /// Returns whether control was taken back. When it is, the caller must also run the body half
    /// of the retake — [`Character::take_control_from_server`] — because
    /// the two live on different objects here exactly as they do in the client.
    ///
    /// The `set_hold_run(hold_run)` tail is run here rather than by the body half because
    /// it is the *interpreter's* hold key; it is a re-assert of the value already held, so it
    /// changes `out.run` only when the projection had drifted.
    pub fn use_time(
        &mut self,
        motions_pending: bool,
        is_moving_to: bool,
        out: &mut CharacterInput,
    ) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if !self
            .lists
            .can_take_control_from_server(motions_pending, is_moving_to)
        {
            return false;
        }
        if !self.lists.take_control_from_server() {
            return false;
        }
        out.run = self
            .lists
            .set_hold_run(self.lists.hold_run, self.ui_toggles_run);
        self.apply_current_movement(out);
        true
    }

    /// Clear all commands, plus the projection.
    ///
    /// **This is not the focus-loss path.** Keyboard-focus loss has one movement-state tail call
    /// of its own, and that function is not this one. See [`Self::lose_keyboard_focus`] for the
    /// three differences and what each of them would cost. Clearing all commands is a real entry
    /// point with real callers (stopping completely, the player-teleported handler) and a test
    /// uses it as the instrument it is: the thing that proves emptying the lists does **not**
    /// clear `standing_longjump`.
    pub fn clear_all_commands(&mut self, out: &mut CharacterInput) {
        self.lists.clear_all_commands();
        let _ = self.lists.set_auto_run(false);
        self.lists.transient_state = false;
        self.apply_current_movement(out);
    }

    /// Losing keyboard focus.
    ///
    /// Window deactivation reaches this path both for keyboard-focus loss and application
    /// deactivation. It clears the keyboard entries from all three command lists, clears held run
    /// and sidestep state, and finishes a held jump. It reapplies the remaining list heads and
    /// sends movement only when autonomy is nonzero and the server does not control the body.
    ///
    /// # Three statements that are easy to get wrong, and what each would cost
    ///
    /// 1. **Clearing all commands, where retail clears only the keyboard.** See
    ///    [`crate::actions::movement::CommandLists::clear_keyboard_commands`]: the mouse head survives
    ///    a focus loss and is re-issued by the `apply_current_movement` below.
    /// 2. **`set_auto_run(false)` and `transient_state = false`, which appear nowhere in
    ///    that function.** `apply_current_movement` opens with
    ///    `if (auto_run) move_player(0x45000005 /* WalkForward */)`, so retail's auto-running
    ///    character keeps running through an alt-tab; either statement would stop him.
    /// 3. **`apply_current_movement` unconditionally, where the client refuses it while the server
    ///    owns the body.** An unconditional
    ///    `apply_current_movement` on a body running a server `MoveTo` cancels it. A non-autonomous
    ///    `0xF74C` is exactly what sets `controlled_by_server`
    ///    ([`Self::lose_control_to_server`]), and it stays set for the whole approach. Alt-tabbing
    ///    mid-MoveTo must not end the MoveTo, because the client does not.
    ///
    /// Finishing the jump belongs to the animation crate; the app-level half of the held-jump
    /// release already runs through `release_pressed_keys`'s per-control synthetic release, which
    /// a focus-release test measures.
    ///
    /// **Named gap:** the movement-event tail is not sent. Returning whether
    /// `apply_current_movement` ran is what a caller would need to send it; nothing does yet, so a
    /// focus loss that changes the movement state does not tell the server. That is a wire
    /// message and should wait until it can be measured against a capture.
    pub fn lose_keyboard_focus(&mut self, out: &mut CharacterInput) -> bool {
        self.lists.clear_keyboard_commands();
        // Pass false to drop the run lock the *key* holds; `set_hold_run`'s answer
        // is the same XOR against `ui_toggles_run` every other caller gets.
        out.run = self.lists.set_hold_run(false, self.ui_toggles_run);
        self.lists.hold_sidestep = false;
        if self.lists.autonomy_level == 0 || self.lists.controlled_by_server {
            return false;
        }
        self.apply_current_movement(out);
        true
    }
}

// Motion-completion routing has no free function here on purpose. A convention where every
// caller must remember to feed drained completion events to the body is one that gets
// forgotten: eight call sites across this file, `dereth_animation` and `world.rs` collected the
// events and never routed them. `MotionTableManager`'s five producers are `pub(crate)` and the
// only drain reachable from this crate is the motion interpolator's completed-motion drain,
// which does both halves and cannot be spelled without the route. An action-freeze integration
// test measures the cost of getting it wrong.

/// How many times physics called back across the seam, per direction.
/// The walkable-state transition is the only caller of either, and only on an edge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GroundEdges {
    pub hit: u64,
    pub left: u64,
}

/// A second handle onto the object's `MotionDriver`.
///
/// Every method forwards; the whole of the seam's behaviour is the animation crate's. The two
/// counters are this crate's, and exist so a test can assert the *callback* half of the seam fired
/// rather than only the call half.
#[derive(Debug, Clone)]
pub struct SharedMotion {
    driver: Rc<RefCell<MotionDriver>>,
    edges: Rc<std::cell::Cell<GroundEdges>>,
}

impl SharedMotion {
    pub fn new(driver: Rc<RefCell<MotionDriver>>) -> Self {
        Self {
            driver,
            edges: Rc::new(std::cell::Cell::new(GroundEdges::default())),
        }
    }
}

impl MotionSource for SharedMotion {
    fn sync_physics_state(&mut self, state: dereth_primitives::MotionPhysicsState) {
        let mut d = self.driver.borrow_mut();
        d.env.object_id = state.object_id;
        d.env.position = state.position;
        d.env.velocity = state.velocity;
        // `MotionEnv::cached_velocity` is the velocity `HandleMoveToPosition`'s quantum block
        // reads. `WorldScene::refresh_object_env` fills it for a server object's driver; left at
        // zero here, the local player's own move-to would never write its extrapolation lead.
        // The same seam field the remote bodies use, refreshed per sub-step after the velocity
        // store.
        d.env.cached_velocity = state.cached_velocity;
        d.env.radius = state.radius;
        d.env.height = state.height;
        d.env.in_cell = state.in_cell;
        d.env.contact = state.contact;
        d.env.on_ground = state.on_ground;
        d.env.gravity_affected = state.gravity_affected;
    }

    fn adjust_position_offset(&mut self, offset: &mut Frame, quantum: f64) {
        let d = self.driver.borrow();
        #[allow(clippy::cast_possible_truncation)] // retail receives a float quantum, widened here
        d.movement.sticky.adjust_offset(
            offset,
            quantum as f32,
            &d.env.position,
            d.env.radius,
            Some(&d.movement.interp),
            &d.env,
        );
    }

    fn advance(&mut self, quantum: f64) -> Frame {
        self.driver.borrow_mut().advance(quantum)
    }
    fn tick_movement(&mut self, now: LocalTime) {
        self.driver.borrow_mut().tick_movement(now);
    }
    /// The movement manager's hit-ground event, fired when the object becomes walkable.
    ///
    /// **The env is corrected here, before the call.** Hit-ground handling runs
    /// `apply_current_movement`, which asks the physics object if it is on the ground *live*; the
    /// separated `MotionEnv` carries a copy that was taken at the top of the frame, when the object
    /// was still airborne. Physics sets `CONTACT_TS` on the line above
    /// its `set_on_walkable(walkable)` call, so at the moment this fires both facts are true by
    /// construction. Without this the movement layer sees an airborne object at the instant it
    /// lands and puts it in the `Falling` substate, where it stays until the next ground edge —
    /// which is a T-pose that never resolves.
    fn hit_ground(&mut self) {
        let mut e = self.edges.get();
        e.hit += 1;
        self.edges.set(e);
        let mut d = self.driver.borrow_mut();
        d.env.contact = true;
        d.env.on_ground = true;
        d.hit_ground();
    }

    /// The leave-ground event comes from the same call site with `on == false`. `on_ground` is
    /// `Contact && OnWalkable`, so clearing the walkable bit clears it whatever contact says.
    fn leave_ground(&mut self) {
        let mut e = self.edges.get();
        e.left += 1;
        self.edges.set(e);
        let mut d = self.driver.borrow_mut();
        d.env.on_ground = false;
        d.leave_ground();
    }
    fn process_hooks(&mut self) {
        self.driver.borrow_mut().process_hooks();
    }
    fn has_collision_geometry(&self) -> bool {
        self.driver.borrow().has_collision_geometry()
    }
    /// The body's adjusted maximum speed, which movement interpolation reads every sub-step.
    /// Without this forward the App's own bodies (every remote body wears a
    /// `SharedMotion`, `WorldScene::prepare_object_physics`) would keep the `7.5` fallback.
    fn motion_max_speed(&self, use_adjusted: bool) -> Option<f32> {
        self.driver.borrow().motion_max_speed(use_adjusted)
    }
    /// Whether interpolation keeps heading, passed by `MoveOrTeleport` to `InterpolateTo`.
    fn is_moving_to(&self) -> bool {
        self.driver.borrow().is_moving_to()
    }
}

use dereth_world_data::setup::setup_geometry;

/// The player's body: one physics object, one animation driver, and the once-per-frame call order
/// that joins them.
/// `TargetManager`'s side of an approach: the object the move-to asked to be watched.
///
/// Setting target `id` with a 0.5-second interval creates one and
/// target simulation refreshes it on a **0.5 simulated-second** gate;
/// `clear_target` destroys it. This keeps exactly those three facts, because the body has no
/// object table of its own -- the application feeds it positions through
/// [`Character::update_target`].
pub use dereth_animation::motion::MotionTarget as ApproachTarget;

pub struct Character {
    /// The client-side physics driver.
    pub world: PhysicsWorld,
    pub handle: PhysHandle,
    driver: Rc<RefCell<MotionDriver>>,
    edges: Rc<std::cell::Cell<GroundEdges>>,
    /// The landblock the render space is relative to, so world positions can be expressed in it.
    viewer_block: (i32, i32),
    /// The same [`DatLandSource`] `world` collides against, kept so the landblock streaming path
    /// can prefetch a block's interior cells into it. `LandSource::env_cell` only
    /// returns visible cells and never loads, so somebody outside physics has to.
    land: Arc<DatLandSource>,
    /// [`crate::camera::CameraControl`] owns the camera's pivot, target, and state,
    /// driven once per frame by [`Self::update_camera`]. It lives on the body because the
    /// camera pivots on that body and its swept sphere runs through [`Self::world`], sharing
    /// the same physics simulation the body walks in.
    pub camera: crate::camera::CameraControl,
    /// Whether the last [`Self::update`] raised
    /// [`dereth_physics::PhysicsNotice::PlayerPhysicsUpdated`] — that is, whether
    /// the physics-update gate opened and the object it stepped was the player.
    ///
    /// **This is what drives the camera smoother, and it has to be.** The smoother runs only
    /// inside the player-physics callback, and that callback runs only from the physics tick's
    /// player arm. So the smoother is on the **body's** clock, not the display's, and between
    /// ticks nothing in the drawn scene moves at all.
    player_physics_updated: bool,
    /// **The impact effect, parked for the scene the way `0xF755`'s scripts are.**
    ///
    /// Every object whose environment or missile collision this tick reached the
    /// collision-effect callback — *not* ordinary object collision, which returns success without further work.
    /// The scene drains it immediately after [`Self::update`] and calls `play_default_script`; playback needs
    /// both `PhysicsScriptTable` and the object's animation driver.
    /// Those belong to `WorldScene`
    /// rather than the `PhysicsWorld` this type owns. Same seam, and same reason, as
    /// [`crate::objects::ScriptEvent`].
    collision_scripts: Vec<dereth_primitives::ObjectId>,
    /// **The house barrier's sparks**, the same seam one field up and for the same
    /// reason. `(the body that was stopped, the house that stopped it, `PlayScript`'s intensity)`.
    /// The client plays it synchronously from inside the entry check; here it has to be handed
    /// back, because the play-script type belongs to the house's public description in
    /// `dereth_client_model`,
    /// and because disabling house-restriction effects is the player's option word.
    restriction_effects: Vec<(
        dereth_primitives::ObjectId,
        dereth_primitives::ObjectId,
        f32,
    )>,
    /// This frame's key latch.
    pub input: CharacterInput,
    /// Last frame's, so a change can be issued as a motion the way a key press is.
    applied: CharacterInput,
    /// The object's scale.
    pub scale: f32,
    /// The setup record this body's part array and collision half are **actually** built from.
    ///
    /// [`ALUVIAN_MALE_SETUP`] until the server's `0xF745` names one, whatever it named afterwards
    /// ([`Self::set_setup_id`]). Kept rather than derived because
    /// [`dereth_animation::data::SetupData`] carries no id of its own, and because
    /// `WorldScene::apply_player_objdesc` has to compare the server's id against the body's
    /// **before** it decides whether an `AnimPartChange`'s part *index* means anything.
    setup_id: DataId,
    /// The motion-table id installed on the body, so [`Self::set_setup_id`] can preserve
    /// the setter's early return on an unchanged id.
    ///
    /// The client's unchanged-id path splits across two functions, and the split is the point:
    ///
    /// * part-array motion-table setup compares the installed motion-table id against the
    ///   argument and **returns 1 without
    ///   destroying the manager** — so the *animation* state survives;
    /// * body setup then destroys and recreates the
    ///   `MovementManager` on that truthy return anyway — so the *movement* state does not.
    ///
    /// `MotionDriver::set_motion_table` cannot express that split: it replaces the
    /// `MotionTableManager` **and** rebuilds the `MovementManager` **and** runs
    /// `enter_default_state`, whose `initialize_state` restarts the sequence. Calling it
    /// unconditionally on a rebuild therefore restarts the idle animation mid-life — measured
    /// on `early-inventory-and-casting`/`short-second-connection`/`short-play-with-training`, the
    /// body reached sequence frame 12/11/12 where an identically settled body that was not rebuilt
    /// reached 10, and the difference is visible as the arms coming up. Skipping it on an unchanged
    /// id reproduces the client's *net* outcome for that case, which is "the animation keeps
    /// playing"; the animation-side split itself remains a gap.
    ///
    /// Every capture in the corpus names `0x09000001`, which is the table the offline body
    /// already carries, so this guard is taken on all five.
    motion_table_id: DataId,
    /// The animation-asset source, kept so the body can be rebuilt from a
    /// different setup record after creation.
    assets: Arc<DatAnimAssets>,
    /// The dat store, for the collision half of the same rebuild ([`setup_geometry`] needs the
    /// decoded setup record, which `AnimAssets` does not hand back).
    store: Arc<RetailDatStore>,
    /// The animation events raised since the last drain. `apply_effects` must not drop
    /// them: three of them — `PlaySound`, `PlaySoundType` — are what a footstep is. They are
    /// buffered rather than acted on here because the sound system is the application's, not the
    /// body's, and the queue is drained every frame by `crate::audio::world_use_time`.
    events: RefCell<Vec<dereth_animation::AnimEvent>>,
    /// The id this body is registered under in the physics world.
    ///
    /// [`PLAYER_OBJECT_ID`] until `0xF746` arrives, the server's own id afterwards.
    object_id: ObjectId,
    /// Counters for the startup line and the tests.
    pub stats: CharacterStats,
}

/// What the character has done, for the log line and for the tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterStats {
    /// How many times the 30 Hz gate opened.
    pub physics_ticks: u64,
    /// Motions accepted by the movement interpolator.
    pub motions_issued: u64,
    /// Motions it refused, with a `WeenieError`.
    pub motions_refused: u64,
    /// The last `WeenieError` a refusal carried, passed through unchanged because the client's
    /// callers compare against the numbers (`dereth_animation::AnimError::Movement`).
    pub last_refusal: u32,
    /// Physics notices drained.
    pub notices: u64,
    /// Collisions that reached the receiver *and* found
    /// `SCRIPTED_COLLISION_PS` set, so an impact script was queued for the scene.
    pub collision_scripts: u64,
    /// Counts when the cell entry-restriction sub-step refuses entry,
    /// i.e. how many times a house barrier stopped this body. Counted before the
    /// `DisableHouseRestrictionEffects` gate, so it measures the *barrier* and not the sparks.
    pub move_restrictions: u64,
    /// How many times the body took a new object id from the server. One on a normal
    /// login; more than one only if a session ends and another character enters.
    pub id_adoptions: u64,
    /// Adoptions the physics world **refused** because another body in
    /// this physics simulation already holds the server's id. Counted per attempt, i.e. per frame for as
    /// long as the condition lasts, so it reads as "frames spent under the wrong id".
    ///
    /// Zero is the only correct value: a non-zero one means the server's own player object had been
    /// given a second body, which [`crate::object_physics::ObjectPhysics::sync`] excludes precisely
    /// so that it cannot be.
    pub id_adoptions_refused: u64,
    /// Server move-to/turn-to buffers handed to the movement manager.
    pub move_tos_performed: u64,
    /// `TargetInfo`s fed to the move-to target updater (the 0.5 s gate already applied).
    pub target_updates: u64,
    /// Stick-to-object calls that resolved a target and reached the position
    /// manager. The corpus carries **16** `StickToObject` buffers
    /// addressed to the session's own character, all in `long-solo-play`.
    pub sticks_applied: u64,
    /// Move-to cancellations reported, and the last error code one carried.
    pub move_tos_failed: u64,
    pub last_move_to_error: u32,
    /// How many times [`Character::set_setup_id`] actually rebuilt the body from a
    /// different setup record. One for a capture whose player is not an Aluvian male, **zero** for one
    /// whose player is — the counter has to distinguish "the server named the same setup" from
    /// "nothing ever asked", which is why it counts rebuilds and not calls.
    pub setup_changes: u64,
    /// How many times [`Character::set_setup_id`] installed a motion table
    /// **without** rebuilding the part array — the `PhysicsDesc` whose `mtable_id` moved while its
    /// `setup_id` stood still, which a setup-only guard would drop entirely.
    ///
    /// Zero across the whole capture corpus, and that is the point: no recorded description
    /// changes only the motion table, so this arm is reached only by a constructed test. A
    /// counter rather than nothing, because "the corpus never does this" is a claim that has to be
    /// re-measurable when the corpus grows.
    pub motion_table_changes: u64,
    /// Teleports whose placement committed,
    /// and teleports whose placement did not.
    ///
    /// A teleport writes `position` either way, so the two are indistinguishable from the
    /// outside — which is exactly what the client does with the `SetPositionError`
    /// Simple position setup returns before smart-box player teleport
    /// discards it. The difference is that an uncommitted teleport leaves contact, the contact
    /// plane and the walkable bit reading the *old* place, so a non-zero second counter means
    /// the body is standing on stale contact state.
    pub teleports_committed: u64,
    pub teleports_uncommitted: u64,
    /// Body-turn requests sent by the camera to the command interpreter
    /// — `MovePlayer` with mouse turning on, `TurnToHeading` in first person — and the
    /// `StopDrift` requests from its dead zone.
    ///
    /// A denominator, not decoration: without it "the turn key did nothing" and "no turn was
    /// ever decided" are the same observation from outside.
    pub camera_turns_issued: u64,
    pub camera_turn_stops: u64,
}

impl std::fmt::Debug for Character {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Character")
            .field("position", &self.position())
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl Character {
    /// Create the body and drop it on the ground at the centre of `landblock`.
    ///
    /// # Errors
    /// [`CharacterError`] when the setup, the motion table or the landblock is unavailable.
    pub fn new(
        store: &Arc<RetailDatStore>,
        region: &Region,
        landblock: u16,
        cell_offset: (f32, f32),
    ) -> Result<Self, CharacterError> {
        let land = Arc::new(DatLandSource::new(Arc::clone(store), region)?);
        let assets = Arc::new(DatAnimAssets::new(Arc::clone(store)));

        // --- The animation half -----------------------------------------------------------------
        let setup = assets
            .setup(ALUVIAN_MALE_SETUP)
            .ok_or(CharacterError::NoSetup(ALUVIAN_MALE_SETUP))?;
        let mut driver = MotionDriver::new(Arc::clone(&assets) as Arc<dyn AnimAssets>);
        // Part-array setup creation then default initialization.
        if !driver.set_setup(Arc::clone(&setup)) {
            return Err(CharacterError::NoPartArray(ALUVIAN_MALE_SETUP));
        }
        // Motion-table setup recreates the movement manager and enters
        // the default state, which is what queues the standing `Ready` cycle.
        if !driver.set_motion_table(ALUVIAN_MALE_MOTION_TABLE) {
            return Err(CharacterError::NoMotionTable(ALUVIAN_MALE_MOTION_TABLE));
        }
        driver.scale = ALUVIAN_MALE_SCALE;

        // --- The physics half -------------------------------------------------------------------
        let raw = store
            .read_typed(DbType::Setup, ALUVIAN_MALE_SETUP)
            .map_err(|_| CharacterError::NoSetup(ALUVIAN_MALE_SETUP))?;
        let decoded =
            Setup::decode_payload_in(store.era_of(ALUVIAN_MALE_SETUP), ALUVIAN_MALE_SETUP, &raw)
                .map_err(|_| CharacterError::NoSetup(ALUVIAN_MALE_SETUP))?;
        let geometry = Arc::new(setup_geometry(&decoded));

        let block = LandblockId(landblock);
        // **The block the body is about to stand in has to be *resident* first.**
        //
        // In retail a physics object is never created into a landblock the landscape has not
        // loaded: changing position loads the landscape window and only then places the body.
        // `CellResolver::get_visible` asks `LandSource::landblock_resident`, which for
        // `DatLandSource` is [`DatLandSource::prefetched`], the fully-loaded state -- so
        // a block that has only been *decoded* (which `ground_height` below does) resolves **no
        // cells at all**. Transitional insertion then takes `get_visible`'s null arm and returns
        // `OK_TS` with no cell collision run, so the body falls through Dereth for ever: 60 frames
        // after this call it is 19 m under the terrain it was placed on, `CONTACT_TS` and
        // `ON_WALKABLE_TS` never set.
        //
        // `load_block_cells` is retail's cell prefetch and is idempotent -- a block already pulled costs
        // one set lookup -- and this is the same prerequisite, for the same reason,
        // `app::apply_player_teleport_at` states for a teleport destination and
        // `WorldScene::attach_character` / `load_pending_scene` state for the drawn window. Those
        // three cover every block *except* the one the body is created in, which is this line.
        land.load_block_cells(block);
        let ground = land
            .as_ref()
            .ground_height(block, cell_offset.0, cell_offset.1)
            .ok_or(CharacterError::NoLandblock(landblock))?;

        let mut world = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn dereth_physics::LandSource>);
        let handle = world.create(PLAYER_OBJECT_ID, geometry, true);

        // Outdoor-position adjustment picks the land cell the origin falls in; the
        // origin stays block-local.
        let mut cell = block.cell(1);
        let mut origin = Vec3::new(cell_offset.0, cell_offset.1, ground);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
        world.enter_cell(handle, cell);

        let driver = Rc::new(RefCell::new(driver));
        let edges = Rc::new(std::cell::Cell::new(GroundEdges::default()));
        {
            let o = world.get_mut(handle).expect("just created");
            o.position = Position::new(cell, Frame::new(origin, Quat::IDENTITY));
            o.scale = ALUVIAN_MALE_SCALE;
            o.set_motion(Box::new(SharedMotion {
                driver: Rc::clone(&driver),
                edges: Rc::clone(&edges),
            }));
            // Mark the body active: without it `update_object` does nothing
            // at all and the body hangs in the air.
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
            o.update_time = 0.0;
        }
        world.calc_cross_cells(handle, false);
        world.set_player(handle);

        let mut c = Self {
            world,
            handle,
            driver,
            edges,
            viewer_block: dereth_world_data::landblock::block_xy(landblock),
            land: Arc::clone(&land),
            camera: crate::camera::CameraControl::new(PLAYER_OBJECT_ID),
            player_physics_updated: false,
            collision_scripts: Vec::new(),
            restriction_effects: Vec::new(),
            input: CharacterInput::default(),
            applied: CharacterInput::default(),
            scale: ALUVIAN_MALE_SCALE,
            setup_id: ALUVIAN_MALE_SETUP,
            motion_table_id: ALUVIAN_MALE_MOTION_TABLE,
            assets: Arc::clone(&assets),
            store: Arc::clone(store),
            events: RefCell::new(Vec::new()),
            object_id: PLAYER_OBJECT_ID,
            stats: CharacterStats::default(),
        };
        c.refresh_env();
        // Say which id the body was created with. Offline this is the only line there
        // will ever be; connected, `adopt_server_id` prints the second one a few frames later.
        tracing::info!(
            "local body created as {:#010X} -- no server id yet",
            PLAYER_OBJECT_ID.0
        );
        Ok(c)
    }

    /// The id this body is registered under in the physics world.
    ///
    /// [`PLAYER_OBJECT_ID`] until the server has spoken, its own id afterwards.
    #[must_use]
    pub const fn object_id(&self) -> ObjectId {
        self.object_id
    }

    /// The setup record this body is **actually** built from.
    #[must_use]
    pub const fn setup_id(&self) -> DataId {
        self.setup_id
    }

    /// The motion table this body is **actually** running.
    ///
    /// This is the installed motion-table id, which the part-array setter compares its argument
    /// against. It is the second half of what [`Self::set_setup_id`] installs, and the half a
    /// setup-only guard cannot see.
    #[must_use]
    pub const fn motion_table_id(&self) -> DataId {
        self.motion_table_id
    }

    /// Rebuild the body from the setup record the server named.
    ///
    /// The client's null-object initialization performs this call: with a part array
    /// already present it replaces the part array's setup rather than
    /// `InitPartArrayObject`, then re-caches the physics-BSP bit.
    /// Rebuilding re-places the array at the object's frame, then applies the descriptor's motion
    /// table immediately afterwards.
    ///
    /// **Why the local body needs it at all.** The player's body is made from the descriptor's
    /// setup id: the create-player message supplies the id before `0xF745`, and that description
    /// supplies the body. This build stands a body up
    /// before anything has logged in (`--no-connect`, every offline test), so the setup has to be
    /// *replaced* when the server finally says what it is. Without the replacement,
    /// `WorldScene::apply_player_objdesc` refuses the whole description of the two
    /// capture players whose setup record is `0x0200004E` — the **human female** body. Eighteen
    /// distinct sex setups ship, and those players would be drawn in
    /// the loincloth `CreateSetup` gave them.
    ///
    /// **This is not char-gen's path.** The heritage group's char-gen setup is `0x02000054` for
    /// all thirteen heritages and is a fallback for a character who does not exist yet. Here the
    /// server has already described a character, so the *only* authority is the id on the wire.
    ///
    /// `Ok(false)` when the body already carries that setup — a re-offered `0xF625` must not wipe
    /// a live part array, because restoring the same description changes palettes only and
    /// unequipping depends on the surviving state.
    ///
    /// **The bool is "the part array was rebuilt", not "the call did nothing".** The
    /// early return above is a guard on the *setup*, and this function installs **two** things:
    /// `PhysicsDesc` carries `setup_id` and `mtable_id` independently, and the client applies them
    /// through two separate calls. So a description that changes only `mtable_id` takes the
    /// unchanged-setup path, installs the table on its own, and still answers `Ok(false)` —
    /// because the caller's `rebuilt` drives a part-mesh re-bake and a sound-table re-read that a
    /// motion table does not move. [`Self::motion_table_id`] and
    /// [`CharacterStats::motion_table_changes`] are what to ask about that half.
    ///
    /// # Errors
    /// [`CharacterError`] when the setup record is missing, will not decode, will not make a part
    /// array, or when the named motion table does not load.
    pub fn set_setup_id(
        &mut self,
        setup_id: DataId,
        motion_table: Option<DataId>,
    ) -> Result<bool, CharacterError> {
        // First compare the id against the one the
        // `MotionTableManager` already holds and **return without touching it** if they match. See
        // [`Self::motion_table_id`] — without this the rebuild runs `enter_default_state`, whose
        // `initialize_state` restarts the idle animation mid-life.
        //
        // This sits above the setup guard on purpose. Below it, the setup comparison would stand
        // in for **the two things this body installs**: `PhysicsDesc` carries `setup_id` and
        // `mtable_id` independently, and null-object initialization (the setup) and motion-table
        // setup are two separate calls in the client, so a description that changes **only**
        // `mtable_id` is one the client applies and a setup-only guard drops entirely. No
        // recorded description does that, so the test for it is constructed rather
        // than replayed.
        let motion_table = motion_table.filter(|m| m.0 != 0 && *m != self.motion_table_id);
        if setup_id == self.setup_id {
            // The setup is unchanged, so the part array must not be rebuilt: restoring the same
            // description changes palettes only and unequipping depends on the surviving state.
            // The table is the other half of `set_description` and is installed on its own, which
            // is what the client's two separate calls do when only the second one's argument moved.
            let Some(mt) = motion_table else {
                return Ok(false);
            };
            // Resolved before anything is mutated, the same promise the rebuild path below makes,
            // and with the same consequence at the caller: `WorldScene::apply_player_objdesc`
            // counts an `Err` as `objdesc_setup_mismatch` and **refuses the whole description**.
            // That is a declared choice rather than an accident — it is what the rebuild path
            // already did for a bad `mtable_id`, so this arm does not invent a second policy — and
            // it is unreachable on shipped data, where every `mtable_id` on the wire resolves.
            if dereth_animation::data::AnimAssets::motion_table(self.assets.as_ref(), mt).is_none()
            {
                return Err(CharacterError::NoMotionTable(mt));
            }
            {
                let mut d = self.driver.borrow_mut();
                assert!(
                    d.set_motion_table(mt),
                    "the motion table was resolved above"
                );
            }
            self.motion_table_id = mt;
            self.stats.motion_table_changes += 1;
            // `false` because the **part array** was not rebuilt: the bool is
            // `WorldScene::apply_player_objdesc`'s `rebuilt`, and it drives a re-bake of the part
            // meshes and a re-read of the body's sound table, neither of which a motion table
            // moves. [`Self::motion_table_id`] and [`CharacterStats::motion_table_changes`] are
            // what a caller or a test asks about this half.
            return Ok(false);
        }
        // Both halves are resolved **before** anything is mutated, so a setup that will not build
        // leaves the body exactly as it was and the caller can refuse the description instead of
        // dressing a half-replaced array.
        let setup = self
            .assets
            .setup(setup_id)
            .ok_or(CharacterError::NoSetup(setup_id))?;
        let raw = self
            .store
            .read_typed(DbType::Setup, setup_id)
            .map_err(|_| CharacterError::NoSetup(setup_id))?;
        let decoded = Setup::decode_payload_in(self.store.era_of(setup_id), setup_id, &raw)
            .map_err(|_| CharacterError::NoSetup(setup_id))?;
        let geometry = Arc::new(setup_geometry(&decoded));
        // Resolved here, for the same reason the setup is: `set_motion_table` destroys and
        // recreates the movement manager, so discovering the table is missing *after* `set_setup`
        // has run would leave the body half-replaced.
        if let Some(mt) = motion_table {
            if dereth_animation::data::AnimAssets::motion_table(self.assets.as_ref(), mt).is_none()
            {
                return Err(CharacterError::NoMotionTable(mt));
            }
        }

        {
            let mut d = self.driver.borrow_mut();
            // Rebuild as a fresh setup plus default initialization.
            if !d.set_setup(Arc::clone(&setup)) {
                return Err(CharacterError::NoPartArray(setup_id));
            }
            // `set_description`'s own call, and the reason it is here rather than left at the
            // Aluvian male's `0x09000001`: a body whose motion table is not its own plays another
            // body's animation ids.
            if let Some(mt) = motion_table {
                assert!(
                    d.set_motion_table(mt),
                    "the motion table was resolved above"
                );
            }
        }
        if let Some(mt) = motion_table {
            self.motion_table_id = mt;
        }

        if let Some(o) = self.world.get_mut(self.handle) {
            o.geometry = Arc::clone(&geometry);
            // Recompute the cached physics-BSP flag after the array is installed.
            // `HAS_PHYSICS_BSP_PS` outranks the cylsphere count in collision selection, so a stale
            // bit is a body that collides
            // by the wrong arm.
            let mut state = o.state();
            state.set_has_physics_bsp(geometry.caches_physics_bsp());
            let _ = o.set_state(state);
        }
        // The new setup has its own radius and height, so the cells the body straddles change.
        self.world.calc_cross_cells(self.handle, false);
        self.setup_id = setup_id;
        self.stats.setup_changes += 1;
        self.refresh_env();
        tracing::info!(
            "local body rebuilt from the server's setup record {:#010X} ({} parts)",
            setup_id.0,
            setup.parts.len()
        );
        Ok(true)
    }

    /// Apply the described object scale to animation and collision together.
    pub fn set_scale(&mut self, scale: f32) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        self.scale = scale;
        {
            let mut driver = self.driver.borrow_mut();
            driver.scale = scale;
            driver
                .part_array
                .set_scale_internal(Vec3::new(scale, scale, scale));
        }
        if let Some(object) = self.world.get_mut(self.handle) {
            object.scale = scale;
        }
        self.world.calc_cross_cells(self.handle, false);
        self.refresh_env();
    }

    /// Take the object id the server sent in `0xF746 Login_CharacterSet`.
    ///
    /// The create-player message hands the client the player's id **before** the
    /// `0xF745` that describes him, and the client then makes his one physics object carrying it —
    /// so in the original the id is an argument to a create and never changes afterwards. This
    /// build stands the body on the ground before it logs in, because the offline modes need a
    /// body and no server, so the same fact arrives here as a **re-key** of
    /// the physics world instead. That assignment performs the two halves of a create
    /// in the other order and refuses an occupied id rather than overwriting it.
    ///
    /// Why it matters: the table is keyed by object id and a second insert for a key replaces the
    /// first, so a body sharing an id with one of the server's objects would be dropped out of
    /// the physics sweep with no error anywhere. With the server's own id on the local
    /// body that clash cannot be constructed — the one object that carries it *is* the player, and
    /// [`crate::object_physics::ObjectPhysics::sync`] excludes him by that same id.
    ///
    /// Idempotent, and meant to be called every frame: `None`, or the id it already has, does
    /// nothing. `App::sync_objects` calls it immediately before
    /// [`crate::objects::ObjectStream::sync_physics`], so the local body is re-keyed *before* any
    /// of the server's objects are registered.
    ///
    /// Returns `true` when the id changed.
    pub fn adopt_server_id(&mut self, id: Option<ObjectId>) -> bool {
        let Some(id) = id else { return false };
        if id == self.object_id {
            return false;
        }
        if !self.world.set_object_id(self.handle, id) {
            self.stats.id_adoptions_refused += 1;
            // Printed once. The condition lasts as long as the other body does, and this runs every
            // frame, so the counter is the number of frames spent in the wrong state and the line
            // is the notice; sixty of the same line a second would be neither.
            if self.stats.id_adoptions_refused == 1 {
                tracing::warn!(
                    "will not re-key the local body {:#010X} -> {:#010X}: \
                     another body already holds that id",
                    self.object_id.0,
                    id.0
                );
            }
            return false;
        }
        // `CameraManager` names its pivot and target by **id**, not by handle
        // (setting the camera pivot), and the camera's default offsets pointed
        // both at the body at construction. Re-point exactly those, so a camera that was tracking
        // something else keeps tracking it.
        if self.camera.manager.pivot_object_id == self.object_id {
            self.camera.manager.pivot_object_id = id;
        }
        if self.camera.manager.target_object_id == self.object_id {
            self.camera.manager.target_object_id = id;
        }
        tracing::info!(
            "local body is {:#010X} (was {:#010X})",
            id.0,
            self.object_id.0
        );
        self.object_id = id;
        self.stats.id_adoptions += 1;
        true
    }

    /// Put the body where the server says the player is.
    ///
    /// Object creation's step 4 player arm calls `store_position(pd.position)`
    /// then enters the player at `pd.position`.
    ///
    /// **It stands for two different retail functions and only one of them is that one.** The
    /// login arrival and `world.rs`'s `--start-cell` are the `enter_world` arrival above;
    /// `app.rs::apply_player_teleport` models
    /// smart-box player teleport through simple object positioning, a
    /// different function with different `SetPositionStruct` flags which — unlike the login
    /// arrival — does **not** clear the link animations. Both reach
    /// the common positioning half modelled below; the link-animation difference is not
    /// modelled here.
    ///
    /// **Two `enter_world` overloads, and it matters which one.**
    /// The physics object's enter-world path is three calls long:
    /// it stores the position, passes `1` to the integer overload, then returns. Everything
    /// interesting is in that overload: a `0x11` flag word, the placement call and the
    /// link-animation clear. The part array removes link animations directly; the physics-object
    /// wrapper has been inlined. Same operation, different call shape.
    ///
    /// The cell is normalised first, because an outdoor position's cell index has to agree with
    /// its origin: the server sends the landblock's cell 1 and the
    /// origin decides which of the 64 it really is.
    pub fn teleport(&mut self, pos: Position) {
        let mut cell = pos.cell;
        let mut origin = pos.frame.origin;
        // **Only for an outdoor position.** The outside-cell conversion also accepts an
        // *interior* cell id and then answers with
        // the land cell the building stands in, which is exactly right for "what is outside this
        // room" and exactly wrong as a normalisation. Applying it unconditionally teleports a
        // player the server placed indoors onto the grass outside.
        if dereth_physics::landdefs::is_outdoors(cell) {
            dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
        }
        let there = Position::new(cell, Frame::new(origin, pos.frame.rotation));
        self.world.enter_cell(self.handle, cell);
        if let Some(o) = self.world.get_mut(self.handle) {
            o.position = there;
            // The object's own clock restarts: `update_object` measures from `update_time`, and a
            // teleport must not be charged the time the body spent standing somewhere else.
            o.update_time = 0.0;
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
        }
        // Refresh before the commit because the ground edge fires inside it: in the client
        // hit-ground and leave-ground handlers read the physics object live, while here they read
        // a snapshot.
        //
        // **This line is correct and unfalsifiable in this tree, and that is written down rather
        // than left to be rediscovered.** Deleting it SURVIVES mutation testing, even in
        // isolation. It has to: the fields those two bodies read before they act —
        // `is_creature`, `gravity_affected`, and the `has_weenie`/load/jump-skill trio
        // `get_leave_ground_velocity` uses — do not change across a teleport, and
        // `SharedMotion::hit_ground` / `leave_ground` correct `contact` and `on_ground`
        // themselves at the call. What it does carry that nothing else would is `env.position`,
        // which reads on the next frame — and a move-to in
        // flight across a teleport is a case no test in this tree builds.
        self.refresh_env();
        // **The position commit, and it is the whole of what a teleport does to contact.**
        //
        // Every client teleport reaches the same placement operation. Player teleport reaches it
        // through simple positioning with reset enabled; the arrival this
        // function's doc comment above names reaches it through the integer overload
        // of the position setter. It runs a **placement**
        // transition at the destination and commits the answer, rewriting `CONTACT_TS` from the
        // transition's own collision result and setting walkable state when
        // `contact_plane.normal.z >= FLOOR_Z`. The walkable setter is a pure
        // **edge** detector, so hit-ground or leave-ground — and
        // through them, the client's only early
        // exit from a full motion ledger — fire exactly when the teleport changes whether the
        // body is standing on walkable ground, and not otherwise.
        //
        // **The ledger is not cleared here, and it must not be.** The edge is the mechanism; a
        // clear ledger is one of its consequences, and so are the contact plane, the sliding
        // normal, `calc_acceleration`'s gravity arm and `handle_all_collisions`' two "was"
        // arguments. Writing the consequence would leave every one of those still reading the
        // state of wherever the body used to be standing. Leaving that state stale accounted for
        // fourteen of twenty-six measured wall penetrations in the interiors tests.
        if self.world.set_position(self.handle, &there) {
            self.stats.teleports_committed += 1;
        } else {
            // `CheckPositionInternal` answered 0, or the ten-deep transition pool was
            // exhausted. The client discards this too — its player-teleport path ignores
            // `SetPositionSimple`'s `SetPositionError` entirely — so the body keeps the position
            // written above and the only record is this counter.
            self.stats.teleports_uncommitted += 1;
        }
        self.world.calc_cross_cells(self.handle, false);
        self.refresh_env();
        // The viewer reset arm: a teleport must not leave the smoother
        // chasing the body across the world, and the sweep must not try to slide a sphere from
        // the old room to the new one.
        if let Some(p) = crate::camera::pivot_state(&self.world, self.handle) {
            self.camera.attach(&p);
        }
    }

    /// Run the cleanup reached by accepted player teleport completion,
    /// not by initial placement. The movement/sticky/target portion uses their existing owners.
    /// Interpolation, constraint and voyeur managers are not attached to this Character yet;
    /// their stop-interpolating, release-constraint and notify-voyeur callbacks remain explicit
    /// residuals.
    pub fn player_teleport_hook(&mut self) {
        self.refresh_env();
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.cancel_move_to(0x3c, &mut ctx);
        movement.unstick_from_object(&mut ctx);
        movement.drain_completed_motions(&mut ctx);
        drop(d);
        self.events.borrow_mut().extend(events);
        self.apply_effect_list(effects);
        // ClearTarget follows the position-manager cleanup even when neither manager was active.
        self.driver.borrow_mut().target = None;
    }

    /// The land source physics reads, so the streaming path can prefetch interior cells into it.
    #[must_use]
    pub fn land(&self) -> &Arc<DatLandSource> {
        &self.land
    }

    /// Re-anchor [`Self::render_frame`] on a new viewer block.
    ///
    /// The renderer draws in a space whose origin is the south-west corner of the **viewer's**
    /// landblock, and that block moves as the player walks. The
    /// body itself does not move: `position` is landblock-local and is untouched.
    ///
    /// **It returns the frame's [`RenderSpace`] token, and is the only thing that can.**
    /// Choosing the block *is* the event every render-space writer has to be after, so the
    /// proof is minted here rather than announced by a comment somewhere else. Calling it again
    /// with the same block is what `crate::world::WorldScene::recenter` does on a frame that
    /// did not cross a boundary — the assignment is idempotent and the token is the point.
    pub fn set_viewer_block(&mut self, block: (i32, i32)) -> RenderSpace {
        self.viewer_block = block;
        RenderSpace::chosen()
    }

    /// The object's world position: the landblock it stands in and its block-local frame.
    #[must_use]
    pub fn position(&self) -> Position {
        self.world.get(self.handle).map_or_else(
            || Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            |o| o.position,
        )
    }

    /// The position expressed in the renderer's **viewer-block-relative** space — the same space
    /// landscape rendering uses, whose origin is the south-west
    /// corner of the block the scene was built around.
    #[must_use]
    pub fn render_frame(&self) -> Frame {
        self.render_frame_of(self.position())
    }

    /// True while the object is in contact with a walkable surface:
    /// `Contact && OnWalkable`.
    #[must_use]
    pub fn on_ground(&self) -> bool {
        self.world
            .get(self.handle)
            .is_some_and(|o| o.transient_state.in_contact() && o.transient_state.on_walkable())
    }

    /// `transient_state & 1` (`CONTACT_TS`) **alone**, which is the exact test that
    /// refuses a log-off. It is deliberately *not* [`Self::on_ground`]: that one is the physics
    /// object's
    /// `Contact && OnWalkable` pair, and a player standing on something the physics calls
    /// unwalkable may still log out.
    #[must_use]
    pub fn in_contact(&self) -> bool {
        self.world
            .get(self.handle)
            .is_some_and(|o| o.transient_state.in_contact())
    }

    /// The setup's own height, 1.835 m for the Aluvian male.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.world
            .get(self.handle)
            .map_or(0.0, dereth_physics::PhysicsObj::height)
    }

    /// The setup's own radius, exposed so a test can say
    /// the **collision** half of a rebuilt body came from the server's setup record too, not only the
    /// drawn half.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.world
            .get(self.handle)
            .map_or(0.0, dereth_physics::PhysicsObj::radius)
    }

    /// The achieved velocity from the physics object. This is
    /// `cached_velocity`, which is zero whenever the object was blocked.
    #[must_use]
    pub fn velocity(&self) -> Vec3 {
        self.world
            .get(self.handle)
            .map_or(Vec3::ZERO, dereth_physics::PhysicsObj::velocity)
    }

    /// How many times physics called back into the animation layer on a ground transition.
    ///
    /// This is the *return* half of the bidirectional seam — firing `HitGround` / `LeaveGround` —
    /// and it is counted so a test can assert it
    /// happened rather than assuming it did.
    #[must_use]
    pub fn ground_edges(&self) -> GroundEdges {
        self.edges.get()
    }

    /// Borrow the animation driver, for the renderer and for the tests.
    #[must_use]
    pub fn driver(&self) -> std::cell::Ref<'_, MotionDriver> {
        self.driver.borrow()
    }

    /// The same, mutably — for `set_no_draw` and the other per-part flags an animation hook or a
    /// server message sets. **Never hold it across [`Character::update`]**: physics borrows the
    /// same cell through the seam.
    #[must_use]
    /// Drain the animation events raised since the last call, in the order they were raised.
    ///
    /// The order is observable, so this is a FIFO `Vec` and never a set.
    pub fn take_anim_events(&self) -> Vec<dereth_animation::AnimEvent> {
        let mut mine = std::mem::take(&mut *self.events.borrow_mut());
        // Anything raised after the last `apply_effects` -- `update_particles` runs after it -- is
        // still on the driver's own queue.
        mine.extend(self.driver.borrow_mut().take_events());
        mine
    }

    pub fn driver_mut(&self) -> std::cell::RefMut<'_, MotionDriver> {
        self.driver.borrow_mut()
    }

    /// The style prelude of local movement unpacking. Finish cancellation's clear-target and
    /// reporting tail before the next accepted command can install a new target subscription.
    pub fn apply_movement_style(&mut self, style: MotionCommand) {
        self.driver.borrow_mut().apply_movement_style(style);
        self.apply_effects();
    }

    /// Local movement unpacking, case 0. The manager cancels synchronously before state
    /// installation; this host must also consume its physics/report effects before a later
    /// move-to sets its target.
    pub fn move_to_interpreted_state(
        &mut self,
        state: &dereth_animation::motion::InterpretedMotionState,
    ) {
        self.driver
            .borrow_mut()
            .move_to_interpreted_state(state, true);
        self.apply_effects();
    }

    /// One frame. `now` is the current timer value, sampled once per frame by [`dereth_client_runtime::platform::clock::Timer`].
    ///
    /// Returns true when the 30 Hz gate opened, which is what returns.
    pub fn update(&mut self, now: LocalTime) -> bool {
        self.refresh_env();
        self.apply_input();

        // The 30 Hz gate, sub-step ladder and remainder rule are all in
        // here and none of them is a frame-rate cap.
        let ticked = self.world.use_time(now, false);
        if ticked {
            self.stats.physics_ticks += 1;
        }

        // The animation layer asked physics for things while it was being stepped; the physics
        // seam hands them back as data rather than calling up.
        self.apply_effects();
        // The notices must not just be counted and thrown away: one of them is the only event
        // that runs the camera smoother. It is latched here and read by
        // [`Self::update_camera`].
        //
        // **The collision notices have a receiver too**: counted and dropped, a bolt that
        // reached its target would keep flying. The client's three collision callbacks are
        // deliberately asymmetric:
        // ordinary object collision returns success without further work,
        // while environment and missile collisions share the scripted collision-effect behavior.
        // Consequently an ordinary object collision does *nothing*, and either of the other two
        // reaches the following collision-effect path:
        //
        // A missing physics body does nothing. With a body, play its default script only
        // when the object's state includes `0x8000`.
        //
        // The `SCRIPTED_COLLISION_PS` test is made here, against the **body's** state word, so
        // that the queue the scene drains carries only objects that really do have a script.
        // *The missile bits are not cleared here*: those stores belong to collision reporting
        // in the physics object, so `dereth-physics` owns them
        // and has already run them by the time this loop sees
        // the notice.
        let mut player_updated = false;
        self.collision_scripts.clear();
        let mut scripted: Vec<dereth_primitives::ObjectId> = Vec::new();
        for n in self.world.drain_notices() {
            self.stats.notices += 1;
            match n {
                dereth_physics::PhysicsNotice::PlayerPhysicsUpdated => player_updated = true,
                // Ordinary object collision returns success. It is transcribed as a no-op rather than left to a
                // catch-all arm, so that a later reader can see it was read.
                dereth_physics::PhysicsNotice::ObjectCollision { .. } => {}
                // Missile and environment collisions both run the same collision-effect routine.
                dereth_physics::PhysicsNotice::MissileCollision { object, .. }
                | dereth_physics::PhysicsNotice::EnvironmentCollision { object, .. } => {
                    scripted.push(object);
                }
                // `report_object_collision_end`. Never raised by this crate yet; that half is
                // still open.
                dereth_physics::PhysicsNotice::CollisionEnd { .. } => {}
                // The cell entry-restriction check refused entry
                // and the refusal ends by playing the restriction object's script on the mover.
                // Queued rather than played, exactly as the collision scripts above.
                dereth_physics::PhysicsNotice::MoveRestricted {
                    object,
                    restriction_obj,
                    intensity,
                } => {
                    self.stats.move_restrictions += 1;
                    self.restriction_effects
                        .push((object, restriction_obj, intensity));
                }
            }
        }
        for object in scripted {
            let plays = self
                .world
                .by_object_id(object)
                .and_then(|h| self.world.get(h))
                .is_some_and(|o| o.state.has_scripted_collision());
            if plays && !self.collision_scripts.contains(&object) {
                self.collision_scripts.push(object);
                self.stats.collision_scripts += 1;
            }
        }
        self.player_physics_updated = player_updated;

        // **The part placement is not here.** `WorldScene::update` runs
        // `place_local_body()` immediately after `recenter()`, with the same driver state and
        // (off a crossing) the same `render_frame()`, and it is
        // a straight assignment of `parts[i].pos` — nothing accumulates, nothing reads the old
        // value, and nothing between the two calls looks at a part. A placement here would be
        // dead on every frame and *wrong* on a crossing frame, where it would write in the space
        // the re-centre was about to leave. It is [`Self::place_parts`], called by the frame loop
        // on the correct side of the re-centre.
        ticked
    }

    /// The objects whose collision this tick asks for an impact script.
    ///
    /// Drained by `crate::world::WorldScene::update` immediately after [`Self::update`]; the
    /// tail plays the object's default script and lives on the scene's
    /// animation driver because that is what holds the `PhysicsScriptTable`.
    #[must_use]
    pub fn take_collision_scripts(&mut self) -> Vec<dereth_primitives::ObjectId> {
        std::mem::take(&mut self.collision_scripts)
    }

    /// The house barriers this tick's transitions ran into. See
    /// [`Self::restriction_effects`].
    #[must_use]
    pub fn take_restriction_effects(
        &mut self,
    ) -> Vec<(
        dereth_primitives::ObjectId,
        dereth_primitives::ObjectId,
        f32,
    )> {
        std::mem::take(&mut self.restriction_effects)
    }

    /// One frame of the real camera.
    ///
    /// The swept-sphere viewer update and camera smoother run in that order,
    /// which is what gives the camera its documented one-frame lag. It runs after
    /// [`Self::update`], because the sweep starts from the pivot the body has just moved to.
    ///
    /// **The last of those three is gated on [`Self::player_physics_updated`].** The sweep
    /// and the held-key repeat are per display frame in the client too. The smoother is not, and
    /// running it every frame
    /// makes the drawn body slide against the drawn camera between physics ticks.
    pub fn update_camera(&mut self, input: crate::camera::CameraInput, now: LocalTime, dt: f64) {
        self.camera.update(
            &mut self.world,
            self.handle,
            self.object_id,
            input,
            now.0,
            dt,
            self.player_physics_updated,
        );
    }

    /// The camera's eye frame in the renderer's **viewer-block-relative** space, or `None` when
    /// the sweep found no valid camera position at all — the viewer cell is null, which is
    /// what makes the world render pass skip the 3D world for that frame.
    #[must_use]
    pub fn camera_render_frame(&self) -> Option<Frame> {
        self.camera.viewer_cell?;
        Some(self.render_frame_of(self.camera.viewer))
    }

    /// The placed viewer position in the renderer's **viewer-block-relative** space — the value
    /// normal-mode rendering hands to the landscape update.
    ///
    /// **This is not [`Self::camera_render_frame`], and the difference is `viewer_cell`.**
    /// Normal-mode rendering reads the viewer's `objcell_id` for the viewpoint and consults
    /// `viewer_cell` only for `seen_outside`; the viewer update's two failure arms both reset the
    /// viewer to the player's position and *then* clear
    /// `viewer_cell`, so on the frame the sweep finds nowhere to stand the viewpoint is the
    /// **body's** position and the block decision is still taken. Gating this on `viewer_cell`
    /// would drop that arm on the floor. `camera_render_frame` is gated because it answers a
    /// different question — where to put the eye — and a NULL `viewer_cell` is what makes the
    /// client stop drawing the 3D world.
    ///
    /// `None` means the camera has never been placed at all: `CameraControl::new` leaves `viewer`
    /// at `CellId(0)`, which is not a position, and resetting the viewer is what first
    /// makes one. [`Self::render_frame_of`] would read `CellId(0)`'s landblock as `(0, 0)` and
    /// answer with a point a hundred and sixty landblocks away, so the guard is load-bearing
    /// rather than defensive.
    #[must_use]
    pub fn viewer_render_frame(&self) -> Option<Frame> {
        self.camera
            .attached()
            .then(|| self.render_frame_of(self.camera.viewer))
    }

    /// Install the object's current frame on the part array for drawing, expressed in this build's
    /// viewer-block-relative render space.
    ///
    /// **It is deliberately not part of [`Self::update`].** In the client the part placement is a
    /// draw-time act and the block origin is applied later still by block drawing; this build
    /// folds that origin into the part frames, so the placement has to
    /// run **after** the viewer block is chosen. `crate::world::WorldScene::place_local_body` is
    /// the one caller in a running client, and it is on the correct side of `recenter`. Anything
    /// driving a `Character` on its own — a test, a tool — has to call this where the frame loop
    /// would have.
    ///
    /// **"After the viewer block is chosen" is a thing the compiler checks**: the
    /// [`RenderSpace`] argument can only have come from [`Self::set_viewer_block`], so a caller
    /// that has not chosen a block has nothing to pass and does not compile. The token is
    /// otherwise unused — it is a proof, not a parameter.
    pub fn place_parts(&self, _space: RenderSpace) {
        let world_frame = self.render_frame();
        self.driver.borrow_mut().update_parts(&world_frame);
    }

    /// A `Position` expressed in the renderer's viewer-block-relative space. [`Self::render_frame`]
    /// is this applied to the body's own position.
    #[must_use]
    pub fn render_frame_of(&self, pos: Position) -> Frame {
        let block = pos.cell.landblock();
        #[allow(clippy::cast_precision_loss)] // a block index difference, at most 255
        let (dx, dy) = (
            (i32::from(block.x()) - self.viewer_block.0) as f32
                * dereth_physics::globals::BLOCK_LENGTH,
            (i32::from(block.y()) - self.viewer_block.1) as f32
                * dereth_physics::globals::BLOCK_LENGTH,
        );
        Frame::new(
            Vec3::new(
                pos.frame.origin.x + dx,
                pos.frame.origin.y + dy,
                pos.frame.origin.z,
            ),
            pos.frame.rotation,
        )
    }

    /// Refresh the physics facts `MotionEnv` carries. Motion interpolation and `MoveToManager` read
    /// them off the object directly in the client; here they are copied across once per frame,
    /// before the step, which is when the client's call sites would have observed them.
    fn refresh_env(&mut self) {
        let Some(o) = self.world.get(self.handle) else {
            return;
        };
        let mut d = self.driver.borrow_mut();
        d.env.on_ground = o.transient_state.in_contact() && o.transient_state.on_walkable();
        d.env.contact = o.transient_state.in_contact();
        d.env.in_cell = o.cell.is_some();
        d.env.is_creature = true;
        d.env.has_weenie = true;
        d.env.gravity_affected = o.state.has_gravity();
        d.env.position = o.position;
        d.env.velocity = o.velocity_vector;
        // See `SharedMotion::sync_physics_state`.
        d.env.cached_velocity = o.cached_velocity;
        d.env.radius = o.radius();
        d.env.height = o.height();
        // The move-to compares its top-level id
        // against the mover's own, and takes the clean-up arm when they match.
        d.env.object_id = self.object_id;
    }

    /// Issue the key changes as motions, the way the client's input layer does: a do-motion on
    /// the press, a stop-motion on the release, one per slot.
    fn apply_input(&mut self) {
        // **The hold-run gate is on the interpreter's state, and it is above the early return
        // for the same reason.**
        //
        // Gating on `self.input.run != self.applied.run` would ask *did the player's run word
        // change since the last frame this body applied one*. Retail asks a different
        // question, and asks it of a different object:
        // it compares `on` against **`raw_state.current_holdkey`**. So the client repairs a
        // disagreement between the two the next time anything asks; with the other gate and
        // `input.run == applied.run`, the interpreter's own bit would never be consulted or
        // written, however wrong it was.
        //
        // Anything that resets `RawMotionState` puts the two out of step, and there is a
        // production producer: rebuilding the motion table recreates the movement manager and
        // calls `enter_default_state`, whose fresh raw motion state leaves
        // `current_holdkey = HoldKey::None` — **walk** — while `CharacterInput::run` still says
        // run. Without this gate the body would walk with the run word set until a physical
        // toggle of the run key (or a jump, whose pending retake drains into
        // `take_control_from_server` and manufactures the missing edge) put it right.
        //
        // The client re-asserts the same statement from taking control from the server,
        // enabling the interpreter, and updating the run toggle; all three pass hold-run state
        // to movement interpolation.
        // With the gate on the right operand, all three land here on the next input
        // the body applies, and the *tail* is what they are for.
        //
        // It is above `input == applied` because retail's is: nothing in `set_hold_run`'s callers
        // is conditioned on the command lists having changed. The compare is free on a still
        // frame — it writes nothing and reapplies nothing when the two already agree.
        let hold_run_changed = self
            .driver
            .borrow_mut()
            .movement
            .interp
            .set_hold_run(self.input.run);
        if hold_run_changed {
            self.reapply_raw_movement();
        }
        if self.input == self.applied {
            return;
        }
        let params = MovementParameters::default();

        // The hold-run key is a state, not a motion: `set_hold_run` only changes what
        // `adjust_motion` substitutes for a subsequent command.
        //
        // **And the tail is not optional.** Setting held-run state has two statements:
        //
        // when the requested run state differs from whether `current_holdkey` is `HoldKey::Run`,
        // set `current_holdkey` to match, then re-apply the current movement.
        //
        // The command interpreter asks for a linked re-apply, so the re-apply is a **link**ed one.
        // Because [`CharacterInput::forward_command`] answers `WalkForward` whether or not the run
        // key is held, the re-apply is the **only** thing that makes Shift pressed mid-walk change
        // anything at all. Without it the body would never accelerate.
        //
        // `apply_current_movement` chooses `apply_raw_movement` when the object has no weenie or
        // is the player, and its movement is autonomous; this body is the
        // autonomous player, so the raw arm is the one taken — `dereth_animation`'s own note says the
        // caller must pick, because neither fact lives in that crate.
        for slot in [
            (self.applied.forward_command(), self.input.forward_command()),
            (
                self.applied.sidestep_command(),
                self.input.sidestep_command(),
            ),
            (self.applied.turn_command(), self.input.turn_command()),
        ] {
            let (was, now) = slot;
            if was == now {
                continue;
            }
            if was != MotionCommand::NONE {
                self.motion(false, was, &params);
            }
            if now != MotionCommand::NONE {
                self.motion(true, now, &params);
            }
        }

        if self.input.jump && !self.applied.jump {
            self.jump();
        }
        self.applied = self.input;
    }

    /// Complete commands preceding a state-observing input callback without advancing physics
    /// or animation. The normal update reuses `applied`, so these edges are not issued twice.
    pub fn flush_command_input(&mut self, input: CharacterInput) {
        self.input = input;
        self.refresh_env();
        self.apply_input();
    }

    /// `apply_current_movement`'s body tail after teleport disabled autorun. Unlike ordinary input
    /// edges, the current command-list projection must be reissued even when a held key is
    /// unchanged: the teleport hook may have stopped its old motion. No simulation or jump edge
    /// runs.
    pub fn reapply_teleport_input(&mut self, input: CharacterInput) {
        self.input = input;
        let params = MovementParameters::default();
        // The return is the "did the bit move" gate `apply_input` uses; this path
        // re-issues the whole projection below either way, so it owes no separate
        // `apply_current_movement` tail and the answer is discarded rather than acted on.
        let _ = self
            .driver
            .borrow_mut()
            .movement
            .interp
            .set_hold_run(input.run);
        let forward = input.forward_command();
        self.motion(
            true,
            if forward == MotionCommand::NONE {
                MotionCommand::READY
            } else {
                forward
            },
            &params,
        );
        // Empty lists stop the corresponding raw modifier; nonempty lists reissue their head.
        let raw = self.driver.borrow().movement.interp.raw_state.clone();
        for (was, now) in [
            (raw.turn_command, input.turn_command()),
            (raw.sidestep_command, input.sidestep_command()),
        ] {
            if now != MotionCommand::NONE {
                self.motion(true, now, &params);
            } else if was != MotionCommand::NONE {
                self.motion(false, was, &params);
            }
        }
        self.applied = input;
    }

    /// Movement interpolation's jump at full extent.
    ///
    /// **This is a debug key, not the client's jump.** The real one is charge-and-release: holding
    /// the key runs against the power bar, linear in time and floored at
    /// `MIN_JUMP_EXTENT = 0.001`) and releasing passes the level as the extent. Production App
    /// owns that chain in crate::jump; this CharacterInput adapter remains for isolated
    /// body/debug callers. `jump` is the documented entry point, and everything after it —
    /// `set_on_walkable(false)`, the leave-ground
    /// callback, `get_leave_ground_velocity` and the resulting local-velocity set — belongs to the
    /// animation and physics crates, unchanged.
    ///
    /// A bare do-motion of `MotionCommand::JUMP` is **not** the way in: motion table
    /// `0x09000001` has no sequence for it in the `NonCombat`/`Ready` state and refuses with
    /// `BAD_MOVEMENT_COMMAND (0x43)`, because the vertical motion is a physics impulse rather than
    /// an animation. Measured, not assumed.
    fn jump(&mut self) {
        self.jump_with_extent(FULL_JUMP_EXTENT);
    }

    /// The body entry for commencing a jump.
    /// In particular, charging in the air is not an invented refusal: release checks it.
    pub fn charge_jump(&mut self) -> u32 {
        self.refresh_env();
        let mut d = self.driver.borrow_mut();
        let MotionDriver { movement, env, .. } = &mut *d;
        movement.interp.charge_jump(0.0, env)
    }

    /// The run-rate inquiry answer, kept as a fact in the shared driver the way
    /// the jump inquiries are.
    ///
    /// **`None` is not "rate 1.0".** It is the inquiry *failing*, which is a different arm:
    /// applying run to the command, obtaining maximum speed, `get_adjusted_max_speed`, and
    /// `get_state_velocity` each use the same fallback: with no game object the rate is 1.0, and
    /// when the game object's run-rate inquiry fails the rate is `my_run_rate` --
    /// so a failed inquiry falls back to `my_run_rate` — the last rate the *server* sent in a
    /// `MoveTo` blob (`unpack_movement`, cases 6 and 7) — and not to the constant.
    pub fn set_run_rate(&self, rate: Option<f32>) {
        self.driver.borrow_mut().env.run_rate = rate;
    }

    /// Current weenie inquiry results at the jump callback, kept as facts in the shared driver.
    /// An absent velocity inquiry is distinct from a valid zero Jump skill.
    pub fn set_jump_qualities(&self, load: f32, can_jump: bool, skill: Option<i32>) {
        let mut d = self.driver.borrow_mut();
        d.env.load = load;
        d.env.jump_permission = Some(can_jump);
        d.env.jump_velocity_available = skill.is_some();
        d.env.jump_skill = skill.unwrap_or(0);
    }

    /// Finishing the jump clears the player interpreter's standing-long-jump state.
    pub fn finish_jump(&self) {
        self.driver.borrow_mut().movement.interp.standing_longjump = false;
    }

    /// Jump with the release extent and return the real error code; the impulse completes
    /// synchronously before the caller reads local velocity and builds `0xF61B`. No ground-edge
    /// inference is needed.
    pub fn jump_with_extent(&mut self, extent: f32) -> u32 {
        self.refresh_env();
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        let err = movement.jump(extent, &mut ctx);
        drop(d);
        if err == 0 {
            self.stats.motions_issued += 1;
        } else {
            self.stats.motions_refused += 1;
            self.stats.last_refusal = err;
        }
        self.apply_effect_list(effects);
        // Clearing the walkable bit fired the leave-ground callback synchronously, which queued
        // the impulse into the driver's own effect list; drain it now rather than a frame late.
        self.apply_effects();
        err
    }

    /// Apply the current raw movement through motion interpolation's do-motion and stop-motion paths.
    ///
    /// The shared MovementManager resolves raw-command cancellation synchronously. Character
    /// still builds a MotionCtx from the driver's fields for the local physics/reporting tail,
    /// on the same locally assembled `MotionCtx` [`Self::motion`] builds and for the same
    /// reason; it must not defer cancellation until after the replacement raw state has been
    /// installed.
    ///
    /// This is `set_hold_run`'s tail and nothing else calls it. **Not** the per-frame retake's
    /// re-assert: that one reapplies the *command lists* through `move_player`, rather than
    /// pushing the *raw state* into the interpreted one, and is
    /// [`MovementCommands::use_time`] plus [`Self::take_control_from_server`]; the two are one
    /// word apart and do different things, which is why they are named apart here.
    fn reapply_raw_movement(&mut self) {
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        // `apply_current_movement(linked = 1, 0)` — linked, and no longjump hint.
        movement.apply_raw_movement(true, false, &mut ctx);
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.drain_completed_motions(&mut ctx);
        drop(d);
        self.apply_effect_list(effects);
    }

    /// The body half of taking control back from the server.
    ///
    /// The interpreter half is [`MovementCommands::use_time`]; these are the four statements it
    /// runs on the physics body between clearing `controlled_by_server` and re-issuing the
    /// command-list heads, and they live here because this is the only side that holds a body:
    ///
    /// The client marks the last move autonomous, stops the player completely with movement-manager
    /// notification, and stops position interpolation before it reapplies the command-list heads.
    ///
    /// **The `applied` reset is what makes the resume happen, and it is not cosmetic.**
    /// [`Self::apply_input`] is edge-triggered (`if self.input == self.applied { return; }`),
    /// which is right while nothing else moves the body — but the server's own
    /// `move_to_interpreted_state` has just stopped it *without* touching `input`, so with the key
    /// still held `input == applied` and the next frame would issue nothing at all. Retail has no
    /// such edge: the interpreter re-issues every list head
    /// through `move_player` unconditionally. Clearing `applied` reproduces that — every currently
    /// held slot is re-issued as a fresh do-motion, with no stop-motion before it, because
    /// `was == NONE`. `jump` is preserved so a held jump key cannot fire a second time.
    ///
    /// `last_move_was_autonomous` is `dereth_physics::PhysicsObj`'s field and has no reader in this
    /// workspace (the motion interpolator's raw/interpreted choice is made by the
    /// caller here), so it is
    /// **not** written: writing a field nothing reads would look like a wire and be none. Named
    /// here so the gap is falsifiable rather than silent.
    /// The interpolation-stop operation is **not** called: it forwards to
    /// the physics interpolation manager, and this body has no
    /// `PositionManager` at all — the interpolation arm is the same one
    /// [`Self::stick_to_object`] already records as absent for the player. There is nothing to
    /// stop, and inventing a handle here would be a wire to nowhere.
    pub fn take_control_from_server(&mut self) {
        self.stop_completely();
        self.applied = CharacterInput {
            jump: self.applied.jump,
            ..CharacterInput::default()
        };
    }

    /// The body half of the stop-completely action.
    ///
    /// With both a player and physics body, the client cancels the command lists, clears its
    /// auxiliary command state, stops the physics object completely with notification enabled,
    /// and sends a movement event.
    ///
    /// Only the physics-body stop is here; `App` owns the two presence guards and composes the
    /// remaining command-interpreter effects across `dereth_animation`'s `MovementCommands` and
    /// `dereth_client_net::client_session::PositionReporter`: cancel command state, stop with notification enabled,
    /// then report movement, in that order. This method remains reachable outside the module so
    /// the Escape-key action can request the body stop; that public reachability is
    /// [`Self::stop_completely`]'s only difference from this.
    pub fn stop_completely_from_action(&mut self) {
        self.stop_completely();
    }

    /// Stopping completely asks the movement manager to perform movement with a
    /// `MovementStruct` whose type is `StopCompletely`, which is
    /// complete stop in the movement interpolator.
    fn stop_completely(&mut self) {
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        // The leading move-to cancel must finish before a fresh command is reapplied. Calling
        // just the interpreter would leave its cancel-move-to effect in this host's unhandled arm.
        let _ = movement.stop_completely(&mut ctx);
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.drain_completed_motions(&mut ctx);
        drop(d);
        self.apply_effect_list(effects);
    }

    /// The move-player tail for a command that is on
    /// no command list, i.e. an emote or a stance.
    ///
    /// It default-constructs movement parameters, copies the action stamp and computed hold key,
    /// sets `MOVEMENT_AUTONOMOUS`, runs the action-stamp hook when requested, and calls do-motion
    /// or stop-motion with notification enabled. A successful stamped action increments the
    /// interpreter's action stamp.
    ///
    /// **Two declared gaps, both named rather than silently approximated.** The `action_stamp`
    /// this build hands over is the default movement parameters' action stamp, not the interpreter's
    /// running counter — that counter has no other reader here and the stamp only
    /// travels on the wire inside a queued `ActionNode`; and the hold key is the default rather
    /// than the one `move_player` computes from the turn/sidestep heads, which for a command on no
    /// list is `HoldKey::Invalid` either way because raw-motion application does not store a hold
    /// key on the action arm.
    pub fn command_motion(&mut self, start: bool, cmd: MotionCommand) {
        self.refresh_env();
        let mut params = MovementParameters::default();
        // `MOVEMENT_AUTONOMOUS` (`0x1000`) is a flag that default movement parameters
        // leave **clear** and `move_player`
        // sets on every command it issues. It is what puts `autonomous` on the `ActionNode`
        // raw-motion application queues, and therefore on the `0xF61C` that reports it.
        params.flags |= dereth_animation::motion::flags::AUTONOMOUS;
        self.motion(start, cmd, &params);
    }

    /// Issue movement arguments `(cmd, 1, extent, 1, 1)` as
    /// camera rotation does with mouse turning on.
    ///
    /// The difference from [`Self::command_motion`] is the third argument: camera rotation passes
    /// an *extent* (`step * 2`, pinned at 1.5) where the keyboard path passes nothing, and
    /// `move_player` puts it on. Everything else is the keyboard
    /// path's — `MOVEMENT_AUTONOMOUS`, and the same two declared gaps.
    pub fn camera_turn_motion(&mut self, cmd: MotionCommand, extent: f32) {
        self.refresh_env();
        let mut params = MovementParameters::default();
        params.flags |= dereth_animation::motion::flags::AUTONOMOUS;
        params.speed = extent;
        self.motion(true, cmd, &params);
        self.stats.camera_turns_issued += 1;
    }

    /// The command interpreter's turn-to-heading path, which camera rotation calls
    /// in first person.
    ///
    /// It default-constructs movement parameters, stores the desired heading, clears
    /// `MOVEMENT_STOP_COMPLETELY`, sets speed to 1, optionally sets context id 2, and asks the
    /// physics object to turn toward the heading.
    ///
    /// Rotation passes `arg2 = 0`, so the `context_id` arm is not this caller's. The destination is
    /// [`dereth_animation::motion::MoveToRequest::TurnToHeading`] — so this is
    /// [`Self::perform_move_to`] with the parameters retail builds, not a second path.
    pub fn turn_to_heading(&mut self, heading: f32) {
        let params = MovementParameters {
            flags: dereth_animation::motion::DEFAULT_FLAGS
                & !dereth_animation::motion::flags::STOP_COMPLETELY,
            desired_heading: heading,
            speed: 1.0,
            ..MovementParameters::default()
        };
        self.perform_move_to(
            &dereth_animation::motion::MoveToRequest::TurnToHeading,
            &params,
            None,
        );
        self.stats.camera_turns_issued += 1;
    }

    /// Stop drift in the mouse-turning dead zone.
    ///
    /// Start with default movement parameters, clear `MOVEMENT_SET_HOLD_KEY`, and set
    /// `hold_key_to_apply` to `HoldKey::None` (1). Stop both turn-right (`0x6500000D`)
    /// and turn-left (`0x6500000E`) motions with those parameters and notification enabled.
    ///
    /// Both, unconditionally and in that order: it is *stop turning*, not *stop this turn*.
    pub fn stop_drift(&mut self) {
        self.refresh_env();
        let params = MovementParameters {
            flags: dereth_animation::motion::DEFAULT_FLAGS
                & !dereth_animation::motion::flags::SET_HOLD_KEY,
            hold_key_to_apply: dereth_animation::motion::HoldKey::None,
            ..MovementParameters::default()
        };
        self.motion(false, MotionCommand::TURN_RIGHT, &params);
        self.motion(false, MotionCommand::TURN_LEFT, &params);
        // A turning key still held keeps its turn, as the interpreter's re-apply of the current
        // movement gives it back.
        let held = self.applied.turn_command();
        if held != MotionCommand::NONE {
            self.motion(true, held, &MovementParameters::default());
        }
        self.stats.camera_turn_stops += 1;
    }

    fn motion(&mut self, start: bool, cmd: MotionCommand, params: &MovementParameters) {
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        let err = if start {
            movement.do_motion(cmd, params, &mut ctx)
        } else {
            movement.stop_motion(cmd, params, &mut ctx)
        };
        // `PerformMovement` always ends with `CheckForCompletedMotions`.
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.drain_completed_motions(&mut ctx);
        drop(d);
        if err == 0 {
            self.stats.motions_issued += 1;
        } else {
            self.stats.motions_refused += 1;
            self.stats.last_refusal = err;
        }
        self.apply_effect_list(effects);
    }

    // -----------------------------------------------------------------------------------------
    // The approach walk.
    //
    // Setting object movement unpacks it into the physics object, then performs the movement.
    // The four move-to and turn-to arms of a `0xF74C` end there; the application decodes the buffer and
    // resolves the target's id, radius and height out of its object table, because
    // the client does exactly that before it builds the struct.
    // -----------------------------------------------------------------------------------------

    /// The movement manager performs the movement.
    ///
    /// `run_rate` is the motion interpreter's cached run rate, which `unpack_movement` writes from the
    /// buffer's trailing float before it dispatches -- and only for the two `MoveTo` arms.
    pub fn perform_move_to(
        &mut self,
        req: &MoveToRequest,
        params: &MovementParameters,
        run_rate: Option<f32>,
    ) {
        self.refresh_env();
        let mut d = self.driver.borrow_mut();
        if let Some(r) = run_rate {
            d.movement.interp.my_run_rate = r;
        }
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.perform_movement(req, params, &mut ctx);
        // `PerformMovement` always ends with `CheckForCompletedMotions`.
        //
        // **The drain routes the completed motions, not just keeps them.** Keeping the events
        // (in `self.events`, for their consumers elsewhere in the client) is not routing
        // them to the half that pops `pending_motions`; one stuck node makes
        // `motions_pending()` permanently true, which jams the per-frame retake and
        // `PlayerInReadyPosition`'s `!motions_pending` shut for the rest of the session.
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.drain_completed_motions(&mut ctx);
        drop(d);
        self.stats.move_tos_performed += 1;
        self.events.borrow_mut().extend(events);
        self.apply_effect_list(effects);
    }

    /// Cancel move-to and unstick before applying any received
    /// style or movement arm. Settle the old host target/reporting tail now; doing it after the
    /// new arm's SetTarget would silently erase that new subscription.
    pub fn prepare_received_movement(&mut self) {
        self.refresh_env();
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.prepare_received_movement(&mut ctx);
        drop(d);
        self.events.borrow_mut().extend(events);
        self.apply_effect_list(effects);
    }

    /// The tail of sticking the physics object to another object.
    ///
    /// The object-table half (object lookup, the one-hop parent walk, radius and
    /// height) is the application's, exactly as it is for [`Self::perform_move_to`]; what
    /// arrives here is the resolved id, radius and height.
    ///
    /// `SharedMotion::adjust_position_offset` applies the correction inside the collision-tested
    /// `UpdatePositionInternal`, after animation-origin scaling and before composition.
    pub fn stick_to_object(&mut self, target: ObjectId, radius: f32, height: f32) {
        // `StickyManager` keeps the radius and the timeout; its offset adjustment works in
        // the plane with z zeroed and never reads a height, which is why the runtime struct does
        // not carry one.
        let _ = height;
        self.refresh_env();
        let now = self.world.last_physics_time();
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.stick_to_object(target, radius, now, &mut ctx);
        drop(d);
        self.stats.sticks_applied += 1;
        self.events.borrow_mut().extend(events);
        self.apply_effect_list(effects);
    }

    /// Unsticking the physics object invokes the position manager's exit-world handling,
    /// then the sticky manager detaches it.
    pub fn unstick_from_object(&mut self) {
        let mut d = self.driver.borrow_mut();
        let MotionDriver {
            assets,
            sequence,
            motion_table,
            movement,
            env,
            target: subscription,
            ..
        } = &mut *d;
        let mut effects: Vec<MotionEffect> = Vec::new();
        let mut published_effects = dereth_animation::motion::TargetedEffects {
            target: subscription,
            pending: &mut effects,
        };
        let mut events: Vec<dereth_animation::AnimEvent> = Vec::new();
        let mut ctx = MotionCtx {
            mgr: motion_table,
            seq: sequence,
            assets: &**assets,
            env,
            effects: &mut published_effects,
            events: &mut events,
        };
        movement.unstick_from_object(&mut ctx);
        drop(d);
        self.events.borrow_mut().extend(events);
        self.apply_effect_list(effects);
    }

    /// The object this body is stuck to, if any.
    #[must_use]
    pub fn sticky_target(&self) -> Option<ObjectId> {
        let d = self.driver.borrow();
        d.movement
            .sticky
            .is_sticky()
            .then(|| d.movement.sticky.target_id)
    }

    /// The object the move-to (or the stick) is watching, if any.
    #[must_use]
    pub fn wanted_target(&self) -> Option<ApproachTarget> {
        // No gate here. The 0.5 s tick is `MotionDriver::handle_targetting`'s
        // and runs per physics sub-step; the host resolves the target's facts every frame into
        // `MotionDriver::target_snapshot` through [`Self::set_target_snapshot`].
        self.driver.borrow().target
    }

    /// The host's half of `TargetManager`'s voyeur model: the watched object's
    /// position, `cached_velocity` and status, resolved off the application's object table once
    /// per frame and consumed by `MotionDriver::handle_targetting` on every sub-step.
    pub fn set_target_snapshot(
        &mut self,
        snapshot: Option<dereth_animation::motion::moveto::TargetSnapshot>,
    ) {
        self.driver.borrow_mut().set_target_snapshot(snapshot);
    }

    /// `AddVoyeur`'s unconditional first update for a fresh subscription, delivered
    /// synchronously as retail does inside `SetTarget`.
    pub fn deliver_first_target_update(&mut self, now: LocalTime) {
        self.refresh_env();
        let fx = self.driver.borrow_mut().deliver_first_target_update(now);
        if let Some(fx) = fx {
            self.stats.target_updates += 1;
            self.apply_effect_list(fx);
        }
    }

    /// Feed one `TargetInfo` to the move-to target updater.
    ///
    /// `position`/`velocity` are the target's, out of the application's object table.
    /// Target interpolation extrapolates by the **cached**
    /// velocity, so a blocked target stops being extrapolated; pass what the object reports.
    ///
    /// `ok = false` is `TargetStatus != Ok` -- the object left the world, was picked up, or timed
    /// out -- and cancels the approach with `NoObject (0x38)` before the first update or
    /// `ObjectGone (0x37)` after it.
    pub fn update_target(&mut self, position: Position, velocity: Vec3, ok: bool) {
        let Some(t) = self.driver.borrow().target else {
            return;
        };
        let now = self.world.last_physics_time();
        let interpolated = dereth_physics::detect::TargetManager::interpolated_position(
            &position,
            velocity,
            f64::from(t.quantum),
        );
        self.refresh_env();
        // The delivery itself lives on the driver
        // (`MotionDriver::receive_target_update`), because `MotionDriver::handle_targetting` --
        // run per physics sub-step from object simulation -- delivers through the same door. This
        // synchronous entry is kept for a host (or a test) that
        // has a `TargetInfo` in hand right now.
        let effects =
            self.driver
                .borrow_mut()
                .receive_target_update(position, interpolated, ok, now);
        self.stats.target_updates += 1;
        self.apply_effect_list(effects);
    }

    /// Whether a move-to is in progress.
    #[must_use]
    pub fn is_moving_to(&self) -> bool {
        self.driver.borrow().movement.is_moving_to()
    }

    /// Drain the physics requests the movement layer made during the step and apply them.
    fn apply_effects(&mut self) {
        let effects = self.driver.borrow_mut().take_effects();
        self.apply_effect_list(effects);
        // The animation events belong to their consumers elsewhere in the client. The audio
        // system consumes the three sound ones, so they are buffered here for one frame instead
        // of dropped; the drain is unconditional
        // (`take_anim_events`), so the queue cannot grow.
        let e = self.driver.borrow_mut().take_events();
        self.events.borrow_mut().extend(e);
    }

    fn apply_effect_list(&mut self, effects: Vec<MotionEffect>) {
        let now = self.world.last_physics_time();
        for e in effects {
            let Some(o) = self.world.get_mut(self.handle) else {
                continue;
            };
            match e {
                // Set local velocity to `v` — the jump impulse.
                MotionEffect::SetLocalVelocity(v) => o.set_local_velocity(v, now),
                // Clear the walkable-surface flag — how a jump leaves the ground.
                MotionEffect::SetOnWalkable(on) => o.set_on_walkable(on),
                // Set heading to `h` — the exact snap at the end of a turn.
                MotionEffect::SetHeading(h) => o.set_heading(h),
                MotionEffect::MoveToFailed(err) => {
                    self.stats.move_tos_failed += 1;
                    self.stats.last_move_to_error = err;
                }
                // These isolated-adapter records cannot leave the owned MovementManager /
                // TargetedEffects path. In particular, arrival/action callbacks must finish
                // before the caller resumes; applying them here would cancel fresh work.
                MotionEffect::CancelMoveTo
                | MotionEffect::UnstickFromObject
                | MotionEffect::StickTo { .. }
                | MotionEffect::SetTarget { .. }
                | MotionEffect::ClearTarget
                | MotionEffect::SetTargetQuantum(_) => {
                    unreachable!("movement callback escaped its synchronous owner: {e:?}");
                }
            }
        }
    }
}

/// The player's own run rate, answered from the client's player description and handed to the motion
/// interpreter.
///
/// **This is the client's number, not the server's, and only for the player.** The inquiry first
/// checks that the object is the player. If that call is false, or the player has no
/// qualities, the whole inquiry returns **false** and the interpreter falls back to `my_run_rate`,
/// which the server writes from a `MoveTo` blob. So: our own body computes it here; every other
/// creature takes the server's.
///
/// [`dereth_client_model::skills::inq_run_rate`] performs the load lookup, the full skill lookup
/// stack for skill `0x18`, current-stamina zeroing, and the movement system's run-rate
/// calculation.
pub fn refresh_run_rate(
    character: &Character,
    qualities: Option<&dereth_client_model::qualities::Qualities>,
    skills: Option<&dereth_assets::tables::SkillTable>,
    filter: Option<&dereth_assets::tables::QualityFilter>,
) {
    let rate = match (qualities, skills) {
        (Some(q), Some(t)) => dereth_rules::skills::inq_run_rate(q, t, filter),
        // No player description yet, or the shipped `SkillTable` has not been read: there are no
        // qualities and the run-rate inquiry returns false. Not a rate of 1.0 — a failed inquiry.
        _ => None,
    };
    character.set_run_rate(rate);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key latch maps onto the documented motion commands.
    #[test]
    fn the_key_latch_maps_onto_the_documented_motion_commands() {
        let mut i = CharacterInput {
            forward: true,
            ..CharacterInput::default()
        };
        assert_eq!(i.forward_command(), MotionCommand::WALK_FORWARD);
        i.run = true;
        assert_eq!(
            i.forward_command(),
            MotionCommand::WALK_FORWARD,
            "the run lives in the hold key, not in the command"
        );
        i.forward = false;
        i.back = true;
        assert_eq!(
            i.forward_command(),
            MotionCommand::WALK_BACKWARDS,
            "back never runs"
        );
        // Both halves of an axis held is neither, which is what a keyboard with both arrows down
        // has to mean if the state machine is not to thrash.
        i.forward = true;
        assert_eq!(i.forward_command(), MotionCommand::NONE);
        let i = CharacterInput {
            step_left: true,
            ..CharacterInput::default()
        };
        assert_eq!(i.sidestep_command(), MotionCommand::SIDE_STEP_LEFT);
        let i = CharacterInput {
            turn_right: true,
            ..CharacterInput::default()
        };
        assert_eq!(i.turn_command(), MotionCommand::TURN_RIGHT);
        assert_eq!(
            CharacterInput::default().turn_command(),
            MotionCommand::NONE
        );
    }
}

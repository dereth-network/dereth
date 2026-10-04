//! Fixture motion boundaries with explicit default numbering and callbacks.

use super::motion::apply_player_teleport_at;
use super::*;

/// Legacy isolated/debug ground-edge observer, retained for the jump body tests. Actual App input
/// uses the direct result and immediate physical velocity in crate::jump;
/// it does not infer acceptance from a later physics edge.
#[must_use]
pub fn jump_edge_sample(character: &crate::character::Character) -> u64 {
    character.ground_edges().left
}

#[must_use]
pub fn body_jump(
    character: &crate::character::Character,
    left_before: u64,
) -> (bool, dereth_protocol::types::Vec3) {
    let accepted = character.ground_edges().left > left_before;
    let rotation = character.position().frame.rotation;
    let global = character
        .world
        .get(character.handle)
        .map_or(dereth_primitives::Vec3::ZERO, |o| o.velocity_vector);
    let local = dereth_physics::math::globaltolocalvec(dereth_physics::math::l2g(rotation), global);
    (
        accepted,
        dereth_protocol::types::Vec3 {
            x: local.x,
            y: local.y,
            z: local.z,
        },
    )
}

/// The control transfer between the server and
/// [`crate::character::MovementCommands`], both halves, once per frame.
///
/// Lose control to the server when the position update returns 1, then
/// apply the command interpreter's control-retake decision and, on it,
/// `take_control_from_server`'s body half. Returns `(lost, retook)`.
///
/// **Free and `pub` for the same reason [`apply_player_teleport`] and [`body_motion`] are**: this
/// is the application's own path, so a test that drives a real body over a real capture must be
/// able to call exactly what [`App::command_interpreter_control_transfer`] calls, without a
/// device, a window or a link. Without that the only observable is the call counter, and a
/// mutation that empties the body of the step survives it (see
/// [`App::player_teleport_use_time`]).
///
/// The order inside is retail's and is load-bearing: the loss must precede the retake, because
/// the per-frame step's first term is `controlled_by_server` and `lose_control_to_server` is what
/// sets it. With no body there is no retake at all: the per-frame step returns before sampling
/// any other gate.
pub fn command_interpreter_control_transfer(
    scene: Option<&mut (dyn crate::present::SceneMut + '_)>,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
) -> (bool, bool) {
    command_interpreter_control_transfer_with_finish(scene, movement, input, |character| {
        if let Some(character) = character {
            character.finish_jump();
        }
    })
}

/// Apply a pending server teleport to the player's own body. With no player the
/// operation is a no-op; otherwise it sets the position and runs the player-position-updated tail.
///
/// [`crate::character::Character::teleport`] **is** the client's simple position set with its
/// teleport flag — it normalises an outdoor cell, re-enters the cell, restarts the object's clock
/// and resets the camera smoother, all of which that position set's `set_frame`/`enter_cell` path
/// does. This is the **call**.
///
/// The implemented player-position-updated (teleport) tail now performs teleport_hook's movement,
/// sticky and target cleanup. The command-interpreter tail is composed by
/// [`complete_player_teleport`]. Interpolation, constraints, voyeur notifications and complete
/// collision-end bookkeeping remain explicit Character-side limitations, not audio-only work.
///
/// Free and `pub` for the same reason [`body_motion`] and [`player_timestamps`] are: this is the
/// application's own path from the object stream to the body, so a test that asserts the body
/// moved has to be able to call exactly what [`App::player_teleport_use_time`] calls. Returns the
/// destination when a teleport was pending, `None` otherwise.
pub fn apply_player_teleport(
    objects: &mut crate::objects::ObjectStream,
    character: &mut crate::character::Character,
) -> Option<dereth_primitives::Position> {
    let pos = objects.take_player_teleport()?;
    apply_player_teleport_at(pos, character);
    Some(pos)
}

/// The accepted teleport chain: teleport the player, update its position, report it teleported.
/// The send tail runs after body cleanup and set_auto_run(0, 1)'s actual raw-motion reapplication,
/// even when autorun was already off. The callback lets App supply its existing session sender
/// and lets headless component tests inspect its real outgoing payload without a live link.
pub fn complete_player_teleport(
    objects: &mut crate::objects::ObjectStream,
    character: &mut crate::character::Character,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    send_movement: impl FnOnce(&crate::character::Character),
) -> Option<dereth_primitives::Position> {
    let pos = objects.take_player_teleport()?;
    complete_player_teleport_at(pos, character, movement, input, send_movement);
    Some(pos)
}

/// Everything the two senders read off the physics object, as one value.
///
/// Free rather than a method so a test can build a real [`crate::character::Character`] over the
/// retail landblocks and assert this translation without a device, a link or an `App` — the
/// `App` method is one call to it. The four timestamps are the caller's because they are the
/// *server's* sequence numbers and live on the object table, not on the body.
#[must_use]
pub fn body_motion(
    character: &crate::character::Character,
    timestamps: dereth_protocol::movement::MoveTimestamps,
) -> dereth_client_net::client_session::PlayerMotion {
    body_motion_in(
        character,
        timestamps,
        dereth_world_data::command_numbering::CommandNumbering::Final,
    )
}

/// Pack the player's raw motion state into its wire form.
///
/// Each field's presence bit is decided by comparing it with a default:
///
/// | bit | field | present when |
/// |---:|---|---|
/// | 0 | `current_holdkey` | `!=` the no-hold-key value (1) |
/// | 1 | `current_style` | `!= 0x8000003D` (`NonCombat`) |
/// | 2 | `forward_command` | `!= 0x41000003` (`Ready`) |
/// | 3 | `forward_holdkey` | `!=` the invalid hold key (0) |
/// | 4 | `forward_speed` | `!= 1.0` |
/// | 5 | `sidestep_command` | `!= 0` |
/// | 6 | `sidestep_holdkey` | `!=` the invalid hold key |
/// | 7 | `sidestep_speed` | `!= 1.0` |
/// | 8 | `turn_command` | `!= 0` |
/// | 9 | `turn_holdkey` | `!=` the invalid hold key |
/// | 10 | `turn_speed` | `!= 1.0` |
/// | 11–15 | the action count |
///
/// Those are exactly the animation interpreter's pending movement fields, which transcribe the same
/// constructor defaults, so this is `Some(x) if x != default` field by field and nothing else.
///
///
/// **What the corpus exercises.** Across the 1,187 recorded `0xF61C` bodies the flags word takes
/// nine distinct values, all of them below `0x800`: `current_holdkey` appears in 1,182 (always
/// the run hold key, 2), `current_style` in 175, `forward_command` in 673 and `turn_command` in
/// 524.
/// **No recorded body carries a queued action, 0 of 1,187**, and none carries a per-axis hold key
/// or a speed other than 1.0 — so the action arm and five of the eleven presence bits below are
/// transcribed but unexercised by the corpus, which is stated here rather than implied by a
/// passing test.
pub fn raw_motion_state_to_wire(
    s: &dereth_animation::motion::RawMotionState,
) -> dereth_protocol::movement::RawMotionState {
    raw_motion_state_to_wire_in(
        s,
        dereth_world_data::command_numbering::CommandNumbering::Final,
    )
}

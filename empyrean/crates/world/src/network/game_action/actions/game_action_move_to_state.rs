// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionMoveToState.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionMoveToState.cs`.

use dereth_protocol::MessageError;
use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_entity::enums::{HoldKey, MotionCommand};
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::network::motion::move_to_state::MoveToState;
use crate::world_objects::{player_move, player_move2, player_networking, player_tick};
use crate::World;

// ACE: GameActionMoveToState.Handle
/// Sent by the client when a movement key is pressed / released. ACE checks `Player.PKLogout`
/// before reading the payload. The MoveToState becomes the current and last one, cancels any
/// move-to chain, runs through the player's physics (a full-physics player only), requests the
/// client's position (unless teleporting; without a broadcast, since MoveToState UpdatePositions
/// were capped to 1 per second in retail) and is broadcast as the player's movement. Movement
/// keys held at run speed end AFK mode (once per axis).
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    //Console.WriteLine($"{session.Player.Name}.MoveToState");

    let player = session_player(w, session);

    if crate::world_objects::player::fields(w, player).pk_logout {
        return Ok(());
    }

    // `new MoveToState(session.Player, message.Payload)`: the rest of the payload, read as ACE's
    // BinaryReader reads it; the payload position is 4-aligned here, so the reader's own
    // alignment matches. ACE's own `MoveToState` reader builds the state this handler keeps and
    // broadcasts, so it is not replaced by `dereth-protocol`'s movement record.
    let rest = message.read_bytes(message.remaining());
    let mut reader = BinaryReader::new(&rest);
    let move_to_state = MoveToState::read(player, &mut reader).map_err(read_error)?;
    player_move::fields_mut(w, player).current_move_to_state = move_to_state.clone();

    if player_move::is_player_moving_to(w, player) {
        player_move::stop_existing_move_to_chains(w, player);
    }

    if player_move2::fields(w, player).is_player_moving_to2 {
        player_move2::stop_existing_move_to_chains2(w, player);
    }

    // MoveToState - UpdatePosition broadcasts were capped to 1 per second in retail
    player_tick::on_move_to_state(w, player, &move_to_state);
    player_move::fields_mut(w, player).last_move_to_state = Some(move_to_state.clone());

    let teleporting = w
        .objects
        .get(player)
        .is_some_and(|o| o.wo.world_object.teleporting);
    if !teleporting {
        let position = move_to_state.position.expect("read with the state");
        player_networking::set_requested_location(w, player, position, false);
    }

    //if (!moveToState.StandingLongJump)
    player_networking::broadcast_movement(w, player, &move_to_state);

    let is_afk = w
        .objects
        .get(player)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_afk);
    if is_afk && move_to_state.raw_motion_state.current_hold_key == HoldKey::Run {
        let raw = &move_to_state.raw_motion_state;
        for command in [raw.forward_command, raw.turn_command, raw.sidestep_command] {
            if command != MotionCommand::Invalid && command != MotionCommand::AFKState {
                player_networking::handle_action_set_afk_mode(w, player, false);
            }
        }
    }

    Ok(())
}

/// A `BinaryReader` failure as the handlers' error (ACE's `EndOfStreamException`).
fn read_error(e: ReadError) -> MessageError {
    match e {
        ReadError::EndOfStream {
            at,
            needed,
            available,
        } => MessageError::UnexpectedEof {
            at,
            needed,
            available,
        },
        ReadError::OutputBufferTooSmall { at } | ReadError::NegativeCount { at } => {
            MessageError::InvalidValue {
                field: "MoveToState",
                value: at as u64,
            }
        }
    }
}

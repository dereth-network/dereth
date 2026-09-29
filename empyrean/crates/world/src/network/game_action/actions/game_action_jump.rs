// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionJump.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionJump.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::Vector3;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::network::structure::jump_pack::JumpPack;
use crate::world_objects::{player_move, player_move2};
use crate::World;

// ACE: GameActionJump.Handle
/// The client jumped: `HandleActionJump`, then any move-to chain is stopped.
// Not ACE's (retail, V300): the jump pack is read as the client lays it out,
// 56 bytes: the extent, the velocity, the player's 32-byte position (cell id, origin, rotation
// w-first), then the instance, server-control, teleport and force-position sequences, and nothing
// after it. The position is read and not used; only the extent and the velocity are, so nothing
// observable changes.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    // JumpPack(BinaryReader)
    let pack = message.decode::<proto::movement::MovementJump>()?.0;
    let _position = pack.position;
    let jump_pack = JumpPack {
        extent: pack.extent,
        velocity: Vector3::new(pack.velocity.x, pack.velocity.y, pack.velocity.z),
        instance_sequence: pack.timestamps.instance,
        server_control_sequence: pack.timestamps.server_control,
        teleport_sequence: pack.timestamps.teleport,
        force_position_sequence: pack.timestamps.force_position,
    };

    let player = session_player(w, session);
    player_move::handle_action_jump(w, player, &jump_pack);

    if player_move::is_player_moving_to(w, player) {
        player_move::stop_existing_move_to_chains(w, player);
    }

    if player_move2::fields(w, player).is_player_moving_to2 {
        player_move2::stop_existing_move_to_chains2(w, player);
    }

    Ok(())
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/GameActionPacket.cs
//! Port of `Source/ACE.Server/Network/GameAction/GameActionPacket.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{self, HandlerResult, Payload};
use crate::World;

// ACE: GameActionPacket.HandleGameAction
/// `[GameMessage(GameMessageOpcode.GameAction, SessionState.WorldConnected)]`: the ordered-action
/// header, then the action's own opcode, then its payload.
pub fn handle_game_action(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    // TODO: verify sequence
    let _sequence = message.read_u32()?;
    let opcode = message.read_u32()?;

    inbound_message_manager::handle_game_action(w, opcode, message, session);
    Ok(())
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetAFKMessage.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetAFKMessage.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_networking;
use crate::World;

// ACE: GameActionSetAFKMessage.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let msg = message
        .decode_padded::<proto::comms::CommunicationSetAfkMessage>()?
        .message;

    let player = session_player(w, session);
    player_networking::handle_action_set_afk_message(w, player, &msg);
    Ok(())
}

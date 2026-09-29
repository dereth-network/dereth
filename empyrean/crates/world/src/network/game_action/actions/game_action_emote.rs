// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionEmote.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionEmote.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player;
use crate::World;

// ACE: GameActionEmote.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let emote = message
        .decode_padded::<proto::comms::CommunicationEmote>()?
        .message;

    let player = session_player(w, session);
    player::handle_action_emote(w, player, &emote);
    Ok(())
}

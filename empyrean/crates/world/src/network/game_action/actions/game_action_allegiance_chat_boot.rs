// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceChatBoot.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceChatBoot.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionAllegianceChatBoot.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let proto::social::AllegianceChatBoot {
        name: player_name,
        reason,
    } = message.decode_padded()?;

    let player = session_player(w, session);
    player_allegiance::handle_action_allegiance_chat_boot(w, player, &player_name, &reason);
    Ok(())
}

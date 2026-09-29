// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceChatGag.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceChatGag.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionAllegianceChatGag.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::AllegianceChatGag>()?;
    let (player_name, enabled) = (m.name, to_boolean(m.gagged));

    let player = session_player(w, session);
    player_allegiance::handle_action_allegiance_chat_gag(w, player, &player_name, enabled);
    Ok(())
}

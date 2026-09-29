// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveFromPlayerConsentList.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveFromPlayerConsentList.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_death;
use crate::World;

// ACE: GameActionRemoveFromPlayerConsentList.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    // the granter we are removing from consent list
    let player_name = message
        .decode_padded::<proto::admin::CharacterRemoveFromPlayerConsentList>()?
        .name;

    let player = session_player(w, session);
    player_death::handle_action_remove_from_player_consent_list(w, player, &player_name);
    Ok(())
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionDisplayPlayerConsentList.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionDisplayPlayerConsentList.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_death;
use crate::World;

// ACE: GameActionDisplayPlayerConsentList.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_death::handle_action_display_player_consent_list(w, player);
    Ok(())
}

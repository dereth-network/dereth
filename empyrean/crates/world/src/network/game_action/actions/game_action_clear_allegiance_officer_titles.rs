// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionClearAllegianceOfficerTitles.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionClearAllegianceOfficerTitles.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionClearAllegianceOfficerTitles.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_allegiance::handle_action_clear_allegiance_officer_titles(w, player);
    Ok(())
}

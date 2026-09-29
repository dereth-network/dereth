// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRecallAllegianceHometown.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRecallAllegianceHometown.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_location;
use crate::World;

// ACE: GameActionRecallAllegianceHometown.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_location::handle_action_recall_allegiance_hometown(w, player);
    Ok(())
}

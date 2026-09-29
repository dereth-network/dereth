// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToMansion.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToMansion.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_location;
use crate::World;

// ACE: GameActionTeleToMansion.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_location::handle_action_tele_to_mansion(w, player);
    Ok(())
}

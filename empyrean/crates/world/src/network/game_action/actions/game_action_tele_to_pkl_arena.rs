// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToPklArena.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToPklArena.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_location;
use crate::World;

// ACE: GameActionTeleToPklArena.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_location::handle_action_tele_to_pkl_arena(w, player);
    Ok(())
}

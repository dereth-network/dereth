// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFinishBarber.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFinishBarber.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_character;
use crate::World;

// ACE: GameActionFinishBarber.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_character::handle_action_finish_barber(w, player, message)
}

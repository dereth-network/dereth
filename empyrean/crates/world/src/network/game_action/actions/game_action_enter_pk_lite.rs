// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionEnterPkLite.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionEnterPkLite.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player;
use crate::World;

// ACE: GameActionEnterPkLite.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player::handle_action_enter_pk_lite(w, player);
    Ok(())
}

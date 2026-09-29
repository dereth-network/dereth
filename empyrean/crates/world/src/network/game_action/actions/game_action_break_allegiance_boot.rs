// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionBreakAllegianceBoot.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionBreakAllegianceBoot.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionBreakAllegianceBoot.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::AllegianceBreakAllegianceBoot>()?;
    let (player_name, account_boot) = (m.name, to_boolean(m.account_boot));

    let player = session_player(w, session);
    player_allegiance::handle_action_break_allegiance_boot(w, player, &player_name, account_boot);
    Ok(())
}

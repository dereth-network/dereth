// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipUpdateRequest.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipUpdateRequest.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_fellowship;
use crate::World;

// ACE: GameActionFellowshipUpdateRequest.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::FellowshipUpdateRequest>()?;
    // indicates if fellowship panel on client is visible
    let panel_open = to_boolean(m.on);

    let player = session_player(w, session);
    player_fellowship::handle_fellowship_update_request(w, player, panel_open);
    Ok(())
}

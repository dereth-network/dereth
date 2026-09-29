// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseBootSpecificGuest.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseBootSpecificGuest.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseBootSpecificGuest.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player_name = message
        .decode_padded::<proto::trade::HouseBootSpecificHouseGuest>()?
        .name; // player name to boot from your house

    let player = session_player(w, session);
    player_house::handle_action_boot(w, player, &player_name, false);
    Ok(())
}

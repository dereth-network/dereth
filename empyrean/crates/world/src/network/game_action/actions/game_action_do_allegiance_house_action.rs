// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionDoAllegianceHouseAction.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionDoAllegianceHouseAction.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionDoAllegianceHouseAction.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::AllegianceDoAllegianceHouseAction>()?;
    let action = empyrean_entity::enums::AllegianceHouseAction(m.action);

    let player = session_player(w, session);
    player_house::handle_action_do_allegiance_house_action(w, player, action);
    Ok(())
}

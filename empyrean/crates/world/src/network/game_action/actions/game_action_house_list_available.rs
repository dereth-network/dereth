// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseListAvailable.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseListAvailable.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseListAvailable.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::HouseListAvailableHouses>()?;
    // type of house being listed
    let house_type = empyrean_entity::enums::HouseType(m.house_type.cs_cast());

    let player = session_player(w, session);
    player_house::handle_action_list_available(w, player, house_type);
    Ok(())
}

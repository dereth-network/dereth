// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRentHouse.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRentHouse.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseRentHouse.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::HouseRentHouse>()?;
    let slumlord = m.slumlord.0;
    let items: Vec<u32> = m.items.iter().map(|i| i.0).collect(); // items being used to pay rent

    let player = session_player(w, session);
    player_house::handle_action_rent_house(w, player, slumlord, items);
    Ok(())
}

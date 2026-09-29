// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseSetOpenStatus.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseSetOpenStatus.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseSetOpenStatus.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::HouseSetOpenHouseStatus>()?;
    let open_house = to_boolean(m.open);

    let player = session_player(w, session);
    player_house::handle_action_set_open_status(w, player, open_house);
    Ok(())
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetDesiredComponentLevel.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetDesiredComponentLevel.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_spells;
use crate::World;

// ACE: GameActionSetDesiredComponentLevel.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::combat::ComponentLevelRequest>()?;
    let component_wcid = m.component_did;
    let amount: u32 = m.level.cs_cast();

    let player = session_player(w, session);
    player_spells::handle_set_desired_component_level(w, player, component_wcid, amount);
    Ok(())
}

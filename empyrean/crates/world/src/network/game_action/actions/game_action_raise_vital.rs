// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseVital.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseVital.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_vitals;
use crate::World;

// ACE: GameActionRaiseVital.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::admin::TrainAttribute2nd>()?;
    let vital = empyrean_entity::enums::PropertyAttribute2nd(m.vital_id.cs_cast());
    let xp_spent = m.xp_spent;

    let player = session_player(w, session);
    player_vitals::handle_action_raise_vital(w, player, vital, xp_spent);
    Ok(())
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseAttribute.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseAttribute.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_attributes;
use crate::World;

// ACE: GameActionRaiseAttribute.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::admin::TrainAttribute>()?;
    let attribute = empyrean_entity::enums::PropertyAttribute(m.attribute_id.cs_cast());
    let xp_spent = m.xp_spent;

    let player = session_player(w, session);
    player_attributes::handle_action_raise_attribute(w, player, attribute, xp_spent);
    Ok(())
}

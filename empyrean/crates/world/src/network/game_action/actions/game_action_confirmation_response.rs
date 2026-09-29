// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionConfirmationResponse.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionConfirmationResponse.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::managers::confirmation_manager;
use crate::World;

// ACE: GameActionConfirmationResponse.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CharacterConfirmationResponse>()?;
    let confirm_type = empyrean_entity::enums::ConfirmationType(m.confirmation_type.cs_cast());
    let context = m.context_id;
    let response = to_boolean(m.accepted);

    let player = session_player(w, session);
    confirmation_manager::handle_response(w, player, confirm_type, context, response, false);
    Ok(())
}

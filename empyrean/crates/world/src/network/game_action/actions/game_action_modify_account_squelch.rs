// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionModifyAccountSquelch.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionModifyAccountSquelch.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::managers::squelch_manager;
use crate::World;

// ACE: GameActionModifyAccountSquelch.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode_padded::<proto::comms::CommunicationModifyAccountSquelch>()?;
    let (squelch, player_name) = (to_boolean(m.add), m.character_name);

    let player = session_player(w, session);
    squelch_manager::handle_action_modify_account_squelch(w, player, squelch, &player_name);
    Ok(())
}

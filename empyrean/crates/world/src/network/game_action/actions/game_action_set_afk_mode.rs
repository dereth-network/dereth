// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetAFKMode.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetAFKMode.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_networking;
use crate::World;

// ACE: GameActionSetAFKMode.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CommunicationSetAfkMode>()?;
    let afk = to_boolean(m.afk);

    let player = session_player(w, session);
    player_networking::handle_action_set_afk_mode(w, player, afk);
    Ok(())
}

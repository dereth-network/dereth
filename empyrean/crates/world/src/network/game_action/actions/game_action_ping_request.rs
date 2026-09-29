// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionPingRequest.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionPingRequest.cs`.

use empyrean_net::SessionId;

use crate::network::game_event::events::game_event_ping_response::game_event_ping_response;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::managers::inbound_message_manager::{HandlerResult, Payload};
use crate::World;

// ACE: GameActionPingRequest.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let msg = game_event_ping_response(w.sessions.get_mut(session).expect("session"));
    enqueue_send(w, session, msg);
    Ok(())
}

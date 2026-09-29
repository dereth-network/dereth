// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventPingResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventPingResponse.cs`.

use dereth_protocol::admin as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventPingResponse.GameEventPingResponse
#[must_use]
pub fn game_event_ping_response(session: &mut SessionData) -> GameMessage {
    game_event_from_proto(
        GameEventType::PingResponse,
        GameMessageGroup::UIQueue,
        session,
        &proto::CharacterReturnPing,
    )
}

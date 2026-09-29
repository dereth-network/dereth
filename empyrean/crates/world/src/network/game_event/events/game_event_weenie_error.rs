// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventWeenieError.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventWeenieError.cs`.

use dereth_protocol::comms as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventWeenieError.GameEventWeenieError
#[must_use]
pub fn game_event_weenie_error(session: &mut SessionData, error_type: WeenieError) -> GameMessage {
    game_event_from_proto(
        GameEventType::WeenieError,
        GameMessageGroup::UIQueue,
        session,
        &proto::CommunicationWeenieError {
            error_type: error_type.0.cs_cast(),
        },
    )
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventConfirmationRequest.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventConfirmationRequest.cs`.

use dereth_protocol::comms as proto;
use empyrean_entity::enums::ConfirmationType;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventConfirmationRequest.GameEventConfirmationRequest
#[must_use]
pub fn game_event_confirmation_request(
    session: &mut SessionData,
    confirmation_type: ConfirmationType,
    context: u32,
    text: &str,
) -> GameMessage {
    let body = proto::CharacterConfirmationRequest {
        confirmation_type: confirmation_type.0.cast_signed(),
        context_id: context,
        text: ace_str(text),
    };
    game_event_from_proto_strings(
        GameEventType::CharacterConfirmationRequest,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[text],
    )
}

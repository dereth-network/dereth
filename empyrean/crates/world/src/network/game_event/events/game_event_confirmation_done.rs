// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventConfirmationDone.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventConfirmationDone.cs`.

use dereth_protocol::comms as proto;
use empyrean_entity::enums::ConfirmationType;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventConfirmationDone.GameEventConfirmationDone
#[must_use]
pub fn game_event_confirmation_done(
    session: &mut SessionData,
    confirmation_type: ConfirmationType,
    context_id: u32,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::CharacterConfirmationDone,
        GameMessageGroup::UIQueue,
        session,
        &proto::CharacterConfirmationDone {
            confirmation_type: confirmation_type.0.cast_signed(),
            context_id,
        },
    )
}

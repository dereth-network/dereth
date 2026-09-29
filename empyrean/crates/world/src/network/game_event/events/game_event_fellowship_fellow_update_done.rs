// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipFellowUpdateDone.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipFellowUpdateDone.cs`.

use dereth_protocol::social as proto;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventFellowshipFellowUpdateDone.GameEventFellowshipFellowUpdateDone
/// Outdated message, not found at the end of retail? ACE leaves the error type unwritten
/// (`//Writer.Write((uint)errorType);  // should this be here?`).
#[must_use]
pub fn game_event_fellowship_fellow_update_done(
    session: &mut SessionData,
    _error_type: WeenieError,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::FellowshipFellowUpdateDone,
        GameMessageGroup::UIQueue,
        session,
        &proto::FellowshipFellowUpdateDone,
    )
}

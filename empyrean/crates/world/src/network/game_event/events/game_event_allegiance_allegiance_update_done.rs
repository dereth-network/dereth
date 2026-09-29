// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceAllegianceUpdateDone.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceAllegianceUpdateDone.cs`.

use dereth_protocol::social as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventAllegianceAllegianceUpdateDone.GameEventAllegianceAllegianceUpdateDone
/// `errorType` defaults to `WeenieError.None` in ACE.
#[must_use]
pub fn game_event_allegiance_allegiance_update_done(
    session: &mut SessionData,
    error_type: WeenieError,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::AllegianceAllegianceUpdateDone,
        GameMessageGroup::UIQueue,
        session,
        &proto::AllegianceUpdateDone {
            failure_type: error_type.0.cs_cast(),
        },
    )
}
